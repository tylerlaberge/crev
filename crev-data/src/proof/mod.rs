//! Some common stuff for both Review and Trust Proofs

use crate::level::Level;
use base64;
use chrono::{self, prelude::*};
use crev_common;
use std::{default, fmt, fs, io, mem, path::Path};

pub mod project_info;
pub mod review;
pub mod revision;
pub mod trust;

pub use self::{project_info::*, revision::*, trust::*};

use crate::Result;

pub trait ContentCommon {
    fn date(&self) -> &chrono::DateTime<FixedOffset>;
    fn author(&self) -> &crate::PubId;

    fn date_utc(&self) -> chrono::DateTime<Utc> {
        self.date().with_timezone(&Utc)
    }

    fn author_id(&self) -> crate::Id {
        self.author().id.clone()
    }

    fn author_url(&self) -> Option<String> {
        self.author().url.as_ref().map(|v| v.url.to_owned())
    }
}

#[derive(Copy, Clone, Debug)]
pub enum ProofType {
    Code,
    Project,
    Trust,
}

impl ProofType {
    fn begin_block(&self) -> &'static str {
        match self {
            ProofType::Code => review::Code::BEGIN_BLOCK,
            ProofType::Project => review::Project::BEGIN_BLOCK,
            ProofType::Trust => Trust::BEGIN_BLOCK,
        }
    }
    fn begin_signature(&self) -> &'static str {
        match self {
            ProofType::Code => review::Code::BEGIN_SIGNATURE,
            ProofType::Project => review::Project::BEGIN_SIGNATURE,
            ProofType::Trust => Trust::BEGIN_SIGNATURE,
        }
    }
    fn end_block(&self) -> &'static str {
        match self {
            ProofType::Code => review::Code::END_BLOCK,
            ProofType::Project => review::Project::END_BLOCK,
            ProofType::Trust => Trust::END_BLOCK,
        }
    }
}

/// Serialized Proof
///
/// A signed proof containing some signed `Content`
#[derive(Debug, Clone)]
pub(crate) struct Serialized {
    /// Serialized content
    pub body: String,
    /// Signature over the body
    pub signature: String,
    /// Type of the `body` (`Content`)
    pub type_: ProofType,
}

/// Content is an enumerator of possible proof contents
#[derive(Debug, Clone)]
pub enum Content {
    Trust(Trust),
    Project(review::Project),
    Code(review::Code),
}

impl fmt::Display for Content {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use self::Content::*;
        match self {
            Trust(trust) => trust.fmt(f),
            Code(code) => code.fmt(f),
            Project(project) => project.fmt(f),
        }
    }
}

impl From<review::Code> for Content {
    fn from(review: review::Code) -> Self {
        Content::Code(review)
    }
}

impl From<review::Project> for Content {
    fn from(review: review::Project) -> Self {
        Content::Project(review)
    }
}

impl From<Trust> for Content {
    fn from(review: Trust) -> Self {
        Content::Trust(review)
    }
}

impl Content {
    pub fn parse(s: &str, type_: ProofType) -> Result<Content> {
        Ok(match type_ {
            ProofType::Code => Content::Code(review::Code::parse(&s)?),
            ProofType::Project => Content::Project(review::Project::parse(&s)?),
            ProofType::Trust => Content::Trust(Trust::parse(&s)?),
        })
    }

    pub fn parse_draft(s: &str, type_: ProofType) -> Result<Content> {
        Ok(match type_ {
            ProofType::Code => Content::Code(review::CodeDraft::parse(&s)?.into()),
            ProofType::Project => Content::Project(review::ProjectDraft::parse(&s)?.into()),
            ProofType::Trust => Content::Trust(TrustDraft::parse(&s)?.into()),
        })
    }
    pub fn sign_by(&self, id: &crate::id::OwnId) -> Result<Proof> {
        let body = self.to_string();
        let signature = id.sign(&body.as_bytes());
        Ok(Proof {
            digest: crev_common::blake2sum(&body.as_bytes()),
            body: body,
            signature: base64::encode_config(&signature, base64::URL_SAFE),
            content: self.clone(),
        })
    }

    pub fn proof_type(&self) -> ProofType {
        use self::Content::*;
        match self {
            Trust(_trust) => ProofType::Trust,
            Code(_review) => ProofType::Code,
            Project(_review) => ProofType::Project,
        }
    }

    pub fn date(&self) -> &chrono::DateTime<FixedOffset> {
        use self::Content::*;
        match self {
            Trust(trust) => trust.date(),
            Code(review) => review.date(),
            Project(review) => review.date(),
        }
    }

    pub fn author_id(&self) -> crate::Id {
        use self::Content::*;
        match self {
            Trust(trust) => trust.author_id(),
            Code(review) => review.author_id(),
            Project(review) => review.author_id(),
        }
    }

    pub fn author_url(&self) -> Option<String> {
        use self::Content::*;
        match self {
            Trust(trust) => trust.author_url(),
            Code(review) => review.author_url(),
            Project(review) => review.author_url(),
        }
    }

    pub fn to_draft_string(&self) -> String {
        use self::Content::*;
        match self.clone() {
            Trust(trust) => format!("{}", TrustDraft::from(trust)),
            Code(review) => format!("{}", review::CodeDraft::from(review)),
            Project(review) => format!("{}", review::ProjectDraft::from(review)),
        }
    }
}

#[derive(Debug, Clone)]
/// A `Proof` with it's content parsed and ready.
pub struct Proof {
    pub body: String,
    pub signature: String,
    pub digest: Vec<u8>,
    pub content: Content,
}

impl fmt::Display for Serialized {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.type_.begin_block())?;
        f.write_str("\n")?;
        f.write_str(&self.body)?;
        f.write_str(self.type_.begin_signature())?;
        f.write_str("\n")?;
        f.write_str(&self.signature)?;
        f.write_str("\n")?;
        f.write_str(self.type_.end_block())?;
        f.write_str("\n")?;

        Ok(())
    }
}

impl fmt::Display for Proof {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.content.proof_type().begin_block())?;
        f.write_str("\n")?;
        f.write_str(&self.body)?;
        f.write_str(self.content.proof_type().begin_signature())?;
        f.write_str("\n")?;
        f.write_str(&self.signature)?;
        f.write_str("\n")?;
        f.write_str(self.content.proof_type().end_block())?;
        f.write_str("\n")?;

        Ok(())
    }
}

impl Serialized {
    pub fn to_parsed(&self) -> Result<Proof> {
        Ok(Proof {
            body: self.body.clone(),
            signature: self.signature.clone(),
            digest: crev_common::blake2sum(&self.body.as_bytes()),
            content: match self.type_ {
                ProofType::Code => Content::Code(review::Code::parse(&self.body)?),
                ProofType::Project => Content::Project(review::Project::parse(&self.body)?),
                ProofType::Trust => Content::Trust(Trust::parse(&self.body)?),
            },
        })
    }

    pub fn parse(reader: impl io::BufRead) -> Result<Vec<Self>> {
        #[derive(PartialEq, Eq)]
        enum Stage {
            None,
            Body,
            Signature,
        }

        impl Default for Stage {
            fn default() -> Self {
                Stage::None
            }
        }

        struct State {
            stage: Stage,
            body: String,
            signature: String,
            type_: ProofType,
            proofs: Vec<Serialized>,
        }

        impl default::Default for State {
            fn default() -> Self {
                State {
                    stage: Default::default(),
                    body: Default::default(),
                    signature: Default::default(),
                    type_: ProofType::Trust, // whatever
                    proofs: vec![],
                }
            }
        }

        impl State {
            fn process_line(&mut self, line: &str) -> Result<()> {
                match self.stage {
                    Stage::None => {
                        let line = line.trim();
                        if line.is_empty() {
                        } else if line == ProofType::Code.begin_block() {
                            self.type_ = ProofType::Code;
                            self.stage = Stage::Body;
                        } else if line == ProofType::Trust.begin_block() {
                            self.type_ = ProofType::Trust;
                            self.stage = Stage::Body;
                        } else if line == ProofType::Project.begin_block() {
                            self.type_ = ProofType::Project;
                            self.stage = Stage::Body;
                        } else {
                            bail!("Parsing error when looking for start of code review proof");
                        }
                    }
                    Stage::Body => {
                        if line.trim() == self.type_.begin_signature() {
                            self.stage = Stage::Signature;
                        } else {
                            self.body += line;
                            self.body += "\n";
                        }
                        if self.body.len() > 16_000 {
                            bail!("Proof body too long");
                        }
                    }
                    Stage::Signature => {
                        if line.trim() == self.type_.end_block() {
                            self.stage = Stage::None;
                            self.proofs.push(Serialized {
                                body: mem::replace(&mut self.body, String::new()),
                                signature: mem::replace(&mut self.signature, String::new()),
                                type_: self.type_,
                            });
                        } else {
                            self.signature += line;
                            self.signature += "\n";
                        }
                        if self.signature.len() > 2000 {
                            bail!("Signature too long");
                        }
                    }
                }
                Ok(())
            }

            fn finish(self) -> Result<Vec<Serialized>> {
                if self.stage != Stage::None {
                    bail!("Unexpected EOF while parsing");
                }
                Ok(self.proofs)
            }
        }

        let mut state: State = Default::default();

        for line in reader.lines() {
            state.process_line(&line?)?;
        }

        state.finish()
    }
}

impl Proof {
    pub fn parse_from(path: &Path) -> Result<Vec<Self>> {
        let file = fs::File::open(path)?;
        Self::parse(io::BufReader::new(file))
    }

    pub fn parse(reader: impl io::BufRead) -> Result<Vec<Self>> {
        let mut v = vec![];
        for serialized in Serialized::parse(reader)?.into_iter() {
            v.push(serialized.to_parsed()?)
        }
        Ok(v)
    }

    /*
    pub fn signature(&self) -> Result<Vec<u8>> {
        let sig = self.signature.trim();
        Ok(base64::decode_config(sig, base64::URL_SAFE)?)
    }
    */

    pub fn signature(&self) -> &str {
        self.signature.trim()
    }

    pub fn verify(&self) -> Result<()> {
        let pubkey = self.content.author_id();
        pubkey.verify_signature(self.body.as_bytes(), self.signature())?;

        Ok(())
    }
}

fn equals_default_digest_type(s: &str) -> bool {
    s == default_digest_type()
}

pub fn default_digest_type() -> String {
    "blake2b".into()
}

fn equals_default_revision_type(s: &str) -> bool {
    s == default_revision_type()
}

pub fn default_revision_type() -> String {
    "git".into()
}

fn equals_default_distrust_level(l: &Level) -> bool {
    *l == default_distrust_level()
}

fn equals_default<T: Default + PartialEq>(t: &T) -> bool {
    *t == Default::default()
}

fn equals_none_level(l: &Level) -> bool {
    *l == Level::None
}

fn default_distrust_level() -> Level {
    Level::None
}

fn none_level() -> Level {
    Level::None
}

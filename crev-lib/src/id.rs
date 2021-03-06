use argonautica::{self, Hasher};
use base64;
use rand::{self, Rng};
/*
use blake2;
use common_failures::prelude::*;
*/
use crev_common::serde::{as_base64, from_base64};
use miscreant;
use serde_yaml;
use std::{
    self, fmt,
    io::{Read, Write},
    path::Path,
};

use crate::Result;
use crev_data::id::{OwnId, PubId};

#[derive(Serialize, Deserialize, Debug)]
pub struct PassConfig {
    version: u32,
    variant: String,
    iterations: u32,
    #[serde(rename = "memory-size")]
    memory_size: u32,
    #[serde(serialize_with = "as_base64", deserialize_with = "from_base64")]
    salt: Vec<u8>,
}

/// Serialized, stored on disk
#[derive(Serialize, Deserialize, Debug)]
pub struct LockedId {
    version: i64,
    url: String,
    #[serde(serialize_with = "as_base64", deserialize_with = "from_base64")]
    #[serde(rename = "public-key")]
    pub public_key: Vec<u8>,
    #[serde(serialize_with = "as_base64", deserialize_with = "from_base64")]
    #[serde(rename = "sealed-secret-key")]
    sealed_secret_key: Vec<u8>,

    #[serde(serialize_with = "as_base64", deserialize_with = "from_base64")]
    #[serde(rename = "seal-nonce")]
    seal_nonce: Vec<u8>,
    pass: PassConfig,
}

impl fmt::Display for LockedId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // https://github.com/dtolnay/serde-yaml/issues/103
        f.write_str(&serde_yaml::to_string(self).map_err(|_| fmt::Error)?)
    }
}

impl LockedId {
    pub fn from_own_id(own_id: &OwnId, passphrase: &str) -> Result<LockedId> {
        use miscreant::aead::Algorithm;
        let mut hasher = Hasher::default();

        hasher
            .configure_memory_size(4096)
            .configure_hash_len(64)
            .opt_out_of_secret_key(true);

        let pwhash = hasher.with_password(passphrase).hash_raw()?;

        let mut siv = miscreant::aead::Aes256Siv::new(pwhash.raw_hash_bytes());

        let seal_nonce: Vec<u8> = rand::thread_rng()
            .sample_iter(&rand::distributions::Standard)
            .take(32)
            .collect();

        let hasher_config = hasher.config();

        assert_eq!(hasher_config.version(), argonautica::config::Version::_0x13);
        Ok(LockedId {
            version: crev_data::current_version(),
            public_key: own_id.keypair.public.to_bytes().to_vec(),
            sealed_secret_key: siv.seal(&seal_nonce, &[], own_id.keypair.secret.as_bytes()),
            seal_nonce: seal_nonce,
            url: own_id.id.url.clone().unwrap().url,
            pass: PassConfig {
                salt: pwhash.raw_salt_bytes().to_vec(),
                iterations: hasher_config.iterations(),
                memory_size: hasher_config.memory_size(),
                version: 0x13,
                variant: hasher_config.variant().as_str().to_string(),
            },
        })
    }

    pub fn to_pubid(&self) -> PubId {
        PubId::new(self.public_key.to_owned(), self.url.to_owned())
    }

    pub fn pub_key_as_base64(&self) -> String {
        base64::encode_config(&self.public_key, base64::URL_SAFE)
    }

    pub fn save_to(&self, path: &Path) -> Result<()> {
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        write!(file, "{}", self)?;

        Ok(())
    }

    pub fn read_from_yaml_file(path: &Path) -> Result<Self> {
        let mut file = std::fs::File::open(path)?;
        let mut content = String::new();
        file.read_to_string(&mut content)?;

        Ok(serde_yaml::from_str::<LockedId>(&content)?)
    }

    pub fn to_unlocked(&self, passphrase: &str) -> Result<OwnId> {
        let LockedId {
            ref version,
            ref url,
            ref public_key,
            ref sealed_secret_key,
            ref seal_nonce,
            ref pass,
        } = self;
        {
            if *version != crev_data::current_version() {
                bail!("Unsupported version");
            }
            use miscreant::aead::Algorithm;

            let mut hasher = Hasher::default();

            hasher
                .configure_memory_size(pass.memory_size)
                .configure_version(argonautica::config::Version::from_u32(pass.version)?)
                .configure_iterations(pass.iterations)
                .configure_variant(std::str::FromStr::from_str(&pass.variant)?)
                .with_salt(&pass.salt)
                .configure_hash_len(64)
                .opt_out_of_secret_key(true);

            let pwhash = hasher.with_password(passphrase).hash_raw()?;

            let mut siv = miscreant::aead::Aes256Siv::new(pwhash.raw_hash_bytes());

            let sec_key = siv.open(&seal_nonce, &[], &sealed_secret_key)?;

            let res = OwnId::new(url.to_owned(), sec_key)?;

            if public_key != &res.keypair.public.to_bytes() {
                bail!("PubKey mismatch");
            }

            Ok(res)
        }
    }
}

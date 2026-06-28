use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct VaultEntry {
    pub id: String,
    pub title: String,
    pub username: String,
    pub password: String, // En clair uniquement en mémoire (zeroize recommandé en prod)
    pub url: String,
    pub last_modified: u64,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct VaultData {
    pub version: u32,
    pub entries: HashMap<String, VaultEntry>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct EncryptedVault {
    pub salt: [u8; 16],
    pub nonce: [u8; 12],
    pub ciphertext: Vec<u8>,
}

impl VaultData {
    pub fn new() -> Self {
        Self {
            version: 1,
            entries: HashMap::new(),
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        bincode::serialize(self).unwrap()
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self, bincode::Error> {
        bincode::deserialize(data)
    }
}

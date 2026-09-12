//! Modèle du coffre et formats de fichier `.vault`.
//!
//! - **v1** ([`EncryptedVault`], lecture seule) : bincode `{ salt, nonce,
//!   ciphertext }`, commence directement par le sel.
//! - **v2** ([`EncryptedVaultV2`]) : 3 octets magic [`V2_MAGIC`] (`KY\x02`) puis
//!   bincode incluant le ciphertext KEM ML-KEM-1024 et la clé secrète ML-KEM
//!   scellée sous la seed Argon2id.
//!
//! Les données en clair ([`VaultData`]) sont sérialisées en bincode avant
//! chiffrement.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Magic bytes présents au début de tout fichier .vault v2 (Kyber intégré).
/// Les fichiers v1 commencent directement par les bytes bincode (sel aléatoire).
pub const V2_MAGIC: &[u8; 3] = b"KY\x02";

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct VaultEntry {
    pub id: String,
    pub title: String,
    pub username: String,
    pub password: String,
    pub url: String,
    pub last_modified: u64,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct VaultData {
    pub version: u32,
    pub entries: HashMap<String, VaultEntry>,
}

/// Format v1 — Argon2id → AES-256-GCM (legacy, lecture seule)
#[derive(Serialize, Deserialize, Debug)]
pub struct EncryptedVault {
    pub salt: [u8; 16],
    pub nonce: [u8; 12],
    pub ciphertext: Vec<u8>,
}

/// Format v2 — Argon2id + Kyber1024 → HKDF → AES-256-GCM
#[derive(Serialize, Deserialize, Debug)]
pub struct EncryptedVaultV2 {
    pub salt: [u8; 16],
    /// Ciphertext KEM Kyber1024 (1568 bytes) — permet de retrouver pq_ss au déverrouillage
    pub pq_ct: Vec<u8>,
    /// Clé secrète Kyber chiffrée sous la seed Argon2id
    pub pq_sk_enc: Vec<u8>,
    pub pq_sk_nonce: [u8; 12],
    pub nonce: [u8; 12],
    pub ciphertext: Vec<u8>,
}

impl Default for VaultData {
    fn default() -> Self {
        Self::new()
    }
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

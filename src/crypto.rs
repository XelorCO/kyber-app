use aes_gcm::{Aes256Gcm, Key, Nonce};
use aes_gcm::aead::{Aead, KeyInit};
use argon2::{Argon2, Algorithm, Version, Params};
use hkdf::Hkdf;
use sha2::Sha256;
use pqcrypto_kyber::kyber1024::*;
use pqcrypto_traits::kem::{SharedSecret as PqSharedSecret, Ciphertext as PqCiphertext};
use rand_core::{OsRng, RngCore};
use x25519_dalek::{EphemeralSecret, PublicKey};
use zeroize::{Zeroize, ZeroizeOnDrop};

#[derive(Zeroize, ZeroizeOnDrop)]
pub struct MasterKey([u8; 32]);

pub fn derive_master_key(passphrase: &str, salt: &[u8]) -> MasterKey {
    let mut key = [0u8; 32];
    // Paramètres Argon2id robustes (memory-hard)
    let params = Params::new(65536, 4, 1, Some(32)).unwrap();
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    argon2.hash_password_into(passphrase.as_bytes(), salt, &mut key).unwrap();
    MasterKey(key)
}

pub fn encrypt_vault_payload(master_key: &MasterKey, plaintext: &[u8]) -> (Vec<u8>, [u8; 12]) {
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&master_key.0));
    let mut nonce_bytes = [0u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher.encrypt(nonce, plaintext).expect("Encryption failure");
    (ciphertext, nonce_bytes)
}

pub fn decrypt_vault_payload(master_key: &MasterKey, nonce_bytes: &[u8; 12], ciphertext: &[u8]) -> Result<Vec<u8>, aes_gcm::Error> {
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&master_key.0));
    let nonce = Nonce::from_slice(nonce_bytes);
    cipher.decrypt(nonce, ciphertext)
}

// Chiffrement hybride asymétrique (Post-Quantique + Classique)
// Utilisé pour le partage ou la récupération de clés.
pub struct HybridKeyPair {
    pub pq_pk: pqcrypto_kyber::kyber1024::PublicKey,
    pub pq_sk: pqcrypto_kyber::kyber1024::SecretKey,
    pub cl_sk: EphemeralSecret,
    pub cl_pk: PublicKey,
}

impl HybridKeyPair {
    pub fn generate() -> Self {
        let (pq_pk, pq_sk) = keypair();
        let cl_sk = EphemeralSecret::random_from_rng(OsRng);
        let cl_pk = PublicKey::from(&cl_sk);
        Self { pq_pk, pq_sk, cl_sk, cl_pk }
    }
}

pub fn hybrid_encapsulate(pq_pk: &pqcrypto_kyber::kyber1024::PublicKey, cl_pk: &PublicKey) -> (Vec<u8>, Vec<u8>) {
    // KEM post-quantique (Kyber1024)
    let (pq_ss, pq_ct) = encapsulate(pq_pk);

    // KEM classique (X25519)
    let cl_sk = EphemeralSecret::random_from_rng(OsRng);
    let my_cl_pk = PublicKey::from(&cl_sk);
    let cl_ss = cl_sk.diffie_hellman(cl_pk);

    // Combiner les shared secrets via HKDF-SHA256 (IND-CCA2 correct)
    // IKM = pq_ss || cl_ss — si l'un est cassé, l'autre protège toujours
    let mut ikm = Vec::with_capacity(pq_ss.as_bytes().len() + cl_ss.as_bytes().len());
    ikm.extend_from_slice(pq_ss.as_bytes());
    ikm.extend_from_slice(cl_ss.as_bytes());

    let hk = Hkdf::<Sha256>::new(None, &ikm);
    let mut okm = [0u8; 32];
    hk.expand(b"PqPassMgr-HybridKEM-v1", &mut okm)
        .expect("HKDF expand failed");

    // Le ciphertext inclut les deux parties publiques (pour le décapsulage)
    let mut combined_ct = Vec::new();
    combined_ct.extend_from_slice(pq_ct.as_bytes());
    combined_ct.extend_from_slice(my_cl_pk.as_bytes());

    (okm.to_vec(), combined_ct)
}

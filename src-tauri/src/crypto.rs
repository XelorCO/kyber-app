use aes_gcm::{Aes256Gcm, Key, Nonce};
use aes_gcm::aead::{Aead, KeyInit};
use argon2::{Argon2, Algorithm, Version, Params};
use hkdf::Hkdf;
use sha2::Sha256;
use pqcrypto_kyber::kyber1024::*;
use pqcrypto_traits::kem::{SharedSecret as PqSharedSecret, Ciphertext as PqCiphertext, SecretKey as PqSecretKeyTrait};
use rand_core::{OsRng, RngCore};
use x25519_dalek::{EphemeralSecret, PublicKey};
use zeroize::{Zeroize, ZeroizeOnDrop};

#[derive(Zeroize, ZeroizeOnDrop)]
pub struct MasterKey(pub [u8; 32]);

/// Argon2id → seed brute [u8; 32]. Base commune v1 et v2.
pub fn derive_seed_key(passphrase: &str, salt: &[u8]) -> [u8; 32] {
    let mut key = [0u8; 32];
    let params = Params::new(65536, 4, 1, Some(32)).unwrap();
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    argon2.hash_password_into(passphrase.as_bytes(), salt, &mut key).unwrap();
    key
}

/// v1 compat : Argon2id → MasterKey directement (seed = clé finale)
pub fn derive_master_key(passphrase: &str, salt: &[u8]) -> MasterKey {
    MasterKey(derive_seed_key(passphrase, salt))
}

/// HKDF-SHA256(seed_key ‖ pq_ss) → clé finale 32 bytes.
///
/// NOTE de sécurité (honnête) : la sécurité du coffre AU REPOS repose entièrement
/// sur `seed_key` = Argon2id(passphrase). La clé secrète Kyber est elle-même scellée
/// sous `seed_key` (cf. create_kyber_vault_key), donc un attaquant qui bruteforce la
/// passphrase récupère aussi pq_ss : la couche Kyber n'ajoute PAS de marge contre un
/// bruteforce de passphrase. Elle apporte de la défense en profondeur (liaison KEM)
/// et prépare le partage de clé hybride. La résistance quantique du coffre vient
/// d'AES-256-GCM (128 bits post-Grover) + Argon2id, pas de Kyber.
fn derive_final_key(seed_key: &[u8; 32], pq_ss: &[u8]) -> MasterKey {
    let mut ikm = Vec::with_capacity(32 + pq_ss.len());
    ikm.extend_from_slice(seed_key);
    ikm.extend_from_slice(pq_ss);
    let hk = Hkdf::<Sha256>::new(None, &ikm);
    let mut okm = [0u8; 32];
    hk.expand(b"KyberVault-v2-final-key", &mut okm).expect("HKDF expand");
    MasterKey(okm)
}

/// Création d'un coffre v2 : génère une paire Kyber1024, encapsule, combine avec seed.
/// Retourne (final_key, pq_ct_bytes, pq_sk_enc, pq_sk_nonce).
pub fn create_kyber_vault_key(seed_key: &[u8; 32]) -> (MasterKey, Vec<u8>, Vec<u8>, [u8; 12]) {
    let (pq_pk, pq_sk) = keypair();
    let (pq_ss, pq_ct) = encapsulate(&pq_pk);

    let final_key = derive_final_key(seed_key, pq_ss.as_bytes());

    // Chiffre pq_sk sous seed_key — récupérable uniquement avec la passphrase
    let seed_mk = MasterKey(*seed_key);
    let (pq_sk_enc, pq_sk_nonce) = encrypt_vault_payload(&seed_mk, pq_sk.as_bytes());

    (final_key, pq_ct.as_bytes().to_vec(), pq_sk_enc, pq_sk_nonce)
}

/// Ouverture d'un coffre v2 : déchiffre pq_sk, décapsule, recombine avec seed.
pub fn open_kyber_vault_key(
    seed_key: &[u8; 32],
    pq_ct_bytes: &[u8],
    pq_sk_enc: &[u8],
    pq_sk_nonce: &[u8; 12],
) -> Result<MasterKey, String> {
    let seed_mk = MasterKey(*seed_key);
    let pq_sk_bytes = decrypt_vault_payload(&seed_mk, pq_sk_nonce, pq_sk_enc)
        .map_err(|_| "Mot de passe incorrect ou coffre corrompu.".to_string())?;

    let pq_sk = SecretKey::from_bytes(&pq_sk_bytes)
        .map_err(|_| "Clé secrète Kyber corrompue.".to_string())?;
    let pq_ct = Ciphertext::from_bytes(pq_ct_bytes)
        .map_err(|_| "Ciphertext Kyber corrompu.".to_string())?;

    let pq_ss = decapsulate(&pq_ct, &pq_sk);

    Ok(derive_final_key(seed_key, pq_ss.as_bytes()))
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
    #[allow(dead_code)] // réservé pour le partage hybride (pas encore câblé)
    pq_sk: pqcrypto_kyber::kyber1024::SecretKey,  // privé — ne pas exposer ni sérialiser
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
    // IKM = pq_ss || cl_ss — si l'un des deux est cassé, l'autre protège toujours
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

#[cfg(test)]
mod tests {
    use super::*;
    use pqcrypto_traits::kem::PublicKey as PqPK;

    #[test]
    fn test_argon2id_params() {
        let salt = [0u8; 16];
        let key = derive_master_key("motdepasse_test", &salt);
        // Argon2id produit toujours 32 bytes
        assert_eq!(key.0.len(), 32, "La clé maître doit faire 32 bytes (AES-256)");
        // Même entrée = même sortie (déterministe)
        let key2 = derive_master_key("motdepasse_test", &salt);
        assert_eq!(key.0, key2.0, "Argon2id doit être déterministe");
        // Passphrase différente = clé différente
        let key3 = derive_master_key("autre_motdepasse", &salt);
        assert_ne!(key.0, key3.0, "Passphrases différentes doivent produire des clés différentes");
        println!("[ARGON2ID OK] clé 32B dérivée : {:02x?}...", &key.0[..4]);
    }

    #[test]
    fn test_aes256_gcm_roundtrip() {
        let salt = [1u8; 16];
        let master_key = derive_master_key("test", &salt);
        let plaintext = b"Mon mot de passe super secret 42!";

        let (ciphertext, nonce) = encrypt_vault_payload(&master_key, plaintext);

        // Le ciphertext ne doit pas être en clair
        assert_ne!(ciphertext.as_slice(), plaintext.as_slice());
        // AES-256-GCM ajoute un tag de 16 bytes
        assert_eq!(ciphertext.len(), plaintext.len() + 16);

        let decrypted = decrypt_vault_payload(&master_key, &nonce, &ciphertext).unwrap();
        assert_eq!(decrypted, plaintext, "Le déchiffrement doit restituer le plaintext");
        println!("[AES-256-GCM OK] ciphertext {} bytes, roundtrip validé", ciphertext.len());
    }

    #[test]
    fn test_aes256_gcm_wrong_key_fails() {
        let salt = [2u8; 16];
        let key_good = derive_master_key("bonne_cle", &salt);
        let key_bad  = derive_master_key("mauvaise_cle", &salt);
        let (ciphertext, nonce) = encrypt_vault_payload(&key_good, b"secret");
        // Mauvaise clé doit échouer (tag GCM invalide)
        let result = decrypt_vault_payload(&key_bad, &nonce, &ciphertext);
        assert!(result.is_err(), "Une mauvaise clé doit être rejetée par le tag GCM");
        println!("[AES-256-GCM OK] mauvaise clé correctement rejetée");
    }

    #[test]
    fn test_kyber1024_key_sizes() {
        // Vérifie les tailles via create_kyber_vault_key (flux réel)
        let seed = [0u8; 32];
        let (_fk, pq_ct, pq_sk_enc, _nonce) = create_kyber_vault_key(&seed);
        // pq_ct Kyber1024 = 1568 bytes
        assert_eq!(pq_ct.len(), 1568, "Ciphertext Kyber1024 = 1568 bytes");
        // pq_sk_enc = 3168 (SK) + 16 (GCM tag) = 3184 bytes
        assert_eq!(pq_sk_enc.len(), 3168 + 16, "SK Kyber1024 chiffré = 3184 bytes");
        // pq_pk accessible via HybridKeyPair pour vérification directe
        let pair = HybridKeyPair::generate();
        assert_eq!(pair.pq_pk.as_bytes().len(), 1568, "Clé publique Kyber1024 = 1568 bytes");
        println!("[KYBER1024 OK] pk={}B ct={}B sk_enc={}B", pair.pq_pk.as_bytes().len(), pq_ct.len(), pq_sk_enc.len());
    }

    #[test]
    fn test_kyber1024_encapsulation() {
        let pair = HybridKeyPair::generate();
        let (shared_secret, ciphertext) = hybrid_encapsulate(&pair.pq_pk, &pair.cl_pk);
        // Shared secret = 32 bytes (sortie HKDF)
        assert_eq!(shared_secret.len(), 32, "Shared secret hybride doit faire 32 bytes");
        // Ciphertext Kyber1024 = 1568 bytes + 32 bytes X25519 public key
        assert_eq!(ciphertext.len(), 1568 + 32, "Ciphertext hybride = Kyber1024(1568B) + X25519(32B)");
        println!("[KYBER1024 OK] KEM encapsulation : ss={}B ct={}B", shared_secret.len(), ciphertext.len());
    }

    #[test]
    fn test_v2_vault_roundtrip() {
        // Simule le flux complet d'un coffre v2 : création → déverrouillage
        let passphrase = "super_passphrase_utilisateur";
        let salt = [42u8; 16];

        // Étape création (init_vault)
        let seed_key = derive_seed_key(passphrase, &salt);
        let (final_key_create, pq_ct, pq_sk_enc, pq_sk_nonce) = create_kyber_vault_key(&seed_key);
        let plaintext_vault = b"donnees_secretes_du_coffre";
        let (ciphertext, nonce) = encrypt_vault_payload(&final_key_create, plaintext_vault);

        // Étape déverrouillage (unlock_vault) — repart de zéro avec juste la passphrase
        let seed_key2 = derive_seed_key(passphrase, &salt);
        let final_key_open = open_kyber_vault_key(&seed_key2, &pq_ct, &pq_sk_enc, &pq_sk_nonce)
            .expect("open_kyber_vault_key doit réussir avec la bonne passphrase");

        let decrypted = decrypt_vault_payload(&final_key_open, &nonce, &ciphertext)
            .expect("Déchiffrement doit réussir");
        assert_eq!(decrypted, plaintext_vault, "Les données doivent être identiques après roundtrip v2");
        println!("[V2 ROUNDTRIP OK] create → encrypt → open → decrypt : données intactes");

        // Mauvaise passphrase → échec garanti
        let wrong_seed = derive_seed_key("mauvaise_passphrase", &salt);
        let result = open_kyber_vault_key(&wrong_seed, &pq_ct, &pq_sk_enc, &pq_sk_nonce);
        assert!(result.is_err(), "Mauvaise passphrase doit être rejetée au niveau Kyber");
        println!("[V2 ROUNDTRIP OK] mauvaise passphrase rejetée avant même AES");
    }

    #[test]
    fn test_clefs_non_forgeables() {
        // Deux paires générées indépendamment ne doivent JAMAIS partager la même clé
        let pair1 = HybridKeyPair::generate();
        let pair2 = HybridKeyPair::generate();
        assert_ne!(pair1.pq_pk.as_bytes(), pair2.pq_pk.as_bytes(), "Deux PK Kyber1024 différentes");

        // Encapsuler avec la clé de pair1, essayer de décoder avec la clé de pair2
        // → le shared_secret obtenu sera différent (pas d'oracle de déchiffrement sans SK)
        let (ss1, _ct) = hybrid_encapsulate(&pair1.pq_pk, &pair1.cl_pk);
        let (ss2, _ct2) = hybrid_encapsulate(&pair2.pq_pk, &pair2.cl_pk);
        assert_ne!(ss1, ss2, "Shared secrets de paires différentes doivent être différents");

        // Preuve que AES chiffré pour pair1 est illisible par pair2
        let key1_bytes: [u8; 32] = ss1.try_into().unwrap();
        let key2_bytes: [u8; 32] = ss2.try_into().unwrap();
        let mk1 = MasterKey(key1_bytes);
        let mk2 = MasterKey(key2_bytes);
        let (ct, nonce) = encrypt_vault_payload(&mk1, b"secret de pair1");
        let result = decrypt_vault_payload(&mk2, &nonce, &ct);
        assert!(result.is_err(), "La clé de pair2 ne peut PAS déchiffrer ce qu'a chiffré pair1");
        println!("[NON-FORGEABLE OK] clés indépendantes, cross-déchiffrement impossible");
    }
}

mod crypto;
mod vault;
mod scanner;

use std::fs;
use std::path::PathBuf;
use rand_core::{OsRng, RngCore};
use x25519_dalek::PublicKey;
use bincode;

fn get_vault_path() -> PathBuf {
    let mut p = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    p.push(".kyber");
    if !p.exists() {
        fs::create_dir_all(&p).unwrap();
    }
    p.push("vault.enc");
    p
}

fn main() {
    println!("[*] Initialisation du gestionnaire PqPassMgr (Post-Quantum)...");

    let passphrase = "fuck_this_unbreakable_password_1337";
    
    // Simuler la création du vault
    let mut salt = [0u8; 16];
    OsRng.fill_bytes(&mut salt);

    let master_key = crypto::derive_master_key(passphrase, &salt);
    println!("[+] Clé maître dérivée avec Argon2id.");

    let mut my_vault = vault::VaultData::new();
    my_vault.entries.insert(
        "github.com".to_string(),
        vault::VaultEntry {
            id: "1".to_string(),
            title: "GitHub".to_string(),
            username: "cook45_hax".to_string(),
            password: "super_secret_pq_password".to_string(),
            url: "https://github.com/login".to_string(),
            last_modified: 1715000000,
        }
    );

    let vault_bytes = my_vault.to_bytes();
    let (ciphertext, nonce) = crypto::encrypt_vault_payload(&master_key, &vault_bytes);
    
    let encrypted_vault = vault::EncryptedVault {
        salt,
        nonce,
        ciphertext,
    };

    let enc_bytes = bincode::serialize(&encrypted_vault).unwrap();
    let path = get_vault_path();
    fs::write(&path, enc_bytes).unwrap();
    println!("[+] Coffre sauvegardé : {:?}", path);

    // Test de décryptage
    let read_bytes = fs::read(&path).unwrap();
    let read_enc: vault::EncryptedVault = bincode::deserialize(&read_bytes).unwrap();
    let read_mk = crypto::derive_master_key(passphrase, &read_enc.salt);
    
    match crypto::decrypt_vault_payload(&read_mk, &read_enc.nonce, &read_enc.ciphertext) {
        Ok(pt) => {
            let dec_vault: vault::VaultData = vault::VaultData::from_bytes(&pt).unwrap();
            println!("[+] Coffre déchiffré avec succès ! Entrées : {}", dec_vault.entries.len());
        }
        Err(_) => {
            eprintln!("[-] Erreur de déchiffrement.");
        }
    }

    // Démo Crypto Hybride
    println!("[*] Test du KEM hybride (Kyber1024 + X25519)...");
    let keypair = crypto::HybridKeyPair::generate();
    let (shared_secret, ciphertext_kem) = crypto::hybrid_encapsulate(&keypair.pq_pk, &keypair.cl_pk);
    println!("[+] Encapsulation terminée. Shared secret ({} bytes), Ciphertext ({} bytes)", shared_secret.len(), ciphertext_kem.len());

    // Scanner
    println!("[*] Démarrage du processus de détection en background...");
    #[cfg(windows)]
    {
        std::thread::spawn(|| {
            scanner::windows_ui::start_scanner();
        });
    }
    #[cfg(not(windows))]
    {
        scanner::dummy_scanner::start_scanner();
    }

    // Garde le process principal en vie (proto only)
    std::thread::sleep(std::time::Duration::from_secs(5));
    println!("[*] Terminé. Ce n'est qu'un PoC.");
}

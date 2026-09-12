//! Chiffrement de fichiers et de dossiers avec la clé du coffre **ouvert**.
//!
//! Format `.kyber` (magic `KYBF`) : `magic[4] ‖ nonce[12] ‖ AES-256-GCM(payload)`.
//! Payload en clair : `[meta_len 4 o LE][JSON FileMeta][données ou archive ZIP]`.
//! Un dossier est zippé en mémoire puis chiffré. Le déchiffrement est borné :
//! protection zip-slip et limite anti zip-bomb (500 Mo décompressés).
//!
//! Distinct du format `KYBP` (par mot de passe) utilisé par le site et
//! l'extension.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::crypto::{decrypt_vault_payload, encrypt_vault_payload, MasterKey};

const MAGIC: &[u8; 4] = b"KYBF";

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct FileMeta {
    pub name: String,
    pub is_folder: bool,
}

#[derive(serde::Serialize, Debug)]
pub struct DecryptResult {
    pub name: String,
    pub path: String,
}

/// Construit le payload plaintext à chiffrer : [meta_len 4B LE][meta JSON][data]
fn build_payload(meta: &FileMeta, data: &[u8]) -> Vec<u8> {
    let meta_json = serde_json::to_vec(meta).unwrap_or_default();
    let meta_len = (meta_json.len() as u32).to_le_bytes();
    let mut payload = Vec::with_capacity(4 + meta_json.len() + data.len());
    payload.extend_from_slice(&meta_len);
    payload.extend_from_slice(&meta_json);
    payload.extend_from_slice(data);
    payload
}

/// Parse le payload déchiffré : extrait (data, FileMeta)
fn parse_payload(plaintext: &[u8]) -> Result<(Vec<u8>, FileMeta), String> {
    if plaintext.len() < 4 {
        return Err("Payload corrompu.".to_string());
    }
    let meta_len =
        u32::from_le_bytes([plaintext[0], plaintext[1], plaintext[2], plaintext[3]]) as usize;
    if plaintext.len() < 4 + meta_len {
        return Err("Métadonnées corrompues.".to_string());
    }
    let meta: FileMeta = serde_json::from_slice(&plaintext[4..4 + meta_len])
        .map_err(|_| "Métadonnées invalides.".to_string())?;
    let data = plaintext[4 + meta_len..].to_vec();
    Ok((data, meta))
}

/// Écrit le fichier .kyber : [MAGIC 4B][nonce 12B][ciphertext]
fn write_kyber_file(
    dest: &Path,
    master_key: &MasterKey,
    meta: &FileMeta,
    data: &[u8],
) -> Result<(), String> {
    let payload = build_payload(meta, data);
    let (ciphertext, nonce) = encrypt_vault_payload(master_key, &payload);

    let mut out = Vec::with_capacity(4 + 12 + ciphertext.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ciphertext);

    fs::write(dest, &out).map_err(|e| format!("Écriture échouée : {}", e))
}

/// Lit et déchiffre un fichier .kyber avec la clé maître courante.
/// Erreur claire si la clé ne correspond pas (GCM tag invalide).
fn read_kyber_file(source: &Path, master_key: &MasterKey) -> Result<(Vec<u8>, FileMeta), String> {
    let raw = fs::read(source).map_err(|e| format!("Lecture échouée : {}", e))?;

    if raw.len() < 4 + 12 {
        return Err("Fichier trop court ou invalide.".to_string());
    }
    if &raw[..4] != MAGIC {
        return Err("Ce fichier n'est pas un fichier Kyber chiffré (.kyber).".to_string());
    }

    let nonce: [u8; 12] = raw[4..16]
        .try_into()
        .map_err(|_| "Nonce invalide".to_string())?;
    let ciphertext = &raw[16..];

    let plaintext = decrypt_vault_payload(master_key, &nonce, ciphertext)
        .map_err(|_| {
            "Déchiffrement impossible — ce fichier a été chiffré avec un coffre différent ou est corrompu.".to_string()
        })?;

    parse_payload(&plaintext)
}

// ─── Dossier → ZIP en mémoire ──────────────────────────────────────────────
fn zip_folder(folder_path: &Path) -> Result<Vec<u8>, String> {
    let mut buf = Vec::new();
    {
        let cursor = std::io::Cursor::new(&mut buf);
        let mut zip = zip::ZipWriter::new(cursor);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);

        let base = folder_path.parent().unwrap_or(folder_path);

        for entry in walkdir::WalkDir::new(folder_path)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            let rel = path
                .strip_prefix(base)
                .map_err(|e| format!("strip_prefix: {}", e))?
                .to_string_lossy()
                .replace('\\', "/");

            if path.is_dir() {
                zip.add_directory(&rel, options)
                    .map_err(|e| e.to_string())?;
            } else {
                zip.start_file(&rel, options).map_err(|e| e.to_string())?;
                let data = fs::read(path).map_err(|e| e.to_string())?;
                zip.write_all(&data).map_err(|e| e.to_string())?;
            }
        }

        zip.finish().map_err(|e| e.to_string())?;
    }
    Ok(buf)
}

// ─── API publique ───────────────────────────────────────────────────────────

/// Chiffre un fichier et le sauvegarde dans dest_path.
pub fn encrypt_file(
    master_key: &MasterKey,
    source_path: &str,
    dest_path: &str,
) -> Result<String, String> {
    let source = Path::new(source_path);
    let dest = Path::new(dest_path);

    let original_name = source
        .file_name()
        .ok_or("Chemin source invalide")?
        .to_string_lossy()
        .to_string();

    const MAX_FILE_SIZE: u64 = 500 * 1024 * 1024; // 500 MB
    let file_size = source.metadata().map(|m| m.len()).unwrap_or(0);
    if file_size > MAX_FILE_SIZE {
        return Err(format!(
            "Fichier trop volumineux ({} MB). Limite : 500 MB.",
            file_size / 1024 / 1024
        ));
    }

    let data = fs::read(source).map_err(|e| format!("Lecture du fichier : {}", e))?;
    let meta = FileMeta {
        name: original_name,
        is_folder: false,
    };

    write_kyber_file(dest, master_key, &meta, &data)?;
    Ok(dest_path.to_string())
}

/// Chiffre un dossier entier (zip → chiffrement) et le sauvegarde dans dest_path.
pub fn encrypt_folder(
    master_key: &MasterKey,
    folder_path: &str,
    dest_path: &str,
) -> Result<String, String> {
    let source = Path::new(folder_path);
    let dest = Path::new(dest_path);

    let folder_name = source
        .file_name()
        .ok_or("Chemin dossier invalide")?
        .to_string_lossy()
        .to_string();

    const MAX_FOLDER_SIZE: u64 = 500 * 1024 * 1024; // 500 MB
    let total_size: u64 = walkdir::WalkDir::new(source)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter_map(|e| e.metadata().ok())
        .filter(|m| m.is_file())
        .map(|m| m.len())
        .sum();
    if total_size > MAX_FOLDER_SIZE {
        return Err(format!(
            "Dossier trop volumineux ({} MB). Limite : 500 MB.",
            total_size / 1024 / 1024
        ));
    }

    let zip_data = zip_folder(source)?;
    let meta = FileMeta {
        name: folder_name,
        is_folder: true,
    };

    write_kyber_file(dest, master_key, &meta, &zip_data)?;
    Ok(dest_path.to_string())
}

/// Déchiffre un fichier .kyber et restaure le fichier original dans dest_dir.
/// Retourne le nom original et le chemin de sortie.
pub fn decrypt_file(
    master_key: &MasterKey,
    source_path: &str,
    dest_dir: &str,
) -> Result<DecryptResult, String> {
    let source = Path::new(source_path);
    let (data, meta) = read_kyber_file(source, master_key)?;

    let dest_base = PathBuf::from(dest_dir);

    if meta.is_folder {
        // Extraire le zip dans dest_dir/nom_dossier/
        let out_dir = dest_base.join(&meta.name);
        fs::create_dir_all(&out_dir).map_err(|e| e.to_string())?;

        // Même limite que la compression (encrypt_folder) : borne dure sur la
        // taille décompressée cumulée, indépendante de ce que l'en-tête zip
        // prétend (une zip-bomb ment sur sa taille annoncée), pour ne pas
        // pouvoir remplir le disque avec un .kyber trafiqué.
        const MAX_DECOMPRESSED_SIZE: u64 = 500 * 1024 * 1024; // 500 MB
        let mut total_extracted: u64 = 0;

        let cursor = std::io::Cursor::new(data);
        let mut archive = zip::ZipArchive::new(cursor).map_err(|e| e.to_string())?;

        for i in 0..archive.len() {
            let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
            // Sanitize: strip any ".." components to prevent path traversal
            let safe_relative: std::path::PathBuf = std::path::Path::new(entry.name())
                .components()
                .filter(|c| matches!(c, std::path::Component::Normal(_)))
                .collect();
            let entry_path = dest_base.join(safe_relative);

            if entry.is_dir() {
                fs::create_dir_all(&entry_path).map_err(|e| e.to_string())?;
            } else {
                if let Some(parent) = entry_path.parent() {
                    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                }
                let mut file = fs::File::create(&entry_path).map_err(|e| e.to_string())?;
                let remaining = MAX_DECOMPRESSED_SIZE.saturating_sub(total_extracted);
                if remaining == 0 {
                    return Err(
                        "Archive trop volumineuse une fois décompressée (limite 500 MB)."
                            .to_string(),
                    );
                }
                use std::io::Read;
                let copied = std::io::copy(&mut entry.by_ref().take(remaining), &mut file)
                    .map_err(|e| e.to_string())?;
                total_extracted += copied;
                // Il reste des octets à lire au-delà de la limite : archive trop grosse.
                let mut probe = [0u8; 1];
                if copied == remaining && entry.read(&mut probe).map(|n| n > 0).unwrap_or(false) {
                    return Err(
                        "Archive trop volumineuse une fois décompressée (limite 500 MB)."
                            .to_string(),
                    );
                }
            }
        }

        Ok(DecryptResult {
            name: meta.name.clone(),
            path: out_dir.to_string_lossy().to_string(),
        })
    } else {
        let out_path = dest_base.join(&meta.name);
        fs::write(&out_path, &data).map_err(|e| format!("Écriture : {}", e))?;

        Ok(DecryptResult {
            name: meta.name.clone(),
            path: out_path.to_string_lossy().to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::derive_master_key;

    fn tmp(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "kyber_filelock_test_{}_{}",
            std::process::id(),
            name
        ));
        p
    }

    #[test]
    fn roundtrip_fichier() {
        let key = derive_master_key("passphrase_test", &[7u8; 16]);
        let src = tmp("src.bin");
        let enc = tmp("src.bin.kyber");
        let out_dir = tmp("out");
        let _ = fs::create_dir_all(&out_dir);
        let contenu = b"Donnees confidentielles \x00\x01\x02 fin.";
        fs::write(&src, contenu).unwrap();

        encrypt_file(&key, src.to_str().unwrap(), enc.to_str().unwrap()).unwrap();

        // Le fichier chiffré commence par le magic KYBF et ne contient pas le clair.
        let raw = fs::read(&enc).unwrap();
        assert_eq!(&raw[..4], MAGIC);
        assert!(!raw.windows(contenu.len()).any(|w| w == contenu));

        let res = decrypt_file(&key, enc.to_str().unwrap(), out_dir.to_str().unwrap()).unwrap();
        let restauré = fs::read(out_dir.join(&res.name)).unwrap();
        assert_eq!(
            restauré, contenu,
            "le déchiffrement doit restituer l'original"
        );

        let _ = fs::remove_file(&src);
        let _ = fs::remove_file(&enc);
        let _ = fs::remove_dir_all(&out_dir);
    }

    #[test]
    fn mauvaise_cle_rejetee() {
        let bonne = derive_master_key("bonne", &[1u8; 16]);
        let mauvaise = derive_master_key("mauvaise", &[1u8; 16]);
        let src = tmp("wk_src.txt");
        let enc = tmp("wk_src.txt.kyber");
        let out_dir = tmp("wk_out");
        let _ = fs::create_dir_all(&out_dir);
        fs::write(&src, b"secret").unwrap();

        encrypt_file(&bonne, src.to_str().unwrap(), enc.to_str().unwrap()).unwrap();
        let res = decrypt_file(&mauvaise, enc.to_str().unwrap(), out_dir.to_str().unwrap());
        assert!(res.is_err(), "une mauvaise clé doit être rejetée (tag GCM)");

        let _ = fs::remove_file(&src);
        let _ = fs::remove_file(&enc);
        let _ = fs::remove_dir_all(&out_dir);
    }

    #[test]
    fn fichier_non_kyber_rejete() {
        let key = derive_master_key("k", &[2u8; 16]);
        let src = tmp("notkyber.dat");
        let out_dir = tmp("nk_out");
        let _ = fs::create_dir_all(&out_dir);
        fs::write(
            &src,
            b"ce n'est pas un fichier kyber, juste du texte assez long",
        )
        .unwrap();

        let res = decrypt_file(&key, src.to_str().unwrap(), out_dir.to_str().unwrap());
        assert!(res.is_err(), "un fichier sans magic KYBF doit être refusé");

        let _ = fs::remove_file(&src);
        let _ = fs::remove_dir_all(&out_dir);
    }
}

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::crypto::{MasterKey, encrypt_vault_payload, decrypt_vault_payload};

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
    let meta_len = u32::from_le_bytes([plaintext[0], plaintext[1], plaintext[2], plaintext[3]]) as usize;
    if plaintext.len() < 4 + meta_len {
        return Err("Métadonnées corrompues.".to_string());
    }
    let meta: FileMeta = serde_json::from_slice(&plaintext[4..4 + meta_len])
        .map_err(|_| "Métadonnées invalides.".to_string())?;
    let data = plaintext[4 + meta_len..].to_vec();
    Ok((data, meta))
}

/// Écrit le fichier .kyber : [MAGIC 4B][nonce 12B][ciphertext]
fn write_kyber_file(dest: &Path, master_key: &MasterKey, meta: &FileMeta, data: &[u8]) -> Result<(), String> {
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

    let nonce: [u8; 12] = raw[4..16].try_into().map_err(|_| "Nonce invalide".to_string())?;
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
            let rel = path.strip_prefix(base)
                .map_err(|e| format!("strip_prefix: {}", e))?
                .to_string_lossy()
                .replace('\\', "/");

            if path.is_dir() {
                zip.add_directory(&rel, options).map_err(|e| e.to_string())?;
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
pub fn encrypt_file(master_key: &MasterKey, source_path: &str, dest_path: &str) -> Result<String, String> {
    let source = Path::new(source_path);
    let dest   = Path::new(dest_path);

    let original_name = source.file_name()
        .ok_or("Chemin source invalide")?
        .to_string_lossy()
        .to_string();

    const MAX_FILE_SIZE: u64 = 500 * 1024 * 1024; // 500 MB
    let file_size = source.metadata().map(|m| m.len()).unwrap_or(0);
    if file_size > MAX_FILE_SIZE {
        return Err(format!("Fichier trop volumineux ({} MB). Limite : 500 MB.", file_size / 1024 / 1024));
    }

    let data = fs::read(source).map_err(|e| format!("Lecture du fichier : {}", e))?;
    let meta = FileMeta { name: original_name, is_folder: false };

    write_kyber_file(dest, master_key, &meta, &data)?;
    Ok(dest_path.to_string())
}

/// Chiffre un dossier entier (zip → chiffrement) et le sauvegarde dans dest_path.
pub fn encrypt_folder(master_key: &MasterKey, folder_path: &str, dest_path: &str) -> Result<String, String> {
    let source = Path::new(folder_path);
    let dest   = Path::new(dest_path);

    let folder_name = source.file_name()
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
        return Err(format!("Dossier trop volumineux ({} MB). Limite : 500 MB.", total_size / 1024 / 1024));
    }

    let zip_data = zip_folder(source)?;
    let meta = FileMeta { name: folder_name, is_folder: true };

    write_kyber_file(dest, master_key, &meta, &zip_data)?;
    Ok(dest_path.to_string())
}

/// Déchiffre un fichier .kyber et restaure le fichier original dans dest_dir.
/// Retourne le nom original et le chemin de sortie.
pub fn decrypt_file(master_key: &MasterKey, source_path: &str, dest_dir: &str) -> Result<DecryptResult, String> {
    let source = Path::new(source_path);
    let (data, meta) = read_kyber_file(source, master_key)?;

    let dest_base = PathBuf::from(dest_dir);

    if meta.is_folder {
        // Extraire le zip dans dest_dir/nom_dossier/
        let out_dir = dest_base.join(&meta.name);
        fs::create_dir_all(&out_dir).map_err(|e| e.to_string())?;

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
                std::io::copy(&mut entry, &mut file).map_err(|e| e.to_string())?;
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

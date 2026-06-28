use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

const PUBLIC_KEY_BYTES: [u8; 32] = [
    79, 20, 240, 118, 49, 159, 159, 39, 145, 191, 249, 177, 130, 147, 12, 174, 219, 254, 116, 195, 1, 33, 216, 9, 125, 236, 161, 105, 20, 244, 237, 171
];

#[derive(Serialize, Deserialize, Debug)]
pub struct LicensePayload {
    pub name: String,
    pub email: String,
    pub tier: String,
}

pub fn get_license_path() -> PathBuf {
    let mut path = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    path.push(".kyber");
    if !path.exists() {
        let _ = fs::create_dir_all(&path);
    }
    path.push("license.key");
    path
}

/// Vérifie une chaîne de licence au format "base64(payload).base64(signature)"
pub fn verify_license_string(license_str: &str) -> Result<LicensePayload, String> {
    let parts: Vec<&str> = license_str.split('.').collect();
    if parts.len() != 2 {
        return Err("Format de licence invalide.".to_string());
    }

    use base64::{Engine as _, engine::general_purpose::STANDARD};

    let payload_bytes = STANDARD.decode(parts[0]).map_err(|_| "Erreur décodage payload")?;
    let sig_bytes = STANDARD.decode(parts[1]).map_err(|_| "Erreur décodage signature")?;

    if sig_bytes.len() != 64 {
        return Err("Taille de signature invalide".to_string());
    }

    let public_key = VerifyingKey::from_bytes(&PUBLIC_KEY_BYTES)
        .map_err(|_| "Erreur clé publique interne".to_string())?;
    
    let signature = Signature::from_slice(&sig_bytes)
        .map_err(|_| "Erreur parsing signature".to_string())?;

    public_key.verify(&payload_bytes, &signature)
        .map_err(|_| "Signature invalide. Licence refusée.".to_string())?;

    let payload: LicensePayload = serde_json::from_slice(&payload_bytes)
        .map_err(|_| "Payload JSON invalide".to_string())?;

    Ok(payload)
}

#[tauri::command]
pub fn check_license() -> Result<LicensePayload, String> {
    let path = get_license_path();
    if !path.exists() {
        return Err("NO_LICENSE".to_string());
    }
    let content = fs::read_to_string(&path).map_err(|_| "Erreur lecture fichier licence")?;
    verify_license_string(content.trim())
}

#[tauri::command]
pub fn activate_license(license_key: String) -> Result<LicensePayload, String> {
    let payload = verify_license_string(&license_key)?;
    let path = get_license_path();
    fs::write(&path, license_key).map_err(|_| "Impossible de sauvegarder la licence")?;
    Ok(payload)
}

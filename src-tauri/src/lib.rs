pub mod crypto;
pub mod scanner;
pub mod vault;
pub mod license;
pub mod filelock;
pub mod session_bridge;

use std::fs;
use std::path::PathBuf;
use tauri::State;
use std::sync::Mutex;
use session_bridge::SessionBridge;
use vault::{VaultData, VaultEntry, EncryptedVault, EncryptedVaultV2, V2_MAGIC};

struct AppState {
    vault_path: Mutex<Option<PathBuf>>,
    vault_data: Mutex<Option<VaultData>>,
    master_key: Mutex<Option<crypto::MasterKey>>,
}

#[tauri::command]
fn check_vault_exists(path: &str) -> bool {
    PathBuf::from(path).exists()
}

/// Verrouille le coffre : purge les données déchiffrées et la clé maître de
/// la mémoire (MasterKey est ZeroizeOnDrop) et coupe la session servie à
/// l'extension navigateur. L'utilisateur devra re-saisir son mot de passe.
#[tauri::command]
fn lock_vault(state: State<'_, AppState>, bridge: State<'_, SessionBridge>) {
    *state.vault_data.lock().unwrap() = None;
    *state.master_key.lock().unwrap() = None;
    *state.vault_path.lock().unwrap() = None;
    bridge.clear_entries();
    log::info!("Coffre verrouillé (mémoire purgée, session extension coupée).");
}

// ── Persistance du dernier chemin de coffre ───────────────────────────────
fn config_path() -> PathBuf {
    let mut p = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    p.push(".kyber");
    let _ = fs::create_dir_all(&p);
    p.push("last_vault.txt");
    p
}

fn save_last_vault_path(path: &PathBuf) {
    let _ = fs::write(config_path(), path.to_string_lossy().as_bytes());
}

// ── Registre des coffres créés (limite freemium) ───────────────────────────
fn vaults_registry_path() -> PathBuf {
    let mut p = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    p.push(".kyber");
    let _ = fs::create_dir_all(&p);
    p.push("vaults.json");
    p
}

fn registered_vaults() -> Vec<String> {
    let path = vaults_registry_path();
    if !path.exists() { return vec![]; }
    fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn register_vault(vault_path: &PathBuf) {
    let mut vaults = registered_vaults();
    let key = vault_path.to_string_lossy().to_string();
    if !vaults.contains(&key) {
        vaults.push(key);
        let _ = fs::write(vaults_registry_path(), serde_json::to_string(&vaults).unwrap());
    }
}

#[tauri::command]
fn get_last_vault_path() -> String {
    fs::read_to_string(config_path()).unwrap_or_default().trim().to_string()
}

#[tauri::command]
fn get_default_vault_path() -> String {
    let mut p = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    p.push("coffre.vault");
    p.to_string_lossy().to_string()
}

#[tauri::command]
fn unlock_vault(state: State<'_, AppState>, bridge: State<'_, SessionBridge>, password: &str, path: &str) -> Result<Vec<VaultEntry>, String> {
    log::info!("Tentative de déverrouillage du coffre à : {}", path);
    let vault_path = PathBuf::from(path);
    
    if vault_path.is_dir() {
        return Err("Le chemin pointe vers un dossier. Vous devez spécifier un nom de fichier (ex: C:\\dossier\\vault.enc)".to_string());
    }
    
    if !vault_path.exists() {
        return Err("NOT_FOUND".to_string());
    }

    let raw = fs::read(&vault_path).map_err(|e| format!("Erreur de lecture du fichier : {}", e))?;

    let (mk, pt) = if raw.starts_with(V2_MAGIC) {
        // Coffre v2 — Argon2id + Kyber1024 + HKDF
        let v2: EncryptedVaultV2 = bincode::deserialize(&raw[3..])
            .map_err(|_| "Le fichier n'est pas un coffre valide ou est corrompu.".to_string())?;
        let seed_key = crypto::derive_seed_key(password, &v2.salt);
        let final_key = crypto::open_kyber_vault_key(&seed_key, &v2.pq_ct, &v2.pq_sk_enc, &v2.pq_sk_nonce)
            .map_err(|_| "Mot de passe incorrect ou coffre corrompu.".to_string())?;
        let pt = crypto::decrypt_vault_payload(&final_key, &v2.nonce, &v2.ciphertext)
            .map_err(|_| "Mot de passe incorrect ou coffre corrompu.".to_string())?;
        (final_key, pt)
    } else {
        // Coffre v1 legacy — Argon2id seul
        let v1: EncryptedVault = bincode::deserialize(&raw)
            .map_err(|_| "Le fichier n'est pas un coffre valide ou est corrompu.".to_string())?;
        let mk = crypto::derive_master_key(password, &v1.salt);
        let pt = crypto::decrypt_vault_payload(&mk, &v1.nonce, &v1.ciphertext)
            .map_err(|_| "Mot de passe incorrect ou coffre corrompu.".to_string())?;
        (mk, pt)
    };

    let dec_vault = VaultData::from_bytes(&pt).map_err(|e| e.to_string())?;
    let entries: Vec<VaultEntry> = dec_vault.entries.values().cloned().collect();

    *state.vault_path.lock().unwrap() = Some(vault_path.clone());
    *state.vault_data.lock().unwrap() = Some(dec_vault);
    *state.master_key.lock().unwrap() = Some(mk);
    save_last_vault_path(&vault_path);
    bridge.set_entries(entries.clone());

    log::info!("Coffre déverrouillé avec succès !");
    Ok(entries)
}

#[tauri::command]
fn init_vault(state: State<'_, AppState>, bridge: State<'_, SessionBridge>, password: &str, path: &str) -> Result<Vec<VaultEntry>, String> {
    use rand_core::{OsRng, RngCore};
    log::info!("Création d'un nouveau coffre à : {}", path);
    let vault_path = PathBuf::from(path);
    
    if vault_path.is_dir() || path.ends_with('/') || path.ends_with('\\') {
        return Err("Vous avez indiqué un dossier. Veuillez ajouter un nom de fichier.".to_string());
    }

    if vault_path.exists() {
        return Err("VAULT_EXISTS".to_string());
    }

    if let Some(parent) = vault_path.parent() {
        if !parent.exists() && parent.to_string_lossy() != "" {
            fs::create_dir_all(parent).map_err(|e| format!("Erreur création dossier: {}", e))?;
        }
    }

    // Freemium : limite à 1 coffre sans licence
    const FREE_VAULT_LIMIT: usize = 1;
    let existing = registered_vaults();
    let already_owns = existing.contains(&vault_path.to_string_lossy().to_string());
    if !already_owns && existing.len() >= FREE_VAULT_LIMIT && license::check_license().is_err() {
        return Err("VAULT_LIMIT_REACHED".to_string());
    }

    let mut salt = [0u8; 16];
    OsRng.fill_bytes(&mut salt);

    // v2 : Argon2id → seed, puis Kyber1024 KEM, puis HKDF combine les deux
    let seed_key = crypto::derive_seed_key(password, &salt);
    let (final_key, pq_ct, pq_sk_enc, pq_sk_nonce) = crypto::create_kyber_vault_key(&seed_key);

    let my_vault = VaultData::new();
    let (ciphertext, nonce) = crypto::encrypt_vault_payload(&final_key, &my_vault.to_bytes());

    let v2 = EncryptedVaultV2 { salt, pq_ct, pq_sk_enc, pq_sk_nonce, nonce, ciphertext };
    let mut out = V2_MAGIC.to_vec();
    out.extend(bincode::serialize(&v2).map_err(|e| format!("Erreur sérialisation: {}", e))?);
    fs::write(&vault_path, out).map_err(|e| format!("Erreur d'écriture : {}", e))?;

    *state.vault_path.lock().unwrap() = Some(vault_path.clone());
    *state.vault_data.lock().unwrap() = Some(my_vault);
    *state.master_key.lock().unwrap() = Some(final_key);
    save_last_vault_path(&vault_path);
    register_vault(&vault_path);
    bridge.set_entries(vec![]);

    log::info!("Nouveau coffre initialisé avec succès.");
    Ok(vec![])
}

#[tauri::command]
fn generate_password() -> String {
    use rand_core::{OsRng, RngCore};
    let chars = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789!@#$%^&*()_+-=[]{}|;:,.<>?";
    let n = chars.len();
    // Rejection sampling: discard bytes in the biased tail to ensure uniform distribution
    let threshold = ((256 - (256 % n)) & 0xFF) as u8;
    let mut result = Vec::with_capacity(42);
    let mut buf = [0u8; 1];
    while result.len() < 42 {
        OsRng.fill_bytes(&mut buf);
        if buf[0] < threshold {
            result.push(chars[(buf[0] as usize) % n] as char);
        }
    }
    result.into_iter().collect()
}

/// Re-chiffre et écrit le coffre sur disque. Préserve le format (v1 ou v2).
fn save_vault(path: &PathBuf, mk: &crypto::MasterKey, vault_data: &VaultData) -> Result<(), String> {
    let (ciphertext, nonce) = crypto::encrypt_vault_payload(mk, &vault_data.to_bytes());
    let raw = fs::read(path).map_err(|e| format!("Lecture: {}", e))?;
    if raw.starts_with(V2_MAGIC) {
        let mut v2: EncryptedVaultV2 = bincode::deserialize(&raw[3..])
            .map_err(|_| "Coffre v2 corrompu.".to_string())?;
        v2.nonce = nonce;
        v2.ciphertext = ciphertext;
        let mut out = V2_MAGIC.to_vec();
        out.extend(bincode::serialize(&v2).map_err(|e| e.to_string())?);
        fs::write(path, out).map_err(|e| format!("Écriture: {}", e))
    } else {
        let mut v1: EncryptedVault = bincode::deserialize(&raw)
            .map_err(|_| "Coffre v1 corrompu.".to_string())?;
        v1.nonce = nonce;
        v1.ciphertext = ciphertext;
        fs::write(path, bincode::serialize(&v1).map_err(|e| e.to_string())?)
            .map_err(|e| format!("Écriture: {}", e))
    }
}

#[tauri::command]
fn add_entry(state: State<'_, AppState>, bridge: State<'_, SessionBridge>, title: &str, username: &str, password: &str, url: &str) -> Result<Vec<VaultEntry>, String> {
    let mut vault_data_lock = state.vault_data.lock().unwrap();
    let mk_lock = state.master_key.lock().unwrap();
    let path_lock = state.vault_path.lock().unwrap();

    let vault_data = vault_data_lock.as_mut().ok_or("Coffre non déverrouillé.")?;
    let mk = mk_lock.as_ref().ok_or("Clé maître non disponible.")?;
    let path = path_lock.as_ref().ok_or("Chemin du coffre inconnu.")?;

    // Freemium : limite à 3 entrées sans licence valide
    const FREE_LIMIT: usize = 10;
    if vault_data.entries.len() >= FREE_LIMIT && license::check_license().is_err() {
        return Err("LIMIT_REACHED".to_string());
    }

    let id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis()
        .to_string();

    let entry = VaultEntry {
        id,
        title: title.to_string(),
        username: username.to_string(),
        password: password.to_string(),
        url: url.to_string(),
        last_modified: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
    };

    vault_data.entries.insert(entry.id.clone(), entry);
    save_vault(path, mk, vault_data)?;
    let entries: Vec<VaultEntry> = vault_data.entries.values().cloned().collect();
    bridge.set_entries(entries.clone());
    Ok(entries)
}

#[tauri::command]
fn delete_entry(state: State<'_, AppState>, bridge: State<'_, SessionBridge>, id: &str) -> Result<Vec<VaultEntry>, String> {
    let mut vault_data_lock = state.vault_data.lock().unwrap();
    let mk_lock = state.master_key.lock().unwrap();
    let path_lock = state.vault_path.lock().unwrap();

    let vault_data = vault_data_lock.as_mut().ok_or("Coffre non déverrouillé.")?;
    let mk = mk_lock.as_ref().ok_or("Clé maître non disponible.")?;
    let path = path_lock.as_ref().ok_or("Chemin du coffre inconnu.")?;

    if vault_data.entries.remove(id).is_none() {
        return Err(format!("Entrée '{}' introuvable.", id));
    }
    save_vault(path, mk, vault_data)?;
    let entries: Vec<VaultEntry> = vault_data.entries.values().cloned().collect();
    bridge.set_entries(entries.clone());
    Ok(entries)
}

#[tauri::command]
fn update_entry(
    state: State<'_, AppState>,
    bridge: State<'_, SessionBridge>,
    id: &str,
    title: &str,
    username: &str,
    password: &str,
    url: &str,
) -> Result<Vec<VaultEntry>, String> {
    let mut vault_data_lock = state.vault_data.lock().unwrap();
    let mk_lock = state.master_key.lock().unwrap();
    let path_lock = state.vault_path.lock().unwrap();

    let vault_data = vault_data_lock.as_mut().ok_or("Coffre non déverrouillé.")?;
    let mk = mk_lock.as_ref().ok_or("Clé maître non disponible.")?;
    let path = path_lock.as_ref().ok_or("Chemin du coffre inconnu.")?;

    let entry = vault_data
        .entries
        .get_mut(id)
        .ok_or_else(|| format!("Entrée '{}' introuvable.", id))?;

    entry.title = title.to_string();
    entry.username = username.to_string();
    entry.password = password.to_string();
    entry.url = url.to_string();
    entry.last_modified = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    save_vault(path, mk, vault_data)?;
    let entries: Vec<VaultEntry> = vault_data.entries.values().cloned().collect();
    bridge.set_entries(entries.clone());
    Ok(entries)
}

// ── Auto-fill natif (enigo) ────────────────────────────────────
#[tauri::command]
fn autofill_password(password: String) -> Result<(), String> {
    use enigo::{Enigo, Keyboard, Settings};
    // Laisse le temps à la fenêtre cible de récupérer le focus
    std::thread::sleep(std::time::Duration::from_millis(450));
    let mut enigo = Enigo::new(&Settings::default())
        .map_err(|e| format!("Enigo init: {}", e))?;
    enigo.text(&password)
        .map_err(|e| format!("Enigo type: {}", e))?;
    Ok(())
}

// ── Clipboard sécurisé avec auto-clear 30s ─────────────────────
#[tauri::command]
fn copy_secure(text: String) -> Result<(), String> {
    use arboard::Clipboard;
    let mut clip = Clipboard::new()
        .map_err(|e| format!("Clipboard: {}", e))?;
    clip.set_text(text)
        .map_err(|e| format!("Set text: {}", e))?;
    // Thread qui efface après 30s
    std::thread::spawn(|| {
        std::thread::sleep(std::time::Duration::from_secs(30));
        if let Ok(mut c) = arboard::Clipboard::new() {
            let _ = c.set_text("");
        }
    });
    Ok(())
}

// ── Générateur avec options ────────────────────────────────────
#[tauri::command]
fn generate_password_options(
    length: usize,
    upper: bool,
    lower: bool,
    digits: bool,
    symbols: bool,
) -> Result<String, String> {
    use rand_core::{OsRng, RngCore};
    let mut charset = Vec::new();
    if upper   { charset.extend_from_slice(b"ABCDEFGHIJKLMNOPQRSTUVWXYZ"); }
    if lower   { charset.extend_from_slice(b"abcdefghijklmnopqrstuvwxyz"); }
    if digits  { charset.extend_from_slice(b"0123456789"); }
    if symbols { charset.extend_from_slice(b"!@#$%^&*()_+-=[]{}|;:,.<>?"); }
    if charset.is_empty() {
        return Err("Sélectionnez au moins un type de caractère.".into());
    }
    let capped = length.min(128);
    let n = charset.len();
    let threshold = ((256 - (256 % n)) & 0xFF) as u8;
    let mut result = Vec::with_capacity(capped);
    let mut buf = [0u8; 1];
    while result.len() < capped {
        OsRng.fill_bytes(&mut buf);
        if buf[0] < threshold {
            result.push(charset[(buf[0] as usize) % n] as char);
        }
    }
    Ok(result.into_iter().collect())
}

// ── Santé du coffre ────────────────────────────────────────────
#[derive(serde::Serialize, Clone)]
struct HealthReport {
    weak: Vec<VaultEntry>,
    duplicates: Vec<VaultEntry>,
    old: Vec<VaultEntry>,
}

fn password_score(pwd: &str) -> u8 {
    let mut s: u32 = 0;
    let l = pwd.len();
    s += match l { 0..=7=>0, 8..=11=>15, 12..=15=>25, 16..=19=>35, _=>40 };
    if pwd.chars().any(|c| c.is_lowercase())  { s += 10; }
    if pwd.chars().any(|c| c.is_uppercase())  { s += 15; }
    if pwd.chars().any(|c| c.is_numeric())    { s += 15; }
    if pwd.chars().any(|c| !c.is_alphanumeric()) { s += 20; }
    s.min(100) as u8
}

#[tauri::command]
fn get_vault_health(state: State<'_, AppState>) -> Result<HealthReport, String> {
    use std::collections::HashMap;
    let lock = state.vault_data.lock().unwrap();
    let vault = lock.as_ref().ok_or("Coffre non déverrouillé.")?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    let ninety_days: u64 = 90 * 24 * 3600;

    let mut weak = vec![];
    let mut old  = vec![];
    let mut pw_map: HashMap<String, Vec<VaultEntry>> = HashMap::new();

    for entry in vault.entries.values() {
        if password_score(&entry.password) < 50 { weak.push(entry.clone()); }
        if now.saturating_sub(entry.last_modified) > ninety_days { old.push(entry.clone()); }
        pw_map.entry(entry.password.clone()).or_default().push(entry.clone());
    }
    let duplicates: Vec<VaultEntry> = pw_map.into_values()
        .filter(|v| v.len() > 1)
        .flatten()
        .collect();

    Ok(HealthReport { weak, duplicates, old })
}

// ── Rappel de rotation des mots de passe (réglage par coffre, annulable à tout moment) ──
fn rotation_settings_path() -> PathBuf {
    let mut p = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    p.push(".kyber");
    let _ = fs::create_dir_all(&p);
    p.push("rotation_settings.json");
    p
}

fn read_rotation_settings() -> std::collections::HashMap<String, bool> {
    let path = rotation_settings_path();
    if !path.exists() { return Default::default(); }
    fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn write_rotation_settings(map: &std::collections::HashMap<String, bool>) {
    if let Ok(s) = serde_json::to_string(map) {
        let _ = fs::write(rotation_settings_path(), s);
    }
}

#[tauri::command]
fn get_rotation_setting(path: &str) -> bool {
    read_rotation_settings().get(path).copied().unwrap_or(false)
}

#[tauri::command]
fn set_rotation_setting(path: &str, enabled: bool) {
    let mut map = read_rotation_settings();
    map.insert(path.to_string(), enabled);
    write_rotation_settings(&map);
}

/// Entrées dont le mot de passe n'a pas été changé depuis 30 jours, uniquement
/// si le rappel est activé pour CE coffre — stocké hors du fichier .vault
/// (registre séparé) pour ne rien changer au format binaire des coffres existants.
#[tauri::command]
fn get_rotation_due(state: State<'_, AppState>) -> Result<Vec<VaultEntry>, String> {
    let vault_lock = state.vault_data.lock().unwrap();
    let path_lock = state.vault_path.lock().unwrap();
    let vault = vault_lock.as_ref().ok_or("Coffre non déverrouillé.")?;
    let path = path_lock.as_ref().ok_or("Coffre non déverrouillé.")?;

    let enabled = read_rotation_settings()
        .get(&path.to_string_lossy().to_string())
        .copied()
        .unwrap_or(false);
    if !enabled {
        return Ok(vec![]);
    }

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    const THIRTY_DAYS: u64 = 30 * 24 * 3600;
    Ok(vault.entries.values()
        .filter(|e| now.saturating_sub(e.last_modified) > THIRTY_DAYS)
        .cloned()
        .collect())
}

// ── Import CSV (Bitwarden / 1Password / générique) ─────────────
#[tauri::command]
fn import_csv(
    state: State<'_, AppState>,
    bridge: State<'_, SessionBridge>,
    csv_content: String,
    format: String,
) -> Result<Vec<VaultEntry>, String> {
    use std::io::Cursor;
    use rand_core::{OsRng, RngCore};

    const MAX_CSV_BYTES: usize = 10 * 1024 * 1024; // 10 MB
    if csv_content.len() > MAX_CSV_BYTES {
        return Err("Fichier CSV trop volumineux (limite : 10 MB).".to_string());
    }

    let mut reader = csv::ReaderBuilder::new()
        .flexible(true)
        .from_reader(Cursor::new(csv_content.as_bytes()));

    let headers = reader.headers()
        .map_err(|e| format!("En-têtes CSV: {}", e))?
        .clone();

    // Trouve l'index d'une colonne par nom (insensible à la casse)
    let col = |names: &[&str]| -> Option<usize> {
        names.iter().find_map(|&n| {
            headers.iter().position(|h| h.to_lowercase() == n.to_lowercase())
        })
    };

    let (i_title, i_user, i_pass, i_url) = match format.as_str() {
        "bitwarden" => (
            col(&["name"]).ok_or("Colonne 'name' manquante")?,
            col(&["login_username"]).unwrap_or(usize::MAX),
            col(&["login_password"]).unwrap_or(usize::MAX),
            col(&["login_uri"]).unwrap_or(usize::MAX),
        ),
        "onepassword" => (
            col(&["title"]).ok_or("Colonne 'Title' manquante")?,
            col(&["username"]).unwrap_or(usize::MAX),
            col(&["password"]).unwrap_or(usize::MAX),
            col(&["website", "url"]).unwrap_or(usize::MAX),
        ),
        _ => (
            // Générique : tente les noms communs
            col(&["name","title","nom"]).ok_or("Colonne titre introuvable")?,
            col(&["username","login_username","user","email"]).unwrap_or(usize::MAX),
            col(&["password","login_password","pass","mot_de_passe"]).unwrap_or(usize::MAX),
            col(&["url","login_uri","website","uri"]).unwrap_or(usize::MAX),
        ),
    };

    let get = |rec: &csv::StringRecord, i: usize| -> String {
        if i == usize::MAX { String::new() } else { rec.get(i).unwrap_or("").to_string() }
    };

    let mut vault_lock = state.vault_data.lock().unwrap();
    let mk_lock    = state.master_key.lock().unwrap();
    let path_lock  = state.vault_path.lock().unwrap();
    let vault = vault_lock.as_mut().ok_or("Coffre non déverrouillé.")?;
    let mk    = mk_lock.as_ref().ok_or("Clé maître indisponible.")?;
    let path  = path_lock.as_ref().ok_or("Chemin du coffre inconnu.")?;

    let is_licensed = license::check_license().is_ok();
    const FREE_LIMIT: usize = 10;

    let mut count = 0usize;
    for result in reader.records() {
        let rec = result.map_err(|e| format!("Ligne CSV: {}", e))?;
        let title = get(&rec, i_title);
        let pass  = get(&rec, i_pass);
        if title.is_empty() && pass.is_empty() { continue; }

        if !is_licensed && vault.entries.len() >= FREE_LIMIT {
            return Err("LIMIT_REACHED".to_string());
        }

        let id = {
            let mut b = [0u8; 8];
            OsRng.fill_bytes(&mut b);
            hex::encode(b)
        };
        vault.entries.insert(id.clone(), VaultEntry {
            id,
            title,
            username: get(&rec, i_user),
            password: pass,
            url: get(&rec, i_url),
            last_modified: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH).unwrap().as_secs(),
        });
        count += 1;
    }

    save_vault(path, mk, vault)?;

    log::info!("[IMPORT] {} entrées importées ({})", count, format);
    let entries: Vec<VaultEntry> = vault.entries.values().cloned().collect();
    bridge.set_entries(entries.clone());
    Ok(entries)
}


// ── Export CSV (Pro uniquement) ────────────────────────────────────────────
#[tauri::command]
fn export_csv(state: State<'_, AppState>) -> Result<String, String> {
    if license::check_license().is_err() {
        return Err("PRO_REQUIRED".to_string());
    }
    let lock = state.vault_data.lock().unwrap();
    let vault = lock.as_ref().ok_or("Coffre non déverrouillé.")?;

    let mut output = String::from("name,username,password,login_uri,last_modified\n");
    for entry in vault.entries.values() {
        let escape = |s: &str| format!("\"{}\"", s.replace('"', "\"\""));
        output.push_str(&format!(
            "{},{},{},{},{}\n",
            escape(&entry.title),
            escape(&entry.username),
            escape(&entry.password),
            escape(&entry.url),
            entry.last_modified,
        ));
    }
    Ok(output)
}

// ── Coffre de Fichiers — Chiffrement/Déchiffrement ────────────────────────────
#[tauri::command]
fn encrypt_file_cmd(
    state: State<'_, AppState>,
    source_path: String,
    dest_path: String,
) -> Result<String, String> {
    let mk_lock = state.master_key.lock().unwrap();
    let mk = mk_lock.as_ref().ok_or("Coffre non déverrouillé. Ouvrez d'abord votre coffre.")?;
    filelock::encrypt_file(mk, &source_path, &dest_path)
}

#[tauri::command]
fn encrypt_folder_cmd(
    state: State<'_, AppState>,
    folder_path: String,
    dest_path: String,
) -> Result<String, String> {
    let mk_lock = state.master_key.lock().unwrap();
    let mk = mk_lock.as_ref().ok_or("Coffre non déverrouillé. Ouvrez d'abord votre coffre.")?;
    filelock::encrypt_folder(mk, &folder_path, &dest_path)
}

#[tauri::command]
fn decrypt_file_cmd(
    state: State<'_, AppState>,
    source_path: String,
    dest_dir: String,
) -> Result<filelock::DecryptResult, String> {
    let mk_lock = state.master_key.lock().unwrap();
    let mk = mk_lock.as_ref().ok_or("Coffre non déverrouillé. Ouvrez d'abord votre coffre.")?;
    filelock::decrypt_file(mk, &source_path, &dest_dir)
}

// ── Migration v1 → v2 (Kyber1024) ────────────────────────────────────────────────
#[tauri::command]
fn is_vault_v1(path: &str) -> bool {
    let p = PathBuf::from(path);
    if !p.exists() { return false; }
    let raw = fs::read(p).unwrap_or_default();
    !raw.starts_with(V2_MAGIC)
}

#[tauri::command]
fn migrate_to_v2(state: State<'_, AppState>, password: &str) -> Result<(), String> {
    use rand_core::{OsRng, RngCore};

    let vault_data_lock = state.vault_data.lock().unwrap();
    let path_lock  = state.vault_path.lock().unwrap();
    let mut mk_lock = state.master_key.lock().unwrap();

    let vault_data = vault_data_lock.as_ref().ok_or("Coffre non déverrouillé.")?;
    let path = path_lock.as_ref().ok_or("Chemin du coffre inconnu.")?;

    let raw = fs::read(path).map_err(|e| format!("Lecture: {}", e))?;
    if raw.starts_with(V2_MAGIC) {
        return Err("Ce coffre est déjà en format v2 (Kyber1024).".to_string());
    }

    // Vérifie le mot de passe sur le fichier v1 avant de toucher quoi que ce soit
    let v1: EncryptedVault = bincode::deserialize(&raw)
        .map_err(|_| "Fichier coffre invalide.".to_string())?;
    let check_key = crypto::derive_master_key(password, &v1.salt);
    crypto::decrypt_vault_payload(&check_key, &v1.nonce, &v1.ciphertext)
        .map_err(|_| "Mot de passe incorrect.".to_string())?;

    // Crée le format v2 à partir des données déjà en mémoire
    let mut salt = [0u8; 16];
    OsRng.fill_bytes(&mut salt);

    let seed_key = crypto::derive_seed_key(password, &salt);
    let (final_key, pq_ct, pq_sk_enc, pq_sk_nonce) = crypto::create_kyber_vault_key(&seed_key);
    let (ciphertext, nonce) = crypto::encrypt_vault_payload(&final_key, &vault_data.to_bytes());

    let v2 = EncryptedVaultV2 { salt, pq_ct, pq_sk_enc, pq_sk_nonce, nonce, ciphertext };
    let mut out = V2_MAGIC.to_vec();
    out.extend(bincode::serialize(&v2).map_err(|e| e.to_string())?);
    fs::write(path, out).map_err(|e| format!("Écriture: {}", e))?;

    *mk_lock = Some(final_key);

    log::info!("Migration v1→v2 réussie : {}", path.display());
    Ok(())
}

// ── Ouvre la page de mise à niveau dans le navigateur système ─────────────────────
#[tauri::command]
fn open_upgrade_url() {
    let url = "https://kyber-security.fr/#pricing";
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("cmd").args(["/c", "start", "", url]).spawn();
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(url).spawn();
    #[cfg(target_os = "linux")]
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState {
            vault_path: Mutex::new(None),
            vault_data: Mutex::new(None),
            master_key: Mutex::new(None),
        })
        .manage(session_bridge::start())
        .invoke_handler(tauri::generate_handler![
            check_vault_exists,
            lock_vault,
            unlock_vault,
            init_vault,
            generate_password,
            generate_password_options,
            add_entry,
            delete_entry,
            update_entry,
            autofill_password,
            copy_secure,
            get_vault_health,
            get_rotation_setting,
            set_rotation_setting,
            get_rotation_due,
            import_csv,
            license::check_license,
            license::activate_license,
            export_csv,
            open_upgrade_url,
            get_last_vault_path,
            get_default_vault_path,
            encrypt_file_cmd,
            encrypt_folder_cmd,
            decrypt_file_cmd,
            is_vault_v1,
            migrate_to_v2
        ])
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }

            let app_handle = app.handle().clone();
            scanner::start_scanner(app_handle);

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

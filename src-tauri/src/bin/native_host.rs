// Hôte de native messaging pour l'extension navigateur Kyber (beta).
// Protocole standard Chrome/Firefox : chaque message est préfixé par sa
// longueur en u32 little-endian, corps en JSON UTF-8, sur stdin/stdout.
//
// L'extension ne fait AUCUN crypto elle-même dans ce mode : ce process
// réutilise directement le moteur de `app_lib` (le même code que l'app
// desktop) pour déverrouiller le vrai fichier .vault de l'utilisateur.

use app_lib::{crypto, license, vault};
use serde::Deserialize;
use serde_json::{json, Value};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::time::Duration;
use vault::{EncryptedVault, EncryptedVaultV2, VaultEntry, V2_MAGIC};

#[derive(Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
enum Request {
    Ping,
    CheckLicense,
    GetLastVaultPath,
    Unlock { path: String, password: String },
    /// Tente de récupérer les entrées d'un coffre déjà déverrouillé dans une
    /// instance de l'app en cours d'exécution (pont local `session_bridge`),
    /// pour éviter à l'extension de redemander le mot de passe maître.
    TryLiveSession,
}

// Chrome/Firefox limitent déjà les messages natifs à 1 Mo côté navigateur ;
// cette borne est une seconde ligne de défense côté process (au cas où ce
// binaire serait un jour invoqué autrement que par un navigateur de confiance).
const MAX_MESSAGE_BYTES: usize = 1024 * 1024;

fn read_message() -> io::Result<Option<Value>> {
    let mut len_buf = [0u8; 4];
    match io::stdin().read_exact(&mut len_buf) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e),
    }
    let len = u32::from_le_bytes(len_buf) as usize;
    if len > MAX_MESSAGE_BYTES {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "message natif trop volumineux"));
    }
    let mut buf = vec![0u8; len];
    io::stdin().read_exact(&mut buf)?;
    let value: Value = serde_json::from_slice(&buf).unwrap_or(Value::Null);
    Ok(Some(value))
}

fn write_message(value: &Value) -> io::Result<()> {
    let body = serde_json::to_vec(value)?;
    let len = (body.len() as u32).to_le_bytes();
    let mut stdout = io::stdout();
    stdout.write_all(&len)?;
    stdout.write_all(&body)?;
    stdout.flush()
}

fn ok(data: Value) -> Value {
    json!({ "ok": true, "data": data })
}

fn err(message: impl Into<String>) -> Value {
    json!({ "ok": false, "error": message.into() })
}

fn get_last_vault_path() -> String {
    let mut p = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    p.push(".kyber");
    p.push("last_vault.txt");
    std::fs::read_to_string(p).unwrap_or_default().trim().to_string()
}

fn unlock(path: &str, password: &str) -> Result<Vec<VaultEntry>, String> {
    let vault_path = PathBuf::from(path);
    if !vault_path.exists() {
        return Err("NOT_FOUND".to_string());
    }
    let raw = std::fs::read(&vault_path).map_err(|e| format!("Erreur de lecture : {}", e))?;

    let pt = if raw.starts_with(V2_MAGIC) {
        let v2: EncryptedVaultV2 = bincode::deserialize(&raw[3..])
            .map_err(|_| "Coffre invalide ou corrompu.".to_string())?;
        let seed_key = crypto::derive_seed_key(password, &v2.salt);
        let final_key = crypto::open_kyber_vault_key(&seed_key, &v2.pq_ct, &v2.pq_sk_enc, &v2.pq_sk_nonce)
            .map_err(|_| "Mot de passe incorrect.".to_string())?;
        crypto::decrypt_vault_payload(&final_key, &v2.nonce, &v2.ciphertext)
            .map_err(|_| "Mot de passe incorrect.".to_string())?
    } else {
        let v1: EncryptedVault = bincode::deserialize(&raw)
            .map_err(|_| "Coffre invalide ou corrompu.".to_string())?;
        let mk = crypto::derive_master_key(password, &v1.salt);
        crypto::decrypt_vault_payload(&mk, &v1.nonce, &v1.ciphertext)
            .map_err(|_| "Mot de passe incorrect.".to_string())?
    };

    let dec_vault = vault::VaultData::from_bytes(&pt).map_err(|e| e.to_string())?;
    Ok(dec_vault.entries.into_values().collect())
}

// ── Pont de session live (voir src/session_bridge.rs côté app) ─────────────
// L'app, quand un coffre est ouvert, écoute en local sur 127.0.0.1 et écrit
// le port + un jeton dans `~/.kyber/session.json`. Si on arrive à s'y
// connecter et que le coffre est bien déverrouillé côté app, on récupère les
// entrées sans jamais demander le mot de passe ici.
#[derive(Deserialize)]
struct SessionInfo {
    port: u16,
    token: String,
}

fn session_file_path() -> PathBuf {
    let mut p = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    p.push(".kyber");
    p.push("session.json");
    p
}

fn try_live_session() -> Value {
    let raw = match std::fs::read_to_string(session_file_path()) {
        Ok(s) => s,
        Err(_) => return err("NO_SESSION"),
    };
    let info: SessionInfo = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(_) => return err("NO_SESSION"),
    };

    let addr: SocketAddr = match format!("127.0.0.1:{}", info.port).parse() {
        Ok(a) => a,
        Err(_) => return err("NO_SESSION"),
    };
    let mut stream = match TcpStream::connect_timeout(&addr, Duration::from_millis(400)) {
        Ok(s) => s,
        // L'app n'est pas lancée, ou le pont n'a pas pu démarrer sur cette
        // instance : ce n'est pas une erreur, juste "pas de session live".
        Err(_) => return err("NO_SESSION"),
    };
    let _ = stream.set_read_timeout(Some(Duration::from_millis(800)));
    let _ = stream.set_write_timeout(Some(Duration::from_millis(500)));

    let req = json!({ "cmd": "get_entries", "token": info.token });
    let mut body = match serde_json::to_vec(&req) {
        Ok(b) => b,
        Err(_) => return err("NO_SESSION"),
    };
    body.push(b'\n');
    if stream.write_all(&body).is_err() {
        return err("NO_SESSION");
    }

    // Même borne défensive que côté serveur (session_bridge.rs) : une
    // réponse légitime tient sur quelques Ko, pas la peine de laisser un
    // pair local (même de confiance) forcer une lecture non bornée.
    let mut reader = BufReader::new(stream.take(64 * 1024));
    let mut line = String::new();
    if reader.read_line(&mut line).unwrap_or(0) == 0 {
        return err("NO_SESSION");
    }
    serde_json::from_str(line.trim()).unwrap_or_else(|_| err("NO_SESSION"))
}

fn handle(req: Request) -> Value {
    match req {
        Request::Ping => {
            let license = license::check_license();
            ok(json!({
                "installed": true,
                "version": env!("CARGO_PKG_VERSION"),
                "licensed": license.is_ok(),
                "tier": license.ok().map(|p| p.tier),
            }))
        }
        Request::CheckLicense => match license::check_license() {
            Ok(payload) => ok(json!({ "name": payload.name, "email": payload.email, "tier": payload.tier })),
            Err(e) => err(e),
        },
        Request::GetLastVaultPath => ok(json!({ "path": get_last_vault_path() })),
        Request::Unlock { path, password } => match unlock(&path, &password) {
            Ok(entries) => ok(json!({ "entries": entries })),
            Err(e) => err(e),
        },
        Request::TryLiveSession => try_live_session(),
    }
}

fn main() {
    // `chrome.runtime.sendNativeMessage` relance un process natif neuf à
    // chaque appel et ferme le pipe après une seule réponse (contrairement à
    // `connectNative`, qui garderait un port ouvert) : ce process ne traite
    // donc jamais qu'un seul message avant de sortir. Aucun état de session
    // n'est conservé ici — l'extension recompose son propre état côté
    // navigateur (chrome.storage.session) après chaque `unlock`.
    match read_message() {
        Ok(Some(value)) => {
            let response = match serde_json::from_value::<Request>(value) {
                Ok(req) => handle(req),
                Err(e) => err(format!("Requête invalide : {}", e)),
            };
            let _ = write_message(&response);
        }
        Ok(None) | Err(_) => {}
    }
}

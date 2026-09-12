//! Pont de session local pour l'extension navigateur.
//!
//! Quand l'app tourne et qu'un coffre est déverrouillé, l'extension peut
//! récupérer les entrées directement, SANS redemander le mot de passe maître,
//! via une petite socket loopback protégée par un jeton aléatoire. Ça évite à
//! l'utilisateur de retaper son mot de passe dans le navigateur alors que le
//! coffre est déjà ouvert dans l'app de bureau.
//!
//! Modèle de confiance : identique aux autres fichiers sous `~/.kyber`
//! (`last_vault.txt`, `vaults.json`) — lisible uniquement par le compte
//! utilisateur courant. La socket n'écoute JAMAIS que sur 127.0.0.1
//! (jamais exposée au réseau) et exige le jeton lu dans `session.json` pour
//! toute donnée sensible (`get_entries`) ; `ping` seul (sans jeton) ne révèle
//! que "l'app tourne", une info de toute façon observable autrement (process,
//! registre du native host).
//
// Ce module ne fait AUCUNE hypothèse sur l'ordre de démarrage : si le port
// est déjà pris (ex : une deuxième instance de Kyber), on abandonne
// simplement — l'extension retombe sur la saisie manuelle du mot de passe,
// rien n'est cassé.

use crate::vault::VaultEntry;
use rand_core::{OsRng, RngCore};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

const PORT: u16 = 47732;

// Une requête légitime ({"cmd":"get_entries","token":"<64 hex>"}) tient sur
// une centaine d'octets ; cette borne évite qu'un process local puisse forcer
// une allocation mémoire non bornée en streamant une "ligne" sans jamais
// envoyer de \n (déni de service, atteignable avant même la vérification du
// jeton puisque `ping` n'en demande pas).
const MAX_REQUEST_BYTES: u64 = 8 * 1024;

/// Comparaison en temps constant (évite une fuite de timing sur le jeton,
/// même si un attaquant avec exécution de code locale peut de toute façon
/// lire `session.json` directement — défense en profondeur).
fn tokens_match(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

pub struct SessionBridgeInner {
    token: String,
    entries: Mutex<Option<Vec<VaultEntry>>>,
}

pub type SessionBridge = Arc<SessionBridgeInner>;

impl SessionBridgeInner {
    /// Appelé par les commandes Tauri à chaque fois que le contenu du coffre
    /// change (déverrouillage, ajout, suppression, édition, import) pour que
    /// l'extension récupère toujours des données à jour au prochain appel.
    pub fn set_entries(&self, entries: Vec<VaultEntry>) {
        *self.entries.lock().unwrap() = Some(entries);
    }

    /// Appelé au verrouillage du coffre : l'extension recevra `LOCKED` à sa
    /// prochaine requête, exactement comme si le coffre n'avait jamais été
    /// ouvert dans cette instance.
    pub fn clear_entries(&self) {
        *self.entries.lock().unwrap() = None;
    }
}

fn session_file() -> PathBuf {
    let mut p = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    p.push(".kyber");
    let _ = std::fs::create_dir_all(&p);
    p.push("session.json");
    p
}

fn new_token() -> String {
    let mut b = [0u8; 32];
    OsRng.fill_bytes(&mut b);
    hex::encode(b)
}

fn write_response(stream: &mut TcpStream, value: &Value) -> std::io::Result<()> {
    let mut body = serde_json::to_vec(value)?;
    body.push(b'\n');
    stream.write_all(&body)
}

fn handle_client(mut stream: TcpStream, bridge: &SessionBridge) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));

    let mut reader = match stream.try_clone() {
        Ok(s) => BufReader::new(s.take(MAX_REQUEST_BYTES)),
        Err(_) => return,
    };
    let mut line = String::new();
    if reader.read_line(&mut line).unwrap_or(0) == 0 {
        return;
    }

    let req: Value = match serde_json::from_str(line.trim()) {
        Ok(v) => v,
        Err(_) => {
            let _ = write_response(&mut stream, &json!({ "ok": false, "error": "BAD_REQUEST" }));
            return;
        }
    };

    let cmd = req.get("cmd").and_then(|v| v.as_str()).unwrap_or("");
    let resp = match cmd {
        "ping" => json!({ "ok": true, "data": { "running": true } }),
        "get_entries" => {
            let token_ok = req
                .get("token")
                .and_then(|v| v.as_str())
                .map(|t| tokens_match(t, &bridge.token))
                .unwrap_or(false);
            if !token_ok {
                json!({ "ok": false, "error": "UNAUTHORIZED" })
            } else {
                match bridge.entries.lock().unwrap().clone() {
                    Some(entries) => json!({ "ok": true, "data": { "entries": entries } }),
                    None => json!({ "ok": false, "error": "LOCKED" }),
                }
            }
        }
        _ => json!({ "ok": false, "error": "UNKNOWN_COMMAND" }),
    };
    let _ = write_response(&mut stream, &resp);
}

fn spawn_listener(listener: TcpListener, bridge: SessionBridge) {
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let b = bridge.clone();
            std::thread::spawn(move || handle_client(stream, &b));
        }
    });
}

/// Démarre le pont en tâche de fond et renvoie le handle partagé à `.manage()`
/// pour que les commandes Tauri puissent mettre à jour le cache d'entrées.
pub fn start() -> SessionBridge {
    let bridge: SessionBridge = Arc::new(SessionBridgeInner {
        token: new_token(),
        entries: Mutex::new(None),
    });

    // Un `session.json` laissé par un lancement précédent (crash, arrêt
    // brutal) ne doit jamais survivre au-delà de cette tentative de démarrage :
    // s'il reste en place alors qu'un autre process a entre-temps pris le
    // port, l'extension pourrait s'y connecter en pensant parler à l'app.
    let _ = std::fs::remove_file(session_file());

    match TcpListener::bind(("127.0.0.1", PORT)) {
        Ok(listener) => {
            let payload = json!({ "port": PORT, "token": bridge.token });
            let path = session_file();
            if let Err(e) = std::fs::write(&path, payload.to_string()) {
                log::warn!("[session_bridge] impossible d'écrire session.json : {}", e);
            }
            // session.json contient le jeton d'accès au coffre déverrouillé :
            // sur Linux/macOS, le restreindre au compte courant (Windows
            // hérite déjà des ACL du dossier utilisateur).
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
            }

            spawn_listener(listener, bridge.clone());
            log::info!(
                "[session_bridge] pont extension démarré sur 127.0.0.1:{}",
                PORT
            );
        }
        Err(e) => {
            // Le port est pris par un autre process (autre instance de Kyber,
            // ou pire, un process tiers) : pas de session.json valide pour
            // cette instance, l'extension retombe proprement sur la saisie
            // manuelle du mot de passe plutôt que de risquer de parler à un
            // process qui n'est pas l'app.
            log::warn!(
                "[session_bridge] port {} indisponible ({}) — pont désactivé pour cette instance (une autre s'exécute probablement déjà)",
                PORT,
                e
            );
        }
    }

    bridge
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Démarre un pont sur un port éphémère (sans toucher à ~/.kyber) et
    /// renvoie le handle + l'adresse à contacter.
    fn test_bridge() -> (SessionBridge, std::net::SocketAddr) {
        let bridge: SessionBridge = Arc::new(SessionBridgeInner {
            token: new_token(),
            entries: Mutex::new(None),
        });
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind éphémère");
        let addr = listener.local_addr().unwrap();
        spawn_listener(listener, bridge.clone());
        (bridge, addr)
    }

    fn roundtrip(addr: std::net::SocketAddr, req: Value) -> Value {
        let mut stream = TcpStream::connect(addr).expect("connexion au pont");
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut body = serde_json::to_vec(&req).unwrap();
        body.push(b'\n');
        stream.write_all(&body).unwrap();
        let mut line = String::new();
        BufReader::new(stream).read_line(&mut line).unwrap();
        serde_json::from_str(line.trim()).unwrap()
    }

    fn entry(id: &str) -> VaultEntry {
        VaultEntry {
            id: id.into(),
            title: "Test".into(),
            username: "user".into(),
            password: "pass".into(),
            url: "https://example.com".into(),
            last_modified: 0,
        }
    }

    #[test]
    fn ping_sans_jeton_ok() {
        let (_bridge, addr) = test_bridge();
        let resp = roundtrip(addr, json!({ "cmd": "ping" }));
        assert_eq!(resp["ok"], true);
        assert_eq!(resp["data"]["running"], true);
    }

    #[test]
    fn get_entries_verrouille_puis_deverrouille_puis_reverrouille() {
        let (bridge, addr) = test_bridge();
        let req = json!({ "cmd": "get_entries", "token": bridge.token });

        // Coffre pas encore ouvert dans l'app → LOCKED
        assert_eq!(roundtrip(addr, req.clone())["error"], "LOCKED");

        // Déverrouillage côté app → entrées servies
        bridge.set_entries(vec![entry("a1")]);
        let resp = roundtrip(addr, req.clone());
        assert_eq!(resp["ok"], true);
        assert_eq!(resp["data"]["entries"][0]["id"], "a1");

        // lock_vault côté app → l'extension doit revoir LOCKED
        bridge.clear_entries();
        assert_eq!(roundtrip(addr, req)["error"], "LOCKED");
    }

    #[test]
    fn get_entries_sans_ou_avec_mauvais_jeton_refuse() {
        let (bridge, addr) = test_bridge();
        bridge.set_entries(vec![entry("a1")]);

        let sans = roundtrip(addr, json!({ "cmd": "get_entries" }));
        assert_eq!(sans["error"], "UNAUTHORIZED");

        let mauvais = roundtrip(addr, json!({ "cmd": "get_entries", "token": "deadbeef" }));
        assert_eq!(mauvais["error"], "UNAUTHORIZED");
    }

    #[test]
    fn commande_inconnue_et_json_invalide() {
        let (_bridge, addr) = test_bridge();
        assert_eq!(
            roundtrip(addr, json!({ "cmd": "nope" }))["error"],
            "UNKNOWN_COMMAND"
        );

        let mut stream = TcpStream::connect(addr).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        stream.write_all(b"pas du json\n").unwrap();
        let mut line = String::new();
        BufReader::new(stream).read_line(&mut line).unwrap();
        let resp: Value = serde_json::from_str(line.trim()).unwrap();
        assert_eq!(resp["error"], "BAD_REQUEST");
    }
}

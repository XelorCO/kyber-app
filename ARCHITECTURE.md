# Architecture de Kyber

Kyber est une application [Tauri 2](https://tauri.app) : un backend Rust
(`src-tauri/`) et un frontend web servi dans une WebView (`ui/`, Vite +
JavaScript vanilla). Aucun serveur, aucun réseau sortant hormis la
vérification de mise à jour.

```
┌──────────────────────────────┐        ┌───────────────────────────────┐
│  ui/ (WebView, JS vanilla)   │  IPC   │  src-tauri/ (Rust)            │
│  index.html · main.js · css  │◀──────▶│  commandes #[tauri::command] │
└──────────────────────────────┘        │  crypto · vault · filelock    │
                                        │  scanner · session_bridge     │
        ┌───────────────────────────────┴───────────────┐
        │  extension/ (MV3)  ◀── native messaging ──▶  bin/native_host  │
        └──────────────────────────────────────────────────────────────┘
```

## Modules Rust (`src-tauri/src/`)

### `crypto.rs` — cœur cryptographique

Toute la crypto vit ici, et **nulle part ailleurs** (le native host et le site
réutilisent cette même chaîne).

- `derive_seed_key(passphrase, salt)` — Argon2id (m = 64 Mio, t = 4, p = 1) → 32 o.
  Base commune v1 et v2.
- `create_kyber_vault_key(seed_key)` — génère une paire ML-KEM-1024, encapsule,
  scelle la clé secrète ML-KEM sous `seed_key` (AES-256-GCM), dérive la clé
  finale via `derive_final_key`. Utilisé à la création d'un coffre.
- `open_kyber_vault_key(seed_key, pq_ct, pq_sk_enc, pq_sk_nonce)` — l'inverse :
  déchiffre la clé secrète ML-KEM, décapsule, recombine. Une mauvaise
  passphrase échoue ici (tag GCM sur `pq_sk_enc`), avant même AES sur le coffre.
- `derive_final_key(seed_key, pq_ss)` — HKDF-SHA256(`seed_key ‖ pq_ss`),
  info `KyberVault-v2-final-key` → 32 o.
- `encrypt_vault_payload` / `decrypt_vault_payload` — AES-256-GCM, nonce
  aléatoire `OsRng` frais à chaque écriture.
- `MasterKey([u8; 32])` — `#[derive(Zeroize, ZeroizeOnDrop)]` : la clé est
  effacée de la mémoire à la libération.
- `HybridKeyPair` / `hybrid_encapsulate` — KEM hybride ML-KEM + X25519, réservé
  au futur partage de clé (pas encore câblé côté UI).

> **Note honnête** (aussi dans le code) : la sécurité du coffre *au repos*
> vient de la passphrase + Argon2id + AES-256-GCM. La clé secrète ML-KEM étant
> scellée sous `seed_key`, la couche post-quantique n'ajoute pas de marge
> contre un bruteforce de passphrase — c'est de la défense en profondeur.

### `vault.rs` — modèle et formats de coffre

- `VaultEntry { id, title, username, password, url, last_modified }`,
  `VaultData { version, entries: HashMap<String, VaultEntry> }` (sérialisé bincode).
- **Format v1** (`EncryptedVault`, lecture seule) : bincode de
  `{ salt[16], nonce[12], ciphertext }`. Commence directement par le sel.
- **Format v2** (`EncryptedVaultV2`) : 3 octets magic `KY\x02` puis bincode de
  `{ salt[16], pq_ct(1568 o), pq_sk_enc(3184 o), pq_sk_nonce[12], nonce[12], ciphertext }`.
- `unlock_vault` (dans `lib.rs`) dispatche sur le magic ; `migrate_to_v2`
  ré-chiffre un coffre v1 en v2 après vérification de la passphrase.

### `filelock.rs` — chiffrement de fichiers et dossiers

- Format **`.kyber`** (magic `KYBF`) : `magic[4] ‖ nonce[12] ‖ AES-256-GCM(payload)`
  avec la clé du coffre **ouvert** (pas de passphrase séparée).
- Payload en clair : `[meta_len 4 o LE][JSON FileMeta { name, is_folder }][données ou ZIP]`.
- Un dossier est zippé en mémoire (`zip` + `walkdir`) puis chiffré.
- Déchiffrement borné : protection zip-slip (pas d'écriture hors du dossier
  cible) et limite anti zip-bomb (500 Mo décompressés).
- Une mauvaise clé → échec du tag GCM → « ce fichier a été chiffré avec un
  coffre différent ».

> Le site web (`kyber-site/lib/kyberfile.ts`) et l'extension utilisent un
> format voisin mais **distinct**, `KYBP`, chiffré par *mot de passe* (chaîne
> Argon2id + ML-KEM identique) et non par la clé d'un coffre.

### `scanner.rs` — détection du champ mot de passe actif

- `start_scanner(AppHandle)` dispatche vers une implémentation par plateforme :
  - **Windows** : `SetWinEventHook` (changement de focus) + UI Automation pour
    lire le contrôle focalisé et savoir si c'est un champ mot de passe.
  - **macOS** : AXUIElement (AppKit / Accessibility).
  - **Linux** : AT-SPI via D-Bus (`atspi`).
- Émet un évènement Tauri `scanner-detected { context }` que `ui/main.js`
  transforme en popup de suggestion.
- Filtre : ignore les champs de Kyber lui-même (`context` contenant « kyber »).
- Limite connue : Chromium n'expose son arbre d'accessibilité que si un client
  d'automatisation est déjà actif → la détection ne se déclenche pas dans le
  navigateur (c'est le rôle de l'extension).

### `session_bridge.rs` — pont local pour l'extension

- Quand un coffre est ouvert, l'app écoute sur `127.0.0.1:47732` (jamais
  exposé au réseau) et écrit `{ port, token }` dans `~/.kyber/session.json`.
- Cache d'entrées en mémoire (`Arc<SessionBridgeInner>`), mis à jour à chaque
  `unlock_vault` / `init_vault` / `add_entry` / `delete_entry` / `update_entry`
  / `import_csv`.
- Protocole ligne-JSON : `ping` (sans jeton, révèle juste « l'app tourne »),
  `get_entries` (jeton exact requis, `LOCKED` si le coffre n'est pas ouvert).
- Comparaison de jeton en temps constant, lectures réseau bornées,
  `session.json` en permissions 0600 (Unix) et nettoyé au démarrage / à
  l'échec de bind.
- 4 tests unitaires (`spawn_listener` sur port éphémère).

### `bin/native_host.rs` — hôte de native messaging

Binaire séparé (`kyber-native-host`), invoqué par l'extension via
`chrome.runtime.sendNativeMessage`. Sans état : un message → une réponse →
sortie. Commandes : `ping`, `get_last_vault_path`, `unlock` (déchiffre un
`.vault` v1/v2 en réutilisant `crypto`), `try_live_session` (relaie vers
`session_bridge`). Borne de 1 Mo sur les messages.

### `lib.rs` — surface de commandes Tauri

Enregistre l'état applicatif (`AppState { vault_path, vault_data, master_key }`),
démarre `session_bridge` et le scanner, et expose les commandes appelées depuis
`ui/main.js` :

| Domaine | Commandes |
|---|---|
| Coffre | `check_vault_exists`, `init_vault`, `unlock_vault`, `lock_vault`, `get_last_vault_path`, `get_default_vault_path` |
| Entrées | `add_entry`, `update_entry`, `delete_entry`, `import_csv`, `export_csv` |
| Sécurité | `get_vault_health`, `get_rotation_setting`, `set_rotation_setting`, `get_rotation_due` |
| Générateur | `generate_password`, `generate_password_options` |
| Système | `autofill_password`, `copy_secure` |
| Fichiers | `encrypt_file_cmd`, `encrypt_folder_cmd`, `decrypt_file_cmd` |
| Migration | `is_vault_v1`, `migrate_to_v2` |

`save_vault` re-chiffre et écrit le coffre en préservant son format (v1 ou v2).

## Frontend (`ui/`)

- `index.html` — écran de login, écran principal (liste, générateur, santé,
  fichiers, paramètres), modales (entrée, migration, mise à jour) et popup
  scanner.
- `main.js` — un seul fichier : IPC vers Rust (`invoke`), rendu de la grille
  d'entrées (un `innerHTML`, délégation d'évènements), recherche debouncée,
  logique du popup scanner, flux de mise à jour (`tauri-plugin-updater`).
- `style.css` — thème sombre graphite, aucune police externe (offline strict).
- Build Vite → `ui/dist/`, référencé par `tauri.conf.json` (`frontendDist`).

## Extension navigateur (`extension/`)

MV3, compagnon de l'app. Ne stocke aucune donnée d'identifiant : passe par le
native host / le pont de session. Voir [`extension/README.md`](extension/README.md)
pour le détail (connexion live, modèle de confiance, chiffrement de fichiers
autonome).

## Mises à jour

`tauri-plugin-updater` vérifie `https://kyber-security.fr/updates/latest.json`
au démarrage. Chaque artefact est signé (minisign) ; la clé publique est dans
`tauri.conf.json`, la clé privée est **hors du dépôt**. Processus de release
détaillé dans [CONTRIBUTING.md](CONTRIBUTING.md).

# Kyber

**Gestionnaire de mots de passe post-quantique, 100 % local, gratuit et open source.**

Kyber chiffre vos mots de passe et vos fichiers sur votre machine, avec une
chaîne Argon2id → ML-KEM-1024 (Kyber1024, NIST FIPS 203) → HKDF-SHA256 →
AES-256-GCM. Aucun compte, aucun cloud, aucune télémétrie. Rien ne sort de
votre ordinateur.

[![Licence](https://img.shields.io/badge/licence-Apache--2.0-blue)](LICENSE)
[![Plateforme](https://img.shields.io/badge/plateforme-Windows-lightgrey)](#installation)
[![Rust](https://img.shields.io/badge/backend-Rust%20%2B%20Tauri%202-orange)](#pile-technique)

Site : <https://kyber-security.fr> · Sécurité : [SECURITY.md](SECURITY.md) ·
Architecture : [ARCHITECTURE.md](ARCHITECTURE.md) · Contribuer : [CONTRIBUTING.md](CONTRIBUTING.md)

---

## Ce que fait Kyber

- **Coffre de mots de passe** chiffré (`.vault`) : ajout, édition, recherche,
  générateur, analyse de sécurité, import/export CSV (Bitwarden / 1Password).
- **Chiffrement de fichiers et dossiers** (`.kyber`) avec la clé du coffre ouvert.
- **Auto-remplissage** : détection native du champ mot de passe actif
  (UI Automation sous Windows) et remplissage depuis le coffre.
- **Extension navigateur** compagnon : remplit vos identifiants dans le
  navigateur en se connectant au coffre déjà ouvert dans l'app, sans
  redemander le mot de passe maître. Voir [`extension/README.md`](extension/README.md).
- **Mises à jour automatiques signées** (minisign, via `tauri-plugin-updater`).

Toutes les fonctionnalités sont disponibles pour tout le monde. Il n'y a pas
de version « Pro », pas de licence, pas de limite.

## Modèle de sécurité (honnête)

Kyber vise une protection **au repos** : un attaquant qui obtient votre fichier
`.vault` ne doit rien pouvoir en tirer sans votre passphrase.

```
passphrase ──Argon2id(64 Mio, t=4, p=1)──▶ seed_key (32 o)
                                            │
             ML-KEM-1024 : keypair + encapsulation ──▶ pq_ss
             (la clé secrète ML-KEM est scellée sous seed_key)
                                            │
             HKDF-SHA256(seed_key ‖ pq_ss) ──▶ clé finale (32 o)
                                            │
             AES-256-GCM ──▶ coffre chiffré + authentifié (.vault)
```

**Ce qui porte réellement la sécurité au repos : la passphrase + Argon2id +
AES-256-GCM.** AES-256 conserve ~128 bits de marge face à un attaquant
quantique (algorithme de Grover), et Argon2id rend le bruteforce de passphrase
coûteux.

**Le rôle de ML-KEM-1024 est la défense en profondeur, pas une garantie
magique.** Sa clé secrète est elle-même chiffrée sous `seed_key` : un attaquant
qui casse la passphrase récupère aussi le secret partagé ML-KEM. La couche
post-quantique lie le KEM au coffre et prépare le partage de clé hybride ;
elle **n'ajoute pas** de marge contre un bruteforce de passphrase. Nous
préférons le dire clairement plutôt que de vendre de « l'inviolable ».

Voir [SECURITY.md](SECURITY.md) pour le modèle de menace complet et la
procédure de signalement de vulnérabilité.

### Ce que Kyber ne protège pas

- Un appareil déjà compromis (keylogger, malware) : le mot de passe maître est
  saisi en clair au clavier.
- Une passphrase faible : toute la sécurité en dépend.
- **La perte du mot de passe maître : le coffre est définitivement
  irrécupérable.** Il n'y a pas de porte dérobée, pas de récupération par email.

## Installation

### Windows (binaire)

Téléchargez `Kyber_x.y.z_x64-setup.exe` depuis
<https://kyber-security.fr/telechargement> et lancez l'installateur.
Les mises à jour suivantes se font automatiquement dans l'app.

### Linux / macOS

Pas encore de binaire officiel. Compilez depuis les sources (ci-dessous) :
les cibles Tauri Linux (`.deb`, `.AppImage`, `.rpm`) et macOS (`.dmg`)
fonctionnent, elles ne sont simplement pas encore publiées ni testées à chaque
release.

## Compiler depuis les sources

Prérequis : [Rust](https://rustup.rs/) (≥ 1.77), Node.js (≥ 18),
et `cargo-tauri` : `cargo install tauri-cli --version "^2"`.
Sous Linux, voir la liste des paquets système dans [CONTRIBUTING.md](CONTRIBUTING.md).

```bash
# 1. Frontend
cd ui && npm install && npm run build && cd ..

# 2. Application
cd src-tauri && cargo tauri build
```

Sorties Windows : `src-tauri/target/release/bundle/nsis/Kyber_<ver>_x64-setup.exe`
et `msi/Kyber_<ver>_x64_en-US.msi`.

Pour une **release signée** (updater), voir [CONTRIBUTING.md](CONTRIBUTING.md) —
la clé de signature est privée et hors du dépôt.

## Pile technique

| Couche | Choix |
|---|---|
| Backend | Rust + [Tauri 2](https://tauri.app) (`src-tauri/`) |
| Frontend | Vite + JavaScript vanilla (`ui/`) |
| Dérivation | `argon2` (Argon2id), `hkdf` + `sha2` (HKDF-SHA256) |
| Chiffrement | `aes-gcm` (AES-256-GCM) |
| Post-quantique | `pqcrypto-kyber` (ML-KEM-1024), `x25519-dalek` (couche hybride) |
| Zéroïsation | `zeroize` (`ZeroizeOnDrop` sur les clés) |
| Updater | `tauri-plugin-updater`, artefacts signés minisign |

Détail module par module : [ARCHITECTURE.md](ARCHITECTURE.md).

## Emplacement des données

Tout est sous `~/.kyber/` (Windows : `C:\Users\<vous>\.kyber\`) :

| Fichier | Contenu |
|---|---|
| `<votre-coffre>.vault` | le coffre chiffré (vous choisissez où le mettre) |
| `last_vault.txt` | chemin du dernier coffre ouvert |
| `vaults.json` | liste des coffres connus (pour l'écran d'accueil) |
| `rotation_settings.json` | préférences de rappel de rotation par coffre |
| `session.json` | port + jeton du pont local pour l'extension (coffre ouvert) |

Aucun de ces fichiers ne quitte la machine.

## Contribuer

Les contributions sont bienvenues : voir [CONTRIBUTING.md](CONTRIBUTING.md)
(mise en place, `cargo fmt` / `clippy` / `test`, conventions). Pour une faille
de sécurité, suivez [SECURITY.md](SECURITY.md) plutôt qu'une issue publique.

## Licence

[Apache-2.0](LICENSE). © 2026 Enzo Paccard. Voir [NOTICE](NOTICE) pour les
composants tiers.

---

## English (summary)

**Kyber** is a post-quantum, fully local, free and open-source password
manager. It encrypts passwords and files on your machine using
Argon2id → ML-KEM-1024 (NIST FIPS 203) → HKDF-SHA256 → AES-256-GCM. No account,
no cloud, no telemetry.

**Honest security note:** at-rest security rests on your passphrase + Argon2id +
AES-256-GCM. The ML-KEM-1024 layer is defense-in-depth — its secret key is
sealed under the Argon2id seed, so it adds no margin against passphrase
brute-force. Losing the master password means the vault is unrecoverable by
design.

Windows binaries: <https://kyber-security.fr>. Build from source: see above.
Backend is Rust + Tauri 2 (`src-tauri/`), frontend is vanilla JS (`ui/`).
Licensed under Apache-2.0. See [ARCHITECTURE.md](ARCHITECTURE.md),
[SECURITY.md](SECURITY.md), [CONTRIBUTING.md](CONTRIBUTING.md).

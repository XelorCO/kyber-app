# Kyber / Gestionnaire de mots de passe post-quantique

Application de bureau 100% locale qui chiffre les mots de passe avec une chaîne
Argon2id + ML-KEM-1024 (Kyber1024, NIST FIPS 203) + HKDF-SHA256 + AES-256-GCM.
Aucun cloud, aucune télémétrie. Site : https://kyber-security.fr

## Stack

- **Backend** : Rust + Tauri 2 (`src-tauri/`)
- **Frontend** : Vite + JS vanilla (`ui/`)
- **Crypto** : `pqc_kyber` (ML-KEM-1024), `aes-gcm`, `argon2`, `hkdf`, `ed25519-dalek` (licences)
- **Updater** : `tauri-plugin-updater`, artefacts signés minisign, manifest `https://kyber-security.fr/updates/latest.json`

## Architecture crypto (coffre .vault v2)

```
passphrase → Argon2id (64 Mio, t=4) → seed key
           → ML-KEM-1024 encapsulation (défense en profondeur)
           → HKDF-SHA256 → AES-256-GCM (chiffrement authentifié)
```

Note honnête : la robustesse au repos vient d'abord de la passphrase + Argon2id + AES-256-GCM.
ML-KEM est une couche de défense en profondeur, pas une garantie magique.

## Freemium

- Gratuit : 10 mots de passe max (`FREE_LIMIT` dans `src-tauri/src/lib.rs`), 1 coffre (`FREE_VAULT_LIMIT`)
- Pro (29 EUR, paiement unique) : illimité + export CSV / licence Ed25519 vérifiée dans `src-tauri/src/license.rs`
- Famille (49 EUR) : même licence, tier `famille`, 5 postes
- La clé de licence est générée côté site (repo `kyber-site`) et envoyée par email après paiement Stripe

## Build

```bash
# 1. Frontend
cd ui && npm install && npm run build

# 2. App (build signé pour l'updater — clé HORS repo, ne jamais commiter)
cd ../src-tauri
TAURI_SIGNING_PRIVATE_KEY="$(cat ~/.kyber-release/updater.key)" \
TAURI_SIGNING_PRIVATE_KEY_PASSWORD="" \
cargo tauri build
```

Sorties Windows : `src-tauri/target/release/bundle/nsis/Kyber_<ver>_x64-setup.exe` (+ `.sig`)
et `msi/Kyber_<ver>_x64_en-US.msi` (+ `.sig`).

## Process de release

1. Bump `version` dans `src-tauri/tauri.conf.json` (+ texte version dans `ui/index.html`)
2. Build signé (ci-dessus)
3. Copier `.exe` + `.msi` dans `../Kyber-site/public/downloads/`
4. Coller le contenu du `.exe.sig` dans `../Kyber-site/public/updates/latest.json` (version, notes, pub_date, url)
5. Mettre à jour les liens de téléchargement dans le site (`app/page.tsx`, `app/telechargement/page.tsx`)
6. Commit + push les deux repos / Vercel déploie le site

## Tests

```bash
cd src-tauri && cargo test --release
```

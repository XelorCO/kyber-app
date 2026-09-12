# Kyber — contexte pour agents IA

Gestionnaire de mots de passe post-quantique, 100 % local, **gratuit et open
source (Apache-2.0)**. Pas de licence, pas de freemium, pas de compte.

- App de bureau : Rust + Tauri 2 (`src-tauri/`) + Vite / JS vanilla (`ui/`)
- Extension navigateur compagnon : `extension/` (MV3)
- Site vitrine : dépôt séparé `kyber-site` (Next.js, `kyber-security.fr`)

## Documentation de référence

- [`README.md`](README.md) — présentation, modèle de sécurité honnête, build
- [`ARCHITECTURE.md`](ARCHITECTURE.md) — rôle de chaque module, formats de fichier
- [`SECURITY.md`](SECURITY.md) — modèle de menace, signalement de faille
- [`CONTRIBUTING.md`](CONTRIBUTING.md) — commandes fmt/clippy/test, processus de release
- [`CHANGELOG.md`](CHANGELOG.md)

## Vérifs avant de proposer un changement

```bash
cd src-tauri
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings   # objectif : zéro warning
cargo test
cd ../ui && npm run build
```

## Points à ne pas oublier

- Toute la crypto est dans `src-tauri/src/crypto.rs` — ne pas la dupliquer.
  La sécurité au repos = passphrase + Argon2id + AES-256-GCM ; ML-KEM-1024 est
  de la défense en profondeur (sa clé est scellée sous la seed Argon2id).
  **Ne pas prétendre « incassable grâce au post-quantique ».**
- Commentaires et messages d'erreur en français.
- `~/.kyber/license.key` n'existe plus dans le code : le système de licence a
  été retiré en 2.0.0.
- Windows : lancer `cargo tauri dev` via un terminal détaché (`Start-Process`),
  sinon le process se fait tuer prématurément.
- Clé de signature updater : **hors du dépôt**, variable
  `TAURI_SIGNING_PRIVATE_KEY` (pas `_PATH`).
- Le dossier `src/` à la racine est un vestige legacy — le vrai code est
  `src-tauri/src/`.

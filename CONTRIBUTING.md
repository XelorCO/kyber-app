# Contribuer à Kyber

Merci de vouloir contribuer. Kyber est un logiciel de sécurité : on privilégie
les changements petits, revus et testés.

## Mise en place

Prérequis :

- [Rust](https://rustup.rs/) ≥ 1.77 (`rustup component add clippy rustfmt`)
- Node.js ≥ 18
- `cargo install tauri-cli --version "^2"`

Dépendances système Linux (Debian / Ubuntu / Kali) :

```bash
sudo apt update && sudo apt install -y \
  libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev \
  librsvg2-dev patchelf libssl-dev pkg-config
```

## Lancer en développement

```bash
cd ui && npm install && npm run build && cd ..
cd src-tauri && cargo tauri dev
```

> **Windows** : lancer `cargo tauri dev` via un terminal détaché
> (`Start-Process`) plutôt que depuis un wrapper qui tue l'arbre de processus
> en fin de commande, sinon l'app se ferme prématurément.

Après une modif du frontend seul : `cd ui && npm run build` suffit (l'app
recharge `ui/dist/`).

## Avant d'ouvrir une PR

```bash
cd src-tauri
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test
cargo check --bins            # vérifie aussi kyber-native-host

cd ../ui && npm run build     # le frontend doit builder
```

L'intégration continue (`.github/workflows/ci.yml`) rejoue exactement ça.
Le dépôt vise **zéro warning clippy**.

## Style

- **Rust** : `rustfmt` par défaut, pas de `unwrap()` sur des entrées
  utilisateur (préférer `?` + message d'erreur clair renvoyé à l'UI).
- **Commentaires et messages d'erreur** : en français, pour rester cohérent
  avec l'existant.
- **JavaScript** (`ui/`, `extension/`) : vanilla, pas de framework, pas de
  build tool pour l'extension (fichiers chargés tels quels).
- **Crypto** : aucune primitive nouvelle sans discussion préalable (issue). Si
  vous touchez `crypto.rs`, ajoutez / mettez à jour les tests de roundtrip.

## Périmètre

Bienvenus : corrections de bugs, portage Linux / macOS, accessibilité,
traductions de l'UI, durcissement, tests, documentation.

À discuter d'abord dans une issue : nouveaux formats de fichier, changements de
la chaîne cryptographique, nouvelles permissions de l'extension, dépendances
lourdes.

## Sécurité

Une faille se signale en privé (voir [SECURITY.md](SECURITY.md)), pas dans une
issue ou une PR publique.

## Processus de release (mainteneur)

1. Bumper `version` dans `src-tauri/tauri.conf.json` et le texte de version
   dans `ui/index.html`. Ajouter une entrée en tête de [CHANGELOG.md](CHANGELOG.md).
2. Build **signé** (la clé updater est privée, hors du dépôt) :
   ```bash
   cd ui && npm run build && cd ../src-tauri
   export TAURI_SIGNING_PRIVATE_KEY="/chemin/vers/updater.key"   # PAS *_PATH (syntaxe Tauri v1, ne signe rien)
   export TAURI_SIGNING_PRIVATE_KEY_PASSWORD=""
   cargo tauri build
   ```
3. **Installer et lancer** le binaire une fois (le manifeste updater déclenche
   la mise à jour automatique de toutes les installations existantes — ne rien
   publier sans avoir vérifié que la version se lance).
4. Copier `.exe` / `.msi` (+ `.sig`) dans `kyber-site/public/downloads/`, coller
   le contenu du `.sig` dans `kyber-site/public/updates/latest.json` (version,
   notes, `pub_date`, `url`).
5. Mettre à jour les liens de téléchargement du site, committer, pousser.

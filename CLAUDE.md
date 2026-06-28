# Kyber — Contexte projet

## C'est quoi
Gestionnaire de mots de passe post-quantique français.
- App de bureau : Rust + Tauri 2 + Vite (JS vanilla)
- Site web : Next.js dans `../Kyber-site/` (GitHub: XelorCO/kyber-site, live sur kyber-security.fr)

## Structure du dossier
```
Kyber/
├── src-tauri/        ← code Rust + config Tauri
│   ├── src/
│   │   ├── main.rs
│   │   ├── license.rs   ← vérification licence Ed25519
│   │   └── ...
│   ├── tauri.conf.json
│   └── Cargo.toml
└── ui/               ← frontend Vite (JS vanilla)
    ├── index.html
    ├── main.js
    └── package.json
```

## Comment compiler

### 1. Builder le frontend
```bash
cd ui && npm install && npm run build
```

### 2. Builder l'app Tauri
```bash
cd src-tauri && cargo tauri build
```
> Si `cargo tauri` n'existe pas : `cargo install tauri-cli --version "^2"`

### Dépendances Linux nécessaires (Debian/Ubuntu/Kali)
```bash
sudo apt update && sudo apt install -y \
  libwebkit2gtk-4.1-dev \
  libgtk-3-dev \
  libayatana-appindicator3-dev \
  librsvg2-dev \
  patchelf \
  libssl-dev \
  pkg-config
```

### Output Linux attendu
```
src-tauri/target/release/bundle/deb/kyber_1.0.0_amd64.deb
src-tauri/target/release/bundle/appimage/Kyber_1.0.0_amd64.AppImage
```

## Après la compilation
Copier les binaires dans le site :
```bash
cp src-tauri/target/release/bundle/appimage/Kyber_1.0.0_amd64.AppImage \
   ../Kyber-site/public/downloads/

cp src-tauri/target/release/bundle/deb/kyber_1.0.0_amd64.deb \
   ../Kyber-site/public/downloads/
```

Puis mettre à jour le lien Linux dans `../Kyber-site/app/page.tsx` :
- Trouver la carte Linux (`platform: 'Linux'`)
- Changer `href: null` → `href: '/downloads/Kyber_1.0.0_amd64.AppImage'`
- Changer `label: 'Bientôt disponible'` → `label: 'Télécharger .AppImage'`
- Changer `available: false` → `available: true`

Puis commit + push depuis `../Kyber-site/` :
```bash
cd ../Kyber-site
git add public/downloads/ app/page.tsx
git commit -m "feat: add Linux AppImage binary"
git push
```

## Infos importantes
- Licence : Ed25519, clé publique dans `src-tauri/src/license.rs` (`PUBLIC_KEY_BYTES`)
- Version actuelle : 1.0.0
- Windows déjà compilé : `Kyber_1.0.0_x64-setup.exe` disponible sur le site

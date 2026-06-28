# PqPassMgr - Post-Quantum Password Manager

A cutting-edge, cross-platform password manager written in Rust and Vue/Tauri. It utilizes post-quantum cryptographic primitives (Kyber / Dilithium) and Argon2id to ensure robust security against current and future threats.

## 🚀 Features
- **Post-Quantum Cryptography**: Built to resist quantum computer attacks using Kyber/Dilithium.
- **Tauri / Rust Backend**: Fast, minimal resource footprint, and memory safe.
- **Vue.js Frontend**: Smooth, responsive, and modern user interface (Glassmorphism design).
- **Standalone Executable**: Ships as a single native application, no browser extensions needed.
- **Smart Window Focus Scanner**: Auto-detects password fields across the OS without hooks that require a browser extension, directly from the Windows API.
- **No Cloud Bullshit**: Fully local. You own your encrypted vault file.

## 🛠️ Tech Stack
- **Frontend**: Vue 3 + Vite
- **Backend**: Rust + Tauri
- **Crypto**: `pqcrypto` crates

## 📖 Documentation
- See [INSTALL.md](INSTALL.md) for build instructions and setup.

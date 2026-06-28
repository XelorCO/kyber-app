# Installation & Build Instructions

## Prerequisites
1. **Rust & Cargo**: Install via [rustup.rs](https://rustup.rs/).
2. **Node.js**: Recommended version 18 or higher.
3. **OS Dependencies**:
   - **Windows**: Visual Studio C++ Build tools.
   - **Linux**: `webkit2gtk`, `build-essential`, `curl`, `wget`, `file`, `libssl-dev`, `libgtk-3-dev`, `libayatana-appindicator3-dev`, `librsvg2-dev`.

## Setup
Clone the repository and install the frontend dependencies:
```bash
git clone https://github.com/YOUR_USERNAME/PqPassMgr.git
cd PqPassMgr/ui
npm install
cd ..
```

## Running in Dev Mode
To run the application with hot-reloading for both the Vue frontend and the Rust backend:
```bash
# From the root of the project (PqPassMgr)
npx tauri dev
```

## Building for Production
To build the standalone executable and generate installers (MSI, NSIS):
```bash
# From the root of the project
npx tauri build
```
Once the build is complete, you can find the output files here:
- **Executable**: `src-tauri/target/release/app.exe`
- **Installers**: `src-tauri/target/release/bundle/`

## Helper Scripts
For Windows users, there is a `build_and_shortcut.ps1` script provided at the root. You can execute it to automatically build the project and create a shortcut on your Desktop:
```powershell
.\build_and_shortcut.ps1
```

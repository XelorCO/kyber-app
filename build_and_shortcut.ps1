$ErrorActionPreference = "Stop"

Write-Host "Building Tauri app..."
Set-Location "C:\Users\magic\.gemini\antigravity\scratch\PqPassMgr"

# Run npx tauri build
cmd.exe /c "npx tauri build"

Write-Host "Creating Desktop Shortcut..."
$WshShell = New-Object -comObject WScript.Shell
$DesktopPath = [Environment]::GetFolderPath("Desktop")
$Shortcut = $WshShell.CreateShortcut("$DesktopPath\PqPassMgr.lnk")
$Shortcut.TargetPath = "C:\Users\magic\.gemini\antigravity\scratch\PqPassMgr\src-tauri\target\release\app.exe"
$Shortcut.WorkingDirectory = "C:\Users\magic\.gemini\antigravity\scratch\PqPassMgr\src-tauri\target\release"
$Shortcut.Description = "Post-Quantum Password Manager"
$Shortcut.Save()

Write-Host "Done!"

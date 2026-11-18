# Troubleshooting & FAQ

### 1. `cargo build --release` says `link.exe not found`
You don't have MSVC C++ Build Tools installed. Switch to the standalone GNU toolchain instead:
```powershell
rustup default stable-x86_64-pc-windows-gnu
cargo build --release
```

### 2. `Access is denied (os error 5)` when rebuilding
An older `winscroller-alt-tab.exe` is still running in the background and holding a file lock. Exit it cleanly before rebuilding:
```powershell
.\target\release\winscroller-alt-tab.exe --quit
cargo build --release
.\target\release\winscroller-alt-tab.exe --replace
```

### 3. `Alt+Tab` opens Windows' default switcher when Task Manager is focused
Windows UIPI prevents non-elevated apps from intercepting shortcuts over elevated Administrator windows. Launch without `--no-elevate` (accept UAC), or right-click the tray icon and choose **Restart as Administrator (for Admin apps)**.

### 4. Can `Alt` ever get stuck down?
No. `src/win_hook.rs` always forwards physical `Alt` key-up transitions through `CallNextHookEx`, polls `GetAsyncKeyState` every `12ms` during Hold mode, and enforces a `15s` inactivity watchdog.

### 5. Windows SmartScreen ("Windows protected your PC")
Self-compiled open-source binaries are not commercially code-signed. Click **More info** → **Run anyway**.

### 6. How do I verify my `config.toml` has no typos?
```powershell
.\target\release\winscroller-alt-tab.exe --validate
```

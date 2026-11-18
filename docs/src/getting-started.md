# Getting Started

This guide walks you through installing prerequisites, cloning **Winscroller Alt+Tab**, compiling the optimized Rust release binary, and configuring autostart on Windows 10 or Windows 11 (64-bit).

---

## 1. Prerequisites

Open **PowerShell as Administrator** (`Win + X` → **Terminal (Admin)**) and install Git and Rust via Windows Package Manager (`winget`):

```powershell
# Install Git for Windows
winget install --id Git.Git --accept-package-agreements --accept-source-agreements

# Install Rustup (Rust toolchain)
winget install --id Rustlang.Rustup --accept-package-agreements --accept-source-agreements
```

**Restart PowerShell** after installation so `git`, `cargo`, and `rustc` appear on your `PATH`.

*(Optional: If you don't have Visual Studio Build Tools installed, switch Rust to the standalone MinGW toolchain: `rustup default stable-x86_64-pc-windows-gnu`.)*

Verify both tools:

```powershell
git --version
cargo --version
```

---

## 2. Clone & Compile

```powershell
git clone https://github.com/fruitsaladwindowsai-zammy/Alt-Tab.git
cd Alt-Tab

# Build optimized standalone binary (LTO + stripped)
cargo build --release

# Initialize and validate your configuration file
.\target\release\winscroller-alt-tab.exe --init-config
.\target\release\winscroller-alt-tab.exe --validate
```

The standalone executable (~700 KB) is built at:

```text
target\release\winscroller-alt-tab.exe
```

---

## 3. Preview & Launch

### Safe Interactive Demo (No Hooks)
```powershell
.\target\release\winscroller-alt-tab.exe --demo
```
Displays a realistic 12-window / 8-workspace field so you can test The Deep, Sounding Line, live search, `+ New desktop`, and drag-and-drop safely.

### Launch as Your Live Alt+Tab Switcher
```powershell
.\target\release\winscroller-alt-tab.exe
```

1. Click **Yes** on the Windows UAC elevation prompt so `Alt+Tab` interception and live window captures work seamlessly over Administrator windows (Task Manager, Admin PowerShell).
2. Prefer running without Administrator rights? Pass `--no-elevate`:
   ```powershell
   .\target\release\winscroller-alt-tab.exe --no-elevate
   ```

---

## 4. Enable Autostart ("Start with Windows")

1. Right-click the **Winscroller Alt+Tab** icon in your Windows system tray.
2. Click **Start with Windows** (adds `HKCU\Software\Microsoft\Windows\CurrentVersion\Run\WinscrollerAltTab`).

---

## 5. Updating to Newer Versions

```powershell
# 1. Gracefully exit the running instance (unlocks the .exe)
.\target\release\winscroller-alt-tab.exe --quit

# 2. Pull latest commits
git pull origin main

# 3. Rebuild and verify tests
cargo build --release
cargo test

# 4. Restart cleanly
.\target\release\winscroller-alt-tab.exe --replace
```

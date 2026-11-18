# Getting Started

Complete step-by-step setup guide for installing **Winscroller Alt+Tab** on Windows 10 or Windows 11 (64-bit).

---

## 1. Install Git & Rust (One-Time Setup)

Open **PowerShell as Administrator** (`Win + X` → **Terminal (Admin)**) and run:

```powershell
winget install --id Git.Git --accept-package-agreements --accept-source-agreements
winget install --id Rustlang.Rustup --accept-package-agreements --accept-source-agreements
```

Close and reopen PowerShell after installation so `git` and `cargo` are on your `PATH`.

*(Optional: If you don't want to install Visual Studio C++ Build Tools, use the standalone MinGW toolchain: `rustup default stable-x86_64-pc-windows-gnu`.)*

---

## 2. Clone & Build

```powershell
git clone https://github.com/fruitsaladwindowsai-zammy/Alt-Tab.git
cd Alt-Tab

cargo build --release
.\target\release\winscroller-alt-tab.exe --init-config
.\target\release\winscroller-alt-tab.exe --validate
```

---

## 3. Preview & Launch

```powershell
# Safe interactive demo (12 windows across 8 workspaces, zero hooks)
.\target\release\winscroller-alt-tab.exe --demo

# Live background switcher (auto-requests UAC elevation for Admin apps)
.\target\release\winscroller-alt-tab.exe

# Run without Administrator elevation
.\target\release\winscroller-alt-tab.exe --no-elevate
```

---

## 4. Enable Autostart

1. Right-click the **Winscroller Alt+Tab** icon in your Windows system tray.
2. Click **Start with Windows** (checkmark enabled).

---

## 5. Clean Update Workflow

```powershell
.\target\release\winscroller-alt-tab.exe --quit
git pull origin main
cargo build --release
.\target\release\winscroller-alt-tab.exe --replace
```

# Installation Guide — Winscroller Alt+Tab

A complete, beginner-friendly guide to installing and running **Winscroller Alt+Tab** on Windows 10 and Windows 11.

---

## Table of Contents
1. [Prerequisites](#1-prerequisites)
2. [Step 1: Install Git & Rust](#step-1-install-git--rust)
3. [Step 2: Clone the Repository](#step-2-clone-the-repository)
4. [Step 3: Build the Optimized Binary](#step-3-build-the-optimized-binary)
5. [Step 4: Launch Winscroller Alt+Tab](#step-4-launch-winscroller-alttab)
6. [Step 5: Configure Autostart (Start with Windows)](#step-5-configure-autostart-start-with-windows)
7. [Step 6: Updating to Future Versions](#step-6-updating-to-future-versions)
8. [Alternative: Install Without Building (Prebuilt Release)](#alternative-install-without-building-prebuilt-release)
9. [Troubleshooting & FAQ](#troubleshooting--faq)

---

## 1. Prerequisites

- **Operating System:** Windows 10 (64-bit) or Windows 11 (64-bit)
- **Permissions:** Standard user account (Admin rights recommended so `Alt+Tab` works seamlessly over Administrator apps like Task Manager)
- **Disk Space:** ~500 MB for the Rust compiler build artifacts (final standalone executable is only ~700 KB)

---

## Step 1: Install Git & Rust

You need two free, standard developer tools: **Git** (to download the source code) and **Rust** (to compile it).

### Option A: The 1-Minute Automated Install (Recommended)

Open **PowerShell** as Administrator (press `Win + X`, then choose **Terminal** or **Windows PowerShell**) and run:

```powershell
winget install --id Git.Git -e --source winget
winget install --id Rustlang.Rustup -e --source winget
```

> **Note:** Once the installation finishes, **close and reopen PowerShell** so your environment variables (`PATH`) refresh.

### Option B: Manual Downloads (Web Browser)

1. **Git:** Download and install from [git-scm.com/download/win](https://git-scm.com/download/win) (choose 64-bit standalone installer, click "Next" through default options).
2. **Rust:** Download and run `rustup-init.exe` from [rustup.rs](https://rustup.rs/). Press `1` and hit `Enter` to proceed with the standard installation.

### Verify Installation
In a fresh PowerShell terminal, run:
```powershell
git --version
cargo --version
```
Both commands should print version numbers (e.g., `git version 2.4x.x` and `cargo 1.xx.x`).

---

## Step 2: Clone the Repository

Choose where you want to keep the project folder (for example, in `C:\Projects` or `D:\Projects`).

Open **PowerShell** and run:

```powershell
# Navigate to your preferred projects folder (or create it)
mkdir C:\Projects -ErrorAction SilentlyContinue
cd C:\Projects

# Clone the repository
git clone https://github.com/fruitsaladwindowsai-zammy/Alt-Tab.git

# Enter the project directory
cd Alt-Tab
```

---

## Step 3: Build the Optimized Binary

Inside the `Alt-Tab` directory, compile the production release:

```powershell
cargo build --release
```

- Cargo will automatically download all dependencies and compile the Direct3D 11 / Direct2D engine.
- This process takes about **30 to 60 seconds** on the first run.
- When finished, the compiled executable will be located at:
  ```text
  target\release\winscroller-alt-tab.exe
  ```

*(Optional) Verify all unit tests pass on your machine:*
```powershell
cargo test
```

---

## Step 4: Launch Winscroller Alt+Tab

### Quick Interactive Demo (No hooks, safe preview)
To see the depth switcher immediately with a simulated 12-window / 8-workspace demo field:

```powershell
.\target\release\winscroller-alt-tab.exe --demo
```

### Run as your Live Alt+Tab Switcher
To launch Winscroller Alt+Tab in the background and replace Windows' default `Alt+Tab`:

```powershell
.\target\release\winscroller-alt-tab.exe
```

1. **UAC Elevation Prompt:** Windows will ask for Administrator permission.
   - **Why?** Windows User Interface Privilege Isolation (UIPI) blocks low-level keyboard hooks and window preview captures when an elevated window (such as Task Manager or an Admin terminal) is focused. Elevating guarantees Winscroller Alt+Tab works everywhere.
   - If you prefer running without elevation, pass `--no-elevate`:
     ```powershell
     .\target\release\winscroller-alt-tab.exe --no-elevate
     ```
2. **System Tray Icon:** A small Winscroller Alt+Tab icon will appear in your Windows system tray (bottom-right near the clock).

Now, whenever you press **`Alt + Tab`**, Winscroller Alt+Tab opens!

---

## Step 5: Configure Autostart (Start with Windows)

You don't need to manually start Winscroller Alt+Tab every time your PC boots:

1. Look at your Windows taskbar tray (bottom-right near the clock, click `^` if hidden).
2. **Right-click** the **Winscroller Alt+Tab** icon.
3. Click **Start with Windows** (a checkmark will appear).

*(To disable, simply right-click the tray icon and click "Start with Windows" again).*

---

## Step 6: Updating to Future Versions

Whenever new features or bug fixes are released, update your local installation in seconds:

```powershell
# Navigate to the repo folder
cd C:\Projects\Alt-Tab

# Pull the latest code
git pull origin main

# Recompile the release binary
cargo build --release

# Restart the running switcher seamlessly
.\target\release\winscroller-alt-tab.exe --replace
```

---

## Alternative: Install Without Building (Prebuilt Release)

If you do not want to install Rust and build from source:

1. Visit the [Releases](https://github.com/fruitsaladwindowsai-zammy/Alt-Tab/releases) page.
2. Download `winscroller-alt-tab.exe` under the latest release assets.
3. Place `winscroller-alt-tab.exe` in a permanent folder (e.g. `C:\Users\<YourUser>\AppData\Local\Programs\WinscrollerAltTab\`).
4. Double-click to launch it, then right-click its tray icon and select **Start with Windows**.

---

## Troubleshooting & FAQ

### "link.exe not found" or "Microsoft C++ Build Tools required"
If `cargo build --release` asks for MSVC build tools and you don't have Visual Studio installed, you can switch to the standalone MinGW/GNU toolchain:
```powershell
rustup default stable-x86_64-pc-windows-gnu
cargo build --release
```

### Windows SmartScreen warning ("Windows protected your PC")
Because open-source binaries you compile yourself are not signed with a Microsoft commercial certificate:
1. Click **More info**.
2. Click **Run anyway**.

### Stuck Alt Key
Winscroller Alt+Tab passes all physical `Alt` key-up transitions directly through Windows' low-level hook pipeline and includes a 15-second watchdog so keys never stick down, even if interrupted.

### Multiple Monitors
Winscroller Alt+Tab automatically detects all active monitors and opens centered on the monitor that currently has mouse/window focus.

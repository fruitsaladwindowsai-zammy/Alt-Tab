# CLI Flags & System Tray

---

## Command-Line Flags

| Flag | Description |
| :--- | :--- |
| *(none)* | Run in background with system tray icon, global hooks & auto-elevation. Wakes existing instance into Browse mode if already running. |
| `-c`, `--config <path>` | Use a custom `config.toml` path instead of `%APPDATA%\winscroller-alt-tab\config.toml`. |
| `--browse` | Launch directly into pinned Browse mode (or wake running instance). |
| `--no-elevate`, `--non-elevated` | Run at standard user privileges without requesting UAC elevation. |
| `-r`, `--replace` | Gracefully close any running instance and start a fresh one. |
| `--quit` | Send `WM_FATHOM_QUIT` to gracefully stop a running instance and exit. |
| `--init-config` | Create default `%APPDATA%\winscroller-alt-tab\config.toml`. |
| `--validate` | Strictly validate `config.toml` and report line-level errors. |
| `--check-prereqs` | Verify Windows 10/11, DirectX/WIC/DWM, VC++ x64 & config prerequisites. |
| `--install-prereqs` | Automatically check and silently install any missing Windows prerequisites. |
| `--skip-prereqs` | Skip automatic startup prerequisite checks. |
| `--demo` | Launch safe interactive demo (12 windows across 8 workspaces, zero hooks). |
| `--render-demo <path>` | Render offscreen `1920x1080` demo PNG via WIC and exit. |
| `--render-gallery <dir>` | Render all 5 feature screenshots and 13 animation frames and exit. |
| `-V`, `--version` | Print version string and exit. |
| `-h`, `--help` | Print CLI usage help and exit. |

---

## System Tray Controls

- **Left-Click Tray Icon:** Opens the overlay in pinned **Browse** mode.
- **Right-Click Tray Icon:**
  - **Open Winscroller Alt+Tab (Browse)**
  - **Open config.toml**
  - **Start with Windows** *(toggles `HKCU\Software\Microsoft\Windows\CurrentVersion\Run\WinscrollerAltTab`)*
  - **Restart as Administrator (for Admin apps)** *(visible when running non-elevated)*
  - **Exit Winscroller Alt+Tab**

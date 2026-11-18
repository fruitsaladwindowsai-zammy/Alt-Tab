# CLI Flags & Tray Controls

## Command-Line Flags

| Flag | Purpose |
| :--- | :--- |
| *(none)* | Run in background with tray icon & global hooks (wakes running instance into Browse mode if already active) |
| `-c`, `--config <path>` | Use a custom `config.toml` path |
| `--browse` | Open immediately in Browse mode (or wake running instance) |
| `--no-elevate`, `--non-elevated` | Run without UAC Administrator elevation |
| `-r`, `--replace` | Gracefully close any running instance and launch fresh |
| `--quit` | Gracefully exit any running instance |
| `--init-config` | Create default `%APPDATA%\winscroller-alt-tab\config.toml` |
| `--validate` | Validate `config.toml` syntax, keys, bounds & hex colors |
| `--check-prereqs` | Verify Windows runtime & DirectX/VC++ prerequisites |
| `--install-prereqs` | Auto-check and silently install any missing Windows prerequisites |
| `--skip-prereqs` | Skip automatic startup prerequisite checks |
| `--demo` | Launch safe 12-window / 8-workspace interactive demo |
| `--render-demo <path>` | Export offscreen `1920x1080` demo PNG and exit |
| `--render-gallery <dir>` | Export all 5 feature screenshots and 13 animation frames |
| `-V`, `--version` | Print version string |
| `-h`, `--help` | Print CLI help summary |

## System Tray Icon

- **Left-Click:** Open Winscroller Alt+Tab in Browse mode.
- **Right-Click:**
  - **Open Winscroller Alt+Tab (Browse)**
  - **Open config.toml**
  - **Start with Windows** *(toggles `HKCU\Software\Microsoft\Windows\CurrentVersion\Run\WinscrollerAltTab`)*
  - **Restart as Administrator (for Admin apps)** *(when not elevated)*
  - **Exit Winscroller Alt+Tab**

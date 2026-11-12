# Winscroller Alt+Tab

A depth-based `Alt+Tab` window switcher and Virtual Desktop workspace overview for Windows 10 and Windows 11, written in pure Rust with Direct3D 11, Direct2D, DirectWrite, WIC, and Win32 — visually inspired by [mtolhuys/fathom](https://github.com/mtolhuys/fathom).

---

## Highlights

- **The Deep (Spatial-Temporal Recency Stack)**: Windows are arranged back into depth by how long ago they were last used (`depth = log2(1 + seconds / 30)`, clamped to 8 fathoms), gliding with smooth `OutCubic` camera transitions and progressive depth fog.
- **The Sounding Line**: Interactive age gauge on the left (`now` / `≈` down to `2h+` / 8 fathoms) plotting each window as a solid or startup-estimated dot with an animated depth diamond. Click or drag along the gauge to scrub through history.
- **The Workspace Map & Desktop Controls**: Minimap cards along the bottom mirror your Windows Virtual Desktops with real window geometry, cached live previews, icons, and an active `on screen` badge. Create a new Virtual Desktop right on the page with **`+ New desktop`**, switch to empty desktops with a click, or **drag and drop** any app between desktops.
- **Plain Adaptive Windows Theme**: Follows your Windows light/dark preference (`AppsUseLightTheme`) and Windows DWM accent color automatically with contrast-checked typography.
- **Zero-Flash Quick Switch**: A 90ms hold-mode reveal delay lets rapid `Alt+Tab` presses switch back to your previous window instantaneously without flashing the overlay.
- **Pure Rust Native Rendering**: Zero Electron/WebView overhead — Direct3D 11 swapchain + Direct2D hardware rendering, background `PrintWindow` (`PW_RENDERFULLCONTENT`) capture, and a dedicated low-level keyboard hook thread (`WH_KEYBOARD_LL`).

---

## Quick Start

### Build from Source

Requires Rust (`cargo`) on Windows (supports `x86_64-pc-windows-gnu` and `x86_64-pc-windows-msvc`).

```powershell
cargo test
cargo build --release
```

The optimized binary is written to `target\release\winscroller-alt-tab.exe`.

### Run Modes

```powershell
# Run in background with system tray icon & global Alt+Tab hook (prompts for UAC elevation by default)
.\target\release\winscroller-alt-tab.exe

# Run without UAC elevation prompt (cannot intercept Alt+Tab over elevated Admin windows)
.\target\release\winscroller-alt-tab.exe --no-elevate

# Replace an already-running instance and start fresh
.\target\release\winscroller-alt-tab.exe --replace

# Open directly into pinned Browse mode
.\target\release\winscroller-alt-tab.exe --browse

# Preview interactively with a 12-window / 8-workspace demo field
.\target\release\winscroller-alt-tab.exe --demo

# Render a 1920x1080 demo screenshot offscreen to PNG
.\target\release\winscroller-alt-tab.exe --render-demo preview.png
```

---

## Keybindings & Controls (Quick Summary)

See **[KEYBINDINGS.md](KEYBINDINGS.md)** for the full interactive reference, edge cases, and mouse/drag-and-drop gestures.

| Input | Action |
| --- | --- |
| `Alt + Tab` / `Alt + Shift + Tab` | Open in Hold mode on previous / oldest window; step deeper / shallower while holding `Alt` |
| Release `Alt` | Focus selected window and close |
| `Alt + \`` or `Ctrl + Alt + Space` | Toggle Browse mode (stays open without holding `Alt`) |
| `Space` *(Hold mode, empty filter)* | Pin Hold mode into Browse mode so releasing `Alt` keeps the overlay open |
| `Tab` / `Shift + Tab` | Step deeper / shallower (wraps) |
| `↓` / `↑` or Vertical Wheel | Step deeper / shallower |
| `→` / `←` or Horizontal Wheel | Jump to most recent window on next / previous Virtual Desktop |
| `PgDn` / `PgUp` | Step 5 windows deeper / shallower |
| `Home` / `End` | Jump to frontmost / oldest window |
| `1`–`9` *(when filter empty)* | Jump to most recent window on Virtual Desktop `1`–`9` |
| Type letters / numbers *(filter active)* | Filter windows live by title, app name, app ID, or workspace |
| `Backspace` / `Ctrl + Backspace` | Delete last filter character / clear filter |
| `Enter` | Focus selected window |
| `Esc` | Clear active filter, or cancel and close without switching |
| **Click** window card / caption / tile | Focus that window |
| **Click** empty workspace card | Switch directly to that Windows Virtual Desktop |
| **Click** `+ New` / `+ New desktop` | Create a new Virtual Desktop directly from the overlay |
| **Drag & Drop** window card or minimap tile | Move window to another Virtual Desktop card or onto `+ New desktop` |
| **Click / Drag** Sounding Line | Scrub directly to any window along the time gauge |

---

## Configuration

Settings live at `%APPDATA%\winscroller-alt-tab\config.toml` (also accessible from the system tray menu via **Open config.toml**). Changes reload automatically when the overlay opens.

```toml
[winscroller]
corner-radius = 10
border-width = 1.0
font-family = "Segoe UI"
font-scale = 1.0
show-scratchpads = true
open-on-single = false

# Optional hex color overrides (#rgb, #rrggbb, or #aarrggbb):
# foreground = "#f3f3f5"
# background = "#1c1c1e"
# accent = "#60cdff"
# urgent = "#ff6b6b"
```

---

## System Tray & Elevation (UIPI)

Right-click the **Winscroller Alt+Tab** system tray icon to:
- **Open Winscroller Alt+Tab (Browse)** *(or left-click tray icon)*
- **Open config.toml**
- **Start with Windows** *(registers/removes `HKCU\Software\Microsoft\Windows\CurrentVersion\Run\WinscrollerAltTab`)*
- **Restart as Administrator (for Admin apps)** *(shown when running unelevated)*
- **Exit Winscroller Alt+Tab**

Because Windows User Interface Privilege Isolation (UIPI) blocks unelevated `WH_KEYBOARD_LL` hooks and `PrintWindow` captures when an elevated window (such as Task Manager or an Administrator terminal) is focused, Winscroller Alt+Tab requests elevation at startup by default (disable with `--no-elevate`).

---

## Documentation

- [KEYBINDINGS.md](KEYBINDINGS.md) — Complete keyboard, mouse, filter, and drag-and-drop reference
- [RELEASE_NOTES.md](RELEASE_NOTES.md) — GitHub Release Notes (`v0.1.0`)
- [CHANGELOG.md](CHANGELOG.md) — Version history
- [CONTRIBUTING.md](CONTRIBUTING.md) — Architecture layout, building, and testing
- [LICENSE](LICENSE) — MIT License

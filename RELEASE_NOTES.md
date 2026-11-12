# Winscroller Alt+Tab `v0.1.0` — Release Notes

First public release of **Winscroller Alt+Tab** — a hardware-accelerated, spatial-temporal `Alt+Tab` window switcher and Virtual Desktop overview for Windows 10 & Windows 11, built completely in pure Rust (`windows` 0.58, Direct3D 11, Direct2D, DirectWrite, WIC, Win32) and visually styled after [`mtolhuys/fathom`](https://github.com/mtolhuys/fathom).

---

## What's Included in `v0.1.0`

### Spatial-Temporal Window Switcher (**The Deep**)
- Replaces Windows' default `Alt+Tab` grid with a z-depth plane field sorted by focus recency (`depth = log2(1 + seconds / 30)` up to 8 fathoms).
- Smooth `220ms` `OutCubic` camera gliding, depth-based scale & fog attenuation, and rounded preview cards displaying app icon, window title, relative time (`just now`, `45 s ago`, `3 min ago`, `earlier`), and fathom depth.
- **90ms Hold Reveal Delay:** Rapid `Alt+Tab` switching happens silently without flashing the overlay; holding `Alt` past 90ms fades the full overlay smoothly into view.
- **Browse Mode (`Alt+\`` or `Ctrl+Alt+Space`):** Pin the full switcher open for searching, scrubbing history, or organizing Virtual Desktops.

### The Sounding Line (Interactive Time Gauge)
- Vertical depth gauge along the left rail (`now` / `≈` through `30s`, `1m`, `3m`, `7m`, `15m`, `31m`, `1h`, `2h+`) on displays `>= 1000px` wide.
- Plots every visible window as a dot (filled when tracked live, hollow when estimated from initial Z-order) with an animated lead diamond tracking your active depth.
- Click or drag anywhere along the Sounding Line to scrub directly through window recency.

### Virtual Desktop Workspace Map, Desktop Creation & Drag-and-Drop
- Live **Workspace Map** across the bottom of the screen mirrors all Windows Virtual Desktops (`1`, `2`, custom desktop names, plus `on screen` indicator) with scaled window geometry, cached previews, and app icons.
- **Create Desktops on the Page:** Click **`+ New`** / **`+ New desktop`** to create a new Windows Virtual Desktop (`Win+Ctrl+D` orchestrated cleanly while preserving your overlay session).
- **Drag-and-Drop Window Organization:** Grab any window card in The Deep or any minimap tile and drop it onto another Virtual Desktop card (or onto **`+ New desktop`**) — moves arbitrary cross-process windows on Windows 11 via `IApplicationViewCollection` + `IVirtualDesktopManagerInternal` (with `IVirtualDesktopManager` fallback).
- **Empty Desktop Switching:** Click any empty desktop card to jump directly to that Virtual Desktop.

### Single Plain Windows Theme
- Automatically adapts to Windows Light/Dark mode (`AppsUseLightTheme`) and your Windows accent color (`DWM\AccentColor`) with WCAG 2 contrast verification across text, cards, and markers.
- Reloads configuration live from `%APPDATA%\winscroller-alt-tab\config.toml`.

### Capture & System Integration
- Asynchronous background `PrintWindow` (`PW_RENDERFULLCONTENT`) capture thread with a 24-snapshot LRU cache (`last seen X ago` retained for minimized or background-desktop windows).
- Dedicated `WH_KEYBOARD_LL` hook thread (`winscroller-kb-hook`) prevents Windows low-level hook timeouts during rendering and passes modifier transitions cleanly so `Alt` never gets stuck down.
- System tray icon with Browse launch, config editor shortcut, `Start with Windows` autostart toggle, and Administrator relaunch option.
- Automatic UAC elevation request at startup (optional `--no-elevate`) so `Alt+Tab` interception and live previews work over elevated windows (Task Manager, Admin terminals, etc.).

---

## Quick Start

1. Download or build `winscroller-alt-tab.exe`:
   ```powershell
   cargo build --release
   ```
2. Launch:
   ```powershell
   .\target\release\winscroller-alt-tab.exe
   ```
3. Press `Alt+Tab` to quick-switch, press `Space` while holding `Alt` to pin open, or press `Alt+\`` / `Ctrl+Alt+Space` for Browse mode.

Full keybindings and gestures: see **[KEYBINDINGS.md](KEYBINDINGS.md)**.

---

## CLI Options

| Flag | Purpose |
| --- | --- |
| *(none)* | Run in background with system tray icon and global hooks |
| `--browse` | Open immediately in Browse mode (or wake an already-running instance) |
| `--no-elevate` | Run as current user without UAC elevation prompt |
| `--replace` | Cleanly terminate any existing instance before starting |
| `--demo` | Open interactive demo preview with 12 windows across 8 workspaces |
| `--render-demo <path>` | Export an offscreen `1920x1080` demo PNG and exit |

---

## Requirements & Notes

- **OS:** Windows 10 or Windows 11 (x64). Cross-process window dragging is validated on Windows 11 builds exposing `IVirtualDesktopManagerInternal` / `IApplicationViewCollection` (including 23H2/24H2/25H2+); standard `IVirtualDesktopManager` acts as fallback.
- **UIPI / Admin Apps:** If launched with `--no-elevate`, Windows prevents intercepting `Alt+Tab` and capturing previews when an elevated Administrator window currently has focus.

---

## Verification (`v0.1.0`)

- `cargo test`: 13/13 unit tests passing (depth/fog curves, recency ordering, layout math, minimap viewport normalization, plain Windows theme contrast guarantees, controller Hold/Browse timers, Virtual Desktop creation & drag/move state logic).
- `cargo build --release`: clean build (`0` warnings) targeting `x86_64-pc-windows-gnu` (LTO + stripped binary).
- `--render-demo`: verified offscreen `1920x1080` D3D11/Direct2D/WIC PNG export.

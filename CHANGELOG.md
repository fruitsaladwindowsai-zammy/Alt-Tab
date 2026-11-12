# Changelog

All notable changes to **Winscroller Alt+Tab** will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-11-12

### Added
- **The Deep** spatial-temporal `Alt+Tab` window switcher with logarithmic fathom depth (`log2(1 + seconds / 30)`), exponential fog, and `OutCubic` camera animation.
- **The Sounding Line** interactive vertical depth gauge (`now` to `2h+`) with click/drag time scrubbing and live/estimated markers.
- **The Workspace Map** bottom bar showing Windows Virtual Desktops with real minimap geometry, cached previews, and `on screen` state.
- **`+ New` / `+ New desktop` card** to create new Windows Virtual Desktops directly from the overlay page without losing your place.
- **Drag-and-Drop between Virtual Desktops:** drag any window plane or minimap tile onto another desktop card or onto `+ New desktop` (uses `IApplicationViewCollection` + `IVirtualDesktopManagerInternal` on Windows 11 with `IVirtualDesktopManager` fallback).
- **Empty desktop click-to-switch:** clicking an empty Virtual Desktop card switches directly to that desktop.
- **Plain Windows Adaptive Theme:** automatically follows Windows light/dark (`AppsUseLightTheme`) and `DWM\AccentColor` with WCAG 2 contrast guarantees and live TOML config reload (`%APPDATA%\winscroller-alt-tab\config.toml`).
- **Dedicated `WH_KEYBOARD_LL` & `SetWinEventHook` thread** (`winscroller-kb-hook`) for responsive `Alt+Tab` / `Alt+Shift+Tab` / `Alt+\`` / `Ctrl+Alt+Space` capture, 90ms zero-flash Hold reveal delay, and `15s` watchdog recovery.
- **Background `PrintWindow` capture worker** with 24-snapshot LRU cache, shell/file icon extraction, and blurry desktop wallpaper backdrop.
- **System tray integration** (Browse, `open config.toml`, `Start with Windows` registry toggle, `Restart as Administrator`, Exit) and CLI modes (`--browse`, `--demo`, `--render-demo`, `--replace`, `--no-elevate`).

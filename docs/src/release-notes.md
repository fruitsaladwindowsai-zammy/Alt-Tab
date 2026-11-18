# Release Notes (`v0.1.0`)

First public release of **Winscroller Alt+Tab** — a pure Rust, Direct3D 11 + Direct2D spatial-temporal `Alt+Tab` window switcher and Virtual Desktop organizer for Windows 10 and 11.

---

## Highlights

- **The Deep:** Logarithmic recency depth stack (`depth = log2(1 + seconds / 30)` up to 8 fathoms), `220ms OutCubic` camera gliding, and exponential depth fog (`0%`–`64%`).
- **The Sounding Line:** Interactive left-rail time gauge (`now` through `2h+`) with live vs. startup-estimated window dots and click/drag scrubbing.
- **Workspace Map & Drag-and-Drop:** Live bottom Virtual Desktop minimap with on-page **`+ New desktop`** creation, empty desktop switching, and cross-process window drag-and-drop on Windows 11 (`IApplicationViewCollection` + `IVirtualDesktopManagerInternal` with `IVirtualDesktopManager` fallback).
- **Zero-Flash `90ms` Hold Reveal:** Instant switching when tapping `Alt+Tab` under 90ms; full overlay fade-in (`130ms`) when held, plus `Space` pinning and `15s` watchdog recovery.
- **Adaptive Plain Windows Theme:** Automatically tracks `AppsUseLightTheme` and `DWM\AccentColor` with WCAG 2 contrast verification (`5.5:1` / `4.5:1` / `3.0:1`) and live TOML reload.
- **CLI & System Tray:** Includes `--browse`, `--demo`, `--no-elevate`, `--replace`, `--quit`, `--init-config`, `--validate`, `--render-demo`, `--render-gallery`, and tray autostart toggle.

## Verification

- `cargo fmt -- --check`: clean
- `cargo clippy --all-targets`: `0` warnings
- `cargo test`: `14/14` passing
- `cargo build --release`: clean LTO + stripped Windows x64 binary

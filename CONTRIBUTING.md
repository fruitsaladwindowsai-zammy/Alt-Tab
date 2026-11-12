# Contributing to Winscroller Alt+Tab

Thank you for helping improve **Winscroller Alt+Tab**!

## Prerequisites

- **OS:** Windows 10 or Windows 11 (x64)
- **Toolchain:** Rust stable (`x86_64-pc-windows-gnu` or `x86_64-pc-windows-msvc`)

No external C/C++ libraries are needed — all Win32, Direct3D 11, Direct2D, DirectWrite, WIC, DWM, and COM bindings come from the `windows` crate (`0.58`).

## Architecture Overview

| Module | Responsibility |
| --- | --- |
| `src/main.rs` | Win32 window loop, CLI flags, elevation/single-instance mutex, mouse & keyboard dispatch |
| `src/controller.rs` | Pure state machine: Hold/Browse modes, `90ms` reveal timer, `OutCubic` animations, filter, drag-and-drop |
| `src/renderer.rs` | Direct3D 11 swapchain, Direct2D/DirectWrite drawing, hit regions, WIC offscreen PNG export |
| `src/layout.rs` | Pure geometry math: deep stage planes, sounding-line marks, minimap normalization & overlap splitting |
| `src/depth.rs` | Logarithmic depth curve (`fathoms`) and exponential depth fog |
| `src/field.rs` | Filtering, human-readable age/depth formatting, Virtual Desktop grouping & navigation |
| `src/recency.rs` | Initial Z-order seeding and `SetWinEventHook` focus recency tracking |
| `src/win_hook.rs` | Dedicated `WH_KEYBOARD_LL` & `SetWinEventHook` background thread |
| `src/win_platform.rs` | Window enumeration, Virtual Desktop COM/Registry integration, desktop creation & window moving, icons, elevation |
| `src/win_capture.rs` | Async background `PrintWindow` (`PW_RENDERFULLCONTENT`) capture thread, LRU snapshot store, wallpaper blur |
| `src/palette.rs` | Single plain Windows light/dark theme derivation with WCAG 2 contrast floors |
| `src/config.rs` | `%APPDATA%\winscroller-alt-tab\config.toml` parser & live reload |
| `src/tray.rs` | System tray icon, popup menu, and autostart registry toggle |
| `src/demo.rs` | Deterministic 12-window / 8-workspace demo dataset for `--demo` & `--render-demo` |

## Development Workflow

```powershell
# Run all unit tests
cargo test

# Build optimized release binary
cargo build --release

# Visually verify UI changes with offscreen PNG export (no hooks installed)
.\target\release\winscroller-alt-tab.exe --render-demo preview.png

# Launch interactive demo window
.\target\release\winscroller-alt-tab.exe --demo
```

### Guidelines

- Keep core state/layout/depth/palette math platform-independent inside `controller.rs`, `layout.rs`, `depth.rs`, `field.rs`, and `palette.rs` with unit tests.
- Never run blocking Direct2D rendering or `PrintWindow` calls inside `ll_keyboard_proc` (`win_hook.rs`); keep the hook thread strictly O(1) atomic checks + `PostMessageW`.
- Always verify `cargo test` and `cargo build --release` compile with zero warnings before submitting a PR.

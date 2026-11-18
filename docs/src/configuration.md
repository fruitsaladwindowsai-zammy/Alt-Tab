# Configuration Guide

Winscroller Alt+Tab reads its configuration from:

```text
%APPDATA%\winscroller-alt-tab\config.toml
```

Changes reload **live** the next time the overlay opens — no restart required.

---

## Initialize & Validate

```powershell
# Create default config.toml (does not overwrite if already present)
.\target\release\winscroller-alt-tab.exe --init-config

# Validate keys, types, numeric ranges, and hex colors
.\target\release\winscroller-alt-tab.exe --validate
```

You can also open the file directly from the tray menu: **Right-click tray icon → Open config.toml**.

---

## Complete Options Reference (`[winscroller]`)

Both `[winscroller]` and legacy `[fathom]` headers are supported, as well as both `kebab-case` and `snake_case` key names.

| Key | Type | Default | Range / Format | Description |
| :--- | :--- | :--- | :--- | :--- |
| `corner-radius` | float | `10.0` | `>= 0.0` | Corner radius (px) for cards, filter pill & map cards |
| `border-width` | float | `1.0` | `>= 0.0` | Stroke width (px) for card outlines and borders |
| `font-family` | string | `"Segoe UI"` | Installed font name | DirectWrite typography family |
| `font-scale` | float | `1.0` | `> 0.0`, clamped `[0.7, 2.0]` | Global font scaling multiplier |
| `show-scratchpads` | bool | `true` | `true` \| `false` | Show special/scratchpad workspaces if present |
| `open-on-single` | bool | `false` | `true` \| `false` | Open Hold mode (`Alt+Tab`) even when only 1 window exists |
| `foreground` | hex | *(Windows auto)* | `#rgb`, `#rrggbb`, `#aarrggbb` | Optional foreground text color override |
| `background` | hex | *(Windows auto)* | `#rgb`, `#rrggbb`, `#aarrggbb` | Optional veil & card base background override |
| `accent` | hex | *(Windows DWM)* | `#rgb`, `#rrggbb`, `#aarrggbb` | Optional selection ring & marker accent override |
| `urgent` | hex | *(Windows auto)* | `#rgb`, `#rrggbb`, `#aarrggbb` | Optional flashing/urgent window indicator override |

---

## Example `config.toml`

```toml
[winscroller]
corner-radius = 10
border-width = 1.0
font-family = "Segoe UI"
font-scale = 1.0
show-scratchpads = true
open-on-single = false

# Optional hex color overrides:
# foreground = "#f3f3f5"
# background = "#1c1c1e"
# accent = "#60cdff"
# urgent = "#ff6b6b"
```

# Configuration Guide — Winscroller Alt+Tab

Winscroller Alt+Tab reads its preferences from a plain TOML configuration file at:

```text
%APPDATA%\winscroller-alt-tab\config.toml
```

Changes reload **live** — there is no need to restart the application. The next time you open or interact with the overlay, `winscroller-alt-tab.exe` checks the file's modification timestamp and refreshes typography, layout options, and colors automatically.

---

## Quick Setup & Validation

```powershell
# Generate a default config.toml (safe — never overwrites an existing file)
.\target\release\winscroller-alt-tab.exe --init-config

# Validate syntax, key names, numeric bounds, and hex color formats
.\target\release\winscroller-alt-tab.exe --validate
```

You can also open the file directly from the Windows system tray by right-clicking the **Winscroller Alt+Tab** tray icon and clicking **Open config.toml**.

See **[winscroller-alt-tab.example.toml](winscroller-alt-tab.example.toml)** for a fully commented reference template.

---

## Section Headers & Key Syntax

- **Accepted sections:** `[winscroller]` (recommended) or `[fathom]` (legacy compatibility). Keys placed before any section header are also accepted.
- **Key naming:** Both `kebab-case` (e.g. `corner-radius`) and `snake_case` (e.g. `corner_radius`) are accepted case-insensitively.
- **Comments:** `#` begins a comment anywhere outside quoted strings (e.g. `accent = "#60cdff" # cyan`).

---

## Complete Configuration Schema

### `[winscroller]`

| Key | Type | Default | Valid Range / Behavior | Description |
| :--- | :--- | :--- | :--- | :--- |
| `corner-radius` | float | `10.0` | `>= 0.0` | Rounded corner radius in logical pixels for window cards, filter pill, and workspace map cards. |
| `border-width` | float | `1.0` | `>= 0.0` | Border stroke width in logical pixels for window frames and active outlines. |
| `font-family` | string | `"Segoe UI"` | Non-empty installed Windows font name | DirectWrite font family used for window titles, timestamps, fathom labels, and key hints. |
| `font-scale` | float | `1.0` | `> 0.0` (clamped to `[0.7, 2.0]`) | Typography scale multiplier applied on top of per-monitor DPI scaling. |
| `show-scratchpads` | bool | `true` | `true` \| `false` | Controls whether special/scratchpad workspace groups (`special:*`) are shown in the field and Workspace Map. |
| `open-on-single` | bool | `false` | `true` \| `false` | When `false`, Hold mode (`Alt+Tab`) requires at least 2 windows to open. Set `true` to open even with a single window. Browse mode (`Alt+\`` / `Ctrl+Alt+Space`) always opens. |
| `foreground` | hex color | *(Windows auto)* | `#rgb`, `#rrggbb`, `#aarrggbb` | Optional override for primary foreground/text color. |
| `background` | hex color | *(Windows auto)* | `#rgb`, `#rrggbb`, `#aarrggbb` | Optional override for backdrop veil and card base surface color. |
| `accent` | hex color | *(Windows DWM accent)* | `#rgb`, `#rrggbb`, `#aarrggbb` | Optional override for selection rings, Sounding Line lead diamond, and active highlights. |
| `urgent` | hex color | *(Windows auto)* | `#rgb`, `#rrggbb`, `#aarrggbb` | Optional override for flashing/urgent window indicator (`HSHELL_FLASH`). |

---

## Adaptive Plain Windows Theme & WCAG 2 Contrast Guarantees

By default (when `foreground`, `background`, `accent`, and `urgent` are omitted), Winscroller Alt+Tab queries Windows directly:

1. **System Polarity:** Reads `HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize\AppsUseLightTheme`.
   - **Dark Mode (`0`):** Foreground `#f3f3f5`, Background `#1c1c1e`, Default Accent `#60cdff`, Urgent `#ff6b6b`.
   - **Light Mode (`1`):** Foreground `#1b1b1f`, Background `#f3f3f3`, Default Accent `#0067c0`, Urgent `#c42b1c`.
2. **Windows Accent Color:** Reads `HKCU\Software\Microsoft\Windows\DWM\AccentColor` (or `ColorizationColor`) so selection highlights automatically match your taskbar and titlebar accent.
3. **Automatic Contrast Solver (`src/palette.rs`):** Regardless of whether colors come from Windows or your custom hex overrides, every derived color is verified against WCAG 2 relative luminance contrast floors:

| Derived Role | Minimum Contrast Ratio | Verified Against |
| :--- | :---: | :--- |
| `text_soft` *(subtitles, metadata)* | **5.5 : 1** | Background (`bg`) |
| `text_soft` *(selected card header)* | **4.5 : 1** | Selected card top surface |
| `text_faint` *(key hints, timestamps)* | **4.5 : 1** | Background (`bg`) |
| `accent_text` *(active text labels)* | **4.5 : 1** | Background (`bg`) |
| `accent` *(borders, rings, markers)* | **3.0 : 1** | Background (`bg`) |
| `urgent` *(flashing window badges)* | **3.0 : 1** | Background (`bg`) |

If an accent or foreground color falls short of its contrast floor, the solver steps it progressively toward pure white (dark mode) or pure black (light mode) until the floor is satisfied.

---

## Example Configurations

### Compact High-Density Developer Setup
```toml
[winscroller]
corner-radius = 6
border-width = 1.0
font-family = "Cascadia Code"
font-scale = 0.95
show-scratchpads = true
open-on-single = false
```

### High-Contrast Custom Accent Override
```toml
[winscroller]
corner-radius = 12
border-width = 1.5
font-family = "Segoe UI Variable Display"
font-scale = 1.15
accent = "#00f5d4"
urgent = "#ff5c8a"
```

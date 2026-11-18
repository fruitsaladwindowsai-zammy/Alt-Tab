# Usage, CLI Flags & System Tray Guide

Complete operational reference for launching, controlling, updating, and automating **Winscroller Alt+Tab** on Windows 10 and 11.

---

## Command-Line Interface (CLI) Reference

Run `winscroller-alt-tab.exe` from PowerShell, Windows Terminal, or a Windows shortcut with any of the following flags:

| Command | Behavior |
| :--- | :--- |
| `winscroller-alt-tab.exe` | Launches in the background with a system tray icon, global `WH_KEYBOARD_LL` hook, and `SetWinEventHook` recency tracker. Requests UAC elevation automatically unless already elevated or `--no-elevate` is passed. If another instance is already running, wakes it into **Browse** mode and exits. |
| `winscroller-alt-tab.exe -c <path>` / `--config <path>` | Uses a custom `config.toml` path instead of `%APPDATA%\winscroller-alt-tab\config.toml` (works with `--init-config`, `--validate`, tray editing, and live reload). |
| `winscroller-alt-tab.exe --browse` | Starts the switcher and immediately opens the overlay pinned in **Browse** mode (or wakes an existing running instance into Browse mode). |
| `winscroller-alt-tab.exe --no-elevate` (`--non-elevated`) | Runs at standard user (Medium Integrity) privileges without requesting a UAC Administrator prompt. *(Note: Windows UIPI blocks `Alt+Tab` interception and live capture while an elevated Admin window has focus).* |
| `winscroller-alt-tab.exe -r` / `--replace` | Cleanly signals any already-running instance (`WM_FATHOM_QUIT`), waits up to 500ms for graceful shutdown, and starts a fresh instance. Ideal after rebuilding from source. |
| `winscroller-alt-tab.exe --quit` | Sends a graceful shutdown message (`WM_FATHOM_QUIT`) to any running instance and exits immediately. |
| `winscroller-alt-tab.exe --init-config` | Creates a default `%APPDATA%\winscroller-alt-tab\config.toml` if one does not already exist and prints its path. |
| `winscroller-alt-tab.exe --validate` | Parses and strictly validates `%APPDATA%\winscroller-alt-tab\config.toml`, reporting line numbers for unknown sections, invalid keys, out-of-range numbers, or malformed hex colors. Exits `0` if valid, `1` on error. |
| `winscroller-alt-tab.exe --demo` | Opens a standalone interactive preview window populated with 12 deterministic mock windows across 8 workspaces (`1`–`7` plus `special:scratchpad`). Does **not** install global keyboard hooks or touch your real windows. |
| `winscroller-alt-tab.exe --render-demo <path>` | Renders a `1920x1080` offscreen Direct3D 11 / Direct2D PNG of the demo scene via WIC and exits (default output: `winscroller_demo.png`). |
| `winscroller-alt-tab.exe --render-gallery <dir>` | Renders all 5 high-resolution feature screenshots and 13 animation frames (`<dir>/frames/frame_00.png`..`frame_12.png`) and exits. |
| `winscroller-alt-tab.exe --version` (`-V`) | Prints the binary version and exits. |
| `winscroller-alt-tab.exe --help` (`-h`) | Prints CLI usage summary and exits. |

---

## Single-Instance Synchronization

Winscroller Alt+Tab registers a session-scoped named mutex (`Local\WinscrollerAltTabMutex`):

- Launching `winscroller-alt-tab.exe` (or `winscroller-alt-tab.exe --browse`) while an instance is already running does **not** spawn a duplicate process; instead, it posts `WM_FATHOM_OPEN_BROWSE` to the active instance to open Browse mode immediately.
- Passing `--replace` overrides this by gracefully closing the running instance first so you can swap in a freshly compiled binary seamlessly.

---

## System Tray Icon & Menu

Once running in the background, Winscroller Alt+Tab displays a diamond-and-ring tray icon in the Windows notification area:

- **Left-Click Tray Icon:** Opens Winscroller Alt+Tab immediately in pinned **Browse** mode.
- **Right-Click Tray Icon:** Opens the context menu:
  1. **Open Winscroller Alt+Tab (Browse)** — Opens the overlay in Browse mode.
  2. **Open config.toml** — Ensures `%APPDATA%\winscroller-alt-tab\config.toml` exists and opens it in your default text editor.
  3. **Start with Windows** — Toggles automatic startup by adding/removing `HKCU\Software\Microsoft\Windows\CurrentVersion\Run\WinscrollerAltTab` pointing to the current executable path.
  4. **Restart as Administrator (for Admin apps)** — *(Shown only when running non-elevated)* Relaunches elevated with `--replace` so Admin windows are supported.
  5. **Exit Winscroller Alt+Tab** — Cleanly unhooks `WH_KEYBOARD_LL` and `SetWinEventHook`, removes the tray icon, and exits.

---

## Hold Mode vs. Browse Mode

### Hold Mode (`Alt + Tab` / `Alt + Shift + Tab`)
- Designed for muscle-memory zero-latency switching.
- **Under 90ms (Quick Tap):** Pressing and releasing `Alt+Tab` rapidly focuses the previous window immediately with **zero visual flash**.
- **Over 90ms (Hold Reveal):** Keeping `Alt` held past 90ms fades in The Deep, The Sounding Line, and the Workspace Map (`130ms` `OutCubic`).
- **Pre-Reveal Interception:** `Esc`, `Space`, and `Tab` pressed during the 90ms grace window are captured so they never leak into the active application.
- **Pinning:** Press `Space` (with an empty filter) while holding `Alt` to convert Hold mode into Browse mode so releasing `Alt` keeps the overlay open.
- **Watchdog Safety:** If a UAC screen or lock event ever interrupts an `Alt` release, a `15s` inactivity watchdog plus a `12ms` `GetAsyncKeyState` poll safely dismisses Hold mode so keys never stick.

### Browse Mode (`Alt + \`` or `Ctrl + Alt + Space`)
- Opens immediately with no reveal delay and stays open hands-free.
- Ideal for typing live search queries, scrubbing the Sounding Line time gauge, clicking `+ New desktop`, or dragging and dropping windows between Virtual Desktops.
- Dismiss with `Enter` (focus selection), `Esc` (cancel), or clicking empty space.

---

## Clean Update Workflow (From Source)

Because Windows locks a running `.exe` while it is active, stop the background instance before rebuilding:

```powershell
# 1. Gracefully exit any running instance
.\target\release\winscroller-alt-tab.exe --quit

# 2. Pull latest code
git pull origin main

# 3. Compile optimized release binary
cargo build --release

# 4. Launch fresh instance
.\target\release\winscroller-alt-tab.exe --replace
```

---

## Crash Diagnostics (`crash.log`)

If an unexpected panic ever occurs, Winscroller Alt+Tab's panic hook automatically appends the Unix timestamp, panic message, and full backtrace to:

```text
%APPDATA%\winscroller-alt-tab\crash.log
```

Attach relevant entries from `crash.log` when filing a [Bug Report](https://github.com/fruitsaladwindowsai-zammy/Alt-Tab/issues/new?template=bug_report.md).

# Administrator & Elevated Apps (Windows UIPI)

Windows enforces **User Interface Privilege Isolation (UIPI)** to prevent standard (Medium Integrity) applications from intercepting keystrokes or capturing graphics from elevated (High Integrity / Administrator) windows such as **Task Manager**, **Registry Editor**, **Device Manager**, or **Administrator PowerShell / Terminal**.

---

## Automatic UAC Elevation

By default, when you launch `winscroller-alt-tab.exe`:

1. It checks `win_platform::is_process_elevated()`.
2. If running at Medium Integrity (and neither `--demo` nor `--no-elevate` was passed), it triggers Windows UAC via `ShellExecuteW("runas")` forwarding `--replace`.
3. Once elevated to High Integrity Level:
   - `Alt+Tab`, `Alt+Shift+Tab`, `Alt+\``, and `Ctrl+Alt+Space` are intercepted reliably **even when Task Manager or an Admin terminal has focus**.
   - `PrintWindow(PW_RENDERFULLCONTENT)` captures live previews of Administrator windows without blank frames.
   - Dragging and dropping elevated windows across Virtual Desktops works seamlessly.

---

## Running Without Elevation (`--no-elevate`)

If you prefer not to grant Administrator privileges (for example, on a restricted corporate laptop):

```powershell
.\target\release\winscroller-alt-tab.exe --no-elevate
```

**Trade-offs when non-elevated:**
- Switching between standard user apps works normally.
- While an elevated Administrator window is actively focused, Windows UIPI blocks `WH_KEYBOARD_LL` from receiving `Alt+Tab`, so Windows' default switcher appears for that window.
- You can upgrade to Administrator anytime from the system tray menu by clicking **Restart as Administrator (for Admin apps)**.

---

## Security Boundary

- Winscroller Alt+Tab opens **zero network sockets** and performs **zero telemetry**.
- Single-instance wake-up uses a session-local `Local\WinscrollerAltTabMutex` and local HWND `PostMessageW` only.
- See [SECURITY.md](https://github.com/fruitsaladwindowsai-zammy/Alt-Tab/blob/main/SECURITY.md) for the full vulnerability disclosure policy.

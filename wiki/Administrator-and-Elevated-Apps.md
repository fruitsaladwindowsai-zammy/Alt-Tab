# Administrator & Elevated Apps (Windows UIPI)

Windows **User Interface Privilege Isolation (UIPI)** blocks standard (Medium Integrity) apps from intercepting keyboard hooks or capturing previews of Administrator (High Integrity) windows such as **Task Manager** or **Administrator PowerShell**.

---

## Automatic UAC Elevation

When you run `winscroller-alt-tab.exe`:
- If not already elevated (and `--no-elevate` / `--demo` are not set), it requests Administrator elevation via UAC (`ShellExecuteW("runas")`) forwarding `--replace`.
- With High Integrity Level, `Alt+Tab` interception, `PrintWindow` previews, and Virtual Desktop drag-and-drop work seamlessly across **all** applications, including Admin windows.

## Opting Out (`--no-elevate`)

```powershell
.\target\release\winscroller-alt-tab.exe --no-elevate
```

You can also elevate on demand later by right-clicking the tray icon and choosing **Restart as Administrator (for Admin apps)**.

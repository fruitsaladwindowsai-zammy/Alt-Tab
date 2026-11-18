# Security Policy

## Supported Versions

| Version | Supported          |
| ------- | ------------------ |
| 0.1.x   | :white_check_mark: |
| < 0.1.0 | :x:                |

---

## Reporting a Vulnerability

Security issues and vulnerabilities should be reported responsibly. Please do **not** open public GitHub issues for security vulnerabilities.

### Reporting Process
1. Open a private security advisory on GitHub via **Security** > **Advisories** > **Report a vulnerability**.
2. Include:
   * Description of the vulnerability and impact.
   * Minimal reproduction steps or proof-of-concept (PoC).
   * Your Windows build version (e.g. Windows 11 23H2 / 24H2 / 25H2).
3. We will acknowledge receipt of your report within 48 hours and work on a coordinated fix and advisory.

---

## Privileges & Integrity Model

* **Optional UAC Elevation (High Integrity Level):** Winscroller Alt+Tab requests Administrator elevation on startup (bypassable via `--no-elevate`) solely so Windows User Interface Privilege Isolation (UIPI) permits its low-level keyboard hook (`WH_KEYBOARD_LL`) and live preview capture (`PrintWindow`) to work seamlessly while elevated windows such as Task Manager or Administrator terminals are focused.
* **Zero Network Surface:** Winscroller Alt+Tab opens no network sockets, sends no telemetry, and operates 100% locally via Win32, Direct3D 11, Direct2D, DirectWrite, WIC, and local COM APIs.
* **Local-Only Single-Instance Synchronization:** Uses a session-scoped `Local\WinscrollerAltTabMutex` and local `PostMessageW` notifications on the overlay HWND.

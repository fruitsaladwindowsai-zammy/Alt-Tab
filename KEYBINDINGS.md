# Winscroller Alt+Tab — Keybindings & Controls

Complete reference for global hotkeys, overlay navigation, filtering, Virtual Desktop controls, and mouse/drag-and-drop interactions.

---

## 1. Global Shortcuts

Installed globally via a dedicated `WH_KEYBOARD_LL` hook thread (`winscroller-kb-hook`):

| Shortcut | Mode | Behavior |
| --- | --- | --- |
| `Alt + Tab` | Hold Mode | Captures `Tab` so Windows' default Task Switcher does not appear; opens on the **previous** (`2nd` most recent) window. Each additional `Tab` steps 1 window deeper. |
| `Alt + Shift + Tab` | Hold Mode | Opens stepping backward to the **oldest** window; subsequent `Shift + Tab` steps shallower. |
| Release `Alt` *(in Hold mode)* | Hold Mode | Commits focus to the selected window and closes the overlay. If `Alt` is released within the **90ms pre-reveal delay**, the switch executes with zero visual flash. |
| `Alt + \`` *(backtick)* | Browse Toggle | Opens the overlay pinned in **Browse** mode (no need to keep `Alt` held), or closes if already open. |
| `Ctrl + Alt + Space` | Browse Toggle | Alternate shortcut to open/close in **Browse** mode. |

> **Pre-reveal protection (`< 90ms`):** While holding `Alt` before the overlay visually reveals, `Esc`, `Space`, and `Tab` are intercepted immediately so keystrokes never leak into the active background application.  
> **Watchdog protection:** In Hold mode, if `Alt` release is ever missed externally, an automatic `15s` input-inactivity watchdog cancels Hold mode safely so keys never get stuck.

---

## 2. Overlay Navigation Keys

Available once the overlay is open (revealed Hold mode or Browse mode):

| Key | Action | Wrapping / Bounds |
| --- | --- | --- |
| `Tab` | Step 1 window deeper (older) | Wraps from oldest back to newest |
| `Shift + Tab` | Step 1 window shallower (newer) | Wraps from newest back to oldest |
| `↓` *(Down Arrow)* | Step 1 window deeper | Clamps at oldest visible window |
| `↑` *(Up Arrow)* | Step 1 window shallower | Clamps at newest visible window |
| `PgDn` *(Page Down)* | Jump 5 windows deeper | Clamps at oldest visible window |
| `PgUp` *(Page Up)* | Jump 5 windows shallower | Clamps at newest visible window |
| `Home` | Jump to newest (frontmost) visible window | First item in filtered order |
| `End` | Jump to oldest visible window | Last item in filtered order |
| `→` *(Right Arrow)* | Jump to the frontmost window of the **next** Virtual Desktop | Wraps across occupied desktops |
| `←` *(Left Arrow)* | Jump to the frontmost window of the **previous** Virtual Desktop | Wraps across occupied desktops |
| `1` – `9` | Jump to the frontmost window on Virtual Desktop `1`–`9` | Active when search filter is empty |
| `Space` *(Hold mode, empty filter)* | Pin overlay into **Browse** mode (stays open after `Alt` is released) | — |
| `Enter` | Focus selected window (switches Virtual Desktop first if needed) and close | — |
| `Esc` | Clear active filter if non-empty; otherwise cancel and close without changing focus | — |

---

## 3. Live Search / Filter

Start typing immediately when the overlay is visible to narrow **The Deep** and highlight matches across the Workspace Map:

| Input | Action |
| --- | --- |
| Printable characters *(a–z, punctuation, numbers when filter non-empty)* | Appends character to the query; matches case-insensitively across **window title**, **application name**, **executable/app ID**, and **Virtual Desktop name/label**. |
| `Space` *(when filter non-empty)* | Appends a space to the filter query. |
| `Backspace` | Removes the last character from the filter query. |
| `Ctrl + Backspace` | Clears the entire filter query in one step. |
| `Esc` *(when filter non-empty)* | Clears the filter without closing the overlay. |

---

## 4. Mouse, Sounding Line & Drag-and-Drop

| Gesture | Target | Action |
| --- | --- | --- |
| **Left Click** | Window plane in The Deep | Focuses that window and closes |
| **Left Click** | Caption bar below active card | Focuses the currently selected window and closes |
| **Left Click** | Minimap window tile | Focuses that window and closes |
| **Left Click** | Occupied Virtual Desktop card *(outside tiles)* | Focuses the frontmost visible window on that desktop |
| **Left Click** | Empty Virtual Desktop card | Switches Windows directly to that Virtual Desktop and closes |
| **Left Click** | **`+ New`** / **`+ New desktop`** card | Creates a new Windows Virtual Desktop right on the page without dismissing the overlay |
| **Drag & Drop** *(`>= 6px` move)* | Window plane *(The Deep)* or Minimap tile **→** Virtual Desktop card | Moves that window directly onto the target Virtual Desktop |
| **Drag & Drop** *(`>= 6px` move)* | Window plane *(The Deep)* or Minimap tile **→** **`+ New desktop`** card | Creates a new Windows Virtual Desktop and moves the window onto it |
| **Hover** *(`>= 3px` move)* | Minimap window tile | Selects & brings that window into focus in The Deep |
| **Click / Drag** | Sounding Line gauge *(left rail, shown when width ≥ 1000px)* | Scrubs selection directly to the window nearest the cursor's depth position |
| **Vertical Mouse Wheel** | Anywhere in overlay | Steps deeper (scroll down) or shallower (scroll up) |
| **Horizontal Mouse Wheel** | Anywhere in overlay | Steps across Virtual Desktops |
| **Left Click** | Empty background area | Cancels and closes without switching focus |

---

## 5. System Tray Controls

- **Left-click tray icon:** Opens Winscroller Alt+Tab in Browse mode.
- **Right-click tray icon:**
  - **Open Winscroller Alt+Tab (Browse)**
  - **Open config.toml**
  - **Start with Windows** *(toggles Run registry entry)*
  - **Restart as Administrator (for Admin apps)** *(shown if not running elevated)*
  - **Exit Winscroller Alt+Tab**

# Winscroller Alt+Tab Wiki Repository

This directory contains the official wiki for **Winscroller Alt+Tab**. It is formatted for direct display in GitHub and is fully compatible with GitHub Wiki (`.wiki.git`).

---

## 📚 Table of Contents

* [Home](Home.md) — Welcome & Overview
* [Getting Started](Getting-Started.md) — Prerequisites, Git clone, building & autostart
* [Core Concepts](Core-Concepts.md) — The Deep, Logarithmic Fathoms & Sounding Line
* [Keyboard Shortcuts](Keyboard-Shortcuts.md) — Global chords, overlay navigation, mouse & drag gestures
* [Virtual Desktops & Drag-and-Drop](Virtual-Desktops-and-Drag-Drop.md) — Workspace Map, `+ New desktop` & Windows 11 COM moves
* [Live Search & Filtering](Live-Search-and-Filtering.md) — Multi-field instant search & synchronized minimap highlights
* [Administrator & Elevated Apps](Administrator-and-Elevated-Apps.md) — Windows UIPI High Integrity Level elevation
* [Configuration Guide](Configuration-Guide.md) — `config.toml` schema, WCAG 2 contrast & live reload
* [CLI Flags & Tray Controls](CLI-Flags-and-Tray-Controls.md) — CLI options, single-instance sync & tray menu
* [Architecture & Internals](Architecture-and-Internals.md) — Pure-Rust D3D11/Direct2D & dedicated hook architecture
* [Troubleshooting & FAQ](Troubleshooting-and-FAQ.md) — Solutions to common build & runtime questions

---

## 🚀 How to Publish to GitHub Wiki

GitHub wikis live in a separate git repository linked to your main repository (`<repo>.wiki.git`).

To publish these pages directly to your GitHub repository's Wiki tab:

```powershell
# 1. Initialize a git repository inside the wiki directory
cd wiki
git init
git checkout -b master

# 2. Add all wiki markdown files and assets
git add .
git commit -m "Initial Winscroller Alt+Tab Wiki documentation"

# 3. Add your GitHub Wiki remote and push
git remote add origin https://github.com/fruitsaladwindowsai-zammy/Alt-Tab.wiki.git
git push -u origin master --force
```

*(Note: Enable the **Wiki** feature in your repository settings on GitHub under **Settings** > **Features** > **Wikis** before pushing.)*

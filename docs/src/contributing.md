# Contributing

Contributions, bug reports, and feature ideas are warmly welcome!

---

## Before Opening a Pull Request

Run the exact verification suite executed by Windows CI (`.github/workflows/ci.yml`):

```powershell
cargo fmt -- --check
cargo clippy --all-targets
cargo test
cargo check --release --tests
```

---

## Engineering Conventions

- **Keep Core Math Pure & Unit-Tested:** Depth curves (`depth.rs`), contrast floors (`palette.rs`), config parsing/validation (`config.rs`), layout geometry (`layout.rs`), filtering (`field.rs`), and overlay state transitions (`controller.rs`) must remain testable without creating Win32 HWNDs.
- **Never Block the Keyboard Hook:** Inside `ll_keyboard_proc` (`src/win_hook.rs`), perform only atomic checks and `PostMessageW`. Never invoke Direct2D, COM, or `PrintWindow` on the hook thread.
- **Keep Docs Synchronized:** Whenever a CLI flag, keybinding, or `config.toml` key changes, update `README.md`, `KEYBINDINGS.md`, `CONFIGURATION.md`, `USAGE.md`, `winscroller-alt-tab.example.toml`, `docs/src/`, `wiki/`, and `CHANGELOG.md`.

---

## Community Standards

- [Code of Conduct](https://github.com/fruitsaladwindowsai-zammy/Alt-Tab/blob/main/CODE_OF_CONDUCT.md)
- [Security Policy](https://github.com/fruitsaladwindowsai-zammy/Alt-Tab/blob/main/SECURITY.md)

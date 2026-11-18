# Building from Source

## Toolchain Support

Winscroller Alt+Tab compiles cleanly on **both** Windows Rust toolchains with zero external C/C++ libraries:

- `stable-x86_64-pc-windows-msvc` (with Visual Studio Build Tools)
- `stable-x86_64-pc-windows-gnu` (standalone MinGW — no Visual Studio required)

All Win32, Direct3D 11, Direct2D, DirectWrite, WIC, DWM, and COM interfaces are provided by the official Microsoft `windows` crate (`0.58`).

---

## Build & Verification Commands

```powershell
# Clone repository
git clone https://github.com/fruitsaladwindowsai-zammy/Alt-Tab.git
cd Alt-Tab

# Verify code formatting
cargo fmt -- --check

# Run Clippy static analysis
cargo clippy --all-targets

# Run all 14 unit tests
cargo test

# Build optimized release binary (opt-level = 3, LTO = true, strip = true)
cargo build --release
```

---

## Visual QA & Documentation Media Generation

You can render fresh 1920×1080 screenshots and animation frames headlessly without installing keyboard hooks:

```powershell
# Export single preview PNG
.\target\release\winscroller-alt-tab.exe --render-demo preview.png

# Export all 5 feature screenshots + 13 walkthrough animation frames
.\target\release\winscroller-alt-tab.exe --render-gallery docs

# Encode frames into docs/how-it-works.gif using native Windows WPF
powershell -ExecutionPolicy Bypass -File .\scripts\make-gif.ps1
```

---

## Packaging a NuGet Tool Package

```powershell
.\package-nuget.ps1
```

Produces `target\winscroller-alt-tab.0.1.0.nupkg` using `winscroller-alt-tab.nuspec`.

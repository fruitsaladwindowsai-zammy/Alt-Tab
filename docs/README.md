# Winscroller Alt+Tab Documentation Site (GitHub Pages)

This `docs/` directory holds the [mdBook](https://rust-lang.github.io/mdBook/) documentation deployed automatically to GitHub Pages via `.github/workflows/gh-pages.yml`.

## Local Preview

```powershell
# Install mdBook once
cargo install mdbook

# Copy visual screenshots/GIF into docs/src/assets for local preview
Copy-Item docs\*.png docs\src\assets\ -Force
Copy-Item docs\*.gif docs\src\assets\ -Force

# Build and serve locally at http://localhost:3000
cd docs
mdbook serve --open
```

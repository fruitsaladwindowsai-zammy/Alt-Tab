//! Configuration loader for `[winscroller]` / `[fathom]` preferences (`%APPDATA%\winscroller-alt-tab\config.toml`).

use crate::palette::{self, DerivedPalette, Rgba, ThemeSource};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[derive(Clone, Debug)]
pub struct Config {
    pub corner_radius: f64,
    pub border_width: f64,
    pub font_family: String,
    pub font_scale: f64,
    pub show_scratchpads: bool,
    pub open_on_single: bool,
    pub custom_foreground: Option<Rgba>,
    pub custom_background: Option<Rgba>,
    pub custom_accent: Option<Rgba>,
    pub custom_urgent: Option<Rgba>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            corner_radius: 10.0,
            border_width: 1.0,
            font_family: "Segoe UI".to_string(),
            font_scale: 1.0,
            show_scratchpads: true,
            open_on_single: false,
            custom_foreground: None,
            custom_background: None,
            custom_accent: None,
            custom_urgent: None,
        }
    }
}

impl Config {
    pub fn config_path() -> Option<PathBuf> {
        if let Some(appdata) = std::env::var_os("APPDATA") {
            return Some(
                PathBuf::from(appdata)
                    .join("winscroller-alt-tab")
                    .join("config.toml"),
            );
        }
        None
    }

    pub fn crash_log_path() -> PathBuf {
        Self::config_path()
            .and_then(|p| p.parent().map(|d| d.join("crash.log")))
            .unwrap_or_else(|| PathBuf::from("winscroller-alt-tab-crash.log"))
    }

    pub fn load_from_path(override_path: Option<&Path>) -> (Self, Option<SystemTime>) {
        let mut cfg = Self::default();
        let path = match override_path {
            Some(p) => p.to_path_buf(),
            None => match Self::config_path() {
                Some(p) => p,
                None => return (cfg, None),
            },
        };
        let modified = fs::metadata(&path).and_then(|m| m.modified()).ok();
        if let Ok(contents) = fs::read_to_string(&path) {
            cfg.apply_toml(&contents);
        }
        (cfg, modified)
    }

    pub fn ensure_default_file_at(override_path: Option<&Path>) -> Option<PathBuf> {
        let path = match override_path {
            Some(p) => p.to_path_buf(),
            None => Self::config_path()?,
        };
        if !path.exists() {
            if let Some(parent) = path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let template = r##"# Winscroller Alt+Tab configuration
# Changes reload live the next time Winscroller Alt+Tab opens or updates.
# Colors follow Windows light/dark mode and Windows accent automatically.

[winscroller]
corner-radius = 10
border-width = 1.0
font-family = "Segoe UI"
font-scale = 1.0
show-scratchpads = true
open-on-single = false

# Optional custom hex color overrides (#rgb, #rrggbb, or #aarrggbb):
# foreground = "#f3f3f5"
# background = "#1c1c1e"
# accent = "#60cdff"
# urgent = "#ff6b6b"
"##;
            let _ = fs::write(&path, template);
        }
        Some(path)
    }

    pub fn validate_toml(input: &str) -> Result<Self, Vec<String>> {
        let mut cfg = Self::default();
        let mut errors = Vec::new();
        let mut in_section = true;

        for (idx, raw_line) in input.lines().enumerate() {
            let line_no = idx + 1;
            let line = strip_comment(raw_line).trim();
            if line.is_empty() {
                continue;
            }
            if line.starts_with('[') && line.ends_with(']') {
                let section = line[1..line.len() - 1].trim().to_ascii_lowercase();
                in_section = section == "winscroller" || section == "fathom";
                if !in_section {
                    errors.push(format!(
                        "line {line_no}: unknown section [{section}] (expected [winscroller] or [fathom])"
                    ));
                }
                continue;
            }
            if !in_section {
                continue;
            }
            let Some((key, val)) = line.split_once('=') else {
                errors.push(format!("line {line_no}: expected 'key = value'"));
                continue;
            };
            let k = key.trim().to_ascii_lowercase();
            let v = unquote(val.trim());
            match k.as_str() {
                "corner-radius" | "corner_radius" => match v.parse::<f64>() {
                    Ok(n) if n.is_finite() && n >= 0.0 => cfg.corner_radius = n,
                    _ => errors.push(format!(
                        "line {line_no}: '{k}' must be a non-negative number, got '{v}'"
                    )),
                },
                "border-width" | "border_width" => match v.parse::<f64>() {
                    Ok(n) if n.is_finite() && n >= 0.0 => cfg.border_width = n,
                    _ => errors.push(format!(
                        "line {line_no}: '{k}' must be a non-negative number, got '{v}'"
                    )),
                },
                "font-family" | "font_family" => {
                    if v.is_empty() {
                        errors.push(format!("line {line_no}: '{k}' cannot be empty"));
                    } else {
                        cfg.font_family = v.to_string();
                    }
                }
                "font-scale" | "font_scale" => match v.parse::<f64>() {
                    Ok(n) if n.is_finite() && n > 0.0 => cfg.font_scale = n.clamp(0.7, 2.0),
                    _ => errors.push(format!(
                        "line {line_no}: '{k}' must be a positive number (clamped to 0.7..=2.0), got '{v}'"
                    )),
                },
                "show-scratchpads" | "show_scratchpads" => {
                    if v.eq_ignore_ascii_case("true") {
                        cfg.show_scratchpads = true;
                    } else if v.eq_ignore_ascii_case("false") {
                        cfg.show_scratchpads = false;
                    } else {
                        errors.push(format!(
                            "line {line_no}: '{k}' must be true or false, got '{v}'"
                        ));
                    }
                }
                "open-on-single" | "open_on_single" => {
                    if v.eq_ignore_ascii_case("true") {
                        cfg.open_on_single = true;
                    } else if v.eq_ignore_ascii_case("false") {
                        cfg.open_on_single = false;
                    } else {
                        errors.push(format!(
                            "line {line_no}: '{k}' must be true or false, got '{v}'"
                        ));
                    }
                }
                "foreground" => match Rgba::parse(v) {
                    Some(c) => cfg.custom_foreground = Some(c),
                    None => errors.push(format!(
                        "line {line_no}: '{k}' must be #rgb, #rrggbb, or #aarrggbb, got '{v}'"
                    )),
                },
                "background" => match Rgba::parse(v) {
                    Some(c) => cfg.custom_background = Some(c),
                    None => errors.push(format!(
                        "line {line_no}: '{k}' must be #rgb, #rrggbb, or #aarrggbb, got '{v}'"
                    )),
                },
                "accent" => match Rgba::parse(v) {
                    Some(c) => cfg.custom_accent = Some(c),
                    None => errors.push(format!(
                        "line {line_no}: '{k}' must be #rgb, #rrggbb, or #aarrggbb, got '{v}'"
                    )),
                },
                "urgent" => match Rgba::parse(v) {
                    Some(c) => cfg.custom_urgent = Some(c),
                    None => errors.push(format!(
                        "line {line_no}: '{k}' must be #rgb, #rrggbb, or #aarrggbb, got '{v}'"
                    )),
                },
                // Legacy key from earlier builds; ignored because the plain Windows theme is used.
                "theme" => {}
                _ => errors.push(format!("line {line_no}: unrecognized key '{k}'")),
            }
        }

        if errors.is_empty() {
            Ok(cfg)
        } else {
            Err(errors)
        }
    }

    pub fn apply_toml(&mut self, input: &str) {
        let mut in_section = true;
        for raw_line in input.lines() {
            let line = strip_comment(raw_line).trim();
            if line.is_empty() {
                continue;
            }
            if line.starts_with('[') && line.ends_with(']') {
                let section = line[1..line.len() - 1].trim().to_ascii_lowercase();
                in_section = section == "winscroller" || section == "fathom";
                continue;
            }
            if !in_section {
                continue;
            }
            let Some((key, val)) = line.split_once('=') else {
                continue;
            };
            let k = key.trim().to_ascii_lowercase();
            let v = unquote(val.trim());
            match k.as_str() {
                "corner-radius" | "corner_radius" => {
                    if let Ok(n) = v.parse::<f64>()
                        && n.is_finite()
                        && n >= 0.0
                    {
                        self.corner_radius = n;
                    }
                }
                "border-width" | "border_width" => {
                    if let Ok(n) = v.parse::<f64>()
                        && n.is_finite()
                        && n >= 0.0
                    {
                        self.border_width = n;
                    }
                }
                "font-family" | "font_family" => {
                    if !v.is_empty() {
                        self.font_family = v.to_string();
                    }
                }
                "font-scale" | "font_scale" => {
                    if let Ok(n) = v.parse::<f64>()
                        && n.is_finite()
                        && n > 0.0
                    {
                        self.font_scale = n.clamp(0.7, 2.0);
                    }
                }
                "show-scratchpads" | "show_scratchpads" => {
                    self.show_scratchpads = !v.eq_ignore_ascii_case("false");
                }
                "open-on-single" | "open_on_single" => {
                    self.open_on_single = v.eq_ignore_ascii_case("true");
                }
                "foreground" => self.custom_foreground = Rgba::parse(v),
                "background" => self.custom_background = Rgba::parse(v),
                "accent" => self.custom_accent = Rgba::parse(v),
                "urgent" => self.custom_urgent = Rgba::parse(v),
                _ => {}
            }
        }
    }

    pub fn resolve_palette(&self, windows_auto: Option<ThemeSource>) -> DerivedPalette {
        let mut src = windows_auto.unwrap_or_else(|| palette::windows_plain_theme(false, None));
        if let Some(fg) = self.custom_foreground {
            src.foreground = fg;
        }
        if let Some(bg) = self.custom_background {
            src.background = bg;
        }
        if let Some(ac) = self.custom_accent {
            src.accent = ac;
        }
        if let Some(ur) = self.custom_urgent {
            src.urgent = ur;
        }
        palette::derive(src)
    }
}

fn strip_comment(line: &str) -> &str {
    let mut in_quotes = false;
    let mut quote_char = '"';
    for (idx, ch) in line.char_indices() {
        if in_quotes {
            if ch == quote_char {
                in_quotes = false;
            }
        } else if ch == '"' || ch == '\'' {
            in_quotes = true;
            quote_char = ch;
        } else if ch == '#' {
            return &line[..idx];
        }
    }
    line
}

fn unquote(val: &str) -> &str {
    let t = val.trim();
    if (t.starts_with('"') && t.ends_with('"') && t.len() >= 2)
        || (t.starts_with('\'') && t.ends_with('\'') && t.len() >= 2)
    {
        &t[1..t.len() - 1]
    } else {
        t
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_fathom_toml_section() {
        let mut cfg = Config::default();
        cfg.apply_toml(
            r##"
[fathom]
theme = "catppuccin"
corner-radius = 14
border-width = 2
font-scale = 1.15
open-on-single = true
show-scratchpads = false
accent = "#ff00aa" # inline comment
"##,
        );
        assert_eq!(cfg.corner_radius, 14.0);
        assert_eq!(cfg.border_width, 2.0);
        assert_eq!(cfg.font_scale, 1.15);
        assert!(cfg.open_on_single);
        assert!(!cfg.show_scratchpads);
        assert!(cfg.custom_accent.is_some());
    }

    #[test]
    fn validates_toml_and_reports_invalid_fields() {
        let valid = r##"
[winscroller]
corner-radius = 10
border-width = 1.0
font-family = "Segoe UI"
font-scale = 1.0
show-scratchpads = true
open-on-single = false
accent = "#60cdff"
"##;
        assert!(Config::validate_toml(valid).is_ok());

        let invalid = r##"
[winscroller]
corner-radius = -5
font-scale = 0
accent = "not-a-hex"
unknown-key = 123
"##;
        let errs = Config::validate_toml(invalid).unwrap_err();
        assert_eq!(errs.len(), 4);
    }
}

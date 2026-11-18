//! Single plain Windows theme color derivation with WCAG 2 contrast guarantees.

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rgba {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Rgba {
    pub const WHITE: Self = Self {
        r: 1.0,
        g: 1.0,
        b: 1.0,
        a: 1.0,
    };
    pub const BLACK: Self = Self {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    };

    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    pub fn with_alpha(self, alpha: f32) -> Self {
        Self {
            a: alpha.clamp(0.0, 1.0),
            ..self
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        let s = value.trim().trim_start_matches('#').to_ascii_lowercase();
        let bytes = s.as_bytes();
        match bytes.len() {
            3 => {
                let r = hex_nibble(bytes[0])? * 17;
                let g = hex_nibble(bytes[1])? * 17;
                let b = hex_nibble(bytes[2])? * 17;
                Some(Self::from_u8(r, g, b, 255))
            }
            6 => {
                let r = u8::from_str_radix(&s[0..2], 16).ok()?;
                let g = u8::from_str_radix(&s[2..4], 16).ok()?;
                let b = u8::from_str_radix(&s[4..6], 16).ok()?;
                Some(Self::from_u8(r, g, b, 255))
            }
            8 => {
                let a = u8::from_str_radix(&s[0..2], 16).ok()?;
                let r = u8::from_str_radix(&s[2..4], 16).ok()?;
                let g = u8::from_str_radix(&s[4..6], 16).ok()?;
                let b = u8::from_str_radix(&s[6..8], 16).ok()?;
                Some(Self::from_u8(r, g, b, a))
            }
            _ => None,
        }
    }

    pub fn from_u8(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self {
            r: r as f32 / 255.0,
            g: g as f32 / 255.0,
            b: b as f32 / 255.0,
            a: a as f32 / 255.0,
        }
    }

    pub fn hsla(h: f32, s: f32, l: f32, a: f32) -> Self {
        let hue = h.rem_euclid(1.0);
        let sat = s.clamp(0.0, 1.0);
        let lig = l.clamp(0.0, 1.0);
        let c = (1.0 - (2.0 * lig - 1.0).abs()) * sat;
        let x = c * (1.0 - ((hue * 6.0) % 2.0 - 1.0).abs());
        let m = lig - c / 2.0;
        let (r1, g1, b1) = match (hue * 6.0) as i32 {
            0 => (c, x, 0.0),
            1 => (x, c, 0.0),
            2 => (0.0, c, x),
            3 => (0.0, x, c),
            4 => (x, 0.0, c),
            _ => (c, 0.0, x),
        };
        Self {
            r: (r1 + m).clamp(0.0, 1.0),
            g: (g1 + m).clamp(0.0, 1.0),
            b: (b1 + m).clamp(0.0, 1.0),
            a: a.clamp(0.0, 1.0),
        }
    }

    fn rounded_rgb(self) -> Self {
        let qr = (self.r.clamp(0.0, 1.0) * 255.0).round() / 255.0;
        let qg = (self.g.clamp(0.0, 1.0) * 255.0).round() / 255.0;
        let qb = (self.b.clamp(0.0, 1.0) * 255.0).round() / 255.0;
        Self {
            r: qr,
            g: qg,
            b: qb,
            a: 1.0,
        }
    }
}

fn hex_nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        _ => None,
    }
}

const SOFT_CONTRAST: f64 = 5.5;
const FAINT_CONTRAST: f64 = 4.5;
const CARD_CONTRAST: f64 = 4.5;
const MARK_CONTRAST: f64 = 3.0;

const SOFT_SHARE: f32 = 0.34;
const FAINT_SHARE: f32 = 0.50;

const DARK_VEIL: [f32; 3] = [0.75, 0.80, 0.86];
const LIGHT_VEIL: [f32; 3] = [0.82, 0.86, 0.90];

#[derive(Clone, Copy, Debug)]
pub struct ThemeSource {
    pub foreground: Rgba,
    pub background: Rgba,
    pub accent: Rgba,
    pub urgent: Rgba,
}

#[derive(Clone, Copy, Debug)]
pub struct DerivedPalette {
    #[allow(dead_code)]
    pub light: bool,
    pub veil_top: Rgba,
    pub veil_middle: Rgba,
    pub veil_bottom: Rgba,
    pub surface_light: Rgba,
    pub surface_light_end: Rgba,
    pub depth_shade: Rgba,
    pub text: Rgba,
    pub text_soft: Rgba,
    pub text_faint: Rgba,
    pub accent: Rgba,
    pub accent_text: Rgba,
    pub urgent: Rgba,
    pub background: Rgba,
    pub card_top: Rgba,
    pub card_bottom: Rgba,
    pub card_selected_top: Rgba,
    pub card_selected_bottom: Rgba,
    pub card_border: Rgba,
    pub card_border_hover: Rgba,
    pub shadow: Rgba,
    pub glow: Rgba,
    pub bed: Rgba,
    pub frame_edge: Rgba,
    pub fog: Rgba,
    pub fog_strength: f32,
    pub panel: Rgba,
    pub panel_border: Rgba,
    pub key: Rgba,
    pub key_border: Rgba,
    pub map_card: Rgba,
    pub map_card_border: Rgba,
    pub map_card_selected: Rgba,
    pub map_card_selected_border: Rgba,
    pub screen: Rgba,
    pub screen_border: Rgba,
    pub screen_border_on: Rgba,
    pub tile: Rgba,
    pub tile_hover: Rgba,
    pub tile_selected: Rgba,
    pub tile_border: Rgba,
    pub tile_border_active: Rgba,
    pub line: Rgba,
    pub tick: Rgba,
    pub mark: Rgba,
    pub mark_border: Rgba,
}

/// Plain Windows light/dark palette built from system theme polarity and optional DWM accent.
pub fn windows_plain_theme(light: bool, accent: Option<Rgba>) -> ThemeSource {
    if light {
        ThemeSource {
            foreground: Rgba::parse("#1b1b1f").unwrap(),
            background: Rgba::parse("#f3f3f3").unwrap(),
            accent: accent.unwrap_or_else(|| Rgba::parse("#0067c0").unwrap()),
            urgent: Rgba::parse("#c42b1c").unwrap(),
        }
    } else {
        ThemeSource {
            foreground: Rgba::parse("#f3f3f5").unwrap(),
            background: Rgba::parse("#1c1c1e").unwrap(),
            accent: accent.unwrap_or_else(|| Rgba::parse("#60cdff").unwrap()),
            urgent: Rgba::parse("#ff6b6b").unwrap(),
        }
    }
}

pub fn mix(a: Rgba, b: Rgba, share: f32) -> Rgba {
    let t = share.clamp(0.0, 1.0);
    Rgba {
        r: a.r + (b.r - a.r) * t,
        g: a.g + (b.g - a.g) * t,
        b: a.b + (b.b - a.b) * t,
        a: 1.0,
    }
}

fn linear(value: f32) -> f64 {
    let v = value as f64;
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

pub fn luminance(c: Rgba) -> f64 {
    0.2126 * linear(c.r) + 0.7152 * linear(c.g) + 0.0722 * linear(c.b)
}

pub fn contrast(a: Rgba, b: Rgba) -> f64 {
    let la = luminance(a);
    let lb = luminance(b);
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

fn readable(color: Rgba, against: Rgba, ratio: f64, toward: Rgba) -> Rgba {
    let mut result = color;
    for step in 1..=40 {
        if contrast(result, against) >= ratio {
            break;
        }
        result = mix(color, toward, step as f32 / 40.0).rounded_rgb();
    }
    result
}

fn tier(fg: Rgba, bg: Rgba, share: f32, floors: &[(Rgba, f64)]) -> Rgba {
    let mut s = share;
    while s > 0.001 {
        let color = mix(fg, bg, s).rounded_rgb();
        if floors
            .iter()
            .all(|(surface, ratio)| contrast(color, *surface) >= *ratio)
        {
            return color;
        }
        s -= 0.02;
    }
    fg
}

pub fn derive(source: ThemeSource) -> DerivedPalette {
    let fg = source.foreground;
    let bg = source.background;
    let accent_in = source.accent;
    let urgent_in = source.urgent;

    let light = luminance(bg) > luminance(fg);
    let away = if light { Rgba::BLACK } else { Rgba::WHITE };
    let veil = if light { LIGHT_VEIL } else { DARK_VEIL };
    let accent = readable(accent_in, bg, MARK_CONTRAST, away);
    let paper = |share: f32| mix(bg, Rgba::WHITE, share);
    let card_selected_top = if light {
        paper(0.90)
    } else {
        mix(bg, fg, 0.12)
    };

    DerivedPalette {
        light,
        veil_top: bg.with_alpha(veil[0]),
        veil_middle: bg.with_alpha(veil[1]),
        veil_bottom: bg.with_alpha(veil[2]),
        surface_light: if light {
            Rgba::WHITE.with_alpha(0.55)
        } else {
            fg.with_alpha(0.06)
        },
        surface_light_end: if light {
            Rgba::WHITE.with_alpha(0.0)
        } else {
            fg.with_alpha(0.0)
        },
        depth_shade: if light { mix(bg, fg, 0.45) } else { bg },
        text: fg,
        text_soft: tier(
            fg,
            bg,
            SOFT_SHARE,
            &[(bg, SOFT_CONTRAST), (card_selected_top, CARD_CONTRAST)],
        ),
        text_faint: tier(fg, bg, FAINT_SHARE, &[(bg, FAINT_CONTRAST)]),
        accent,
        accent_text: readable(accent_in, bg, FAINT_CONTRAST, away),
        urgent: readable(urgent_in, bg, MARK_CONTRAST, away),
        background: bg,
        card_top: if light {
            paper(0.72)
        } else {
            mix(bg, fg, 0.085)
        },
        card_bottom: if light {
            paper(0.50)
        } else {
            mix(bg, fg, 0.035)
        },
        card_selected_top,
        card_selected_bottom: if light {
            paper(0.70)
        } else {
            mix(bg, fg, 0.06)
        },
        card_border: fg.with_alpha(if light { 0.16 } else { 0.11 }),
        card_border_hover: fg.with_alpha(if light { 0.34 } else { 0.30 }),
        shadow: if light {
            mix(fg, Rgba::BLACK, 0.5).with_alpha(0.20)
        } else {
            Rgba::BLACK.with_alpha(0.55)
        },
        glow: accent_in.with_alpha(if light { 0.34 } else { 0.38 }),
        bed: mix(bg, fg, if light { 0.06 } else { 0.04 }),
        frame_edge: fg.with_alpha(if light { 0.12 } else { 0.06 }),
        fog: bg,
        fog_strength: if light { 0.70 } else { 1.0 },
        panel: if light {
            paper(0.70).with_alpha(0.94)
        } else {
            bg.with_alpha(0.85)
        },
        panel_border: fg.with_alpha(if light { 0.18 } else { 0.14 }),
        key: if light {
            paper(0.80)
        } else {
            fg.with_alpha(0.08)
        },
        key_border: fg.with_alpha(if light { 0.24 } else { 0.20 }),
        map_card: if light {
            paper(0.60).with_alpha(0.80)
        } else {
            fg.with_alpha(0.045)
        },
        map_card_border: fg.with_alpha(if light { 0.13 } else { 0.10 }),
        map_card_selected: if light {
            mix(paper(0.60), accent_in, 0.08).with_alpha(0.90)
        } else {
            accent_in.with_alpha(0.08)
        },
        map_card_selected_border: accent.with_alpha(0.70),
        screen: if light {
            fg.with_alpha(0.05)
        } else {
            Rgba::BLACK.with_alpha(0.22)
        },
        screen_border: fg.with_alpha(if light { 0.16 } else { 0.12 }),
        screen_border_on: fg.with_alpha(if light { 0.32 } else { 0.28 }),
        tile: if light {
            paper(0.80)
        } else {
            mix(Rgba::new(0.12, 0.12, 0.12, 1.0), fg, 0.08).with_alpha(0.90)
        },
        tile_hover: if light {
            mix(bg, fg, 0.12)
        } else {
            fg.with_alpha(0.20)
        },
        tile_selected: if light {
            mix(paper(0.80), accent_in, 0.24)
        } else {
            accent_in.with_alpha(0.36)
        },
        tile_border: fg.with_alpha(if light { 0.30 } else { 0.24 }),
        tile_border_active: fg.with_alpha(if light { 0.70 } else { 0.75 }),
        line: fg.with_alpha(if light { 0.26 } else { 0.18 }),
        tick: fg.with_alpha(if light { 0.38 } else { 0.28 }),
        mark: fg.with_alpha(if light { 0.55 } else { 0.60 }),
        mark_border: fg.with_alpha(0.50),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_plain_theme_meets_contrast_floors() {
        for (light, accent) in [
            (false, None),
            (true, None),
            (false, Some(Rgba::parse("#0078d4").unwrap())),
            (true, Some(Rgba::parse("#0078d4").unwrap())),
        ] {
            let p = derive(windows_plain_theme(light, accent));
            assert!(contrast(p.text_soft, p.background) >= SOFT_CONTRAST);
            assert!(contrast(p.text_soft, p.card_selected_top) >= CARD_CONTRAST);
            assert!(contrast(p.text_faint, p.background) >= FAINT_CONTRAST);
            assert!(contrast(p.accent, p.background) >= MARK_CONTRAST);
            assert!(contrast(p.accent_text, p.background) >= FAINT_CONTRAST);
            assert!(contrast(p.urgent, p.background) >= MARK_CONTRAST);
        }
    }
}

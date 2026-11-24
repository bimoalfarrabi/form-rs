//! Colour parsing and 8-bit helpers.
//!
//! `getShaderColorFromString` (paper-design/shaders 0.0.82) is a plain sRGB
//! component parser: it never linearises. The shaders therefore do their
//! arithmetic on sRGB-encoded values, and the render target must be a
//! non-sRGB `Rgba8Unorm` buffer so the bytes reach the screen unchanged.

/// Straight (non-premultiplied) sRGB colour, components in `0..=1`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Rgba {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Rgba {
    pub const BLACK: Rgba = Rgba {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    };
    pub const WHITE: Rgba = Rgba {
        r: 1.0,
        g: 1.0,
        b: 1.0,
        a: 1.0,
    };
    /// The library's fallback for an unparseable colour.
    pub const FALLBACK: Rgba = Rgba {
        r: 0.5,
        g: 0.5,
        b: 0.5,
        a: 1.0,
    };

    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    pub fn to_array(self) -> [f32; 4] {
        [self.r, self.g, self.b, self.a]
    }

    /// Parse `#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa`, `rgb()`, `rgba()`,
    /// `hsl()` or `hsla()`, matching `getShaderColorFromString` exactly.
    pub fn parse(input: &str) -> Option<Self> {
        let s = input.trim();
        let c = if let Some(hex) = s.strip_prefix('#') {
            Self::from_hex(hex)?
        } else if s.starts_with("rgb") {
            Self::parse_rgb(s)?
        } else if s.starts_with("hsl") {
            Self::parse_hsl(s)?
        } else {
            return None;
        };
        Some(Rgba {
            r: c.r.clamp(0.0, 1.0),
            g: c.g.clamp(0.0, 1.0),
            b: c.b.clamp(0.0, 1.0),
            a: c.a.clamp(0.0, 1.0),
        })
    }

    /// Accepts a hex string with or without the leading `#`.
    pub fn from_hex(hex: &str) -> Option<Self> {
        let hex = hex.strip_prefix('#').unwrap_or(hex);
        let expanded: String = if hex.len() == 3 || hex.len() == 4 {
            hex.chars().flat_map(|c| [c, c]).collect()
        } else {
            hex.to_string()
        };
        let full = if expanded.len() == 6 {
            format!("{expanded}ff")
        } else {
            expanded
        };
        if full.len() != 8 || !full.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        let byte = |i: usize| {
            u8::from_str_radix(&full[i..i + 2], 16)
                .ok()
                .map(|v| v as f32 / 255.0)
        };
        Some(Rgba {
            r: byte(0)?,
            g: byte(2)?,
            b: byte(4)?,
            a: byte(6)?,
        })
    }

    fn parse_rgb(s: &str) -> Option<Self> {
        let inner = s.split_once('(')?.1.rsplit_once(')')?.0;
        let parts: Vec<&str> = inner.split(',').map(|p| p.trim()).collect();
        if parts.len() < 3 || parts.len() > 4 {
            return None;
        }
        let channel = |p: &str| p.parse::<f32>().ok().map(|v| v / 255.0);
        let a = match parts.get(3) {
            Some(p) => p.parse::<f32>().ok()?,
            None => 1.0,
        };
        Some(Rgba {
            r: channel(parts[0])?,
            g: channel(parts[1])?,
            b: channel(parts[2])?,
            a,
        })
    }

    fn parse_hsl(s: &str) -> Option<Self> {
        let inner = s.split_once('(')?.1.rsplit_once(')')?.0;
        let parts: Vec<&str> = inner.split(',').map(|p| p.trim()).collect();
        if parts.len() < 3 || parts.len() > 4 {
            return None;
        }
        let h: f32 = parts[0].parse().ok()?;
        let sat: f32 = parts[1].trim_end_matches('%').parse().ok()?;
        let light: f32 = parts[2].trim_end_matches('%').parse().ok()?;
        let a: f32 = match parts.get(3) {
            Some(p) => p.parse().ok()?,
            None => 1.0,
        };
        let h_decimal = h / 360.0;
        let s_decimal = sat / 100.0;
        let l_decimal = light / 100.0;
        let (r, g, b) = if sat == 0.0 {
            (l_decimal, l_decimal, l_decimal)
        } else {
            let q = if l_decimal < 0.5 {
                l_decimal * (1.0 + s_decimal)
            } else {
                l_decimal + s_decimal - l_decimal * s_decimal
            };
            let p = 2.0 * l_decimal - q;
            let hue2rgb = |mut t: f32| {
                if t < 0.0 {
                    t += 1.0;
                }
                if t > 1.0 {
                    t -= 1.0;
                }
                if t < 1.0 / 6.0 {
                    p + (q - p) * 6.0 * t
                } else if t < 1.0 / 2.0 {
                    q
                } else if t < 2.0 / 3.0 {
                    p + (q - p) * (2.0 / 3.0 - t) * 6.0
                } else {
                    p
                }
            };
            (
                hue2rgb(h_decimal + 1.0 / 3.0),
                hue2rgb(h_decimal),
                hue2rgb(h_decimal - 1.0 / 3.0),
            )
        };
        Some(Rgba { r, g, b, a })
    }

    /// `#RRGGBB` for the hex field in the UI (uppercase, no alpha).
    pub fn to_hex(self) -> String {
        let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        format!("#{:02X}{:02X}{:02X}", q(self.r), q(self.g), q(self.b))
    }

    /// Premultiplied 8-bit RGBA, the way the shaders write into the buffer.
    pub fn to_premultiplied_u8(self) -> [u8; 4] {
        let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        [
            q(self.r * self.a),
            q(self.g * self.a),
            q(self.b * self.a),
            q(self.a),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: Rgba, b: Rgba) -> bool {
        (a.r - b.r).abs() < 1e-6
            && (a.g - b.g).abs() < 1e-6
            && (a.b - b.b).abs() < 1e-6
            && (a.a - b.a).abs() < 1e-6
    }

    #[test]
    fn short_hex_expands() {
        assert!(close(Rgba::from_hex("#fff").unwrap(), Rgba::WHITE));
        let c = Rgba::from_hex("#0a0a0a").unwrap();
        assert!((c.r - 10.0 / 255.0).abs() < 1e-6);
        assert_eq!(c.a, 1.0);
    }

    #[test]
    fn alpha_hex() {
        let c = Rgba::from_hex("#00000080").unwrap();
        assert!((c.a - 128.0 / 255.0).abs() < 1e-6);
    }

    #[test]
    fn rgb_and_hsl() {
        assert!(close(
            Rgba::parse("rgb(255, 0, 0)").unwrap(),
            Rgba::new(1.0, 0.0, 0.0, 1.0)
        ));
        assert!(close(
            Rgba::parse("rgba(0,0,0,0.5)").unwrap(),
            Rgba::new(0.0, 0.0, 0.0, 0.5)
        ));
        assert!(close(
            Rgba::parse("hsl(0, 100%, 50%)").unwrap(),
            Rgba::new(1.0, 0.0, 0.0, 1.0)
        ));
    }

    #[test]
    fn round_trip_hex() {
        assert_eq!(Rgba::from_hex("#3448ff").unwrap().to_hex(), "#3448FF");
    }
}

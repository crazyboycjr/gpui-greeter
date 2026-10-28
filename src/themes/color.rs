//! Colours as themes write them: `#rgb`, `#rrggbb`, `#rrggbbaa` or
//! `transparent`.

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const TRANSPARENT: Color = Color::rgba(0.0, 0.0, 0.0, 0.0);
    pub const WHITE: Color = Color::rgba(1.0, 1.0, 1.0, 1.0);
    pub const BLACK: Color = Color::rgba(0.0, 0.0, 0.0, 1.0);

    pub const fn rgba(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    pub fn parse(text: &str) -> Result<Self, String> {
        let wrong = || format!("{text:?} is not a colour like \"#e8d5c0\" or \"#e8d5c080\"");
        if text == "transparent" {
            return Ok(Self::TRANSPARENT);
        }
        let hex = text.strip_prefix('#').ok_or_else(wrong)?;
        if !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(wrong());
        }
        let digit = |at: usize| u8::from_str_radix(&hex[at..at + 1], 16).unwrap_or(0);
        let byte = |at: usize| u8::from_str_radix(&hex[at..at + 2], 16).unwrap_or(0);
        let channels: [u8; 4] = match hex.len() {
            3 => [digit(0) * 17, digit(1) * 17, digit(2) * 17, 255],
            6 => [byte(0), byte(2), byte(4), 255],
            8 => [byte(0), byte(2), byte(4), byte(6)],
            _ => return Err(wrong()),
        };
        let [r, g, b, a] = channels.map(|channel| f32::from(channel) / 255.0);
        Ok(Self { r, g, b, a })
    }

    /// The colour `t` of the way from `self` to `to`.
    pub fn mix(self, to: Color, t: f32) -> Color {
        let lerp = |a: f32, b: f32| a + (b - a) * t;
        Color {
            r: lerp(self.r, to.r),
            g: lerp(self.g, to.g),
            b: lerp(self.b, to.b),
            a: lerp(self.a, to.a).clamp(0.0, 1.0),
        }
    }

    pub fn with_alpha(self, alpha: f32) -> Color {
        Color { a: alpha, ..self }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_written_forms() {
        assert_eq!(Color::parse("#fff").unwrap(), Color::WHITE);
        assert_eq!(
            Color::parse("#1793d1").unwrap(),
            Color::rgba(23.0 / 255.0, 147.0 / 255.0, 209.0 / 255.0, 1.0)
        );
        assert_eq!(Color::parse("#00000080").unwrap().a, 128.0 / 255.0);
        assert_eq!(Color::parse("transparent").unwrap().a, 0.0);
        for wrong in ["red", "#12", "#12345", "1793d1", "#ggg", "#+12345"] {
            assert!(Color::parse(wrong).is_err(), "{wrong}");
        }
    }

    #[test]
    fn mixes_halfway() {
        let grey = Color::BLACK.mix(Color::WHITE, 0.5);
        assert_eq!((grey.r, grey.a), (0.5, 1.0));
    }
}

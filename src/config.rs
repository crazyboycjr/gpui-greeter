//! The greeter's settings, read from `/etc/gpui-greeter/config.toml`.
//!
//! A missing or broken file never keeps anyone from logging in: the greeter
//! says what was wrong in its log and starts with the defaults.

use std::path::{Path, PathBuf};

use serde::Deserialize;

pub const DEFAULT_PATH: &str = "/etc/gpui-greeter/config.toml";

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub theme: Theme,
    pub language: Language,
    pub paper: Paper,
    pub frost: Frost,
    pub neon: Neon,
    pub wallpaper: Wallpaper,
    pub sessions: Sessions,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Theme {
    #[default]
    Paper,
    PaperNight,
    Frost,
    Neon,
}

impl Theme {
    pub fn parse(name: &str) -> Option<Self> {
        Self::deserialize(serde::de::value::StrDeserializer::<serde::de::value::Error>::new(name))
            .ok()
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
pub enum Language {
    #[default]
    #[serde(rename = "en")]
    English,
    #[serde(rename = "zh-CN")]
    Chinese,
}

impl Language {
    pub fn parse(name: &str) -> Option<Self> {
        Self::deserialize(serde::de::value::StrDeserializer::<serde::de::value::Error>::new(name))
            .ok()
    }
}

/// A colour written as `#rrggbb`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(try_from = "String")]
pub struct Rgb(pub u32);

impl TryFrom<String> for Rgb {
    type Error = String;

    fn try_from(text: String) -> Result<Self, String> {
        text.strip_prefix('#')
            .filter(|hex| hex.len() == 6)
            .and_then(|hex| u32::from_str_radix(hex, 16).ok())
            .map(Rgb)
            .ok_or_else(|| format!("{text:?} is not a colour like \"#b0612e\""))
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Paper {
    pub accent: Rgb,
    pub background: PaperBackground,
}

impl Default for Paper {
    fn default() -> Self {
        Self {
            accent: Rgb(0xB0612E),
            background: PaperBackground::Desk,
        }
    }
}

/// What lies under Paper's clock.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PaperBackground {
    /// A sheet of paper on a wooden desk, among a pen, a pencil and a cup.
    #[default]
    Desk,
    /// Ledger paper: blue rules and a red margin.
    Ledger,
    /// A drafting sheet: a grid, a frame with zones and a title block, which
    /// holds the session and the power actions.
    Drafting,
    Plain,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Frost {
    /// How far the wallpaper blurs behind the login form, in the mockup's
    /// pixels (8 to 48).
    pub blur: f32,
    /// How much it darkens there (0 to 0.6).
    pub dim: f32,
    /// Seconds without input before the login form gives way to the clock.
    pub idle_seconds: u64,
}

impl Default for Frost {
    fn default() -> Self {
        Self {
            blur: 30.0,
            dim: 0.28,
            idle_seconds: 60,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Neon {
    pub accent: Rgb,
    pub scanlines: bool,
    pub background: NeonBackground,
}

impl Default for Neon {
    fn default() -> Self {
        Self {
            accent: Rgb(0x45F0FF),
            scanlines: true,
            background: NeonBackground::Skyline,
        }
    }
}

/// What lies behind Neon's terminal.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NeonBackground {
    /// A city at night, its windows lit in the accent.
    #[default]
    Skyline,
    /// A circuit board with pulses running along its traces.
    Circuit,
    /// A synthwave sunset over a grid floor that runs toward the viewer.
    Horizon,
    /// A faint grid.
    Grid,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Wallpaper {
    pub dir: PathBuf,
    pub pick: Pick,
    /// Where Bing's picture of the day is found: Bing's archive, whose answer
    /// names it, or any address that answers with a picture itself.
    pub bing_url: String,
}

impl Default for Wallpaper {
    fn default() -> Self {
        Self {
            dir: PathBuf::from("/usr/local/share/gpui-greeter/wallpapers"),
            pick: Pick::Random,
            bing_url: "https://www.bing.com/HPImageArchive.aspx?format=js&idx=0&n=1".into(),
        }
    }
}

/// Which wallpaper Frost shows.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(from = "String")]
pub enum Pick {
    /// Another one each time the greeter starts.
    Random,
    /// The same one all day.
    Daily,
    /// Bing's picture of the day, fetched in the background; a random one
    /// until it arrives.
    Bing,
    /// This file in the wallpaper directory.
    File(String),
}

impl From<String> for Pick {
    fn from(text: String) -> Self {
        match text.as_str() {
            "random" => Pick::Random,
            "daily" => Pick::Daily,
            "bing" => Pick::Bing,
            _ => Pick::File(text),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Sessions {
    /// Where the X sessions' desktop entries are.
    pub dir: PathBuf,
    /// What runs a session's command: by default startx, so that the user's
    /// own `~/.xserverrc` and X setup apply as they do on a console.
    pub wrapper: String,
}

impl Default for Sessions {
    fn default() -> Self {
        Self {
            dir: PathBuf::from("/usr/share/xsessions"),
            wrapper: "startx /usr/bin/env".into(),
        }
    }
}

impl Config {
    pub fn load(path: &Path) -> Self {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Self::default(),
            Err(error) => {
                log::error!(
                    "cannot read {}: {error}; the defaults apply",
                    path.display()
                );
                return Self::default();
            }
        };
        match toml::from_str::<Self>(&text) {
            Ok(config) => config.clamped(),
            Err(error) => {
                log::error!("{}: {error}; the defaults apply", path.display());
                Self::default()
            }
        }
    }

    fn clamped(mut self) -> Self {
        self.frost.blur = self.frost.blur.clamp(0.0, 64.0);
        self.frost.dim = self.frost.dim.clamp(0.0, 0.9);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_every_section() {
        let config: Config = toml::from_str(
            r##"
            theme = "paper-night"
            language = "zh-CN"
            [paper]
            accent = "#3E6E8E"
            background = "drafting"
            [frost]
            blur = 24
            [neon]
            scanlines = false
            background = "horizon"
            [wallpaper]
            pick = "lake.jpg"
            "##,
        )
        .unwrap();
        assert_eq!(config.theme, Theme::PaperNight);
        assert_eq!(config.language, Language::Chinese);
        assert_eq!(config.paper.accent, Rgb(0x3E6E8E));
        assert_eq!(config.paper.background, PaperBackground::Drafting);
        assert_eq!(config.neon.background, NeonBackground::Horizon);
        assert_eq!(config.frost.blur, 24.0);
        assert_eq!(config.frost.dim, 0.28);
        assert!(!config.neon.scanlines);
        assert_eq!(config.wallpaper.pick, Pick::File("lake.jpg".into()));
        assert_eq!(config.sessions.wrapper, "startx /usr/bin/env");
    }

    #[test]
    fn rejects_unknown_keys_and_bad_colours() {
        assert!(toml::from_str::<Config>("themes = \"neon\"").is_err());
        assert!(toml::from_str::<Config>("[neon]\naccent = \"cyan\"").is_err());
    }

    #[test]
    fn the_shipped_settings_are_the_defaults() {
        let shipped: Config = toml::from_str(include_str!("../dist/config.toml")).unwrap();
        assert_eq!(shipped, Config::default());
    }

    #[test]
    fn parses_names_from_the_command_line() {
        assert_eq!(Theme::parse("frost"), Some(Theme::Frost));
        assert_eq!(Theme::parse("night"), None);
        assert_eq!(Language::parse("zh-CN"), Some(Language::Chinese));
    }
}

//! Theme folders: looks described by a `theme.toml` and the pictures, SVGs
//! and fonts beside it. They live outside the package, one folder per theme
//! in `/usr/local/share/gpui-greeter/themes`, and `theme = "<name>"` picks
//! one. `skills/gpui-greeter-theme/reference.md` documents the format.
//!
//! Loading checks everything a theme refers to before the greeter draws it,
//! so `check-theme`, `install-theme` and the greeter agree on what is wrong;
//! a theme with an error gives way to Paper.

mod color;
pub mod facts;
mod files;
pub mod install;
mod keys;
mod layer;
mod motion;
mod parse;
mod template;
pub mod tiles;

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

pub use color::Color;
pub use files::{Asset, AssetKind, FontFile, render_svg};
pub use keys::{Action, Chord, Held, Keymap, TileAction};
pub use layer::*;
pub use motion::{Animated, Easing, Motion, Transition};
pub use parse::{BUILT_IN_STRINGS, sized, static_size};
pub use template::{Binding, Fill, Piece, Template, format_time, uptime};

use crate::config::Language;

/// The frame layers are laid out on: a 4K screen. A window scales it by
/// min(width / 3840, height / 2160), and layers anchor to its edges or its
/// centre, so other shapes and sizes keep the layout.
pub const FRAME: (f32, f32) = (3840.0, 2160.0);
pub const MANIFEST: &str = "theme.toml";
/// The `format` this greeter reads.
pub const FORMAT: i64 = 1;
/// Names the built-in looks take.
pub const RESERVED: [&str; 4] = ["paper", "paper-night", "frost", "neon"];

/// Whether `name` can name a theme folder: lower-case letters, digits, `-`
/// and `_`, starting with a letter or a digit.
pub fn valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|first| first.is_ascii_lowercase() || first.is_ascii_digit())
        && name.len() <= 64
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Warning,
    Error,
}

/// Something wrong with a theme, in a file and, when it can tell, on a line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    pub severity: Severity,
    /// The file, relative to the theme folder.
    pub file: String,
    pub line: Option<usize>,
    pub message: String,
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let severity = match self.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };
        match self.line {
            Some(line) => write!(f, "{}:{line}: {severity}: {}", self.file, self.message),
            None => write!(f, "{}: {severity}: {}", self.file, self.message),
        }
    }
}

/// A theme folder, read and checked: everything the greeter needs to draw
/// it, its pictures decoded and its SVGs parsed.
pub struct Folder {
    /// The folder's name, which `theme` names.
    pub name: String,
    /// What the theme calls itself.
    pub title: String,
    pub dir: PathBuf,
    pub background: Color,
    /// Whether the greeter starts at rest, and goes back to it with Escape.
    pub rest: bool,
    /// How long without input before the greeter goes back to rest.
    pub rest_after: Option<Duration>,
    /// How long the welcome shows before the session starts.
    pub welcome: Duration,
    /// The text style every layer inherits.
    pub text: Props,
    pub strings: Strings,
    pub keys: Keymap,
    pub layers: Vec<Layer>,
    /// The layers on the screen itself, bottom first.
    pub roots: Vec<usize>,
    pub fonts: Vec<FontFile>,
    pub assets: Vec<Asset>,
    /// The tiles layer, if the theme has one.
    pub tiles: Option<usize>,
    /// What a wallpaper layer blurs it by, if the theme shows the wallpaper.
    pub wallpaper: Option<f32>,
    /// The system facts the theme's text uses.
    pub uses: facts::Uses,
}

/// The theme's own words, by language; English stands in for a missing
/// translation, and the greeter's own words for both.
#[derive(Default)]
pub struct Strings {
    pub english: std::collections::HashMap<Arc<str>, Template>,
    pub chinese: std::collections::HashMap<Arc<str>, Template>,
}

impl Strings {
    pub fn get(&self, language: Language, key: &str) -> Option<&Template> {
        let own = match language {
            Language::English => None,
            Language::Chinese => self.chinese.get(key),
        };
        own.or_else(|| self.english.get(key))
    }
}

impl Folder {
    /// Reads and checks the theme in `dir`: the theme on success, with any
    /// warnings, or every problem found.
    pub fn load(dir: &Path) -> Result<(Folder, Vec<Problem>), Vec<Problem>> {
        let name = dir
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_string();
        let mut problems = Vec::new();
        let folder = parse::folder(dir, &name, &mut problems);
        problems.sort_by(|a, b| (b.severity, &a.file, a.line).cmp(&(a.severity, &b.file, b.line)));
        match folder {
            Some(folder) if !problems.iter().any(|p| p.severity == Severity::Error) => {
                Ok((folder, problems))
            }
            _ => Err(problems),
        }
    }

    pub fn layer_named(&self, id: &str) -> Option<usize> {
        self.layers
            .iter()
            .position(|layer| layer.id.as_deref() == Some(id))
    }
}

/// The folder `theme` names in the configured themes folder, read and
/// checked; on any error, the problems are logged and the caller falls
/// back to a built-in look.
pub fn load_configured(themes_dir: &Path, name: &str) -> Option<Folder> {
    let dir = themes_dir.join(name);
    match Folder::load(&dir) {
        Ok((folder, warnings)) => {
            for warning in warnings {
                log::warn!("theme {name}: {warning}");
            }
            Some(folder)
        }
        Err(problems) => {
            for problem in problems.iter().filter(|p| p.severity == Severity::Error) {
                log::error!("theme {name}: {problem}");
            }
            log::error!(
                "theme {name} in {} cannot be loaded; Paper instead",
                dir.display()
            );
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_example_theme_reads_without_a_problem() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("skills/gpui-greeter-theme/dial");
        let (folder, warnings) = Folder::load(&dir).unwrap_or_else(|problems| {
            panic!(
                "{}",
                problems
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        });
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(folder.name, "dial");
        assert!(folder.rest);
        assert!(
            folder.fonts[0]
                .families
                .iter()
                .any(|f| f == "Noto Serif Display")
        );
        let hand = folder.layer_named("hand").unwrap();
        assert!(folder.layers[hand].motions[0].clock);
        let sweep = folder.layer_named("sweep").unwrap();
        assert!(matches!(
            folder.layers[sweep].repeat,
            Repeat::Copies { count: 60, .. }
        ));
    }

    #[test]
    fn names_stay_inside_the_themes_folder() {
        assert!(valid_name("console"));
        assert!(valid_name("my-theme_2"));
        assert!(!valid_name(""));
        assert!(!valid_name(".."));
        assert!(!valid_name("a/b"));
        assert!(!valid_name("-a"));
        assert!(!valid_name("Console"));
        assert!(!valid_name(&"a".repeat(65)));
    }
}

//! Who logged in last and with which session, so the greeter offers the
//! same again, and the theme picked at the login screen. Kept in
//! `$XDG_STATE_HOME/gpui-greeter/state.toml`, which for the greeter user is
//! under its home, `/var/lib/gpui-greeter`.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Remembered {
    pub user: Option<String>,
    pub session: Option<String>,
    /// The theme picked at the login screen.
    pub theme: Option<String>,
    /// The settings' `theme` when it was picked: once that changes, the
    /// settings win.
    pub theme_setting: Option<String>,
}

impl Remembered {
    pub fn load() -> Self {
        let Some(path) = path() else {
            return Self::default();
        };
        std::fs::read_to_string(path)
            .ok()
            .and_then(|text| toml::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        let Some(path) = path() else { return };
        let written = path
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| std::fs::write(&path, toml::to_string(self).unwrap_or_default()));
        if let Err(error) = written {
            log::warn!(
                "cannot remember the last login in {}: {error}",
                path.display()
            );
        }
    }
}

impl Remembered {
    /// The theme to start with: the one picked, unless the settings' theme
    /// has changed since.
    pub fn theme_for(&self, setting: &str) -> Option<&str> {
        self.theme
            .as_deref()
            .filter(|_| self.theme_setting.as_deref() == Some(setting))
    }
}

fn path() -> Option<PathBuf> {
    let state = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state"))
        })?;
    Some(state.join("gpui-greeter/state.toml"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_changed_setting_wins_over_the_theme_picked() {
        let remembered = Remembered {
            theme: Some("frost".into()),
            theme_setting: Some("paper".into()),
            ..Remembered::default()
        };
        assert_eq!(remembered.theme_for("paper"), Some("frost"));
        assert_eq!(remembered.theme_for("console"), None);
        let old: Remembered = toml::from_str("user = \"alice\"\nsession = \"xmonad\"").unwrap();
        assert_eq!(old.theme_for("paper"), None);
    }
}

//! Who logged in last and with which session, so the greeter offers the
//! same again. Kept in `$XDG_STATE_HOME/gpui-greeter/state.toml`, which for
//! the greeter user is under its home, `/var/lib/gpui-greeter`.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Remembered {
    pub user: Option<String>,
    pub session: Option<String>,
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

fn path() -> Option<PathBuf> {
    let state = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state"))
        })?;
    Some(state.join("gpui-greeter/state.toml"))
}

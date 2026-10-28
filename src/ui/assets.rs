//! The fonts and icons, built into the binary: the greeter runs before
//! anyone's fonts or icon themes are at hand; and a theme folder's own
//! fonts and SVGs.

use std::borrow::Cow;
use std::sync::PoisonError;

use anyhow::Result;
use gpui_kit::{App, AssetSource, SharedString};

use super::looks::SvgAssets;
use crate::themes::Folder;

macro_rules! assets {
    ($($path:literal),* $(,)?) => {
        [$(($path, include_bytes!(concat!("../../assets/", $path)) as &[u8])),*]
    };
}

const FONTS: [(&str, &[u8]); 11] = assets![
    "fonts/Inter-ExtraLight.ttf",
    "fonts/Inter-Light.ttf",
    "fonts/Inter-Regular.ttf",
    "fonts/Inter-Medium.ttf",
    "fonts/Fraunces144pt-Light.ttf",
    "fonts/Fraunces44pt-Regular.ttf",
    "fonts/Fraunces17pt-Regular.ttf",
    "fonts/JetBrainsMono-Regular.ttf",
    "fonts/JetBrainsMono-Medium.ttf",
    "fonts/JetBrainsMono-Bold.ttf",
    "fonts/JetBrainsMono-ExtraBold.ttf",
];

const ICONS: [(&str, &[u8]); 11] = assets![
    "icons/arrow-right.svg",
    "icons/caret-up.svg",
    "icons/chevron-left.svg",
    "icons/chevron-up.svg",
    "icons/corner.svg",
    "icons/monitor.svg",
    "icons/moon.svg",
    "icons/power.svg",
    "icons/restart.svg",
    "icons/spinner.svg",
    "icons/theme.svg",
];

pub fn register_fonts(cx: &mut App, folder: Option<&Folder>) {
    let fonts = FONTS
        .iter()
        .map(|(_, bytes)| Cow::Borrowed(*bytes))
        .collect();
    if let Err(error) = cx.text_system().add_fonts(fonts) {
        log::error!("cannot load the built-in fonts: {error:#}");
    }
    let Some(folder) = folder else {
        return;
    };
    let own: Vec<Cow<'static, [u8]>> = folder
        .fonts
        .iter()
        .map(|font| Cow::Owned(font.bytes.to_vec()))
        .collect();
    if !own.is_empty()
        && let Err(error) = cx.text_system().add_fonts(own)
    {
        log::error!("theme {}: cannot load its fonts: {error:#}", folder.name);
    }
}

/// The icons, and the SVGs theme folders draw in a colour of their
/// choosing, which grow as theme folders load.
pub struct Assets {
    pub svgs: SvgAssets,
}

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        let svgs = self.svgs.read().unwrap_or_else(PoisonError::into_inner);
        if let Some(bytes) = svgs.get(path) {
            return Ok(Some(Cow::Owned(bytes.to_vec())));
        }
        Ok(ICONS
            .iter()
            .find(|(name, _)| *name == path)
            .map(|(_, bytes)| Cow::Borrowed(*bytes)))
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let svgs = self.svgs.read().unwrap_or_else(PoisonError::into_inner);
        Ok(ICONS
            .iter()
            .map(|(name, _)| (*name).to_string())
            .chain(svgs.keys().cloned())
            .filter(|name| name.starts_with(path))
            .map(SharedString::from)
            .collect())
    }
}

//! The fonts and icons, built into the binary: the greeter runs before
//! anyone's fonts or icon themes are at hand.

use std::borrow::Cow;

use anyhow::Result;
use gpui_kit::{App, AssetSource, SharedString};

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

const ICONS: [(&str, &[u8]); 10] = assets![
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
];

pub fn register_fonts(cx: &mut App) {
    let fonts = FONTS
        .iter()
        .map(|(_, bytes)| Cow::Borrowed(*bytes))
        .collect();
    if let Err(error) = cx.text_system().add_fonts(fonts) {
        log::error!("cannot load the built-in fonts: {error:#}");
    }
}

pub struct Icons;

impl AssetSource for Icons {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        Ok(ICONS
            .iter()
            .find(|(name, _)| *name == path)
            .map(|(_, bytes)| Cow::Borrowed(*bytes)))
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        Ok(ICONS
            .iter()
            .filter(|(name, _)| name.starts_with(path))
            .map(|(name, _)| (*name).into())
            .collect())
    }
}

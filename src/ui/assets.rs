//! The fonts and icons, built into the binary: the greeter runs before
//! anyone's fonts or icon themes are at hand; and a theme folder's own
//! fonts and SVGs.

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use gpui_kit::{App, AssetSource, SharedString};

use crate::themes::{AssetKind, Folder};

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

/// The icons, and the SVGs a theme draws in a colour of its choosing.
pub struct Assets {
    theme: HashMap<String, Arc<[u8]>>,
}

impl Assets {
    pub fn new(folder: Option<&Folder>) -> Self {
        let theme = folder
            .into_iter()
            .flat_map(|folder| folder.assets.iter())
            .filter_map(|asset| match &asset.kind {
                AssetKind::Svg { bytes, .. } => {
                    Some((super::theme::folder::asset_key(&asset.path), bytes.clone()))
                }
                AssetKind::Raster(_) => None,
            })
            .collect();
        Self { theme }
    }
}

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if let Some(bytes) = self.theme.get(path) {
            return Ok(Some(Cow::Owned(bytes.to_vec())));
        }
        Ok(ICONS
            .iter()
            .find(|(name, _)| *name == path)
            .map(|(_, bytes)| Cow::Borrowed(*bytes)))
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        Ok(ICONS
            .iter()
            .map(|(name, _)| *name)
            .chain(self.theme.keys().map(String::as_str))
            .filter(|name| name.starts_with(path))
            .map(|name| name.to_string().into())
            .collect())
    }
}

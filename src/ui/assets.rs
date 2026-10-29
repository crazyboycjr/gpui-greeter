//! The fonts and icons, built into the binary: the greeter runs before
//! anyone's fonts or icon themes are at hand; a theme folder's own fonts
//! and SVGs; and gpui-kit's icons, which its components draw.

use std::borrow::Cow;
use std::sync::PoisonError;

use anyhow::Result;
use gpui_kit::{App, AssetSource, SharedString};

use super::looks::SvgAssets;
use crate::themes::{Folder, face_families};

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

/// The box of the outline that a built-in face of `family` at `weight`
/// draws for `ch`, in ems from the glyph's origin on the baseline, y up:
/// left, bottom, right, top. `None` unless that face is what drew it, as
/// glyph `id`.
pub fn outline(family: &str, weight: f32, ch: char, id: u32) -> Option<[f32; 4]> {
    FONTS.iter().find_map(|(_, bytes)| {
        let face = ttf_parser::Face::parse(bytes, 0).ok()?;
        let drew = f32::from(face.weight().to_number()) == weight
            && face_families(&face).iter().any(|name| name == family);
        let glyph = face
            .glyph_index(ch)
            .filter(|glyph| drew && u32::from(glyph.0) == id)?;
        let bounds = face.glyph_bounding_box(glyph)?;
        let em = f32::from(face.units_per_em());
        Some(
            [bounds.x_min, bounds.y_min, bounds.x_max, bounds.y_max]
                .map(|edge| f32::from(edge) / em),
        )
    })
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
        match ICONS.iter().find(|(name, _)| *name == path) {
            Some((_, bytes)) => Ok(Some(Cow::Borrowed(*bytes))),
            None => gpui_kit::assets::Assets.load(path),
        }
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let svgs = self.svgs.read().unwrap_or_else(PoisonError::into_inner);
        let mut names: Vec<SharedString> = ICONS
            .iter()
            .map(|(name, _)| (*name).to_string())
            .chain(svgs.keys().cloned())
            .filter(|name| name.starts_with(path))
            .map(SharedString::from)
            .collect();
        for name in gpui_kit::assets::Assets.list(path)? {
            if !names.contains(&name) {
                names.push(name);
            }
        }
        Ok(names)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measures_the_outlines_the_built_in_faces_draw() {
        let extra_light = FONTS
            .iter()
            .map(|(_, bytes)| ttf_parser::Face::parse(bytes, 0).unwrap())
            .find(|face| {
                face.weight().to_number() == 200 && face_families(face).contains(&"Inter".into())
            })
            .unwrap();
        let c = u32::from(extra_light.glyph_index('C').unwrap().0);
        let advance = f32::from(
            extra_light
                .glyph_hor_advance(ttf_parser::GlyphId(c as u16))
                .unwrap(),
        ) / f32::from(extra_light.units_per_em());
        let cap = f32::from(extra_light.capital_height().unwrap())
            / f32::from(extra_light.units_per_em());
        let [left, bottom, right, top] = outline("Inter", 200.0, 'C', c).unwrap();
        // A round capital overshoots its cap height and the baseline, and
        // its side bearings keep it inside its advance.
        assert!(bottom < 0.0 && top > cap, "{bottom} {top} {cap}");
        assert!(0.0 < left && left < right && right < advance);
        assert_eq!(outline("Inter", 200.0, 'C', c + 1), None);
        assert_eq!(outline("Inter", 600.0, 'C', c), None);
        assert_eq!(outline("Fraunces 17pt", 200.0, 'C', c), None);
    }
}

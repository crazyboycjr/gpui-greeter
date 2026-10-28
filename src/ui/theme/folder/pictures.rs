//! A theme's pictures, made ready before its windows open: pictures decoded
//! once for every window, and each SVG drawn in its own colours at the size
//! it takes on each monitor, kept on disk for the next start, as the drawn
//! backgrounds are.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use anyhow::Result;
use fast_image_resize::{FilterType, ResizeAlg, ResizeOptions, Resizer};
use gpui_kit::RenderImage;
use image::RgbaImage;

use crate::scenery::stored::Stored;
use crate::themes::{
    Animated, AssetId, AssetKind, FRAME, Folder, Kind, Layer, Picture, render_svg, sized,
};
use crate::wallpaper::render_image;

/// The theme's pictures, for every window.
#[derive(Default)]
pub struct Pictures {
    pub images: HashMap<AssetId, Arc<RenderImage>>,
}

/// The theme's SVGs drawn for one monitor.
#[derive(Default)]
pub struct MonitorPictures {
    drawn: Vec<(AssetId, (u32, u32), Arc<RenderImage>)>,
}

impl MonitorPictures {
    /// The drawing of `asset` closest to `pixels`.
    pub fn svg(&self, asset: AssetId, pixels: (f32, f32)) -> Option<&Arc<RenderImage>> {
        self.drawn
            .iter()
            .filter(|(id, _, _)| *id == asset)
            .min_by_key(|(_, size, _)| {
                let area = |w: f32, h: f32| (w * h).max(1.0).ln();
                ((area(size.0 as f32, size.1 as f32) - area(pixels.0, pixels.1)).abs() * 1000.0)
                    as u64
            })
            .map(|(_, _, image)| image)
    }
}

/// Decodes the theme's pictures for GPUI, each no larger than it can be
/// shown on the largest monitor.
pub fn decode(folder: &Folder, largest: (u32, u32)) -> Pictures {
    let mut images = HashMap::new();
    for (id, asset) in folder.assets.iter().enumerate() {
        let AssetKind::Raster(image) = &asset.kind else {
            continue;
        };
        let (width, height) = image.dimensions();
        let cover = (largest.0 as f32 / width as f32).max(largest.1 as f32 / height as f32);
        let image = if cover < 1.0 {
            let size = (
                ((width as f32 * cover).ceil() as u32).max(1),
                ((height as f32 * cover).ceil() as u32).max(1),
            );
            match shrink(image, size) {
                Ok(smaller) => smaller,
                Err(error) => {
                    log::warn!(
                        "theme {}: cannot shrink {}: {error:#}",
                        folder.name,
                        asset.path
                    );
                    (**image).clone()
                }
            }
        } else {
            (**image).clone()
        };
        images.insert(id, render_image(image));
    }
    Pictures { images }
}

fn shrink(image: &RgbaImage, (width, height): (u32, u32)) -> Result<RgbaImage> {
    let mut out = RgbaImage::new(width, height);
    let options = ResizeOptions::new().resize_alg(ResizeAlg::Convolution(FilterType::CatmullRom));
    Resizer::new().resize(image, &mut out, &options)?;
    Ok(out)
}

/// Draws the SVGs shown in their own colours for a monitor of `pixels`, or
/// reads them back from `cache`.
pub fn draw(folder: &Folder, pixels: (u32, u32), cache: Option<&Path>) -> MonitorPictures {
    let unit = (pixels.0 as f32 / FRAME.0).min(pixels.1 as f32 / FRAME.1);
    let mut wanted: Vec<(AssetId, (u32, u32), Picture)> = Vec::new();
    for (index, layer) in folder.layers.iter().enumerate() {
        let Kind::Svg {
            picture,
            tint: false,
        } = layer.kind
        else {
            continue;
        };
        let zoom = largest_zoom(&folder.layers, index);
        let looks = std::iter::once(&layer.base).chain(layer.overrides.iter().map(|o| &o.props));
        for props in looks {
            let Some(file) = props.file.or(layer.base.file) else {
                continue;
            };
            let width = props.width.or(layer.base.width);
            let height = props.height.or(layer.base.height);
            let aspect = folder.assets[file].aspect();
            let Some(size) = sized(&folder.layers, index, width, height, Some(aspect)) else {
                continue;
            };
            let device = |frame: f32| ((frame * unit * zoom).ceil() as u32).clamp(1, 16_384);
            let size = if layer.anchor == crate::themes::Anchor::Fill && layer.parent.is_none() {
                // A background covers the whole monitor, whatever its shape.
                let [top, right, bottom, left] = layer.inset;
                let inset = |a: f32, b: f32| ((a + b) * unit).round() as u32;
                (
                    pixels.0.saturating_sub(inset(left, right)).max(1),
                    pixels.1.saturating_sub(inset(top, bottom)).max(1),
                )
            } else {
                (device(size.0), device(size.1))
            };
            let entry = (file, size, picture);
            if !wanted.iter().any(|(f, s, _)| *f == file && *s == size) {
                wanted.push(entry);
            }
        }
    }
    let mut drawn = Vec::new();
    for (file, size, picture) in wanted {
        let asset = &folder.assets[file];
        let AssetKind::Svg { bytes, .. } = &asset.kind else {
            continue;
        };
        let stored = cache.map(|dir| {
            let name = asset.path.replace(['/', '.'], "_");
            let fit = format!("{:?}{:?}", picture.fit, picture.focus);
            Stored::at(
                dir.join(format!("{name}-{}x{}.rgba", size.0, size.1)),
                &[bytes, fit.as_bytes()],
                size,
            )
        });
        let pixels = match stored.as_ref().and_then(Stored::load) {
            Some(pixels) => RgbaImage::from_raw(size.0, size.1, pixels),
            None => match render_svg(bytes, size, picture) {
                Ok(image) => {
                    if let Some(stored) = &stored {
                        stored.save(image.as_raw());
                    }
                    Some(image)
                }
                Err(error) => {
                    log::error!(
                        "theme {}: cannot draw {}: {error:#}",
                        folder.name,
                        asset.path
                    );
                    None
                }
            },
        };
        if let Some(image) = pixels {
            drawn.push((file, size, render_image(image)));
        }
    }
    MonitorPictures { drawn }
}

/// The most a layer is ever scaled, with the layers around it, so its SVG
/// is drawn large enough not to blur.
fn largest_zoom(layers: &[Layer], index: usize) -> f32 {
    std::iter::successors(Some(index), |&at| layers[at].parent)
        .map(|at| {
            let layer = &layers[at];
            let looks =
                std::iter::once(&layer.base).chain(layer.overrides.iter().map(|o| &o.props));
            let scales = looks.filter_map(|props| props.scale);
            let motions = layer
                .motions
                .iter()
                .chain(layer.overrides.iter().flat_map(|o| o.motions.iter()))
                .filter(|motion| motion.property == Animated::Scale)
                .flat_map(|motion| motion.frames.iter().map(|frame| frame.value));
            let pane = layer
                .pane
                .as_ref()
                .map(|pane| pane.max_scale.max(pane.max_scale_full));
            let own = scales.chain(motions).fold(1.0f32, f32::max);
            own * pane.unwrap_or(1.0)
        })
        .product::<f32>()
        .clamp(1.0, 8.0)
}

//! The drawn backgrounds of Paper and Neon, from the mockups' SVG and CSS.
//! Each is drawn once for a monitor, at its size; what moves in it is left
//! to the window to animate over the picture.

mod grain;
mod neon;
mod paper;
mod random;
pub(crate) mod stored;

use std::path::Path;
use std::sync::Arc;

use anyhow::{Context as _, Result};
use gpui_kit::RenderImage;
use image::RgbaImage;
use resvg::{tiny_skia, usvg};

use crate::config::{Config, NeonBackground, PaperBackground, Rgb, Theme};
use crate::wallpaper::render_image;
use stored::Stored;

pub use neon::{Band, Floor, Pulse};

/// The mockups' frame, in their CSS pixels, which their drawings fill. The
/// drawings take a CSS pixel for a point.
pub const MOCKUP: (f64, f64) = (1600.0, 900.0);

/// A window measured in points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frame {
    pub width: f64,
    pub height: f64,
    /// The window's pixels per point.
    pub scale: f64,
    /// The window's size in its own pixels.
    pub pixels: (u32, u32),
}

impl Frame {
    /// A window of `pixels`, `scale` of them to a point.
    pub fn new(pixels: (u32, u32), scale: f64) -> Self {
        Self {
            width: f64::from(pixels.0) / scale,
            height: f64::from(pixels.1) / scale,
            scale,
            pixels,
        }
    }

    /// How much wider and taller than the mockup the frame is.
    fn growth(&self) -> (f64, f64) {
        (self.width - MOCKUP.0, self.height - MOCKUP.1)
    }

    /// The opening of an SVG document drawn in points.
    fn svg(&self) -> String {
        format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{}" height="{}" viewBox="0 0 {} {}">"#,
            self.pixels.0, self.pixels.1, self.width, self.height
        )
    }
}

/// The scenery a look draws.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Look {
    Paper {
        background: PaperBackground,
        night: bool,
    },
    Neon {
        background: NeonBackground,
        accent: Rgb,
    },
}

impl Look {
    /// What the configured theme draws, if it draws anything.
    pub fn of(config: &Config) -> Option<Self> {
        let paper = |night| {
            (config.paper.background != PaperBackground::Plain).then_some(Look::Paper {
                background: config.paper.background,
                night,
            })
        };
        match config.theme {
            Theme::Paper => paper(false),
            Theme::PaperNight => paper(true),
            Theme::Neon if config.neon.background != NeonBackground::Grid => Some(Look::Neon {
                background: config.neon.background,
                accent: config.neon.accent,
            }),
            Theme::Neon | Theme::Frost | Theme::Folder(_) => None,
        }
    }

    /// The name its drawing is kept under: one for each look and
    /// background, so that switching between them reads each back.
    fn key(&self) -> String {
        match self {
            Look::Paper { background, night } => {
                let night = if *night { "-night" } else { "" };
                format!("paper{night}-{}", background.name())
            }
            Look::Neon { background, .. } => format!("neon-{}", background.name()),
        }
    }
}

pub struct Scenery {
    pub picture: Arc<RenderImage>,
    pub motion: Motion,
}

/// What moves over the picture, in the points of its frame.
#[derive(Debug, Clone, PartialEq)]
pub enum Motion {
    Still,
    /// The tips of the skyline's antennas, which blink together.
    Beacons(Vec<(f64, f64)>),
    /// Light running along some of the circuit's traces.
    Pulses(Vec<Pulse>),
    /// The horizon's floor, whose crosswise lines run toward the viewer.
    Floor(Floor),
}

/// Draws `look` for a window of `pixels`, `unit` of them to a point,
/// or reads it back from the `cache` folder, where the pictures drawn
/// before are kept.
pub fn draw(look: Look, pixels: (u32, u32), unit: f64, cache: Option<&Path>) -> Result<Scenery> {
    let frame = Frame::new(pixels, unit);
    let (svg, motion) = match look {
        Look::Paper { background, night } => (paper::svg(background, night, &frame), Motion::Still),
        Look::Neon { background, accent } => neon::svg(background, accent.0, &frame),
    };
    let grain = matches!(look, Look::Paper { .. });
    let stored = cache.map(|dir| {
        // Before each look had a name of its own, every look kept its
        // drawing under the size alone, and they overwrote one another.
        let _ = std::fs::remove_file(dir.join(format!("{}x{}.rgba", pixels.0, pixels.1)));
        Stored::new(dir, &look.key(), &svg, pixels, grain)
    });
    let rgba = match stored.as_ref().and_then(Stored::load) {
        Some(rgba) => rgba,
        None => {
            let rgba = paint(&svg, &frame, grain)?;
            if let Some(stored) = &stored {
                stored.save(&rgba);
            }
            rgba
        }
    };
    // Every scenery is opaque, so its premultiplied pixels are plain ones.
    let picture = RgbaImage::from_raw(pixels.0, pixels.1, rgba)
        .context("the scenery's pixels do not fill its picture")?;
    Ok(Scenery {
        picture: render_image(picture),
        motion,
    })
}

fn paint(svg: &str, frame: &Frame, grain: bool) -> Result<Vec<u8>> {
    let mut canvas = rasterize(svg, frame.pixels)?;
    if grain {
        grain::spread(&mut canvas, frame.scale)?;
    }
    Ok(canvas.take())
}

fn rasterize(svg: &str, (width, height): (u32, u32)) -> Result<tiny_skia::Pixmap> {
    let tree = usvg::Tree::from_str(svg, &usvg::Options::default())?;
    let mut canvas =
        tiny_skia::Pixmap::new(width, height).context("cannot make a canvas this large")?;
    let size = tree.size();
    let fit = tiny_skia::Transform::from_scale(
        width as f32 / size.width(),
        height as f32 / size.height(),
    );
    resvg::render(&tree, fit, &mut canvas.as_mut());
    Ok(canvas)
}

/// `#rrggbb`.
fn hex(rgb: u32) -> String {
    format!("#{rgb:06x}")
}

/// `color` at `alpha` laid over the opaque `ground`: what a translucent CSS
/// colour stop shows over the page under it.
fn over(color: u32, alpha: f64, ground: u32) -> u32 {
    let channel = |shift: u32| {
        let (top, under) = (
            f64::from((color >> shift) & 0xFF),
            f64::from((ground >> shift) & 0xFF),
        );
        ((top * alpha + under * (1.0 - alpha)).round() as u32) << shift
    };
    channel(16) | channel(8) | channel(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_measure_the_window_in_mockup_pixels() {
        let near = |a: f64, b: f64| (a - b).abs() < 1e-9;
        let tall = Frame::new((1920, 1200), 1.2);
        assert!(near(tall.width, 1600.0) && near(tall.height, 1000.0));
        let (wider, taller) = tall.growth();
        assert!(near(wider, 0.0) && near(taller, 100.0));
    }

    #[test]
    fn translucent_stops_settle_on_the_ground() {
        assert_eq!(over(0x45F0FF, 0.09, 0x06070A), 0x0C1C20);
        assert_eq!(over(0xFFFFFF, 1.0, 0x000000), 0xFFFFFF);
    }

    #[test]
    fn draws_each_look_at_a_small_size() {
        let paper = |background, night| Look::Paper { background, night };
        let neon = |background| Look::Neon {
            background,
            accent: Rgb(0x45F0FF),
        };
        for look in [
            paper(PaperBackground::Desk, false),
            paper(PaperBackground::Desk, true),
            paper(PaperBackground::Ledger, false),
            paper(PaperBackground::Drafting, true),
            neon(NeonBackground::Skyline),
            neon(NeonBackground::Circuit),
            neon(NeonBackground::Horizon),
        ] {
            let scenery = draw(look, (320, 180), 0.2, None).unwrap();
            let size = scenery.picture.size(0);
            assert_eq!((size.width.0, size.height.0), (320, 180), "{look:?}");
            let pixels = scenery.picture.as_bytes(0).unwrap();
            assert!(
                pixels.chunks(4).all(|pixel| pixel[3] == 255),
                "{look:?} leaves holes"
            );
        }
    }
}

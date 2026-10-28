//! The drawn backgrounds of Paper and Neon, from the mockups' SVG and CSS.
//! Each is drawn once for a monitor, at its size; what moves in it is left
//! to the window to animate over the picture.

mod grain;
mod neon;
mod paper;
mod random;

use std::sync::Arc;

use anyhow::{Context as _, Result};
use gpui_kit::RenderImage;
use image::RgbaImage;
use resvg::{tiny_skia, usvg};

use crate::config::{Config, NeonBackground, PaperBackground, Rgb, Theme};
use crate::wallpaper::render_image;

pub use neon::{Band, Floor, Pulse};

/// The mockups' frame, in their CSS pixels.
pub const MOCKUP: (f64, f64) = (1600.0, 900.0);

/// A window measured in the mockups' pixels: the mockups' frame at the
/// largest size that fits, grown along one side to the window's shape.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frame {
    pub width: f64,
    pub height: f64,
    /// The window's pixels per mockup pixel.
    pub scale: f64,
    /// The window's size in its own pixels.
    pub pixels: (u32, u32),
}

impl Frame {
    pub fn fit(pixels: (u32, u32)) -> Self {
        let (width, height) = (f64::from(pixels.0), f64::from(pixels.1));
        let scale = (width / MOCKUP.0).min(height / MOCKUP.1);
        Self {
            width: width / scale,
            height: height / scale,
            scale,
            pixels,
        }
    }

    /// How much wider and taller than the mockup the frame is.
    fn growth(&self) -> (f64, f64) {
        (self.width - MOCKUP.0, self.height - MOCKUP.1)
    }

    /// The opening of an SVG document drawn in the mockups' pixels.
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
            Theme::Neon | Theme::Frost => None,
        }
    }
}

pub struct Scenery {
    pub picture: Arc<RenderImage>,
    pub motion: Motion,
}

/// What moves over the picture, in the mockups' pixels of its frame.
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

/// Draws `look` for a window of `pixels`.
pub fn draw(look: Look, pixels: (u32, u32)) -> Result<Scenery> {
    let frame = Frame::fit(pixels);
    let (svg, motion) = match look {
        Look::Paper { background, night } => (paper::svg(background, night, &frame), Motion::Still),
        Look::Neon { background, accent } => neon::svg(background, accent.0, &frame),
    };
    let mut canvas = rasterize(&svg, pixels)?;
    if matches!(look, Look::Paper { .. }) {
        grain::spread(&mut canvas, frame.scale)?;
    }
    // Every scenery is opaque, so its premultiplied pixels are plain ones.
    let picture = RgbaImage::from_raw(pixels.0, pixels.1, canvas.take())
        .context("the scenery's pixels do not fill its picture")?;
    Ok(Scenery {
        picture: render_image(picture),
        motion,
    })
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
    fn frames_grow_along_one_side() {
        let near = |a: f64, b: f64| (a - b).abs() < 1e-9;
        let wide = Frame::fit((3840, 1600));
        assert!(near(wide.width, 2160.0) && near(wide.height, 900.0));
        let tall = Frame::fit((1920, 1200));
        assert!(near(tall.width, 1600.0) && near(tall.height, 1000.0) && near(tall.scale, 1.2));
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
            let scenery = draw(look, (320, 180)).unwrap();
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

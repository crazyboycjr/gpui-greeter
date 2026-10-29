//! Frost's wallpaper: which picture, and its sharp and blurred forms for a
//! monitor, made once in the background when the greeter starts.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context as _, Result};
use fast_image_resize::{FilterType, ResizeAlg, ResizeOptions, Resizer};
use gpui_kit::RenderImage;
use image::{DynamicImage, ImageDecoder as _, ImageReader, RgbaImage, imageops};

use crate::config::Pick;

const EXTENSIONS: [&str; 4] = ["jpg", "jpeg", "png", "webp"];

/// The picture format `head`, a file's first bytes, shows, as the
/// extension it goes by.
pub fn picture_format(head: &[u8]) -> Option<&'static str> {
    if head.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("jpg")
    } else if head.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("png")
    } else if head.len() >= 12 && &head[..4] == b"RIFF" && &head[8..12] == b"WEBP" {
        Some("webp")
    } else {
        None
    }
}

pub fn is_picture_name(name: &str) -> bool {
    Path::new(name)
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| EXTENSIONS.contains(&ext.to_lowercase().as_str()))
}

/// The pictures in `dir` the greeter may show, by name.
pub fn candidates(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut pictures: Vec<PathBuf> = entries
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .filter(|entry| is_picture_name(&entry.file_name().to_string_lossy()))
        .map(|entry| entry.path())
        .collect();
    pictures.sort();
    pictures
}

/// The picture `pick` names among `pictures` now: a random one differs from
/// one start to the next, a daily one from one day to the next.
pub fn picked(pictures: &[PathBuf], pick: &Pick) -> Option<PathBuf> {
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |time| {
            time.as_nanos() as u64 ^ u64::from(std::process::id())
        });
    let day = jiff::Zoned::now()
        .date()
        .since(jiff::civil::date(2000, 1, 1))
        .map_or(0, |span| span.get_days().into());
    choose(pictures, pick, seed, day)
}

/// The picture `pick` names among `pictures`; `seed` varies the random one
/// and `day` the daily one.
pub fn choose(pictures: &[PathBuf], pick: &Pick, seed: u64, day: i64) -> Option<PathBuf> {
    if pictures.is_empty() {
        return None;
    }
    let at = |n: u64| pictures[(n % pictures.len() as u64) as usize].clone();
    match pick {
        Pick::File(name) => pictures
            .iter()
            .find(|path| path.file_name().is_some_and(|file| file == name.as_str()))
            .cloned()
            .or_else(|| Some(at(seed))),
        Pick::Daily => Some(at(day.unsigned_abs())),
        Pick::Random | Pick::Bing | Pick::BingFile(_) => Some(at(seed)),
    }
}

/// A decoded picture, turned the way its camera meant.
pub struct Picture(RgbaImage);

impl Picture {
    pub fn open(path: &Path) -> Result<Self> {
        let mut decoder = ImageReader::open(path)?
            .with_guessed_format()?
            .into_decoder()?;
        let orientation = decoder.orientation()?;
        let mut image = DynamicImage::from_decoder(decoder)
            .with_context(|| format!("cannot decode {}", path.display()))?;
        image.apply_orientation(orientation);
        Ok(Self(image.into_rgba8()))
    }

    pub fn dimensions(&self) -> (u32, u32) {
        self.0.dimensions()
    }

    /// The picture cropped about its centre and scaled to fill `size`
    /// exactly, like CSS's `object-fit: cover`.
    pub fn cover(&self, (width, height): (u32, u32)) -> Result<RgbaImage> {
        let mut out = RgbaImage::new(width.max(1), height.max(1));
        let options = ResizeOptions::new()
            .fit_into_destination(None)
            .resize_alg(ResizeAlg::Convolution(FilterType::CatmullRom))
            .use_alpha(false);
        Resizer::new().resize(&self.0, &mut out, &options)?;
        Ok(out)
    }
}

/// What Frost draws behind one monitor: the picture as it is, as it sits
/// behind the login form, and a softer blur between the two, which the
/// change from one to the other passes through.
#[derive(Clone)]
pub struct Backdrop {
    pub sharp: Arc<RenderImage>,
    pub soft: Arc<RenderImage>,
    pub blurred: Arc<RenderImage>,
}

/// The blurred forms are made at a quarter of the size, which the GPU's
/// filtering scales back up without a trace, blurred as they are.
const BLUR_DOWNSCALE: u32 = 4;

impl Backdrop {
    /// `blur` is the Gaussian's standard deviation in device pixels;
    /// `brightness` scales the blurred form's colour.
    pub fn new(picture: &Picture, size: (u32, u32), blur: f32, brightness: f32) -> Result<Self> {
        let sharp = picture.cover(size)?;
        let small = imageops::resize(
            &sharp,
            (size.0 / BLUR_DOWNSCALE).max(1),
            (size.1 / BLUR_DOWNSCALE).max(1),
            imageops::FilterType::Triangle,
        );
        let blurred = |sigma: f32, brightness: f32| {
            let mut image = imageops::fast_blur(&small, sigma / BLUR_DOWNSCALE as f32);
            for pixel in image.pixels_mut() {
                for channel in &mut pixel.0[..3] {
                    *channel = (f32::from(*channel) * brightness).round().min(255.0) as u8;
                }
            }
            render_image(image)
        };
        Ok(Self {
            soft: blurred(blur / 3.0, (1.0 + brightness) / 2.0),
            blurred: blurred(blur, brightness),
            sharp: render_image(sharp),
        })
    }
}

/// GPUI draws pictures from BGRA pixels.
pub fn render_image(mut image: RgbaImage) -> Arc<RenderImage> {
    for pixel in image.pixels_mut() {
        pixel.0.swap(0, 2);
    }
    Arc::new(RenderImage::new([image::Frame::new(image)]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_a_named_daily_or_random_picture() {
        let pictures: Vec<PathBuf> = ["a.jpg", "b.png", "c.webp"]
            .iter()
            .map(PathBuf::from)
            .collect();
        assert_eq!(
            choose(&pictures, &Pick::File("b.png".into()), 0, 0),
            Some("b.png".into())
        );
        assert_eq!(choose(&pictures, &Pick::Daily, 99, 4), Some("b.png".into()));
        assert_eq!(
            choose(&pictures, &Pick::Random, 5, 0),
            Some("c.webp".into())
        );
        assert_eq!(choose(&[], &Pick::Random, 5, 0), None);
    }

    #[test]
    fn covers_without_stretching() {
        // A red band in the middle of a wide black picture.
        let image = RgbaImage::from_fn(400, 100, |x, _| {
            image::Rgba([if (175..225).contains(&x) { 255 } else { 0 }, 0, 0, 255])
        });
        let covered = Picture(image).cover((100, 100)).unwrap();
        assert_eq!(covered.dimensions(), (100, 100));
        assert!(covered.get_pixel(50, 50).0[0] > 245);
        assert!(covered.get_pixel(2, 50).0[0] < 10);
    }
}

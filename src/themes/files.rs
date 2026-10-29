//! A theme's files: found only inside its folder and read with limits, so a
//! theme can neither reach other files nor make the greeter allocate
//! without bound.

use std::io::Read as _;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use anyhow::{Context as _, Result};
use image::{ImageReader, RgbaImage};
use resvg::{tiny_skia, usvg};

use super::layer::{Fit, Picture};

/// The largest file a theme may hold.
pub const MAX_FILE: u64 = 64 << 20;
/// The largest side of a picture, and the most pixels in one.
pub const MAX_SIDE: u32 = 16_384;
pub const MAX_PIXELS: u64 = 64 << 20;

/// A picture or an SVG a theme draws.
pub struct Asset {
    /// Relative to the theme folder, as theme.toml names it.
    pub path: String,
    pub kind: AssetKind,
}

pub enum AssetKind {
    Raster(Arc<RgbaImage>),
    Svg {
        bytes: Arc<[u8]>,
        /// Its own size, in its own units.
        size: (f32, f32),
    },
}

impl Asset {
    /// Width over height.
    pub fn aspect(&self) -> f32 {
        let (width, height) = match &self.kind {
            AssetKind::Raster(image) => (image.width() as f32, image.height() as f32),
            AssetKind::Svg { size, .. } => *size,
        };
        if height > 0.0 { width / height } else { 1.0 }
    }

    /// Its own size: a picture's pixels, an SVG's units.
    pub fn natural_size(&self) -> (f32, f32) {
        match &self.kind {
            AssetKind::Raster(image) => (image.width() as f32, image.height() as f32),
            AssetKind::Svg { size, .. } => *size,
        }
    }
}

pub struct FontFile {
    pub path: String,
    pub bytes: Arc<[u8]>,
    /// The family names its faces answer to.
    pub families: Vec<String>,
    pub faces: Vec<FontFace>,
}

/// A face of a font file, as the greeter's text system matches it.
pub struct FontFace {
    /// Its typographic family names, or else its family names.
    pub families: Vec<String>,
    pub weight: u16,
    pub italic: bool,
    /// The text system takes no face without an "m" for a layer's font
    /// or its fallback.
    pub has_m: bool,
}

/// `relative` inside `dir`, as long as it names a regular file there:
/// never an absolute path, `..`, or a link that leads out.
pub fn resolve(dir: &Path, relative: &str) -> Result<PathBuf, String> {
    let full = inside(dir, relative)?;
    if !full.is_file() {
        return Err(format!("{relative} is not a file"));
    }
    Ok(full)
}

/// `relative` resolved inside `dir`: never an absolute path, `..`, or a
/// link that leads out.
fn inside(dir: &Path, relative: &str) -> Result<PathBuf, String> {
    let path = Path::new(relative);
    if relative.is_empty() {
        return Err("names no file".into());
    }
    if path.is_absolute()
        || path
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(format!(
            "{relative:?} must be a path inside the theme folder, without \"..\" or \".\""
        ));
    }
    let root = dir
        .canonicalize()
        .map_err(|error| format!("cannot open the theme folder: {error}"))?;
    let full = root
        .join(path)
        .canonicalize()
        .map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => format!("{relative} does not exist"),
            _ => format!("{relative}: {error}"),
        })?;
    if !full.starts_with(&root) {
        return Err(format!("{relative} leads out of the theme folder"));
    }
    Ok(full)
}

/// The pictures in `relative`, a folder inside `dir`, by name, as paths
/// relative to `dir`: those install-theme copies, so no hidden files and
/// no links.
pub fn pictures_in(dir: &Path, relative: &str) -> Result<Vec<String>, String> {
    let full = inside(dir, relative)?;
    let entries = std::fs::read_dir(&full).map_err(|error| match error.kind() {
        std::io::ErrorKind::NotADirectory => format!("{relative} is not a folder"),
        _ => format!("{relative}: {error}"),
    })?;
    let mut names: Vec<String> = entries
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| !name.starts_with('.') && is_raster(name))
        .collect();
    names.sort();
    Ok(names
        .into_iter()
        .map(|name| {
            Path::new(relative)
                .join(name)
                .to_string_lossy()
                .into_owned()
        })
        .collect())
}

fn is_raster(relative: &str) -> bool {
    matches!(
        extension(relative).as_str(),
        "png" | "jpg" | "jpeg" | "webp"
    )
}

/// Checks a picture as far as its header, without decoding it: a kind a
/// theme draws, inside the folder, and no larger than a picture may be.
pub fn check_picture(dir: &Path, relative: &str) -> Result<(), String> {
    if !is_raster(relative) {
        return Err(format!("{relative}: slides are PNG, JPEG or WebP pictures"));
    }
    let path = resolve(dir, relative)?;
    let length = std::fs::metadata(&path)
        .map_err(|error| format!("{relative}: {error}"))?
        .len();
    if length > MAX_FILE {
        return Err(format!("{relative} is larger than {} MiB", MAX_FILE >> 20));
    }
    let (width, height) = ImageReader::open(&path)
        .and_then(ImageReader::with_guessed_format)
        .map_err(|error| format!("{relative}: {error}"))?
        .into_dimensions()
        .map_err(|error| format!("{relative}: {error}"))?;
    let fits = width > 0
        && height > 0
        && width <= MAX_SIDE
        && height <= MAX_SIDE
        && u64::from(width) * u64::from(height) <= MAX_PIXELS;
    if !fits {
        return Err(format!(
            "{relative}: {width}×{height} pixels is too large a picture"
        ));
    }
    Ok(())
}

/// The bytes of `relative`, refusing files over `MAX_FILE`.
pub fn read(dir: &Path, relative: &str) -> Result<Vec<u8>, String> {
    let path = resolve(dir, relative)?;
    let file = std::fs::File::open(&path).map_err(|error| format!("{relative}: {error}"))?;
    let mut bytes = Vec::new();
    file.take(MAX_FILE + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("{relative}: {error}"))?;
    if bytes.len() as u64 > MAX_FILE {
        return Err(format!("{relative} is larger than {} MiB", MAX_FILE >> 20));
    }
    Ok(bytes)
}

fn extension(relative: &str) -> String {
    Path::new(relative)
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default()
        .to_lowercase()
}

pub fn is_font(relative: &str) -> bool {
    matches!(extension(relative).as_str(), "ttf" | "otf" | "ttc")
}

/// A picture or an SVG, decoded or parsed.
pub fn load_asset(dir: &Path, relative: &str) -> Result<Asset, String> {
    let bytes = read(dir, relative)?;
    let kind = match extension(relative).as_str() {
        "svg" => {
            let tree = parse_svg(&bytes).map_err(|error| format!("{relative}: {error}"))?;
            let size = tree.size();
            AssetKind::Svg {
                bytes: bytes.into(),
                size: (size.width(), size.height()),
            }
        }
        _ if is_raster(relative) => AssetKind::Raster(Arc::new(
            decode(&bytes).map_err(|error| format!("{relative}: {error:#}"))?,
        )),
        _ => {
            return Err(format!(
                "{relative}: themes draw PNG, JPEG, WebP and SVG files"
            ));
        }
    };
    Ok(Asset {
        path: relative.to_string(),
        kind,
    })
}

/// A picture's pixels, refusing one larger than a picture may be.
pub fn decode(bytes: &[u8]) -> Result<RgbaImage> {
    let mut reader = ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    limits.max_alloc = Some(MAX_PIXELS * 4);
    reader.limits(limits);
    let (width, height) = reader.into_dimensions()?;
    anyhow::ensure!(
        u64::from(width) * u64::from(height) <= MAX_PIXELS && width > 0 && height > 0,
        "{width}×{height} pixels is too large a picture"
    );
    let mut reader = ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format()?;
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(MAX_PIXELS * 4);
    reader.limits(limits);
    Ok(reader.decode().context("cannot decode it")?.into_rgba8())
}

/// Parses an SVG as the greeter draws it: references to other files are
/// refused, since a theme's SVG must hold all it draws.
pub fn parse_svg(bytes: &[u8]) -> Result<usvg::Tree, String> {
    let outside = Arc::new(Mutex::new(Vec::<String>::new()));
    let options = usvg::Options {
        image_href_resolver: usvg::ImageHrefResolver {
            resolve_data: usvg::ImageHrefResolver::default_data_resolver(),
            resolve_string: Box::new({
                let outside = outside.clone();
                move |href: &str, _: &usvg::Options| {
                    outside
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .push(href.to_string());
                    None
                }
            }),
        },
        ..usvg::Options::default()
    };
    let tree = usvg::Tree::from_data(bytes, &options).map_err(|error| error.to_string())?;
    let outside = outside.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(href) = outside.first() {
        return Err(format!(
            "it refers to {href:?}; an SVG must hold its pictures itself, as data: URLs"
        ));
    }
    let size = tree.size();
    if !(size.width() > 0.0 && size.height() > 0.0) {
        return Err("it has no size".into());
    }
    Ok(tree)
}

/// Draws an SVG in its own colours into `pixels`, fitted as `picture` says,
/// with straight (not premultiplied) alpha.
pub fn render_svg(bytes: &[u8], pixels: (u32, u32), picture: Picture) -> Result<RgbaImage> {
    let (width, height) = (pixels.0.clamp(1, MAX_SIDE), pixels.1.clamp(1, MAX_SIDE));
    anyhow::ensure!(
        u64::from(width) * u64::from(height) <= MAX_PIXELS,
        "{width}×{height} is too large to draw"
    );
    let tree = parse_svg(bytes).map_err(anyhow::Error::msg)?;
    let mut canvas =
        tiny_skia::Pixmap::new(width, height).context("cannot make a canvas this large")?;
    let size = tree.size();
    let (sx, sy) = (width as f32 / size.width(), height as f32 / size.height());
    let transform = match picture.fit {
        Fit::Stretch => tiny_skia::Transform::from_scale(sx, sy),
        Fit::Contain | Fit::Cover => {
            let scale = if picture.fit == Fit::Cover {
                sx.max(sy)
            } else {
                sx.min(sy)
            };
            let (drawn_w, drawn_h) = (size.width() * scale, size.height() * scale);
            let (fx, fy) = if picture.fit == Fit::Cover {
                picture.focus
            } else {
                (0.5, 0.5)
            };
            tiny_skia::Transform::from_scale(scale, scale).post_translate(
                (width as f32 - drawn_w) * fx,
                (height as f32 - drawn_h) * fy,
            )
        }
    };
    resvg::render(&tree, transform, &mut canvas.as_mut());
    let mut image = RgbaImage::new(width, height);
    for (out, pixel) in image.pixels_mut().zip(canvas.pixels()) {
        let color = pixel.demultiply();
        out.0 = [color.red(), color.green(), color.blue(), color.alpha()];
    }
    Ok(image)
}

/// A font file, with the family names it answers to.
pub fn load_font(dir: &Path, relative: &str) -> Result<FontFile, String> {
    if !is_font(relative) {
        return Err(format!("{relative}: fonts are TTF, OTF or TTC files"));
    }
    let bytes = read(dir, relative)?;
    let count = ttf_parser::fonts_in_collection(&bytes).unwrap_or(1);
    let mut faces = Vec::new();
    for index in 0..count.min(64) {
        let face = ttf_parser::Face::parse(&bytes, index)
            .map_err(|error| format!("{relative}: not a font ({error})"))?;
        faces.push(FontFace {
            families: face_families(&face),
            weight: face.weight().to_number(),
            italic: face.style() != ttf_parser::Style::Normal || face.italic_angle() != 0.0,
            has_m: face.glyph_index('m').is_some(),
        });
    }
    let mut families: Vec<String> = Vec::new();
    for family in faces.iter().flat_map(|face| &face.families) {
        if !families.contains(family) {
            families.push(family.clone());
        }
    }
    if families.is_empty() {
        return Err(format!("{relative}: the font names no family"));
    }
    Ok(FontFile {
        path: relative.to_string(),
        bytes: bytes.into(),
        families,
        faces,
    })
}

/// The names a face answers to, as the text system's font database reads
/// them: its typographic family, or its family when it has none.
pub fn face_families(face: &ttf_parser::Face) -> Vec<String> {
    let named = |id: u16| {
        let mut names: Vec<String> = Vec::new();
        for name in face.names() {
            if let Some(family) = name.to_string().filter(|_| name.name_id == id)
                && !names.contains(&family)
            {
                names.push(family);
            }
        }
        names
    };
    let typographic = named(ttf_parser::name_id::TYPOGRAPHIC_FAMILY);
    if typographic.is_empty() {
        named(ttf_parser::name_id::FAMILY)
    } else {
        typographic
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("gpui-greeter-files-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("art")).unwrap();
        dir
    }

    #[test]
    fn stays_inside_the_folder() {
        let dir = scratch("inside");
        std::fs::write(dir.join("art/a.svg"), "<svg/>").unwrap();
        std::fs::write(dir.parent().unwrap().join("outside.svg"), "<svg/>").unwrap();
        std::os::unix::fs::symlink("../outside.svg", dir.join("link.svg")).unwrap();
        assert!(resolve(&dir, "art/a.svg").is_ok());
        for wrong in [
            "",
            "/etc/passwd",
            "../outside.svg",
            "art/../art/a.svg",
            "./art/a.svg",
            "link.svg",
            "art",
            "missing.svg",
        ] {
            assert!(resolve(&dir, wrong).is_err(), "{wrong}");
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn svgs_hold_what_they_draw() {
        let inline = br##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="20"><rect width="10" height="20" fill="#f00"/></svg>"##;
        let tree = parse_svg(inline).unwrap();
        assert_eq!((tree.size().width(), tree.size().height()), (10.0, 20.0));
        let outside = br#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="10" height="10"><image width="10" height="10" xlink:href="/etc/hostname"/></svg>"#;
        assert!(parse_svg(outside).unwrap_err().contains("/etc/hostname"));
        assert!(parse_svg(b"not svg").is_err());
    }

    #[test]
    fn draws_svgs_fitted() {
        let half = br##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10"><rect width="10" height="10" fill="#ff0000"/></svg>"##;
        let contain = Picture {
            fit: Fit::Contain,
            focus: (0.5, 0.5),
        };
        let image = render_svg(half, (40, 40), contain).unwrap();
        // 40 wide and 20 tall, in the middle; the red half on its left.
        assert_eq!(image.get_pixel(5, 20).0, [255, 0, 0, 255]);
        assert_eq!(image.get_pixel(30, 20).0[3], 0);
        assert_eq!(image.get_pixel(5, 5).0[3], 0);
    }

    #[test]
    fn refuses_pictures_too_large_to_hold() {
        let dir = scratch("large");
        let png = |width, height| {
            let mut bytes = Vec::new();
            RgbaImage::new(width, height)
                .write_to(
                    &mut std::io::Cursor::new(&mut bytes),
                    image::ImageFormat::Png,
                )
                .unwrap();
            bytes
        };
        std::fs::write(dir.join("ok.png"), png(4, 2)).unwrap();
        std::fs::write(dir.join("wide.png"), png(MAX_SIDE + 1, 1)).unwrap();
        let ok = load_asset(&dir, "ok.png").unwrap();
        assert_eq!(ok.aspect(), 2.0);
        assert!(load_asset(&dir, "wide.png").is_err());
        std::fs::write(dir.join("notes.txt"), "hello").unwrap();
        assert!(load_asset(&dir, "notes.txt").is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn names_a_fonts_families() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets");
        let font = load_font(&dir, "fonts/Inter-Light.ttf").unwrap();
        assert!(
            font.families.iter().any(|family| family == "Inter"),
            "{:?}",
            font.families
        );
        assert!(load_font(&dir, "icons/moon.svg").is_err());
    }
}

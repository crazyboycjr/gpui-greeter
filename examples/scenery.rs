//! Draws a look's scenery to a picture, to compare it with the mockups.
//!
//!     cargo run --example scenery -- LOOK OUT.png [WIDTHxHEIGHT]
//!
//! LOOK is paper-desk, paper-night-ledger, neon-circuit and so on: a theme
//! and a background.

use gpui_greeter::config::{NeonBackground, PaperBackground, Rgb};
use gpui_greeter::scenery::{self, Look};

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [look, out, rest @ ..] = &args[..] else {
        anyhow::bail!("usage: scenery LOOK OUT.png [WIDTHxHEIGHT]");
    };
    let (width, height) = rest
        .first()
        .and_then(|size| size.split_once('x'))
        .map(|(w, h)| Ok::<_, anyhow::Error>((w.parse()?, h.parse()?)))
        .transpose()?
        .unwrap_or((3840, 2160));
    let look = parse(look).ok_or_else(|| anyhow::anyhow!("no look {look:?}"))?;
    let started = std::time::Instant::now();
    let scenery = scenery::draw(look, (width, height), None)?;
    eprintln!("drawn in {:?}", started.elapsed());
    let mut pixels = scenery.picture.as_bytes(0).unwrap().to_vec();
    for pixel in pixels.chunks_mut(4) {
        pixel.swap(0, 2);
    }
    image::RgbaImage::from_raw(width, height, pixels)
        .unwrap()
        .save(out)?;
    Ok(())
}

fn parse(name: &str) -> Option<Look> {
    let paper = |night, background: &str| {
        let background = match background {
            "desk" => PaperBackground::Desk,
            "ledger" => PaperBackground::Ledger,
            "drafting" => PaperBackground::Drafting,
            _ => return None,
        };
        Some(Look::Paper { background, night })
    };
    if let Some(background) = name.strip_prefix("paper-night-") {
        return paper(true, background);
    }
    if let Some(background) = name.strip_prefix("paper-") {
        return paper(false, background);
    }
    let background = match name.strip_prefix("neon-")? {
        "skyline" => NeonBackground::Skyline,
        "circuit" => NeonBackground::Circuit,
        "horizon" => NeonBackground::Horizon,
        _ => return None,
    };
    Some(Look::Neon {
        background,
        accent: Rgb(0x45F0FF),
    })
}

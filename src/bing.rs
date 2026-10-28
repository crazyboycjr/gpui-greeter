//! Bing's picture of the day for Frost, fetched in the background and kept
//! in the greeter's cache, one file per day.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context as _, Result, ensure};
use serde::Deserialize;

use crate::wallpaper::picture_format;

/// The days of pictures kept.
const KEEP: usize = 7;
/// Bing's UHD pictures run to a few megabytes.
const LARGEST: u64 = 40 << 20;

/// The picture fetched on `day` (as `2026-10-27`), if it was.
pub fn of_day(dir: &Path, day: &str) -> Option<PathBuf> {
    Some(dir.join(format!("{day}.jpg"))).filter(|path| path.is_file())
}

/// The most recent picture fetched on any day.
pub fn latest(dir: &Path) -> Option<PathBuf> {
    pictures(dir).pop()
}

/// Fetches the picture of the day as the one of `day`, and forgets all but
/// the last week's. `url` is Bing's archive, whose answer names the picture,
/// or any address that answers with a picture itself.
pub fn fetch(url: &str, dir: &Path, day: &str) -> Result<PathBuf> {
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(60)))
        .build()
        .new_agent();
    let mut bytes = get(&agent, url)?;
    if picture_format(&bytes).is_none() {
        bytes = get(&agent, &picture_url(url, &bytes)?)?;
    }
    ensure!(
        picture_format(&bytes).is_some(),
        "{url} did not lead to a picture"
    );
    std::fs::create_dir_all(dir)?;
    let path = dir.join(format!("{day}.jpg"));
    let partial = dir.join(format!("{day}.partial"));
    std::fs::write(&partial, &bytes)?;
    std::fs::rename(&partial, &path)?;
    let pictures = pictures(dir);
    for old in &pictures[..pictures.len().saturating_sub(KEEP)] {
        let _ = std::fs::remove_file(old);
    }
    Ok(path)
}

fn get(agent: &ureq::Agent, url: &str) -> Result<Vec<u8>> {
    let mut response = agent
        .get(url)
        .call()
        .with_context(|| format!("cannot fetch {url}"))?;
    Ok(response
        .body_mut()
        .with_config()
        .limit(LARGEST)
        .read_to_vec()?)
}

/// The UHD picture that the archive's `answer` names, on the archive's host.
fn picture_url(archive: &str, answer: &[u8]) -> Result<String> {
    #[derive(Deserialize)]
    struct Answer {
        images: Vec<Image>,
    }
    #[derive(Deserialize)]
    struct Image {
        urlbase: String,
    }
    let answer: Answer = serde_json::from_slice(answer)
        .with_context(|| format!("{archive} answered with neither a picture nor Bing's archive"))?;
    let base = &answer
        .images
        .first()
        .with_context(|| format!("{archive} named no picture"))?
        .urlbase;
    let origin = archive
        .find("://")
        .map(|scheme| scheme + 3)
        .map(|host| {
            archive[host..]
                .find('/')
                .map_or(archive.len(), |path| host + path)
        })
        .map(|end| &archive[..end])
        .with_context(|| format!("{archive} is not a web address"))?;
    Ok(format!("{origin}{base}_UHD.jpg"))
}

/// The fetched pictures, oldest first: their names are their days.
fn pictures(dir: &Path) -> Vec<PathBuf> {
    let mut pictures: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|entries| entries.flatten().map(|entry| entry.path()).collect())
        .unwrap_or_default();
    pictures.retain(|path| path.extension().is_some_and(|ext| ext == "jpg"));
    pictures.sort();
    pictures
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_day_s_picture_or_the_latest() {
        let dir = std::env::temp_dir().join(format!("gpui-greeter-bing-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for day in ["2026-10-25", "2026-10-26"] {
            std::fs::write(dir.join(format!("{day}.jpg")), [0xFF, 0xD8, 0xFF]).unwrap();
        }
        std::fs::write(dir.join("2026-10-27.partial"), b"half").unwrap();
        assert_eq!(of_day(&dir, "2026-10-27"), None);
        assert_eq!(of_day(&dir, "2026-10-25"), Some(dir.join("2026-10-25.jpg")));
        assert_eq!(latest(&dir), Some(dir.join("2026-10-26.jpg")));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_archive_names_the_uhd_picture() {
        let answer = br#"{"images": [{"urlbase": "/th?id=OHR.Lake_EN-US0123", "url": "/th?id=OHR.Lake_EN-US0123_1920x1080.jpg"}]}"#;
        assert_eq!(
            picture_url(
                "https://www.bing.com/HPImageArchive.aspx?format=js&idx=0&n=1",
                answer
            )
            .unwrap(),
            "https://www.bing.com/th?id=OHR.Lake_EN-US0123_UHD.jpg"
        );
        assert!(picture_url("https://www.bing.com/", b"<html>").is_err());
        assert!(picture_url("https://www.bing.com/", br#"{"images": []}"#).is_err());
    }
}

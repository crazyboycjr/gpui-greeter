//! Bing's pictures of the day, kept in the Bing folder (`[wallpaper]
//! bing_dir`, `/usr/local/share/gpui-greeter/bing4k`): each under its day,
//! `2026-10-29.jpg`, beside `2026-10-29.json`, which holds its title and
//! copyright. The greeter and its daily timer fetch them as the greeter's
//! user, who owns the folder; the lock screen and the settings, run as
//! anyone, show what is there.

use std::fs;
use std::io::Write as _;
use std::os::unix::fs::{OpenOptionsExt as _, PermissionsExt as _};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context as _, Result, ensure};
use serde::{Deserialize, Serialize};

use crate::wallpaper::picture_format;

/// How a wallpaper pick names one of the pictures kept: `bing4k/` and its
/// file's name.
pub const PICK_PREFIX: &str = "bing4k/";
/// Bing's UHD pictures run to a few megabytes.
const LARGEST: u64 = 40 << 20;
/// The most pictures one answer keeps: the archive tells of eight days.
const MOST_AT_ONCE: usize = 8;

/// What Bing says of a picture.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct About {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub copyright: String,
    /// Bing's page about it.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub link: String,
}

/// A picture kept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Kept {
    pub path: PathBuf,
    /// Its file's name, which a pick gives after [`PICK_PREFIX`].
    pub name: String,
    /// Its day, as `2026-10-29`.
    pub day: String,
    pub about: Option<About>,
}

/// Whether `name` is one a picture is kept under: a day, then a picture's
/// extension.
pub fn is_kept_name(name: &str) -> bool {
    name.split_once('.')
        .is_some_and(|(day, extension)| ["jpg", "png", "webp"].contains(&extension) && is_day(day))
}

/// Whether `day` is a day written as `2026-10-29`, and only so.
fn is_day(day: &str) -> bool {
    day.len() == 10
        && day
            .parse::<jiff::civil::Date>()
            .is_ok_and(|date| date.to_string() == day)
}

/// The pictures kept in `dir`, the newest first.
pub fn kept(dir: &Path) -> Vec<Kept> {
    let mut kept: Vec<Kept> = names(dir)
        .into_iter()
        .map(|name| {
            let day = name.split_once('.').map_or("", |(day, _)| day).to_string();
            let about = fs::read(dir.join(format!("{day}.json")))
                .ok()
                .and_then(|bytes| serde_json::from_slice(&bytes).ok());
            Kept {
                path: dir.join(&name),
                name,
                day,
                about,
            }
        })
        .collect();
    kept.sort_by(|a, b| b.name.cmp(&a.name));
    kept
}

/// The newest picture kept in `dir`.
pub fn latest(dir: &Path) -> Option<PathBuf> {
    names(dir).into_iter().max().map(|name| dir.join(name))
}

/// The picture kept in `dir` under `name`, if there is one.
pub fn named(dir: &Path, name: &str) -> Option<PathBuf> {
    let path = dir.join(name);
    let there = fs::symlink_metadata(&path).is_ok_and(|meta| meta.is_file());
    (is_kept_name(name) && there).then_some(path)
}

/// The day a picture kept is of.
pub fn day_of(path: &Path) -> Option<&str> {
    let name = path.file_name()?.to_str()?;
    is_kept_name(name).then(|| name.split_once('.').map_or(name, |(day, _)| day))
}

/// The names of the pictures in `dir`: files alone, not links.
fn names(dir: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| is_kept_name(name))
        .collect()
}

/// Asks `url` for Bing's picture of the day and keeps in `dir` each picture
/// its answer names that is not kept yet, with what Bing says of it. `url`
/// is Bing's archive, whose answer names the pictures and their days, or
/// any address that answers with a picture itself, kept as `today`'s.
/// Returns the pictures kept now, the oldest first.
pub fn fetch(url: &str, dir: &Path, today: &str) -> Result<Vec<PathBuf>> {
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(60)))
        .build()
        .new_agent();
    let answer = get(&agent, url)?;
    let mut added = Vec::new();
    if let Some(format) = picture_format(&answer) {
        let name = format!("{today}.{format}");
        if named(dir, &name).is_none() {
            keep(dir, &name, &answer, None)?;
            added.push(dir.join(name));
        }
        return Ok(added);
    }
    let pictures = archive(url, &answer)?;
    for picture in pictures.into_iter().take(MOST_AT_ONCE).rev() {
        let name = format!("{}.jpg", picture.day);
        if named(dir, &name).is_some() {
            continue;
        }
        let bytes = get(&agent, &picture.url)?;
        ensure!(
            picture_format(&bytes) == Some("jpg"),
            "{} is not a JPEG picture",
            picture.url
        );
        keep(dir, &name, &bytes, Some(&picture.about))?;
        added.push(dir.join(name));
    }
    Ok(added)
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

/// A picture the archive names: its day, its UHD picture's address and
/// what Bing says of it.
#[derive(Debug, PartialEq)]
struct Named {
    day: String,
    url: String,
    about: About,
}

/// The pictures the archive's `answer` names, the newest first, each at its
/// UHD size on the archive's host.
fn archive(archive: &str, answer: &[u8]) -> Result<Vec<Named>> {
    #[derive(Deserialize)]
    struct Answer {
        images: Vec<Image>,
    }
    #[derive(Deserialize)]
    struct Image {
        startdate: String,
        urlbase: String,
        #[serde(default)]
        title: String,
        #[serde(default)]
        copyright: String,
        #[serde(default)]
        copyrightlink: String,
    }
    let answer: Answer = serde_json::from_slice(answer)
        .with_context(|| format!("{archive} answered with neither a picture nor Bing's archive"))?;
    ensure!(!answer.images.is_empty(), "{archive} named no picture");
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
    answer
        .images
        .into_iter()
        .map(|image| {
            let date = &image.startdate;
            let day = (date.len() == 8 && date.is_ascii())
                .then(|| format!("{}-{}-{}", &date[..4], &date[4..6], &date[6..]))
                .filter(|day| is_day(day))
                .with_context(|| format!("{archive} gave a picture the day {date:?}"))?;
            ensure!(
                image.urlbase.starts_with('/'),
                "{archive} named a picture elsewhere: {:?}",
                image.urlbase
            );
            Ok(Named {
                day,
                url: format!("{origin}{}_UHD.jpg", image.urlbase),
                about: About {
                    title: image.title,
                    copyright: image.copyright,
                    link: image.copyrightlink,
                },
            })
        })
        .collect()
}

/// Keeps `bytes` in `dir` as `name`, what Bing says of it first, each file
/// whole or not at all, and readable by everyone: the lock screen and the
/// settings show them as their users.
fn keep(dir: &Path, name: &str, bytes: &[u8], about: Option<&About>) -> Result<()> {
    fs::create_dir_all(dir).with_context(|| format!("cannot create {}", dir.display()))?;
    if let Some(about) = about {
        let day = name.split_once('.').map_or(name, |(day, _)| day);
        write_whole(
            &dir.join(format!("{day}.json")),
            &serde_json::to_vec_pretty(about)?,
        )?;
    }
    write_whole(&dir.join(name), bytes)
}

fn write_whole(path: &Path, bytes: &[u8]) -> Result<()> {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .context("no file name")?;
    let partial = path.with_file_name(format!(".{name}.partial"));
    let _ = fs::remove_file(&partial);
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o644)
        .open(&partial)
        .with_context(|| format!("cannot write {}", partial.display()))?;
    file.write_all(bytes)?;
    fs::set_permissions(&partial, fs::Permissions::from_mode(0o644))?;
    fs::rename(&partial, path).with_context(|| format!("cannot write {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::{BufRead as _, BufReader};
    use std::net::TcpListener;

    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("gpui-greeter-bing-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn jpeg() -> Vec<u8> {
        let mut bytes = Vec::new();
        image::RgbImage::from_pixel(16, 9, image::Rgb([40, 90, 140]))
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Jpeg,
            )
            .unwrap();
        bytes
    }

    #[test]
    fn keeps_pictures_under_their_days_alone() {
        for name in ["2026-10-29.jpg", "2026-02-28.png", "2025-12-31.webp"] {
            assert!(is_kept_name(name), "{name}");
        }
        for name in [
            "2026-10-29.jpeg",
            "2026-13-01.jpg",
            "2026-02-30.jpg",
            "26-10-29.jpg",
            "2026-10-29.jpg.partial",
            ".2026-10-29.jpg.partial",
            "2026-10-29.json",
            "../2026-10-29.jpg",
            "lake.jpg",
            "+002026-10-29.jpg",
        ] {
            assert!(!is_kept_name(name), "{name}");
        }
    }

    #[test]
    fn lists_the_pictures_kept_newest_first_with_what_bing_says() {
        let dir = scratch("kept");
        for day in ["2026-10-27", "2026-10-29", "2026-10-28"] {
            fs::write(dir.join(format!("{day}.jpg")), jpeg()).unwrap();
        }
        let about = About {
            title: "Where mist meets millennia".into(),
            copyright: "Standing stones (© A Photographer)".into(),
            link: String::new(),
        };
        fs::write(
            dir.join("2026-10-29.json"),
            serde_json::to_vec(&about).unwrap(),
        )
        .unwrap();
        fs::write(dir.join(".2026-10-30.jpg.partial"), b"half").unwrap();
        fs::write(dir.join("notes.txt"), b"not a picture").unwrap();
        std::os::unix::fs::symlink(dir.join("2026-10-27.jpg"), dir.join("2026-10-31.jpg")).unwrap();

        let kept = kept(&dir);
        let days: Vec<&str> = kept.iter().map(|kept| kept.day.as_str()).collect();
        assert_eq!(days, ["2026-10-29", "2026-10-28", "2026-10-27"]);
        assert_eq!(kept[0].about.as_ref(), Some(&about));
        assert_eq!(kept[1].about, None);
        assert_eq!(latest(&dir), Some(dir.join("2026-10-29.jpg")));
        assert_eq!(
            named(&dir, "2026-10-28.jpg"),
            Some(dir.join("2026-10-28.jpg"))
        );
        assert_eq!(named(&dir, "2026-10-31.jpg"), None, "a link");
        assert_eq!(named(&dir, "../kept/2026-10-28.jpg"), None);
        assert_eq!(day_of(&dir.join("2026-10-28.jpg")), Some("2026-10-28"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn the_archive_names_each_day_s_uhd_picture() {
        let answer = r#"{"images": [{"startdate": "20261029", "urlbase": "/th?id=OHR.Stones_EN-US0123", "title": "Where mist meets millennia", "copyright": "Standing stones (© A Photographer)", "copyrightlink": "https://www.bing.com/search?q=stones"}]}"#;
        let named = archive(
            "https://www.bing.com/HPImageArchive.aspx?format=js&idx=0&n=1",
            answer.as_bytes(),
        )
        .unwrap();
        assert_eq!(
            named,
            [Named {
                day: "2026-10-29".into(),
                url: "https://www.bing.com/th?id=OHR.Stones_EN-US0123_UHD.jpg".into(),
                about: About {
                    title: "Where mist meets millennia".into(),
                    copyright: "Standing stones (© A Photographer)".into(),
                    link: "https://www.bing.com/search?q=stones".into(),
                },
            }]
        );
        let bing = "https://www.bing.com/";
        assert!(archive(bing, b"<html>").is_err());
        assert!(archive(bing, br#"{"images": []}"#).is_err());
        assert!(
            archive(
                bing,
                br#"{"images": [{"startdate": "2026-1029", "urlbase": "/a"}]}"#
            )
            .is_err()
        );
        assert!(
            archive(
                bing,
                br#"{"images": [{"startdate": "20261029", "urlbase": "evil"}]}"#
            )
            .is_err()
        );
    }

    /// A web server that answers each request for a path in `pages` with
    /// its bytes, and anything else with 404, until the test ends.
    fn serve(pages: Vec<(String, Vec<u8>)>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                let _ = reader.read_line(&mut line);
                let path = line.split_whitespace().nth(1).unwrap_or("").to_string();
                loop {
                    let mut header = String::new();
                    if reader.read_line(&mut header).unwrap_or(0) == 0 || header == "\r\n" {
                        break;
                    }
                }
                let page = pages.iter().find(|(at, _)| *at == path);
                let (status, body) = match page {
                    Some((_, body)) => ("200 OK", body.as_slice()),
                    None => ("404 Not Found", &b""[..]),
                };
                let head = format!(
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(head.as_bytes());
                let _ = stream.write_all(body);
            }
        });
        format!("http://{address}")
    }

    #[test]
    fn fetches_each_new_picture_once_with_what_bing_says() {
        let dir = scratch("fetch");
        let answer = r#"{"images": [{"startdate": "20261029", "urlbase": "/th?id=OHR.Stones", "title": "Stones", "copyright": "(© A Photographer)"}, {"startdate": "20261028", "urlbase": "/th?id=OHR.Lake", "title": "Lake", "copyright": "(© Another)"}]}"#;
        let host = serve(vec![
            ("/archive?n=2".into(), answer.as_bytes().to_vec()),
            ("/th?id=OHR.Stones_UHD.jpg".into(), jpeg()),
            ("/th?id=OHR.Lake_UHD.jpg".into(), jpeg()),
            ("/picture".into(), jpeg()),
        ]);
        let added = fetch(&format!("{host}/archive?n=2"), &dir, "2026-10-30").unwrap();
        assert_eq!(
            added,
            [dir.join("2026-10-28.jpg"), dir.join("2026-10-29.jpg")]
        );
        assert_eq!(fs::read(dir.join("2026-10-29.jpg")).unwrap(), jpeg());
        let kept = kept(&dir);
        assert_eq!(
            kept[0].about.as_ref().map(|about| about.title.as_str()),
            Some("Stones")
        );
        let mode = fs::metadata(dir.join("2026-10-29.jpg"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o644);

        // What is kept already is not fetched again.
        assert!(
            fetch(&format!("{host}/archive?n=2"), &dir, "2026-10-30")
                .unwrap()
                .is_empty()
        );
        // An address that answers with a picture gives today's.
        let added = fetch(&format!("{host}/picture"), &dir, "2026-10-30").unwrap();
        assert_eq!(added, [dir.join("2026-10-30.jpg")]);
        assert!(fetch(&format!("{host}/nothing"), &dir, "2026-10-31").is_err());
        let _ = fs::remove_dir_all(dir);
    }
}

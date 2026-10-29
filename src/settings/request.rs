//! What the settings app hands the root helper on its standard input: the
//! settings to change, the wallpapers to add, with their bytes, or to
//! remove, and whether the daily Bing timer is to be on. The pictures
//! travel as bytes so that the helper, which runs as root, never opens a
//! path the user names.
//!
//! A request is a line naming the format, a line of JSON, then the
//! pictures' bytes one after another, as long as the JSON says each is:
//!
//! ```text
//! gpui-greeter settings 1
//! {"set":{"theme":"frost"},"add":[{"name":"lake.jpg","size":48213}],"remove":["old.png"],"bing_timer":true}
//! <48213 bytes of lake.jpg>
//! ```

use std::collections::BTreeMap;
use std::io::{BufRead, Read, Write};

use anyhow::{Context as _, Result, bail, ensure};
use serde::{Deserialize, Serialize};

use super::setting::{Setting, plain_picture_name};

const MAGIC: &str = "gpui-greeter settings 1";
const MAX_HEADER: u64 = 64 << 10;
/// A picture larger than this is no wallpaper.
pub const MAX_PICTURE: u64 = 64 << 20;
const MAX_TOTAL: u64 = 512 << 20;
const MAX_PICTURES: usize = 64;
const MAX_REMOVALS: usize = 4096;

#[derive(Debug, Default, PartialEq)]
pub struct Request {
    pub settings: Vec<Setting>,
    pub add: Vec<NewPicture>,
    /// Names of pictures in the wallpaper folder.
    pub remove: Vec<String>,
    /// Turns the daily Bing timer on or off.
    pub bing_timer: Option<bool>,
}

#[derive(Debug, PartialEq)]
pub struct NewPicture {
    pub name: String,
    pub bytes: Vec<u8>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Header {
    #[serde(default)]
    set: BTreeMap<String, serde_json::Value>,
    #[serde(default)]
    add: Vec<Added>,
    #[serde(default)]
    remove: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    bing_timer: Option<bool>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Added {
    name: String,
    size: u64,
}

impl Request {
    pub fn is_empty(&self) -> bool {
        self.settings.is_empty()
            && self.add.is_empty()
            && self.remove.is_empty()
            && self.bing_timer.is_none()
    }

    pub fn write(&self, out: &mut impl Write) -> Result<()> {
        let header = Header {
            set: self
                .settings
                .iter()
                .map(|setting| (setting.key().to_string(), setting.json()))
                .collect(),
            add: self
                .add
                .iter()
                .map(|picture| Added {
                    name: picture.name.clone(),
                    size: picture.bytes.len() as u64,
                })
                .collect(),
            remove: self.remove.clone(),
            bing_timer: self.bing_timer,
        };
        writeln!(out, "{MAGIC}")?;
        serde_json::to_writer(&mut *out, &header)?;
        writeln!(out)?;
        for picture in &self.add {
            out.write_all(&picture.bytes)?;
        }
        out.flush()?;
        Ok(())
    }

    /// Reads a request, refusing anything malformed or too large before
    /// reading on: a name that could leave the wallpaper folder, a setting
    /// outside the closed set, more bytes than announced.
    pub fn read(input: &mut impl BufRead) -> Result<Self> {
        let magic = line(input, 64).context("not a settings request")?;
        ensure!(magic == MAGIC, "not a settings request");
        let header: Header = serde_json::from_str(&line(input, MAX_HEADER)?)
            .context("the request's header does not read")?;
        let settings = header
            .set
            .iter()
            .map(|(key, value)| Setting::parse(key, value))
            .collect::<Result<Vec<_>>>()?;
        ensure!(
            header.add.len() <= MAX_PICTURES,
            "more than {MAX_PICTURES} pictures at once"
        );
        ensure!(
            header.remove.len() <= MAX_REMOVALS,
            "more than {MAX_REMOVALS} removals at once"
        );
        let mut total = 0u64;
        for (index, added) in header.add.iter().enumerate() {
            ensure!(
                is_new_name(&added.name),
                "{:?} is not a name the settings give a new picture",
                added.name
            );
            ensure!(
                !header.add[..index]
                    .iter()
                    .any(|other| other.name == added.name),
                "{} is added twice",
                added.name
            );
            ensure!(
                added.size <= MAX_PICTURE,
                "{} is larger than {} MiB",
                added.name,
                MAX_PICTURE >> 20
            );
            total += added.size;
            ensure!(
                total <= MAX_TOTAL,
                "the pictures come to more than {} MiB",
                MAX_TOTAL >> 20
            );
        }
        for (index, name) in header.remove.iter().enumerate() {
            ensure!(plain_picture_name(name), "{name:?} names no wallpaper");
            ensure!(
                !header.remove[..index].contains(name),
                "{name} is removed twice"
            );
            ensure!(
                !header.add.iter().any(|added| added.name == *name),
                "{name} is both added and removed"
            );
        }
        let mut add = Vec::with_capacity(header.add.len());
        for added in header.add {
            let mut bytes = vec![0; added.size as usize];
            input
                .read_exact(&mut bytes)
                .with_context(|| format!("the request ends inside {}", added.name))?;
            add.push(NewPicture {
                name: added.name,
                bytes,
            });
        }
        let mut rest = [0u8; 1];
        ensure!(
            input.read(&mut rest)? == 0,
            "the request goes on past its pictures"
        );
        Ok(Request {
            settings,
            add,
            remove: header.remove,
            bing_timer: header.bing_timer,
        })
    }
}

/// One line of at most `limit` bytes, without its newline.
fn line(input: &mut impl BufRead, limit: u64) -> Result<String> {
    let mut bytes = Vec::new();
    input.take(limit + 1).read_until(b'\n', &mut bytes)?;
    if bytes.last() != Some(&b'\n') {
        bail!("a line of the request is too long or cut short");
    }
    bytes.pop();
    String::from_utf8(bytes).context("the request is not text where it should be")
}

/// Whether `name` is one the settings give a picture they add: lower-case
/// letters and digits in words joined by `-`, and the extension of a
/// picture format, as [`new_name`] makes them.
pub fn is_new_name(name: &str) -> bool {
    let Some((stem, extension)) = name.rsplit_once('.') else {
        return false;
    };
    ["jpg", "png", "webp"].contains(&extension)
        && (1..=60).contains(&stem.len())
        && stem.split('-').all(|word| {
            !word.is_empty()
                && word
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        })
}

/// The name a picture called `original`, of format `extension` (jpg, png
/// or webp), takes in the wallpaper folder, distinct from those `taken`.
pub fn new_name(original: &str, extension: &str, taken: impl Fn(&str) -> bool) -> String {
    let stem = match original.rsplit_once('.') {
        Some((stem, _)) if !stem.is_empty() => stem,
        _ => original,
    };
    let mut words: Vec<String> = Vec::new();
    let mut word = String::new();
    for c in stem.chars().flat_map(char::to_lowercase) {
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            word.push(c);
        } else if !word.is_empty() {
            words.push(std::mem::take(&mut word));
        }
    }
    if !word.is_empty() {
        words.push(word);
    }
    let mut stem = String::new();
    for word in words {
        if stem.len() + 1 + word.len() > 52 {
            break;
        }
        if !stem.is_empty() {
            stem.push('-');
        }
        stem.push_str(&word);
    }
    if stem.is_empty() {
        stem = "wallpaper".into();
    }
    let free = |candidate: &String| !taken(candidate);
    let first = format!("{stem}.{extension}");
    if free(&first) {
        return first;
    }
    (2..)
        .map(|n| format!("{stem}-{n}.{extension}"))
        .find(free)
        .expect("some number is free")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Theme;

    fn request() -> Request {
        Request {
            settings: vec![Setting::Theme(Theme::Frost), Setting::Scale(1.2)],
            add: vec![
                NewPicture {
                    name: "lake.jpg".into(),
                    bytes: vec![0xFF, 0xD8, 0xFF, 1, 2, 3],
                },
                NewPicture {
                    name: "dunes-2.png".into(),
                    bytes: b"\x89PNG\r\n\x1a\n...".to_vec(),
                },
            ],
            remove: vec!["Old Photo (1).JPG".into()],
            bing_timer: Some(true),
        }
    }

    fn encoded(request: &Request) -> Vec<u8> {
        let mut bytes = Vec::new();
        request.write(&mut bytes).unwrap();
        bytes
    }

    fn read(bytes: &[u8]) -> Result<Request> {
        Request::read(&mut std::io::Cursor::new(bytes))
    }

    #[test]
    fn reads_what_it_writes() {
        let request = request();
        let mut read_back = read(&encoded(&request)).unwrap();
        read_back.settings.sort_by_key(|setting| setting.key());
        let mut expected = request;
        expected.settings.sort_by_key(|setting| setting.key());
        assert_eq!(read_back, expected);
        assert!(read(&encoded(&Request::default())).unwrap().is_empty());
    }

    #[test]
    fn refuses_malformed_requests() {
        let good = encoded(&request());
        let text = |bytes: &[u8]| String::from_utf8_lossy(bytes).into_owned();
        let header = |json: &str| format!("{MAGIC}\n{json}\n").into_bytes();
        for (bad, why) in [
            (good[..good.len() - 1].to_vec(), "cut short"),
            ([good.clone(), b"x".to_vec()].concat(), "trailing bytes"),
            (b"gpui-greeter settings 2\n{}\n".to_vec(), "another format"),
            (text(&good).replacen(MAGIC, "", 1).into_bytes(), "no magic"),
            (
                header("{\"set\":{\"sessions.wrapper\":\"id\"}}"),
                "a key outside the set",
            ),
            (header("{\"run\":\"id\"}"), "an unknown field"),
            (
                header("{\"bing_timer\":\"on\"}"),
                "a timer that is not on or off",
            ),
            (
                header("{\"bing_timer\":{\"unit\":\"sshd.service\"}}"),
                "another unit",
            ),
            (
                header("{\"add\":[{\"name\":\"../x.jpg\",\"size\":0}]}"),
                "a path",
            ),
            (
                header("{\"add\":[{\"name\":\"x.svg\",\"size\":0}]}"),
                "not a picture",
            ),
            (
                header("{\"add\":[{\"name\":\"X.jpg\",\"size\":0}]}"),
                "not a new name",
            ),
            (
                header(
                    "{\"add\":[{\"name\":\"x.jpg\",\"size\":0},{\"name\":\"x.jpg\",\"size\":0}]}",
                ),
                "twice",
            ),
            (
                header("{\"add\":[{\"name\":\"x.jpg\",\"size\":99999999999}]}"),
                "too large",
            ),
            (
                header("{\"remove\":[\"../../etc/passwd\"]}"),
                "a path to remove",
            ),
            (header("{\"remove\":[\"/etc/x.jpg\"]}"), "an absolute path"),
            (header("{\"remove\":[\".x.jpg\"]}"), "a hidden file"),
            (
                header("{\"add\":[{\"name\":\"x.jpg\",\"size\":0}],\"remove\":[\"x.jpg\"]}"),
                "added and removed",
            ),
            (
                [MAGIC.as_bytes(), b"\n", &[b' '; 70 << 10], b"{}\n"].concat(),
                "a header too long",
            ),
        ] {
            assert!(read(&bad).is_err(), "accepted {why}");
        }
    }

    #[test]
    fn names_new_pictures_plainly() {
        let none = |_: &str| false;
        assert_eq!(
            new_name("Lake at Dawn.JPG", "jpg", none),
            "lake-at-dawn.jpg"
        );
        assert_eq!(new_name("IMG_2041", "png", none), "img-2041.png");
        assert_eq!(new_name("日落.webp", "webp", none), "wallpaper.webp");
        assert_eq!(new_name(".hidden.jpg", "jpg", none), "hidden.jpg");
        assert_eq!(new_name("../../x.jpg", "jpg", none), "x.jpg");
        let taken = |name: &str| ["lake.jpg", "lake-2.jpg"].contains(&name);
        assert_eq!(new_name("lake.jpeg", "jpg", taken), "lake-3.jpg");
        let long = new_name(&format!("{}.png", "word ".repeat(40)), "png", none);
        assert!(is_new_name(&long), "{long}");
        for name in [
            "lake-at-dawn.jpg",
            "img-2041.png",
            "wallpaper.webp",
            "lake-3.jpg",
        ] {
            assert!(is_new_name(name), "{name}");
        }
        for name in [
            "Lake.jpg",
            "lake.jpeg",
            "-lake.jpg",
            "lake--x.jpg",
            "lake.svg",
            ".jpg",
        ] {
            assert!(!is_new_name(name), "{name}");
        }
    }
}

//! `gpui-greeter apply-settings`: the root side of the settings app. It
//! reads one request from its standard input and makes the changes only
//! root can: the settings in `/etc/gpui-greeter/config.toml`, the pictures
//! in the wallpaper folder those settings name, and whether the daily Bing
//! timer runs.
//!
//! The request comes from the user, so it is checked whole before anything
//! changes. Settings are those of a closed set, each checked; new pictures
//! must decode as pictures and take plain names; removals must name
//! pictures in the folder; the folders written to must be ones only root
//! can change; the timer is the package's own, and only turned on or off.
//! The helper opens no path the request names: the pictures' bytes come in
//! the request itself.

use std::fs;
use std::io::{Cursor, Write as _};
use std::os::unix::fs::{MetadataExt as _, OpenOptionsExt as _, PermissionsExt as _};
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, bail, ensure};
use image::{ImageReader, Limits};

use super::request::{Request, is_new_name};
use super::setting::{self, Setting, plain_picture_name};
use super::timer::{TIMER, Units};
use crate::config::{Config, Pick, Theme};
use crate::wallpaper::picture_format;
use crate::{bing, themes};

/// The settings a new file starts from: the shipped ones, every default
/// written out with what it does.
const SHIPPED: &str = include_str!("../../dist/config.toml");

/// Makes the changes `request` asks for to the settings at `config` and to
/// the timer among `units`, and says what it did, a line for each change.
pub fn apply(request: &Request, config: &Path, units: Units) -> Result<Vec<String>> {
    let text = match fs::symlink_metadata(config) {
        Ok(meta) if meta.file_type().is_symlink() => {
            bail!("{} is a link; edit what it points to", config.display())
        }
        Ok(_) => fs::read_to_string(config)
            .with_context(|| format!("cannot read {}", config.display()))?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => SHIPPED.to_string(),
        Err(error) => {
            return Err(error).with_context(|| format!("cannot read {}", config.display()));
        }
    };
    let current: Config = toml::from_str(&text).with_context(|| {
        format!(
            "{} does not read as the greeter reads it; fix it first",
            config.display()
        )
    })?;

    // Everything is checked before anything changes.
    for setting in &request.settings {
        if let Setting::Theme(Theme::Folder(name)) = setting {
            let installed = current.themes.dir.join(name).join(themes::MANIFEST);
            ensure!(
                !themes::RESERVED.contains(&name.as_str()) && installed.is_file(),
                "no theme folder {name} is installed in {}",
                current.themes.dir.display()
            );
        }
    }
    let wallpapers = &current.wallpaper.dir;
    for picture in &request.add {
        ensure!(
            is_new_name(&picture.name),
            "{:?} is not a name the settings give a new picture",
            picture.name
        );
        let format = picture_format(&picture.bytes)
            .with_context(|| format!("{} is not a JPEG, PNG or WebP picture", picture.name))?;
        ensure!(
            picture.name.ends_with(&format!(".{format}")),
            "{} holds a {format} picture",
            picture.name
        );
        decode(&picture.bytes).with_context(|| format!("{} does not decode", picture.name))?;
    }
    for name in &request.remove {
        ensure!(plain_picture_name(name), "{name:?} names no wallpaper");
        let path = wallpapers.join(name);
        let is_file = fs::symlink_metadata(&path).is_ok_and(|meta| meta.file_type().is_file());
        ensure!(
            is_file,
            "{name} is not a picture in {}",
            wallpapers.display()
        );
    }
    let there_after = |name: &str| {
        request.add.iter().any(|picture| picture.name == name)
            || (!request.remove.iter().any(|removed| removed == name)
                && fs::symlink_metadata(wallpapers.join(name))
                    .is_ok_and(|meta| meta.file_type().is_file()))
    };
    let new_pick = request.settings.iter().find_map(|setting| match setting {
        Setting::WallpaperPick(pick) => Some(pick),
        _ => None,
    });
    match (new_pick, &current.wallpaper.pick) {
        (Some(Pick::File(name)), _) => ensure!(
            there_after(name),
            "wallpaper.pick names {name}, which is not in {}",
            wallpapers.display()
        ),
        (Some(Pick::BingFile(name)), _) => ensure!(
            bing::named(&current.wallpaper.bing_dir, name).is_some(),
            "wallpaper.pick names {name}, which is not among Bing's pictures in {}",
            current.wallpaper.bing_dir.display()
        ),
        (None, Pick::File(name)) if request.remove.contains(name) => {
            bail!("{name} is the wallpaper picked; pick another before removing it")
        }
        _ => {}
    }
    if request.bing_timer.is_some() {
        ensure!(units.installed(), "{TIMER} is not installed");
    }
    let edited = if request.settings.is_empty() {
        None
    } else {
        Some(setting::edit(&text, &request.settings)?)
    };
    let pictures_change = !request.add.is_empty() || !request.remove.is_empty();
    if pictures_change {
        let existing = wallpapers
            .ancestors()
            .find(|dir| dir.exists())
            .unwrap_or(Path::new("/"));
        held(existing)?;
    }
    if edited.is_some() {
        held(config.parent().unwrap_or(Path::new("/")))?;
    }

    // Then the changes: pictures first, so that the settings never name
    // one that is not there yet.
    let mut done = Vec::new();
    if pictures_change {
        fs::create_dir_all(wallpapers)
            .with_context(|| format!("cannot create {}", wallpapers.display()))?;
        held(wallpapers)?;
        fs::set_permissions(wallpapers, fs::Permissions::from_mode(0o755))?;
    }
    for picture in &request.add {
        write_new(&wallpapers.join(&picture.name), &picture.bytes)?;
        done.push(format!("added    {}", picture.name));
    }
    for name in &request.remove {
        fs::remove_file(wallpapers.join(name)).with_context(|| format!("cannot remove {name}"))?;
        done.push(format!("removed  {name}"));
    }
    if let Some(edited) = edited {
        if let Some(dir) = config.parent()
            && !dir.exists()
        {
            fs::create_dir_all(dir)?;
            fs::set_permissions(dir, fs::Permissions::from_mode(0o755))?;
        }
        write_new(config, edited.as_bytes())?;
        for setting in &request.settings {
            done.push(format!("set      {} = {}", setting.key(), setting.json()));
        }
    }
    // The timer last, once the settings it follows are written.
    if let Some(on) = request.bing_timer {
        done.push(units.set(on)?);
    }
    Ok(done)
}

/// Decodes `bytes` as a picture, within limits a wallpaper keeps to.
fn decode(bytes: &[u8]) -> Result<()> {
    let mut reader = ImageReader::new(Cursor::new(bytes)).with_guessed_format()?;
    let mut limits = Limits::default();
    limits.max_image_width = Some(16_384);
    limits.max_image_height = Some(16_384);
    limits.max_alloc = Some(1 << 30);
    reader.limits(limits);
    reader.decode()?;
    Ok(())
}

/// Checks that only whoever runs this (root, under pkexec) can change
/// `dir` and the folders above it, so that nobody else can swap what is
/// written there for a link elsewhere. A folder anyone may write to is
/// allowed only if it is sticky and root's, as `/tmp` is.
fn held(dir: &Path) -> Result<()> {
    let me = rustix::process::geteuid().as_raw();
    let dir = dir
        .canonicalize()
        .with_context(|| format!("cannot open {}", dir.display()))?;
    for folder in dir.ancestors() {
        let meta = fs::metadata(folder)?;
        let mode = meta.mode();
        let owner_ok = meta.uid() == 0 || meta.uid() == me;
        let shared = mode & 0o022 != 0;
        let sticky = mode & 0o1000 != 0 && meta.uid() == 0;
        ensure!(
            meta.is_dir() && owner_ok && (!shared || sticky),
            "{} can be changed by other users; the settings write only where root alone can",
            folder.display()
        );
    }
    Ok(())
}

/// Writes `bytes` to `path` whole or not at all, readable by everyone, the
/// greeter's user included.
fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
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
    file.sync_all()?;
    fs::set_permissions(&partial, fs::Permissions::from_mode(0o644))?;
    fs::rename(&partial, path).with_context(|| format!("cannot replace {}", path.display()))?;
    Ok(())
}

/// Where the settings live: `/etc/gpui-greeter/config.toml`, or, in a debug
/// build's tests, the same under a test folder.
pub fn config_path(root: Option<&Path>) -> PathBuf {
    let path = Path::new(crate::config::DEFAULT_PATH);
    match root {
        Some(root) => root.join(path.strip_prefix("/").unwrap_or(path)),
        None => path.to_path_buf(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::PaperBackground;
    use crate::settings::request::NewPicture;

    /// A test folder with the shipped settings pointing at its own
    /// wallpaper, themes and Bing folders.
    fn scratch(name: &str) -> (PathBuf, PathBuf) {
        let root =
            std::env::temp_dir().join(format!("gpui-greeter-helper-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("etc/gpui-greeter")).unwrap();
        fs::create_dir_all(root.join("themes/console")).unwrap();
        fs::write(root.join("themes/console/theme.toml"), "format = 1\n").unwrap();
        let config = config_path(Some(&root));
        let text = SHIPPED
            .replace(
                "/usr/local/share/gpui-greeter/wallpapers",
                &root.join("wallpapers").display().to_string(),
            )
            .replace(
                "/usr/local/share/gpui-greeter/themes",
                &root.join("themes").display().to_string(),
            )
            .replace(
                "/usr/local/share/gpui-greeter/bing4k",
                &root.join("bing4k").display().to_string(),
            );
        fs::write(&config, text).unwrap();
        (root, config)
    }

    /// Applies `request` to the settings at `config`, and to the units of
    /// the test folder that holds them.
    fn apply(request: &Request, config: &Path) -> Result<Vec<String>> {
        let root = config
            .ancestors()
            .nth(3)
            .expect("root/etc/gpui-greeter/config.toml");
        super::apply(request, config, Units::Root(root))
    }

    fn png() -> Vec<u8> {
        let mut bytes = Vec::new();
        image::RgbaImage::from_pixel(8, 4, image::Rgba([200, 120, 40, 255]))
            .write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png)
            .unwrap();
        bytes
    }

    fn picture(name: &str, bytes: Vec<u8>) -> NewPicture {
        NewPicture {
            name: name.into(),
            bytes,
        }
    }

    #[test]
    fn applies_settings_and_pictures() {
        let (root, config) = scratch("apply");
        let request = Request {
            settings: vec![
                Setting::Theme(Theme::Folder("console".into())),
                Setting::PaperBackground(PaperBackground::Ledger),
                Setting::WallpaperPick(Pick::File("dunes.png".into())),
            ],
            add: vec![picture("dunes.png", png())],
            remove: vec![],
            bing_timer: None,
        };
        let done = apply(&request, &config).unwrap();
        assert_eq!(done.len(), 4, "{done:?}");
        let written = Config::load(&config);
        assert_eq!(written.theme, Theme::Folder("console".into()));
        assert_eq!(written.paper.background, PaperBackground::Ledger);
        assert_eq!(written.wallpaper.pick, Pick::File("dunes.png".into()));
        let text = fs::read_to_string(&config).unwrap();
        assert!(
            text.contains("# gpui-greeter's settings"),
            "the comments stay"
        );
        let picture = root.join("wallpapers/dunes.png");
        assert_eq!(fs::read(&picture).unwrap(), png());
        let mode = |path: &Path| fs::metadata(path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&picture), 0o644);
        assert_eq!(mode(&config), 0o644);
        assert_eq!(mode(&root.join("wallpapers")), 0o755);

        // Removing the picture picked takes another pick with it.
        let remove = Request {
            remove: vec!["dunes.png".into()],
            ..Request::default()
        };
        assert!(apply(&remove, &config).is_err());
        assert!(picture.exists());
        let remove = Request {
            settings: vec![Setting::WallpaperPick(Pick::Random)],
            ..remove
        };
        apply(&remove, &config).unwrap();
        assert!(!picture.exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn refuses_what_it_should_not_do() {
        let (root, config) = scratch("refuse");
        let before = fs::read(&config).unwrap();
        fs::create_dir_all(root.join("wallpapers")).unwrap();
        fs::write(root.join("wallpapers/notes.txt"), "not a picture").unwrap();
        fs::write(root.join("private.jpg"), png()).unwrap();
        std::os::unix::fs::symlink(root.join("private.jpg"), root.join("wallpapers/link.jpg"))
            .unwrap();
        let cases = [
            (
                "a theme that is not installed",
                Request {
                    settings: vec![Setting::Theme(Theme::Folder("gone".into()))],
                    ..Request::default()
                },
            ),
            (
                "a text file as a picture",
                Request {
                    add: vec![picture("notes.png", b"hello".to_vec())],
                    ..Request::default()
                },
            ),
            (
                "a PNG named as a JPEG",
                Request {
                    add: vec![picture("dunes.jpg", png())],
                    ..Request::default()
                },
            ),
            (
                "a truncated picture",
                Request {
                    add: vec![picture("dunes.png", png()[..40].to_vec())],
                    ..Request::default()
                },
            ),
            (
                "a removal that is not a picture there",
                Request {
                    remove: vec!["gone.jpg".into()],
                    ..Request::default()
                },
            ),
            (
                "a removal through a link",
                Request {
                    remove: vec!["link.jpg".into()],
                    ..Request::default()
                },
            ),
            (
                "a removal outside the folder",
                Request {
                    remove: vec!["../private.jpg".into()],
                    ..Request::default()
                },
            ),
            (
                "a picture added outside the folder",
                Request {
                    add: vec![picture("../dunes.png", png())],
                    ..Request::default()
                },
            ),
            (
                "a pick of a picture that is not there",
                Request {
                    settings: vec![Setting::WallpaperPick(Pick::File("gone.jpg".into()))],
                    ..Request::default()
                },
            ),
        ];
        for (why, request) in cases {
            let refused = apply(&request, &config);
            assert!(refused.is_err(), "accepted {why}");
        }
        assert_eq!(fs::read(&config).unwrap(), before, "nothing changed");
        assert!(root.join("private.jpg").exists());
        assert!(root.join("wallpapers/link.jpg").exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn writes_only_where_others_cannot() {
        let (root, config) = scratch("held");
        let open = root.join("wallpapers");
        fs::create_dir_all(&open).unwrap();
        fs::set_permissions(&open, fs::Permissions::from_mode(0o777)).unwrap();
        let request = Request {
            add: vec![picture("dunes.png", png())],
            ..Request::default()
        };
        let refused = apply(&request, &config).unwrap_err();
        assert!(
            format!("{refused:#}").contains("other users"),
            "{refused:#}"
        );
        assert!(!open.join("dunes.png").exists());

        // Settings that are a link, or that do not read, are left alone.
        fs::set_permissions(&open, fs::Permissions::from_mode(0o755)).unwrap();
        let elsewhere = root.join("elsewhere.toml");
        fs::rename(&config, &elsewhere).unwrap();
        std::os::unix::fs::symlink(&elsewhere, &config).unwrap();
        let theme = Request {
            settings: vec![Setting::Theme(Theme::Neon)],
            ..Request::default()
        };
        assert!(apply(&theme, &config).is_err());
        fs::remove_file(&config).unwrap();
        fs::write(&config, "sessions = { wrapper = 1 }\n").unwrap();
        assert!(apply(&theme, &config).is_err());

        // A missing file starts from the shipped one.
        fs::remove_file(&config).unwrap();
        apply(&theme, &config).unwrap();
        let text = fs::read_to_string(&config).unwrap();
        assert!(text.starts_with("# gpui-greeter's settings"), "{text}");
        assert_eq!(Config::load(&config).theme, Theme::Neon);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn picks_one_of_bing_s_pictures_that_is_there() {
        let (root, config) = scratch("bing");
        fs::create_dir_all(root.join("bing4k")).unwrap();
        fs::write(root.join("bing4k/2026-10-29.jpg"), png()).unwrap();
        let pick = |name: &str| Request {
            settings: vec![Setting::WallpaperPick(Pick::BingFile(name.into()))],
            ..Request::default()
        };
        assert!(apply(&pick("2026-10-28.jpg"), &config).is_err());
        apply(&pick("2026-10-29.jpg"), &config).unwrap();
        assert_eq!(
            Config::load(&config).wallpaper.pick,
            Pick::BingFile("2026-10-29.jpg".into())
        );
        assert!(
            fs::read_to_string(&config)
                .unwrap()
                .contains("pick = \"bing4k/2026-10-29.jpg\"\n")
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn turns_the_package_s_timer_on_and_off() {
        if !Path::new("/usr/bin/systemctl").exists() {
            return;
        }
        let (root, config) = scratch("timer");
        let before = fs::read(&config).unwrap();
        let timer = |on| Request {
            bing_timer: Some(on),
            ..Request::default()
        };
        let refused = apply(&timer(true), &config).unwrap_err();
        assert!(
            format!("{refused:#}").contains("not installed"),
            "{refused:#}"
        );

        let units = root.join("usr/lib/systemd/system");
        fs::create_dir_all(&units).unwrap();
        for unit in ["gpui-greeter-bing.timer", "gpui-greeter-bing.service"] {
            let shipped = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("dist")
                .join(unit);
            fs::copy(shipped, units.join(unit)).unwrap();
        }
        let wanted = root.join("etc/systemd/system/timers.target.wants/gpui-greeter-bing.timer");
        let done = apply(&timer(true), &config).unwrap();
        assert_eq!(done, ["enabled  gpui-greeter-bing.timer"]);
        assert!(fs::symlink_metadata(&wanted).is_ok(), "{wanted:?}");
        assert_eq!(Units::Root(&root).enabled(), Some(true));
        apply(&timer(false), &config).unwrap();
        assert!(fs::symlink_metadata(&wanted).is_err());
        assert_eq!(Units::Root(&root).enabled(), Some(false));
        assert_eq!(fs::read(&config).unwrap(), before, "the settings stay");
        let _ = fs::remove_dir_all(root);
    }
}

//! What the settings app may change in `config.toml`: a closed set of keys,
//! each value checked as the greeter reads it, and each written into the
//! file in place, keeping its comments. The root helper takes these and
//! nothing else, so it can never be made to set the command that starts a
//! session, a folder or an address.

use anyhow::{Context as _, Result, bail, ensure};
use serde::Deserialize as _;
use serde_json::Value as Json;
use toml_edit::{DocumentMut, Item, Table, Value};

use crate::config::{Config, Language, NeonBackground, PaperBackground, Pick, Rgb, Theme};
use crate::wallpaper::is_picture_name;

/// `scale`'s range, as the greeter clamps it.
pub const SCALE: (f32, f32) = (0.5, 1.5);
/// Frost's blur and dim, as the settings' comments give them.
pub const BLUR: (f32, f32) = (8.0, 48.0);
pub const DIM: (f32, f32) = (0.0, 0.6);
/// Seconds before Frost's form gives way to the clock.
pub const IDLE: (u64, u64) = (10, 600);

#[derive(Debug, Clone, PartialEq)]
pub enum Setting {
    Theme(Theme),
    Language(Language),
    Scale(f32),
    PaperAccent(Rgb),
    PaperBackground(PaperBackground),
    FrostBlur(f32),
    FrostDim(f32),
    FrostIdle(u64),
    NeonAccent(Rgb),
    NeonScanlines(bool),
    NeonBackground(NeonBackground),
    WallpaperPick(Pick),
}

impl Setting {
    /// The key, as `config.toml` and the helper's requests name it.
    pub fn key(&self) -> &'static str {
        match self {
            Setting::Theme(_) => "theme",
            Setting::Language(_) => "language",
            Setting::Scale(_) => "scale",
            Setting::PaperAccent(_) => "paper.accent",
            Setting::PaperBackground(_) => "paper.background",
            Setting::FrostBlur(_) => "frost.blur",
            Setting::FrostDim(_) => "frost.dim",
            Setting::FrostIdle(_) => "frost.idle_seconds",
            Setting::NeonAccent(_) => "neon.accent",
            Setting::NeonScanlines(_) => "neon.scanlines",
            Setting::NeonBackground(_) => "neon.background",
            Setting::WallpaperPick(_) => "wallpaper.pick",
        }
    }

    /// Reads `value` as the setting `key` takes it, and refuses any other
    /// key, a value of another type and one out of range.
    pub fn parse(key: &str, value: &Json) -> Result<Self> {
        let text = || {
            value
                .as_str()
                .with_context(|| format!("{key} takes a string"))
        };
        let number = || {
            value
                .as_f64()
                .filter(|number| number.is_finite())
                .with_context(|| format!("{key} takes a number"))
        };
        let within = |number: f64, (low, high): (f32, f32)| {
            ensure!(
                (f64::from(low)..=f64::from(high)).contains(&number),
                "{key} takes {low} to {high}, not {number}"
            );
            Ok(number)
        };
        let named = |names: &[&str]| -> Result<String> {
            let name = text()?;
            ensure!(
                names.contains(&name),
                "{key} takes {}, not {name:?}",
                names.join(", ")
            );
            Ok(name.to_string())
        };
        Ok(match key {
            "theme" => Setting::Theme(
                Theme::parse(text()?)
                    .with_context(|| format!("{:?} names no look", text().unwrap_or("")))?,
            ),
            "language" => Setting::Language(
                Language::parse(text()?).with_context(|| format!("{key} takes en or zh-CN"))?,
            ),
            "scale" => Setting::Scale(hundredths(within(number()?, SCALE)?)),
            "paper.accent" => Setting::PaperAccent(colour(key, text()?)?),
            "paper.background" => {
                let names = PaperBackground::ALL.map(PaperBackground::name);
                let name = named(&names)?;
                Setting::PaperBackground(PaperBackground::deserialize(
                    serde::de::value::StrDeserializer::<serde::de::value::Error>::new(&name),
                )?)
            }
            "frost.blur" => Setting::FrostBlur(within(number()?, BLUR)?.round() as f32),
            "frost.dim" => Setting::FrostDim(hundredths(within(number()?, DIM)?)),
            "frost.idle_seconds" => {
                let seconds = value
                    .as_u64()
                    .with_context(|| format!("{key} takes a whole number of seconds"))?;
                ensure!(
                    (IDLE.0..=IDLE.1).contains(&seconds),
                    "{key} takes {} to {} seconds, not {seconds}",
                    IDLE.0,
                    IDLE.1
                );
                Setting::FrostIdle(seconds)
            }
            "neon.accent" => Setting::NeonAccent(colour(key, text()?)?),
            "neon.scanlines" => Setting::NeonScanlines(
                value
                    .as_bool()
                    .with_context(|| format!("{key} takes true or false"))?,
            ),
            "neon.background" => {
                let names = NeonBackground::ALL.map(NeonBackground::name);
                let name = named(&names)?;
                Setting::NeonBackground(NeonBackground::deserialize(
                    serde::de::value::StrDeserializer::<serde::de::value::Error>::new(&name),
                )?)
            }
            "wallpaper.pick" => {
                let pick = Pick::from(text()?.to_string());
                match &pick {
                    Pick::File(name) => ensure!(
                        plain_picture_name(name),
                        "{key} takes random, daily, bing or a picture's file name, not {name:?}"
                    ),
                    Pick::BingFile(name) => ensure!(
                        crate::bing::is_kept_name(name),
                        "{key} names one of Bing's pictures by its day, as \
                         bing4k/2026-10-29.jpg, not {name:?}"
                    ),
                    Pick::Random | Pick::Daily | Pick::Bing => {}
                }
                Setting::WallpaperPick(pick)
            }
            _ => bail!("{key:?} is not a setting this can change"),
        })
    }

    /// The value as a request carries it.
    pub fn json(&self) -> Json {
        match self {
            Setting::Theme(theme) => theme.name().into(),
            Setting::Language(language) => language.name().into(),
            Setting::Scale(number) | Setting::FrostBlur(number) | Setting::FrostDim(number) => {
                rounded(*number).into()
            }
            Setting::PaperAccent(rgb) | Setting::NeonAccent(rgb) => hex(*rgb).into(),
            Setting::PaperBackground(background) => background.name().into(),
            Setting::FrostIdle(seconds) => (*seconds).into(),
            Setting::NeonScanlines(on) => (*on).into(),
            Setting::NeonBackground(background) => background.name().into(),
            Setting::WallpaperPick(pick) => pick.name().into(),
        }
    }

    /// The value as `config.toml` writes it.
    fn toml(&self) -> Value {
        match self {
            Setting::FrostBlur(number) => (number.round() as i64).into(),
            Setting::Scale(number) | Setting::FrostDim(number) => rounded(*number).into(),
            Setting::FrostIdle(seconds) => (*seconds as i64).into(),
            Setting::NeonScanlines(on) => (*on).into(),
            other => match other.json() {
                Json::String(text) => text.into(),
                _ => unreachable!("every other setting is written as a string"),
            },
        }
    }

    /// Sets it in `config`.
    pub fn apply(&self, config: &mut Config) {
        match self {
            Setting::Theme(theme) => config.theme = theme.clone(),
            Setting::Language(language) => config.language = *language,
            Setting::Scale(scale) => config.scale = *scale,
            Setting::PaperAccent(rgb) => config.paper.accent = *rgb,
            Setting::PaperBackground(background) => config.paper.background = *background,
            Setting::FrostBlur(blur) => config.frost.blur = *blur,
            Setting::FrostDim(dim) => config.frost.dim = *dim,
            Setting::FrostIdle(seconds) => config.frost.idle_seconds = *seconds,
            Setting::NeonAccent(rgb) => config.neon.accent = *rgb,
            Setting::NeonScanlines(on) => config.neon.scanlines = *on,
            Setting::NeonBackground(background) => config.neon.background = *background,
            Setting::WallpaperPick(pick) => config.wallpaper.pick = pick.clone(),
        }
    }
}

/// Whether `name` is a picture's file name, alone: no folder, no hidden
/// file, nothing but a name in the wallpaper folder.
pub fn plain_picture_name(name: &str) -> bool {
    name.len() <= 255
        && !name.starts_with('.')
        && !name.contains(['/', '\0'])
        && is_picture_name(name)
}

fn colour(key: &str, text: &str) -> Result<Rgb> {
    Rgb::try_from(text.to_string()).map_err(|error| anyhow::anyhow!("{key}: {error}"))
}

/// `#RRGGBB`.
pub fn hex(rgb: Rgb) -> String {
    format!("#{:06X}", rgb.0 & 0xFF_FFFF)
}

/// `number` to two places, as the settings write it, without the binary
/// noise of an `f32` (1.1, not 1.100000023841858).
fn rounded(number: f32) -> f64 {
    (f64::from(number) * 100.0).round() / 100.0
}

fn hundredths(number: f64) -> f32 {
    ((number * 100.0).round() / 100.0) as f32
}

/// What differs between `saved` and `draft`, among the settings above.
pub fn changes(saved: &Config, draft: &Config) -> Vec<Setting> {
    let all = |config: &Config| {
        [
            Setting::Theme(config.theme.clone()),
            Setting::Language(config.language),
            Setting::Scale(hundredths(f64::from(config.scale))),
            Setting::PaperAccent(config.paper.accent),
            Setting::PaperBackground(config.paper.background),
            Setting::FrostBlur(config.frost.blur.round()),
            Setting::FrostDim(hundredths(f64::from(config.frost.dim))),
            Setting::FrostIdle(config.frost.idle_seconds),
            Setting::NeonAccent(config.neon.accent),
            Setting::NeonScanlines(config.neon.scanlines),
            Setting::NeonBackground(config.neon.background),
            Setting::WallpaperPick(config.wallpaper.pick.clone()),
        ]
    };
    all(saved)
        .into_iter()
        .zip(all(draft))
        .filter(|(before, after)| before != after)
        .map(|(_, after)| after)
        .collect()
}

/// `text`, the settings file, with `settings` written into it, its comments
/// and layout kept. A file the greeter would not read is refused, as is an
/// edit that would read back as anything but what was asked.
pub fn edit(text: &str, settings: &[Setting]) -> Result<String> {
    let before: Config = toml::from_str(text)
        .context("the settings do not read as the greeter reads them; fix them first")?;
    let mut document: DocumentMut = text
        .parse()
        .context("the settings are not valid TOML; fix them first")?;
    for setting in settings {
        let (table, key) = match setting.key().split_once('.') {
            Some((table, key)) => (Some(table), key),
            None => (None, setting.key()),
        };
        let table = match table {
            None => document.as_table_mut() as &mut dyn toml_edit::TableLike,
            Some(name) => {
                if !document.contains_key(name) {
                    document.insert(name, Item::Table(Table::new()));
                }
                document[name]
                    .as_table_like_mut()
                    .with_context(|| format!("[{name}] in the settings is not a table"))?
            }
        };
        let value = setting.toml();
        match table.get_mut(key).and_then(Item::as_value_mut) {
            Some(old) => {
                let decor = old.decor().clone();
                *old = value;
                *old.decor_mut() = decor;
            }
            None => {
                table.insert(key, Item::Value(value));
            }
        }
    }
    let edited = document.to_string();
    let after: Config =
        toml::from_str(&edited).context("the edited settings would not read back")?;
    let mut expected = before;
    for setting in settings {
        setting.apply(&mut expected);
    }
    ensure!(
        after == expected,
        "the edited settings would read back as something else"
    );
    Ok(edited)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn parse(key: &str, value: Json) -> Result<Setting> {
        Setting::parse(key, &value)
    }

    #[test]
    fn takes_each_setting_as_the_greeter_reads_it() {
        assert_eq!(
            parse("theme", json!("console")).unwrap(),
            Setting::Theme(Theme::Folder("console".into()))
        );
        assert_eq!(
            parse("language", json!("zh-CN")).unwrap(),
            Setting::Language(Language::Chinese)
        );
        assert_eq!(
            parse("scale", json!(1.2000001)).unwrap(),
            Setting::Scale(1.2)
        );
        assert_eq!(
            parse("paper.accent", json!("#3e6e8e")).unwrap(),
            Setting::PaperAccent(Rgb(0x3E6E8E))
        );
        assert_eq!(
            parse("paper.background", json!("ledger")).unwrap(),
            Setting::PaperBackground(PaperBackground::Ledger)
        );
        assert_eq!(
            parse("frost.blur", json!(24.4)).unwrap(),
            Setting::FrostBlur(24.0)
        );
        assert_eq!(
            parse("frost.idle_seconds", json!(90)).unwrap(),
            Setting::FrostIdle(90)
        );
        assert_eq!(
            parse("neon.scanlines", json!(false)).unwrap(),
            Setting::NeonScanlines(false)
        );
        assert_eq!(
            parse("wallpaper.pick", json!("Lake at dawn.JPG")).unwrap(),
            Setting::WallpaperPick(Pick::File("Lake at dawn.JPG".into()))
        );
        assert_eq!(
            parse("wallpaper.pick", json!("bing")).unwrap(),
            Setting::WallpaperPick(Pick::Bing)
        );
        assert_eq!(
            parse("wallpaper.pick", json!("bing4k/2026-10-29.jpg")).unwrap(),
            Setting::WallpaperPick(Pick::BingFile("2026-10-29.jpg".into()))
        );
    }

    #[test]
    fn refuses_other_keys_types_and_values() {
        for (key, value) in [
            ("sessions.wrapper", json!("sh -c 'id'")),
            ("wallpaper.dir", json!("/etc")),
            ("wallpaper.bing_url", json!("https://example.org/")),
            ("themes.dir", json!("/tmp")),
            ("themes.cycle", json!(["paper"])),
            ("paper", json!({"accent": "#000000"})),
            ("theme", json!("../../etc")),
            ("theme", json!(3)),
            ("language", json!("fr")),
            ("scale", json!(3.0)),
            ("scale", json!("1.0")),
            ("scale", json!(f64::NAN)),
            ("paper.accent", json!("red")),
            ("paper.background", json!("wood")),
            ("frost.blur", json!(100)),
            ("frost.dim", json!(-0.1)),
            ("frost.idle_seconds", json!(1.5)),
            ("frost.idle_seconds", json!(1)),
            ("neon.scanlines", json!("yes")),
            ("neon.background", json!("ocean")),
            ("wallpaper.pick", json!("../secret.jpg")),
            ("wallpaper.pick", json!("/etc/shadow")),
            ("wallpaper.pick", json!(".hidden.jpg")),
            ("wallpaper.pick", json!("notes.txt")),
            ("wallpaper.pick", json!("bing4k/../../etc/shadow")),
            ("wallpaper.pick", json!("bing4k/lake.jpg")),
            ("wallpaper.pick", json!("bing4k/2026-10-29.json")),
            ("wallpaper.bing_dir", json!("/etc")),
        ] {
            assert!(parse(key, value.clone()).is_err(), "{key} = {value}");
        }
    }

    #[test]
    fn writes_values_back_as_they_were_read() {
        for setting in [
            Setting::Scale(1.1),
            Setting::FrostDim(0.28),
            Setting::FrostBlur(30.0),
            Setting::PaperAccent(Rgb(0x5B7F4F)),
            Setting::WallpaperPick(Pick::File("dunes.png".into())),
            Setting::WallpaperPick(Pick::BingFile("2026-10-29.jpg".into())),
            Setting::Theme(Theme::PaperNight),
        ] {
            assert_eq!(
                Setting::parse(setting.key(), &setting.json()).unwrap(),
                setting
            );
        }
        assert_eq!(Setting::Scale(1.1).json(), json!(1.1));
    }

    #[test]
    fn edits_keep_the_comments_and_the_rest() {
        let shipped = include_str!("../../dist/config.toml");
        let settings = [
            Setting::Theme(Theme::Frost),
            Setting::Scale(1.25),
            Setting::PaperBackground(PaperBackground::Drafting),
            Setting::FrostBlur(24.0),
            Setting::NeonScanlines(false),
            Setting::WallpaperPick(Pick::Daily),
        ];
        let edited = edit(shipped, &settings).unwrap();
        assert!(edited.contains("theme = \"frost\"\n"), "{edited}");
        assert!(edited.contains("scale = 1.25\n"), "{edited}");
        assert!(edited.contains("background = \"drafting\"\n"), "{edited}");
        assert!(edited.contains("blur = 24\n"), "{edited}");
        assert!(edited.contains("scanlines = false\n"), "{edited}");
        assert!(edited.contains("pick = \"daily\"\n"), "{edited}");
        // Every comment and every other line is as it was.
        let lines = |text: &str| -> Vec<String> {
            text.lines()
                .filter(|line| {
                    let key = line.split('=').next().unwrap_or("").trim();
                    !["theme", "scale", "background", "blur", "scanlines", "pick"].contains(&key)
                })
                .map(String::from)
                .collect()
        };
        assert_eq!(lines(&edited), lines(shipped));
        let config: Config = toml::from_str(&edited).unwrap();
        assert_eq!(config.theme, Theme::Frost);
        assert_eq!(config.neon.background, NeonBackground::Skyline);
    }

    #[test]
    fn edits_files_that_leave_keys_out() {
        let edited = edit(
            "# Mine.\nlanguage = \"zh-CN\" # for now\n",
            &[
                Setting::Language(Language::English),
                Setting::NeonAccent(Rgb(0xFFB547)),
                Setting::Theme(Theme::Neon),
            ],
        )
        .unwrap();
        assert_eq!(
            edited,
            "# Mine.\nlanguage = \"en\" # for now\ntheme = \"neon\"\n\n[neon]\naccent = \"#FFB547\"\n"
        );
        let inline = edit(
            "paper = { accent = \"#B0612E\" }\n",
            &[Setting::PaperBackground(PaperBackground::Plain)],
        )
        .unwrap();
        let config: Config = toml::from_str(&inline).unwrap();
        assert_eq!(config.paper.background, PaperBackground::Plain);
        assert_eq!(config.paper.accent, Rgb(0xB0612E));
    }

    #[test]
    fn refuses_files_the_greeter_cannot_read() {
        assert!(edit("theme = ", &[Setting::Scale(1.0)]).is_err());
        assert!(edit("colour = \"blue\"\n", &[Setting::Scale(1.0)]).is_err());
        assert!(edit("paper = 3\n", &[Setting::Scale(1.0)]).is_err());
        assert!(edit("neon = 3\n", &[Setting::NeonScanlines(true)]).is_err());
    }

    #[test]
    fn finds_what_changed() {
        let saved = Config::default();
        let mut draft = saved.clone();
        assert!(changes(&saved, &draft).is_empty());
        draft.scale = 1.1;
        draft.frost.blur = 30.2;
        draft.wallpaper.pick = Pick::File("lake.jpg".into());
        assert_eq!(
            changes(&saved, &draft),
            [
                Setting::Scale(1.1),
                Setting::WallpaperPick(Pick::File("lake.jpg".into()))
            ]
        );
    }
}

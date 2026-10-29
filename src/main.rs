use std::os::fd::RawFd;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use gpui_greeter::config::{self, Config, Language, Pick, Theme};
use gpui_greeter::lock::{self, link::ViewEnd};
use gpui_greeter::remembered::{self, Remembered};
use gpui_greeter::themes::{self, Chord, Folder, Severity, State};
use gpui_greeter::ui::preview::{self, Preview};
use gpui_greeter::{sync, ui};

const USAGE: &str = "\
usage: gpui-greeter [--config FILE] [--theme THEME] [--language LANG] [--dry-run]
           the greeter, as greetd starts it, with GREETD_SOCK set
       gpui-greeter sync-wallpapers FROM [--to DIR] [--config FILE]
           copy the pictures in FROM where the greeter can read them:
           to DIR, by default the wallpaper folder of the config
       gpui-greeter check-theme DIR
           read the theme folder DIR as the greeter would, and say what
           is wrong with it, by file and line
       gpui-greeter install-theme DIR [--name NAME] [--select] [--to DIR]
                                      [--config FILE]
           check the theme folder DIR and copy it, as NAME (by default
           the folder's name), to DIR, by default the config's themes
           folder; --select also sets theme = NAME in the config
       gpui-greeter preview DIR [--state STATE] [--lang LANG] [--size WxH]
                                [--shot FILE] [--at MS] [--user NAME]
                                [--host NAME] [--wallpaper FILE]
                                [--press KEY]... [--hover X,Y] [--lock]
           show the theme folder DIR in a window on $DISPLAY, against a
           fake greetd that takes the password \"test\"; with --shot, draw
           it in an Xvfb of its own with lavapipe and write FILE once the
           screen settles (or MS after the state shows), then stop.
           STATE is rest, idle, typing, checking, error or welcome; the
           size is 3840x2160 for a screenshot, 1920x1080 otherwise.
           Once the state shows, --press presses KEY, written as in a
           theme's [keys], and --hover puts the pointer at X,Y; --lock
           shows the theme as the lock screen
       gpui-greeter lock [--config FILE] [--theme THEME] [--language LANG]
                         [--dry-run]
           lock the X session on $DISPLAY: the looks over every monitor,
           at rest until a key, the keyboard and the pointer held until
           the user's password is typed, which PAM checks as the service
           gpui-greeter-lock. F9 switches looks there too; the theme is
           the config's until one is picked there

  --config FILE    settings, /etc/gpui-greeter/config.toml by default
  --theme THEME    paper, paper-night, frost, neon or a theme folder's
                   name, over the config's
  --language LANG  en or zh-CN, over the config's
  --dry-run        power buttons only say what they would do
";

fn main() -> ExitCode {
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("warn,gpui_greeter=info"),
    )
    .init();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let outcome = match args.first().map(String::as_str) {
        Some("--help" | "-h") => {
            print!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Some("sync-wallpapers") => sync_wallpapers(&args[1..]),
        Some("check-theme") => check_theme(&args[1..]),
        Some("install-theme") => install_theme(&args[1..]),
        Some("preview") => run_preview(&args[1..]),
        Some("lock") => lock(&args[1..]),
        _ => greet(&args),
    };
    match outcome {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("gpui-greeter: {error:#}");
            ExitCode::FAILURE
        }
    }
}

struct Args<'a> {
    rest: std::slice::Iter<'a, String>,
}

impl<'a> Args<'a> {
    fn value(&mut self, flag: &str) -> anyhow::Result<&'a str> {
        self.rest
            .next()
            .map(String::as_str)
            .ok_or_else(|| anyhow::anyhow!("{flag} needs a value\n{USAGE}"))
    }
}

/// What the login screen and the lock screen start with.
struct Start {
    config: PathBuf,
    theme: Option<Theme>,
    language: Option<Language>,
    dry_run: bool,
}

impl Default for Start {
    fn default() -> Self {
        Self {
            config: PathBuf::from(config::DEFAULT_PATH),
            theme: None,
            language: None,
            dry_run: false,
        }
    }
}

impl Start {
    /// Takes `arg` if it is one of their options.
    fn option(&mut self, arg: &str, args: &mut Args) -> anyhow::Result<bool> {
        match arg {
            "--config" => self.config = args.value(arg)?.into(),
            "--theme" => {
                let name = args.value(arg)?;
                self.theme =
                    Some(Theme::parse(name).ok_or_else(|| anyhow::anyhow!("no theme {name:?}"))?);
            }
            "--language" => {
                let name = args.value(arg)?;
                self.language = Some(
                    Language::parse(name).ok_or_else(|| anyhow::anyhow!("no language {name:?}"))?,
                );
            }
            "--dry-run" => self.dry_run = true,
            _ => return Ok(false),
        }
        Ok(true)
    }

    /// Shows the screen, with the look last picked there, which `kept`
    /// remembers; `lock` makes it the lock screen's view.
    fn run(self, kept: &str, lock: Option<ViewEnd>) -> anyhow::Result<()> {
        let mut config = Config::load(&self.config);
        config.language = self.language.unwrap_or(config.language);
        let setting = self.theme.clone().unwrap_or(config.theme.clone());
        // The look picked last time, unless the settings have changed
        // since or the command line names one.
        let picked = self.theme.is_none().then(|| {
            Remembered::load(kept)
                .theme_for(setting.name())
                .and_then(Theme::parse)
        });
        let mut folder = None;
        config.theme = Theme::Paper;
        for candidate in picked.flatten().into_iter().chain([setting.clone()]) {
            let loaded = match &candidate {
                Theme::Folder(name) => match themes::load_configured(&config.themes.dir, name) {
                    Some(loaded) => Some(loaded),
                    None => continue,
                },
                _ => None,
            };
            folder = loaded;
            config.theme = candidate;
            break;
        }
        ui::run(ui::Options {
            config,
            setting,
            dry_run: self.dry_run,
            folder,
            preview: None,
            lock,
        })
    }
}

fn greet(args: &[String]) -> anyhow::Result<()> {
    let mut args = Args { rest: args.iter() };
    let mut start = Start::default();
    while let Some(arg) = args.rest.next() {
        if !start.option(arg, &mut args)? {
            anyhow::bail!("unknown argument {arg:?}\n{USAGE}");
        }
    }
    start.run(remembered::LOGIN, None)
}

/// The lock's supervisor, or, with `--view`, the view it starts.
fn lock(args: &[String]) -> anyhow::Result<()> {
    let mut args = Args { rest: args.iter() };
    let mut start = Start::default();
    let mut view = None;
    #[cfg(feature = "fake-pam")]
    let mut service = lock::pam::Service::default();
    #[cfg(not(feature = "fake-pam"))]
    let service = lock::pam::Service::default();
    while let Some(arg) = args.rest.next() {
        match arg.as_str() {
            "--view" => view = Some(args.value(arg)?.parse::<RawFd>()?),
            #[cfg(feature = "fake-pam")]
            "--pam-dir" => service = lock::pam::Service::fake(Path::new(args.value(arg)?)),
            _ if start.option(arg, &mut args)? => {}
            other => anyhow::bail!("unknown argument {other:?}\n{USAGE}"),
        }
    }
    match view {
        Some(fd) => start.run(remembered::LOCK, Some(ViewEnd::adopt(fd)?)),
        None => lock::run(lock::Options {
            config: start.config,
            theme: start.theme,
            language: start.language,
            dry_run: start.dry_run,
            service,
        }),
    }
}

fn run_preview(args: &[String]) -> anyhow::Result<()> {
    let mut args = Args { rest: args.iter() };
    let mut dir = None;
    let (mut state, mut language, mut size, mut shot, mut at) = (None, None, None, None, None);
    let (mut user, mut host, mut wallpaper) = (None, None, None);
    let (mut presses, mut hover, mut lock) = (Vec::new(), None, false);
    let mut config_path = PathBuf::from(config::DEFAULT_PATH);
    while let Some(arg) = args.rest.next() {
        match arg.as_str() {
            "--press" => presses.push(Chord::parse(args.value(arg)?).map_err(anyhow::Error::msg)?),
            "--hover" => {
                let text = args.value(arg)?;
                let at = text
                    .split_once(',')
                    .and_then(|(x, y)| Some((x.trim().parse().ok()?, y.trim().parse().ok()?)));
                hover = Some(
                    at.ok_or_else(|| anyhow::anyhow!("--hover takes X,Y, such as 1920,1800"))?,
                );
            }
            "--state" => {
                let name = args.value(arg)?;
                state = Some(State::parse(name).ok_or_else(|| {
                    anyhow::anyhow!(
                        "no state {name:?}: rest, idle, typing, checking, error or welcome"
                    )
                })?);
            }
            "--lang" | "--language" => {
                let name = args.value(arg)?;
                language = Some(
                    Language::parse(name).ok_or_else(|| anyhow::anyhow!("no language {name:?}"))?,
                );
            }
            "--size" => {
                let text = args.value(arg)?;
                let parsed = text
                    .split_once('x')
                    .and_then(|(w, h)| Some((w.parse::<u32>().ok()?, h.parse::<u32>().ok()?)))
                    .filter(|&(w, h)| (320..=7680).contains(&w) && (240..=4320).contains(&h));
                size = Some(parsed.ok_or_else(|| {
                    anyhow::anyhow!("--size takes WIDTHxHEIGHT, such as 3840x2160")
                })?);
            }
            "--shot" => shot = Some(PathBuf::from(args.value(arg)?)),
            "--at" => {
                let ms: u64 = args.value(arg)?.parse()?;
                at = Some(Duration::from_millis(ms));
            }
            "--user" => user = Some(args.value(arg)?.to_string()),
            "--host" => host = Some(args.value(arg)?.to_string()),
            "--wallpaper" => wallpaper = Some(PathBuf::from(args.value(arg)?)),
            "--config" => config_path = args.value(arg)?.into(),
            "--lock" => lock = true,
            path if dir.is_none() && !path.starts_with('-') => dir = Some(PathBuf::from(path)),
            other => anyhow::bail!("unknown argument {other:?}\n{USAGE}"),
        }
    }
    let dir = dir.ok_or_else(|| anyhow::anyhow!("which folder?\n{USAGE}"))?;
    anyhow::ensure!(
        hover.is_none() || shot.is_some(),
        "--hover is for screenshots; in a window, move the pointer there yourself"
    );
    let folder = match Folder::load(&dir) {
        Ok((folder, warnings)) => {
            for warning in warnings {
                eprintln!("{warning}");
            }
            folder
        }
        Err(problems) => {
            for problem in problems {
                eprintln!("{problem}");
            }
            anyhow::bail!(
                "{} cannot be shown; check-theme says the same",
                dir.display()
            );
        }
    };
    let mut config = if config_path.exists() {
        Config::load(&config_path)
    } else {
        Config::default()
    };
    config.theme = Theme::Folder(folder.name.clone());
    config.language = language.unwrap_or(config.language);
    if let Some(picture) = &wallpaper {
        let file = picture
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| anyhow::anyhow!("--wallpaper takes a picture file"))?;
        config.wallpaper.dir = picture
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or(Path::new("."))
            .to_path_buf();
        config.wallpaper.pick = Pick::File(file.to_string());
    }
    let size = size.unwrap_or(if shot.is_some() {
        (3840, 2160)
    } else {
        (1920, 1080)
    });
    let xvfb = match shot {
        Some(_) => {
            let xvfb = preview::start_xvfb(size)?;
            // SAFETY: no other thread runs yet.
            unsafe { std::env::set_var("DISPLAY", &xvfb.display) };
            Some(xvfb)
        }
        None => None,
    };
    preview::prefer_lavapipe(xvfb.is_some());
    let socket_dir = preview::start_fake_greetd()?;
    let outcome = Arc::new(Mutex::new(None));
    let ran = ui::run(ui::Options {
        setting: config.theme.clone(),
        config,
        dry_run: true,
        folder: Some(folder),
        preview: Some(Preview {
            state,
            size,
            shot: shot.clone(),
            at,
            presses,
            hover,
            user,
            host,
            lock,
            outcome: outcome.clone(),
        }),
        lock: None,
    });
    let _ = std::fs::remove_dir_all(socket_dir);
    drop(xvfb);
    ran?;
    if let Some(shot) = shot {
        match outcome
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
        {
            Some(Ok(())) => println!("{}", shot.display()),
            Some(Err(error)) => return Err(error),
            None => anyhow::bail!("the preview ended before its screenshot"),
        }
    }
    Ok(())
}

fn check_theme(args: &[String]) -> anyhow::Result<()> {
    let [dir] = args else {
        anyhow::bail!("which folder?\n{USAGE}");
    };
    let dir = Path::new(dir);
    let (folder, problems) = match Folder::load(dir) {
        Ok((folder, warnings)) => (Some(folder), warnings),
        Err(problems) => (None, problems),
    };
    for problem in &problems {
        println!("{problem}");
    }
    let Some(folder) = folder else {
        let errors = problems
            .iter()
            .filter(|problem| problem.severity == Severity::Error)
            .count();
        anyhow::bail!(
            "{}: {errors} error(s); the greeter would show Paper instead",
            dir.display()
        );
    };
    let mut families: Vec<&str> = Vec::new();
    for family in folder.fonts.iter().flat_map(|font| &font.families) {
        if !families.contains(&family.as_str()) {
            families.push(family);
        }
    }
    let slides: usize = folder
        .layers
        .iter()
        .filter_map(|layer| layer.kind.slideshow())
        .map(|show| show.files.len())
        .sum();
    println!(
        "{}: {} \"{}\" reads well: {} layers, {} pictures{}, fonts {}{}",
        dir.display(),
        folder.name,
        folder.title,
        folder.layers.len(),
        folder.assets.len(),
        if slides > 0 {
            format!(", {slides} slides")
        } else {
            String::new()
        },
        if families.is_empty() {
            "built in".to_string()
        } else {
            families.join(", ")
        },
        if problems.is_empty() {
            String::new()
        } else {
            format!(", {} warning(s)", problems.len())
        },
    );
    Ok(())
}

fn install_theme(args: &[String]) -> anyhow::Result<()> {
    let mut args = Args { rest: args.iter() };
    let (mut from, mut name, mut to, mut select) = (None, None, None, false);
    let mut config_path = PathBuf::from(config::DEFAULT_PATH);
    while let Some(arg) = args.rest.next() {
        match arg.as_str() {
            "--name" => name = Some(args.value(arg)?.to_string()),
            "--to" => to = Some(PathBuf::from(args.value(arg)?)),
            "--config" => config_path = args.value(arg)?.into(),
            "--select" => select = true,
            path if from.is_none() && !path.starts_with('-') => from = Some(PathBuf::from(path)),
            other => anyhow::bail!("unknown argument {other:?}\n{USAGE}"),
        }
    }
    let from = from.ok_or_else(|| anyhow::anyhow!("which folder?\n{USAGE}"))?;
    let name = themes::install::theme_name(&from, name.as_deref())?;
    let to = to.unwrap_or_else(|| Config::load(&config_path).themes.dir);
    let installed = themes::install::install(&from, &to, &name).map_err(|error| {
        let denied = error
            .chain()
            .filter_map(|cause| cause.downcast_ref::<std::io::Error>())
            .any(|io| io.kind() == std::io::ErrorKind::PermissionDenied);
        if denied {
            error.context(format!("{} needs root: run it with sudo", to.display()))
        } else {
            error
        }
    })?;
    for warning in &installed.warnings {
        println!("{warning}");
    }
    for file in &installed.copied {
        println!("copied     {file}");
    }
    for file in &installed.skipped {
        println!("skipped    {file}");
    }
    println!(
        "installed  {} in {}",
        installed.name,
        installed.dir.display()
    );
    if select {
        themes::install::select(&config_path, &installed.name)?;
        println!(
            "selected   theme = \"{}\" in {}",
            installed.name,
            config_path.display()
        );
    } else {
        println!(
            "pick it with theme = \"{}\" in {}, or install again with --select",
            installed.name,
            config_path.display()
        );
    }
    Ok(())
}

fn sync_wallpapers(args: &[String]) -> anyhow::Result<()> {
    let mut args = Args { rest: args.iter() };
    let (mut from, mut to, mut config_path) = (None, None, PathBuf::from(config::DEFAULT_PATH));
    while let Some(arg) = args.rest.next() {
        match arg.as_str() {
            "--to" => to = Some(PathBuf::from(args.value(arg)?)),
            "--config" => config_path = args.value(arg)?.into(),
            path if from.is_none() && !path.starts_with('-') => from = Some(PathBuf::from(path)),
            other => anyhow::bail!("unknown argument {other:?}\n{USAGE}"),
        }
    }
    let from = from.ok_or_else(|| anyhow::anyhow!("which folder?\n{USAGE}"))?;
    let to = to.unwrap_or_else(|| Config::load(&config_path).wallpaper.dir);
    let report = sync::sync(&from, &to)?;
    for name in &report.copied {
        println!("copied     {name}");
    }
    for name in &report.removed {
        println!("removed    {name}");
    }
    for name in &report.duplicates {
        println!("duplicate  {name}");
    }
    for name in &report.not_pictures {
        println!("skipped    {name} (not a picture)");
    }
    println!(
        "{}: {} copied, {} unchanged, {} removed",
        to.display(),
        report.copied.len(),
        report.unchanged,
        report.removed.len(),
    );
    Ok(())
}

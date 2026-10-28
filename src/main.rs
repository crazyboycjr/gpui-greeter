use std::path::PathBuf;
use std::process::ExitCode;

use gpui_greeter::config::{self, Config, Language, Theme};
use gpui_greeter::{sync, ui};

const USAGE: &str = "\
usage: gpui-greeter [--config FILE] [--theme THEME] [--language LANG] [--dry-run]
           the greeter, as greetd starts it, with GREETD_SOCK set
       gpui-greeter sync-wallpapers FROM [--to DIR] [--config FILE]
           copy the pictures in FROM where the greeter can read them:
           to DIR, by default the wallpaper folder of the config

  --config FILE    settings, /etc/gpui-greeter/config.toml by default
  --theme THEME    paper, paper-night, frost or neon, over the config's
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

fn greet(args: &[String]) -> anyhow::Result<()> {
    let mut args = Args { rest: args.iter() };
    let (mut config_path, mut theme, mut language, mut dry_run) =
        (PathBuf::from(config::DEFAULT_PATH), None, None, false);
    while let Some(arg) = args.rest.next() {
        match arg.as_str() {
            "--config" => config_path = args.value(arg)?.into(),
            "--theme" => {
                let name = args.value(arg)?;
                theme =
                    Some(Theme::parse(name).ok_or_else(|| anyhow::anyhow!("no theme {name:?}"))?);
            }
            "--language" => {
                let name = args.value(arg)?;
                language = Some(
                    Language::parse(name).ok_or_else(|| anyhow::anyhow!("no language {name:?}"))?,
                );
            }
            "--dry-run" => dry_run = true,
            other => anyhow::bail!("unknown argument {other:?}\n{USAGE}"),
        }
    }
    let mut config = Config::load(&config_path);
    config.theme = theme.unwrap_or(config.theme);
    config.language = language.unwrap_or(config.language);
    ui::run(ui::Options { config, dry_run })
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

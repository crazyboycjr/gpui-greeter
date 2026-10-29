//! `gpui-greeter settings`: a window for the greeter's settings, run as the
//! user. It shows each look as the greeter draws it, small, and changes the
//! look, Frost's wallpapers, the user's avatar, the size and the language.
//!
//! Nothing changes until Apply. The avatar goes to AccountsService, which
//! lets users set their own; the settings file, the wallpaper folder and
//! the daily Bing timer are root's, and change in one run of
//! `gpui-greeter apply-settings` under pkexec, which takes a closed set of
//! changes and checks every one (see [`helper`]).

mod app;
pub mod apply;
pub mod avatar;
pub mod helper;
pub mod request;
pub mod setting;
mod style;
pub mod timer;
mod words;

use std::path::{Path, PathBuf};
use std::rc::Rc;

use anyhow::Result;
use gpui_kit::{AppContext as _, Bounds, TitlebarOptions, WindowBounds, WindowOptions, px, size};

use crate::config::Config;
use crate::ui::{self, SvgAssets};
use crate::x11::{Monitor, Rect, X11};
use apply::Elevate;

pub use app::Section;

/// What `gpui-greeter settings` was asked for.
#[derive(Debug, Default)]
pub struct Options {
    /// The section to open at.
    pub section: Option<Section>,
    /// A folder standing in for `/`, whose `etc/gpui-greeter/config.toml`
    /// is the settings, whose `var/lib/AccountsService/icons` keeps the
    /// avatars and whose `usr/lib/systemd/system` holds the Bing timer;
    /// the helper then runs as the user. Debug builds only.
    pub root: Option<PathBuf>,
    /// Behaves as if polkit could not ask for a password. Debug builds only.
    pub no_polkit: bool,
    /// Files the file dialogs answer with, in turn, instead of asking the
    /// desktop. Debug builds only.
    pub answers: Vec<PathBuf>,
}

/// Where the app reads and writes, and how it reaches root.
#[derive(Debug, Clone)]
pub struct Paths {
    pub config: PathBuf,
    pub icons: PathBuf,
    /// Where a request is kept for sudo, and avatars wait to be set.
    pub cache: PathBuf,
    pub elevate: Elevate,
}

impl Paths {
    fn new(options: &Options) -> Self {
        let cache = crate::cache::dir("settings").unwrap_or_else(std::env::temp_dir);
        match &options.root {
            Some(root) => Self {
                config: helper::config_path(Some(root)),
                icons: rooted(root, Path::new(avatar::ICONS)),
                cache: root.join("cache"),
                elevate: if options.no_polkit {
                    Elevate::Unavailable { root: root.clone() }
                } else {
                    Elevate::Direct { root: root.clone() }
                },
            },
            None => Self {
                config: helper::config_path(None),
                icons: PathBuf::from(avatar::ICONS),
                cache,
                elevate: Elevate::Pkexec,
            },
        }
    }

    /// The units the helper changes: the system's, or the test folder's.
    pub fn units(&self) -> timer::Units<'_> {
        match &self.elevate {
            Elevate::Pkexec => timer::Units::System,
            Elevate::Direct { root } | Elevate::Unavailable { root } => timer::Units::Root(root),
        }
    }

    /// Reads the settings as the greeter would, and the error that makes
    /// it fall back to the defaults, if any.
    pub fn load(&self) -> (Config, Option<String>) {
        let text = match std::fs::read_to_string(&self.config) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return (Config::default(), None);
            }
            Err(error) => return (Config::default(), Some(error.to_string())),
        };
        match toml::from_str::<Config>(&text) {
            Ok(_) => (Config::load(&self.config), None),
            Err(error) => (Config::default(), Some(error.message().to_string())),
        }
    }
}

fn rooted(root: &Path, path: &Path) -> PathBuf {
    root.join(path.strip_prefix("/").unwrap_or(path))
}

/// Opens the settings window and runs until it is closed.
pub fn run(options: Options) -> Result<()> {
    let x11 = Rc::new(X11::connect()?);
    let monitors = x11.monitors();
    // The monitor the greeter would show its form on; with none to ask
    // about, a 27-inch one.
    let monitor = if monitors.is_empty() {
        Monitor {
            rect: Rect {
                x: 0,
                y: 0,
                width: 2560,
                height: 1440,
            },
            size_mm: (0, 0),
            primary: true,
            refresh: None,
        }
    } else {
        monitors[ui::form_monitor(&monitors, x11.pointer())].clone()
    };
    let paths = Paths::new(&options);
    let svgs = SvgAssets::default();
    gpui_kit::application()
        .with_assets(ui::asset_source(svgs.clone()))
        .run(move |cx| {
            gpui_kit::init(cx);
            ui::register_fonts(cx);
            let (config, _) = paths.load();
            let title = words::words(config.language).title;
            let options_window = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(980.0), px(760.0)),
                    cx,
                ))),
                titlebar: Some(TitlebarOptions {
                    title: Some(title.into()),
                    ..TitlebarOptions::default()
                }),
                app_id: Some("gpui-greeter-settings".into()),
                window_min_size: Some(size(px(800.0), px(600.0))),
                ..WindowOptions::default()
            };
            let opened = gpui_kit::open_window(options_window, cx, |window, cx| {
                cx.new(|cx| {
                    app::Settings::new(
                        app::Start {
                            paths,
                            x11,
                            monitor,
                            svgs,
                            section: options.section,
                            answers: options.answers,
                        },
                        window,
                        cx,
                    )
                })
            });
            if let Err(error) = opened {
                log::error!("cannot open the settings window: {error:#}");
                cx.quit();
            }
            cx.on_window_closed(|cx, _| cx.quit()).detach();
        });
    Ok(())
}

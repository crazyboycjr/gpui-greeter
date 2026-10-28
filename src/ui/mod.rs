//! The greeter on screen: a window over each monitor, the login form on one
//! of them, drawn with GPUI and gpui-kit.

mod assets;
mod model;
mod motion;
pub mod preview;
mod screen;
mod text;
mod theme;

use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::Result;
use gpui_kit::{
    App, AppContext as _, Bounds, Entity, Window, WindowBackgroundAppearance, WindowBounds,
    WindowKind, WindowOptions, point, px, size,
};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

use crate::config::{Config, Pick, Theme};
use crate::power::Power;
use crate::scenery::{self, Look, Scenery};
use crate::sessions::Session;
use crate::themes::Folder;
use crate::users::User;
use crate::wallpaper::{self, Backdrop, Picture};
use crate::x11::{Monitor, Rect, X11};
use crate::{bing, cache};
use crate::{sessions, users};
use model::{Greeter, Setup};
use screen::Screen;
use theme::folder::{MonitorPictures, Pictures, pictures};

/// Frost's blurred wallpaper is darkened this much, as the mockup's
/// `brightness(.86)`.
const BLURRED_BRIGHTNESS: f32 = 0.86;

pub struct Options {
    pub config: Config,
    /// Power actions are only logged.
    pub dry_run: bool,
    /// The theme folder `config.theme` names, loaded.
    pub folder: Option<Folder>,
    /// Runs as `gpui-greeter preview`: one window, the size asked for.
    pub preview: Option<preview::Preview>,
}

pub fn run(options: Options) -> Result<()> {
    let Options {
        config,
        dry_run,
        folder,
        preview,
    } = options;
    let folder = folder.map(Arc::new);
    let x11 = Rc::new(X11::connect()?);
    let monitors = match &preview {
        Some(preview) => vec![Monitor {
            rect: Rect {
                x: 0,
                y: 0,
                width: preview.size.0,
                height: preview.size.1,
            },
            size_mm: (0, 0),
            primary: true,
            refresh: None,
        }],
        None => {
            x11.keep_monitors_awake();
            x11.monitors()
        }
    };
    let drawing = Look::of(&config).map(|look| draw_scenery(look, &monitors));
    let preparing = folder.clone().map(|folder| prepare(folder, &monitors));
    let form = form_monitor(&monitors, x11.pointer());
    let scale = set_scale(&monitors[form]);
    let shows_wallpaper =
        config.theme == Theme::Frost || folder.as_ref().is_some_and(|f| f.wallpaper.is_some());
    let wallpaper = shows_wallpaper.then(|| Wallpaper::pick(&config));
    let waits_for_wallpaper = wallpaper
        .as_ref()
        .is_some_and(|wallpaper| wallpaper.now.is_some() || wallpaper.bing.is_some());
    let users = match preview.as_ref().and_then(|preview| preview.user.clone()) {
        Some(name) => vec![User {
            display_name: name.clone(),
            name,
            avatar: None,
            shell: "/bin/sh".into(),
        }],
        None => users::load(),
    };
    let mut sessions = sessions::load(&config.sessions.dir);
    if preview.is_some() && sessions.is_empty() {
        sessions.push(Session {
            id: "desktop".into(),
            name: "Desktop".into(),
            exec: vec!["true".into()],
            desktop_names: None,
        });
    }
    let blur = match folder.as_ref().and_then(|folder| folder.wallpaper) {
        Some(frame_px) => Blur {
            css_px: frame_px * theme::DESIGN_FRAME.0 / crate::themes::FRAME.0,
            brightness: 1.0,
        },
        None => Blur {
            css_px: config.frost.blur,
            brightness: BLURRED_BRIGHTNESS,
        },
    };
    let setup = Setup {
        users,
        sessions,
        monitors: monitors.clone(),
        form,
        x11: x11.clone(),
        power: Power { dry_run },
        folder: folder.clone(),
        preview: preview.is_some(),
        hostname: preview.as_ref().and_then(|preview| preview.host.clone()),
        config,
    };
    log::info!(
        "{} monitor(s), the form on {:?}, scale {scale}",
        monitors.len(),
        monitors[form].rect
    );

    gpui_kit::application()
        .with_assets(assets::Assets::new(folder.as_deref()))
        .run(move |cx| {
            gpui_kit::init(cx);
            assets::register_fonts(cx, folder.as_deref());
            let greeter = cx.new(|cx| Greeter::new(setup, cx));
            let mut sceneries = drawing.map(finish_scenery).unwrap_or_default().into_iter();
            let (shared, mut drawn) = match preparing.map(Preparing::finish) {
                Some((shared, drawn)) => (Some(shared), drawn.into_iter()),
                None => (None, Vec::new().into_iter()),
            };
            let mut screens = Vec::new();
            for (output, monitor) in monitors.iter().enumerate() {
                let scenery = sceneries.next().flatten();
                let pictures = shared.clone().zip(drawn.next());
                match open_screen(cx, &greeter, output, monitor, scale, scenery, pictures) {
                    Ok((screen, xid)) => {
                        if let Some(xid) = xid
                            && let Err(error) = x11.place(xid, monitor.rect)
                        {
                            log::warn!("cannot place the window on {:?}: {error:#}", monitor.rect);
                        }
                        greeter.update(cx, |greeter, _| greeter.set_window(output, xid));
                        screens.push((screen, monitor.clone()));
                    }
                    Err(error) => {
                        log::error!("cannot open a window on {:?}: {error:#}", monitor.rect)
                    }
                }
            }
            if let (Some(preview), Some((screen, monitor))) = (&preview, screens.first()) {
                preview::drive(
                    cx,
                    preview,
                    greeter.clone(),
                    screen.clone(),
                    x11.clone(),
                    monitor.rect,
                    waits_for_wallpaper,
                );
            }
            if let Some(wallpaper) = wallpaper {
                let screens = Screens {
                    windows: screens,
                    requests: Cell::new(0),
                };
                wallpaper.show(cx, Rc::new(screens), blur);
            }
        });
    Ok(())
}

/// The primary monitor, else the one under the pointer, else the largest.
fn form_monitor(monitors: &[Monitor], pointer: Option<(i32, i32)>) -> usize {
    let under_pointer = pointer.and_then(|at| {
        monitors
            .iter()
            .position(|monitor| monitor.rect.contains_point(at))
    });
    monitors
        .iter()
        .position(|monitor| monitor.primary)
        .or(under_pointer)
        .or_else(|| {
            (0..monitors.len()).max_by_key(|&i| {
                u64::from(monitors[i].rect.width) * u64::from(monitors[i].rect.height)
            })
        })
        .unwrap_or(0)
}

/// GPUI applies one scale to every window; it is the form's monitor's,
/// from its pixel density, unless GPUI_X11_SCALE_FACTOR says otherwise.
fn set_scale(monitor: &Monitor) -> f32 {
    if let Some(scale) = std::env::var("GPUI_X11_SCALE_FACTOR")
        .ok()
        .and_then(|value| value.parse().ok())
    {
        return scale;
    }
    let scale = monitor.scale();
    // SAFETY: GPUI and every other thread start later.
    unsafe { std::env::set_var("GPUI_X11_SCALE_FACTOR", scale.to_string()) };
    scale
}

/// Frost's wallpaper: the picture to show now, and, when the pick is
/// Bing's picture of the day and today's is not fetched yet, where to fetch
/// it from, to show once it arrives.
struct Wallpaper {
    now: Option<PathBuf>,
    bing: Option<BingFetch>,
}

#[derive(Clone)]
struct BingFetch {
    url: String,
    dir: PathBuf,
    day: String,
}

impl Wallpaper {
    fn pick(config: &Config) -> Self {
        let today = jiff::Zoned::now().date();
        let local = || {
            let pictures = wallpaper::candidates(&config.wallpaper.dir);
            let seed = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |time| {
                    time.as_nanos() as u64 ^ u64::from(std::process::id())
                });
            let day = today
                .since(jiff::civil::date(2000, 1, 1))
                .map_or(0, |span| span.get_days().into());
            let picked = wallpaper::choose(&pictures, &config.wallpaper.pick, seed, day);
            if picked.is_none() {
                log::warn!("no wallpaper in {}", config.wallpaper.dir.display());
            }
            picked
        };
        let bing_dir = cache::dir("bing").filter(|_| config.wallpaper.pick == Pick::Bing);
        let Some(dir) = bing_dir else {
            return Self {
                now: local(),
                bing: None,
            };
        };
        let day = today.to_string();
        if let Some(fetched) = bing::of_day(&dir, &day) {
            return Self {
                now: Some(fetched),
                bing: None,
            };
        }
        Self {
            now: bing::latest(&dir).or_else(local),
            bing: Some(BingFetch {
                url: config.wallpaper.bing_url.clone(),
                dir,
                day,
            }),
        }
    }

    fn show(self, cx: &mut App, screens: Rc<Screens>, blur: Blur) {
        if let Some(path) = self.now {
            load_backdrops(cx, path, screens.clone(), blur);
        }
        if let Some(fetch) = self.bing {
            fetch_bing(cx, fetch, screens, blur);
        }
    }
}

/// The windows Frost's wallpaper goes to, and how many pictures were asked
/// for: a window keeps the one asked for last, whichever is made first.
struct Screens {
    windows: Vec<(Entity<Screen>, Monitor)>,
    requests: Cell<u64>,
}

/// Fetches today's picture, trying again for a while in case the network
/// comes up after the greeter.
fn fetch_bing(cx: &mut App, fetch: BingFetch, screens: Rc<Screens>, blur: Blur) {
    const ATTEMPTS: u32 = 6;
    const PAUSE: Duration = Duration::from_secs(20);
    cx.spawn(async move |cx| {
        for attempt in 1..=ATTEMPTS {
            let job = fetch.clone();
            let fetched = cx
                .background_executor()
                .spawn(async move { bing::fetch(&job.url, &job.dir, &job.day) })
                .await;
            match fetched {
                Ok(path) => {
                    cx.update(|cx| load_backdrops(cx, path, screens, blur));
                    return;
                }
                Err(error) => log::warn!("Bing's picture, attempt {attempt}: {error:#}"),
            }
            cx.background_executor().timer(PAUSE).await;
        }
    })
    .detach();
}

/// Starts drawing each monitor's scenery on a thread of its own, while the
/// toolkit starts.
fn draw_scenery(look: Look, monitors: &[Monitor]) -> Vec<JoinHandle<Option<Arc<Scenery>>>> {
    let cache = cache::dir("scenery");
    monitors
        .iter()
        .map(|monitor| {
            let pixels = (monitor.rect.width, monitor.rect.height);
            let cache = cache.clone();
            std::thread::spawn(move || {
                let started = Instant::now();
                match scenery::draw(look, pixels, cache.as_deref()) {
                    Ok(scenery) => {
                        log::info!("scenery for {pixels:?} drawn in {:?}", started.elapsed());
                        Some(Arc::new(scenery))
                    }
                    Err(error) => {
                        log::error!("cannot draw the scenery for {pixels:?}: {error:#}");
                        None
                    }
                }
            })
        })
        .collect()
}

/// Each monitor's scenery, once drawn: the windows open with it, so that
/// the look never shows without it.
fn finish_scenery(drawing: Vec<JoinHandle<Option<Arc<Scenery>>>>) -> Vec<Option<Arc<Scenery>>> {
    drawing
        .into_iter()
        .map(|thread| thread.join().ok().flatten())
        .collect()
}

/// How Frost, or a theme's wallpaper layer, blurs the wallpaper.
#[derive(Debug, Clone, Copy)]
struct Blur {
    css_px: f32,
    brightness: f32,
}

/// A theme folder's pictures, being made ready while the toolkit starts.
struct Preparing {
    shared: JoinHandle<Pictures>,
    monitors: Vec<JoinHandle<MonitorPictures>>,
}

fn prepare(folder: Arc<Folder>, monitors: &[Monitor]) -> Preparing {
    let largest = monitors.iter().fold((1, 1), |(w, h), monitor| {
        (w.max(monitor.rect.width), h.max(monitor.rect.height))
    });
    let cache = cache::dir("themes").map(|dir| dir.join(&folder.name));
    let shared = {
        let folder = folder.clone();
        std::thread::spawn(move || pictures::decode(&folder, largest))
    };
    let monitors = monitors
        .iter()
        .map(|monitor| {
            let (folder, cache) = (folder.clone(), cache.clone());
            let pixels = (monitor.rect.width, monitor.rect.height);
            std::thread::spawn(move || {
                let started = Instant::now();
                let drawn = pictures::draw(&folder, pixels, cache.as_deref());
                log::info!(
                    "theme {} for {pixels:?} ready in {:?}",
                    folder.name,
                    started.elapsed()
                );
                drawn
            })
        })
        .collect();
    Preparing { shared, monitors }
}

impl Preparing {
    fn finish(self) -> (Arc<Pictures>, Vec<Arc<MonitorPictures>>) {
        let shared = Arc::new(self.shared.join().unwrap_or_default());
        let drawn = self
            .monitors
            .into_iter()
            .map(|thread| Arc::new(thread.join().unwrap_or_default()))
            .collect();
        (shared, drawn)
    }
}

fn open_screen(
    cx: &mut App,
    greeter: &Entity<Greeter>,
    output: usize,
    monitor: &Monitor,
    scale: f32,
    scenery: Option<Arc<Scenery>>,
    pictures: Option<(Arc<Pictures>, Arc<MonitorPictures>)>,
) -> Result<(Entity<Screen>, Option<u32>)> {
    let rect = monitor.rect;
    let logical = |pixels: f32| px(pixels / scale);
    let bounds = Bounds::new(
        point(logical(rect.x as f32), logical(rect.y as f32)),
        size(logical(rect.width as f32), logical(rect.height as f32)),
    );
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: None,
        focus: false,
        show: true,
        kind: WindowKind::Normal,
        is_movable: false,
        is_resizable: false,
        is_minimizable: false,
        window_background: WindowBackgroundAppearance::Opaque,
        app_id: Some("gpui-greeter".into()),
        ..Default::default()
    };
    let (handle, screen) = gpui_kit::open_window(options, cx, |window, cx| {
        cx.new(|cx| {
            Screen::new(
                greeter.clone(),
                output,
                monitor.clone(),
                scenery,
                pictures,
                window,
                cx,
            )
        })
    })?;
    let xid = handle.update(cx, |_, window, _| xid(window)).ok().flatten();
    Ok((screen, xid))
}

fn xid(window: &Window) -> Option<u32> {
    match HasWindowHandle::window_handle(window).ok()?.as_raw() {
        RawWindowHandle::Xcb(handle) => Some(handle.window.get()),
        RawWindowHandle::Xlib(handle) => u32::try_from(handle.window).ok(),
        _ => None,
    }
}

/// Makes each monitor's sharp and blurred wallpaper in the background and
/// hands it to its window.
fn load_backdrops(cx: &mut App, path: PathBuf, screens: Rc<Screens>, blur: Blur) {
    log::info!("wallpaper {}", path.display());
    let request = screens.requests.get() + 1;
    screens.requests.set(request);
    let jobs: Vec<((u32, u32), f32)> = screens
        .windows
        .iter()
        .map(|(_, monitor)| {
            let (width, height) = (monitor.rect.width, monitor.rect.height);
            (
                (width, height),
                blur.css_px * theme::design_unit(width as f32, height as f32),
            )
        })
        .collect();
    cx.spawn(async move |cx| {
        let made = cx
            .background_executor()
            .spawn(async move {
                let picture = Picture::open(&path)?;
                jobs.into_iter()
                    .map(|(size, sigma)| Backdrop::new(&picture, size, sigma, blur.brightness))
                    .collect::<Result<Vec<_>>>()
            })
            .await;
        match made {
            Ok(backdrops) => {
                for ((screen, _), backdrop) in screens.windows.iter().zip(backdrops) {
                    screen.update(cx, |screen, cx| screen.set_backdrop(backdrop, request, cx));
                }
            }
            Err(error) => log::error!("cannot show the wallpaper: {error:#}"),
        }
    })
    .detach();
}

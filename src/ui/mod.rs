//! The greeter on screen: a window over each monitor, the login form on one
//! of them, drawn with GPUI and gpui-kit.

mod assets;
mod frames;
mod looks;
pub mod miniature;
mod model;
mod motion;
pub mod preview;
mod screen;
mod text;
mod theme;

pub use frames::enabled as frames_timed;
pub use looks::SvgAssets;

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use anyhow::Result;
use gpui_kit::{
    AnyWindowHandle, App, AppContext as _, Bounds, Entity, KeyBinding, Modifiers, MouseButton,
    MouseDownEvent, MouseExitEvent, MouseMoveEvent, MouseUpEvent, NoAction, PlatformInput, Window,
    WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions, point, px, size,
};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

use crate::config::{Config, Pick, Theme};
use crate::lock::link::{self, ToLock, ToView};
use crate::power::Power;
use crate::sessions::Session;
use crate::themes::{FRAME, Folder};
use crate::users::User;
use crate::wallpaper::{self, Backdrop, Picture};
use crate::x11::{Monitor, Rect, X11};
use crate::{bing, scenery, vt};
use crate::{sessions, users};
use looks::Looks;
use model::{Greeter, Setup};
use screen::Screen;

/// Frost's blurred wallpaper is darkened this much, as the mockup's
/// `brightness(.86)`.
const BLURRED_BRIGHTNESS: f32 = 0.86;

/// How long the windows wait for the first look's pictures; a drawing that
/// takes longer, as the first of a heavy SVG may, shows once it is done.
const PICTURES_WAIT: Duration = Duration::from_millis(1500);

pub struct Options {
    pub config: Config,
    /// The theme the settings name, which a theme picked at the login
    /// screen is remembered against.
    pub setting: Theme,
    /// Power actions are only logged.
    pub dry_run: bool,
    /// The theme folder `config.theme` names, loaded.
    pub folder: Option<Folder>,
    /// Runs as `gpui-greeter preview`: one window, the size asked for.
    pub preview: Option<preview::Preview>,
    /// Runs as the lock screen's view, for the supervisor at the link's
    /// other end.
    pub lock: Option<link::ViewEnd>,
}

pub fn run(options: Options) -> Result<()> {
    let Options {
        config,
        setting,
        dry_run,
        folder,
        preview,
        lock,
    } = options;
    let lock_screen = lock.is_some() || preview.as_ref().is_some_and(|preview| preview.lock);
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
            // The session's monitors keep the settings it gave them.
            if lock.is_none() {
                x11.keep_monitors_awake();
            }
            x11.monitors()
        }
    };
    let form = form_monitor(&monitors, x11.pointer());
    let scale = set_scale(&monitors[form]);
    let preparing = looks::start(&config, folder.as_ref(), &monitors);
    let link = lock.map(link::ViewEnd::listen).transpose()?;
    let users = match preview.as_ref().and_then(|preview| preview.user.clone()) {
        Some(name) => vec![User {
            display_name: name.clone(),
            name,
            avatar: None,
            shell: "/bin/sh".into(),
        }],
        None if lock_screen => users::current().into_iter().collect(),
        None => users::load(),
    };
    let mut sessions = sessions::load(&config.sessions.dir);
    if lock_screen {
        sessions = sessions::current(sessions).into_iter().collect();
    }
    if preview.is_some() && sessions.is_empty() {
        sessions.push(Session {
            id: "desktop".into(),
            name: "Desktop".into(),
            exec: vec!["true".into()],
            desktop_names: None,
        });
    }
    let svgs: SvgAssets = Arc::new(RwLock::new(HashMap::new()));
    if let Some(folder) = &folder {
        looks::add_svgs(&svgs, folder);
    }
    let waits_for_wallpaper = wallpaper_blur(&config.theme, folder.as_deref(), &config).is_some()
        && !wallpaper::candidates(&config.wallpaper.dir).is_empty();
    log::info!(
        "{} monitor(s), the form on {:?}, scale {scale}",
        monitors.len(),
        monitors[form].rect
    );

    gpui_kit::application()
        .with_assets(assets::Assets { svgs: svgs.clone() })
        .run(move |cx| {
            gpui_kit::init(cx);
            // gpui-kit's root moves the focus on Tab and Shift+Tab, which
            // would keep them from the greeter's own keys.
            cx.bind_keys([
                KeyBinding::new("tab", NoAction, None),
                KeyBinding::new("shift-tab", NoAction, None),
            ]);
            assets::register_fonts(cx, folder.as_deref());
            let (prepared, later) = preparing.finish(Some(PICTURES_WAIT));
            let first = config.theme.clone();
            let switching = preview.is_none();
            let looks = Looks::new(&config, &monitors, first.clone(), prepared, svgs, switching);
            let setup = Setup {
                users,
                sessions,
                monitors: monitors.clone(),
                form,
                x11: x11.clone(),
                power: Power { dry_run },
                looks,
                setting,
                preview: preview.is_some(),
                hostname: preview.as_ref().and_then(|preview| preview.host.clone()),
                config: config.clone(),
                lock_screen,
                supervisor: link.as_ref().map(|(sender, _)| sender.clone()),
            };
            let greeter = cx.new(|cx| Greeter::new(setup, cx));
            greeter.update(cx, |_, cx| {
                for (output, thread) in later {
                    looks::deliver(first.clone(), output, thread, cx);
                }
            });
            let mut screens = Vec::new();
            let mut windows = Vec::new();
            for (output, monitor) in monitors.iter().enumerate() {
                match open_screen(cx, &greeter, output, monitor, scale, link.is_some()) {
                    Ok((screen, window, xid)) => {
                        if let Some(xid) = xid
                            && let Err(error) = x11.place(xid, monitor.rect)
                        {
                            log::warn!("cannot place the window on {:?}: {error:#}", monitor.rect);
                        }
                        greeter.update(cx, |greeter, _| greeter.set_window(output, xid));
                        screens.push((screen, monitor.clone()));
                        windows.push((window, monitor.rect, xid));
                    }
                    Err(error) => {
                        log::error!("cannot open a window on {:?}: {error:#}", monitor.rect)
                    }
                }
            }
            let first_window = windows.first().map(|(window, _, _)| *window);
            if let Some((sender, said)) = link {
                let xids: Vec<u32> = windows.iter().filter_map(|(_, _, xid)| *xid).collect();
                if let Some(&keys) = xids.first() {
                    let shown = ToLock::Shown {
                        windows: xids.clone(),
                        keys,
                    };
                    if let Err(error) = sender.send(&shown) {
                        log::error!("cannot reach the lock's supervisor: {error}");
                    }
                }
                let windows = windows
                    .iter()
                    .map(|(window, rect, _)| (*window, *rect))
                    .collect();
                follow_supervisor(cx, greeter.clone(), windows, said);
            }
            if let (Some(preview), Some((screen, monitor)), Some(window)) =
                (&preview, screens.first(), first_window)
            {
                preview::drive(
                    cx,
                    preview,
                    greeter.clone(),
                    screen.clone(),
                    window,
                    x11.clone(),
                    monitor.rect,
                    waits_for_wallpaper,
                );
            }
            let screens = Rc::new(Screens {
                windows: screens,
                requests: Cell::new(0),
                shown: RefCell::new(None),
            });
            // The login screen fetches Bing's pictures as the greeter's
            // user; the lock screen and previews, run as anyone, show them.
            let fetch = !lock_screen && preview.is_none();
            follow_wallpaper(cx, greeter.clone(), screens, config, fetch);
            if preview.is_none() {
                follow_vt(cx, greeter.clone());
            }
            if switching {
                // The other looks, once the first frames are out.
                cx.spawn(async move |cx| {
                    cx.background_executor().timer(Duration::from_secs(2)).await;
                    greeter.update(cx, |greeter, cx| greeter.prepare_looks(cx));
                })
                .detach();
            }
        });
    Ok(())
}

/// What another app of this binary needs to draw the looks, as the
/// settings app's preview does: an asset source with the looks' icons and
/// theme folders' SVGs, which `svgs` collects.
pub fn asset_source(svgs: SvgAssets) -> impl gpui_kit::AssetSource {
    assets::Assets { svgs }
}

/// Hands GPUI the looks' fonts.
pub fn register_fonts(cx: &mut App) {
    assets::register_fonts(cx, None);
}

/// The primary monitor, else the one under the pointer, else the largest.
pub fn form_monitor(monitors: &[Monitor], pointer: Option<(i32, i32)>) -> usize {
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

/// The wallpaper: the picture to show now, and, when the pick is Bing's
/// picture of the day and today's is not kept yet, where to fetch it from,
/// to show once it arrives.
struct Wallpaper {
    now: Option<PathBuf>,
    bing: Option<BingFetch>,
}

#[derive(Clone)]
struct BingFetch {
    url: String,
    dir: PathBuf,
    today: String,
}

impl Wallpaper {
    /// The picture `config` picks; with `fetch`, Bing's picture of the day
    /// is fetched into the Bing folder, which only the greeter's user may
    /// write.
    fn pick(config: &Config, fetch: bool) -> Self {
        let local = || {
            let pictures = wallpaper::candidates(&config.wallpaper.dir);
            let picked = wallpaper::picked(&pictures, &config.wallpaper.pick);
            if picked.is_none() {
                log::warn!("no wallpaper in {}", config.wallpaper.dir.display());
            }
            picked
        };
        let dir = &config.wallpaper.bing_dir;
        match &config.wallpaper.pick {
            Pick::Bing => {
                let today = jiff::Zoned::now().date().to_string();
                let latest = bing::latest(dir);
                // Bing's day can begin before the one here.
                let current = latest
                    .as_deref()
                    .and_then(bing::day_of)
                    .is_some_and(|day| day >= today.as_str());
                Self {
                    now: latest.or_else(local),
                    bing: (fetch && !current).then(|| BingFetch {
                        url: config.wallpaper.bing_url.clone(),
                        dir: dir.clone(),
                        today,
                    }),
                }
            }
            Pick::BingFile(name) => Self {
                now: bing::named(dir, name)
                    .or_else(|| bing::latest(dir))
                    .or_else(local),
                bing: None,
            },
            _ => Self {
                now: local(),
                bing: None,
            },
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

/// The windows the wallpaper goes to, and how many pictures were asked for:
/// a window keeps the one asked for last, whichever is made first.
struct Screens {
    windows: Vec<(Entity<Screen>, Monitor)>,
    requests: Cell<u64>,
    /// The picture shown last, and how it was blurred.
    shown: RefCell<Option<(PathBuf, Blur)>>,
}

/// How Frost, or a theme's wallpaper layer, blurs the wallpaper.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Blur {
    /// In pixels of the 3840 by 2160 frame: a share of the picture, which
    /// fills the monitor whatever the look's scale.
    frame_px: f32,
    brightness: f32,
}

/// How the look blurs the wallpaper, if it shows one.
fn wallpaper_blur(theme: &Theme, folder: Option<&Folder>, config: &Config) -> Option<Blur> {
    match theme {
        // Frost's blur is in the pixels of the mockup's frame.
        Theme::Frost => Some(Blur {
            frame_px: config.frost.blur * FRAME.0 / scenery::MOCKUP.0 as f32,
            brightness: BLURRED_BRIGHTNESS,
        }),
        Theme::Folder(_) => folder
            .and_then(|folder| folder.wallpaper)
            .map(|frame_px| Blur {
                frame_px,
                brightness: 1.0,
            }),
        _ => None,
    }
}

/// Shows the wallpaper behind every look that draws one, picked once and
/// blurred as each look asks; `fetch` as [`Wallpaper::pick`] takes it.
fn follow_wallpaper(
    cx: &mut App,
    greeter: Entity<Greeter>,
    screens: Rc<Screens>,
    config: Config,
    fetch: bool,
) {
    let picked = Cell::new(false);
    let update = move |greeter: Entity<Greeter>, cx: &mut App| {
        let blur = {
            let greeter = greeter.read(cx);
            wallpaper_blur(greeter.theme(), greeter.folder.as_deref(), &config)
        };
        let Some(blur) = blur else {
            return;
        };
        let shown = screens.shown.borrow().clone();
        match shown {
            Some((_, done)) if done == blur => {}
            Some((path, _)) => load_backdrops(cx, path, screens.clone(), blur),
            None if !picked.get() => {
                picked.set(true);
                Wallpaper::pick(&config, fetch).show(cx, screens.clone(), blur);
            }
            None => {}
        }
    };
    update(greeter.clone(), cx);
    cx.observe(&greeter, update).detach();
}

/// Takes the windows off the screen while the greeter's VT is in the
/// background, and puts them back when it returns.
fn follow_vt(cx: &mut App, greeter: Entity<Greeter>) {
    let (sender, receiver) = async_channel::unbounded();
    vt::watch(move |shown| {
        let _ = sender.send_blocking(shown);
    });
    cx.spawn(async move |cx| {
        while let Ok(shown) = receiver.recv().await {
            greeter.update(cx, |greeter, cx| greeter.set_on_screen(shown, cx));
        }
    })
    .detach();
}

/// Follows what the lock's supervisor says: the pointer it holds moves and
/// clicks over the windows here, and PAM's word goes to the greeter.
fn follow_supervisor(
    cx: &mut App,
    greeter: Entity<Greeter>,
    windows: Vec<(AnyWindowHandle, Rect)>,
    said: async_channel::Receiver<ToView>,
) {
    cx.spawn(async move |cx| {
        let mut under: Option<AnyWindowHandle> = None;
        let mut held: Option<MouseButton> = None;
        while let Ok(message) = said.recv().await {
            let (x, y) = match message {
                ToView::Moved { x, y }
                | ToView::Pressed { x, y, .. }
                | ToView::Released { x, y, .. } => (x, y),
                said => {
                    greeter.update(cx, |greeter, cx| greeter.checked(said, cx));
                    continue;
                }
            };
            let Some(&(window, rect)) =
                windows.iter().find(|(_, rect)| rect.contains_point((x, y)))
            else {
                continue;
            };
            if let Some(left) = under.filter(|left| *left != window) {
                let _ = cx.update_window(left, |_, window, cx| {
                    let exit = MouseExitEvent {
                        position: window.mouse_position(),
                        pressed_button: held,
                        modifiers: Modifiers::default(),
                    };
                    window.dispatch_event(PlatformInput::MouseExited(exit), cx);
                });
            }
            under = Some(window);
            let button = |number: u8| match number {
                1 => Some(MouseButton::Left),
                2 => Some(MouseButton::Middle),
                3 => Some(MouseButton::Right),
                _ => None,
            };
            let _ = cx.update_window(window, |_, window, cx| {
                let scale = window.scale_factor();
                let position = point(
                    px((x - rect.x) as f32 / scale),
                    px((y - rect.y) as f32 / scale),
                );
                let modifiers = Modifiers::default();
                let input = match message {
                    ToView::Pressed { button: number, .. } => {
                        let button = button(number)?;
                        held = Some(button);
                        PlatformInput::MouseDown(MouseDownEvent {
                            button,
                            position,
                            modifiers,
                            click_count: 1,
                            first_mouse: false,
                        })
                    }
                    ToView::Released { button: number, .. } => {
                        let button = button(number)?;
                        held = None;
                        PlatformInput::MouseUp(MouseUpEvent {
                            button,
                            position,
                            modifiers,
                            click_count: 1,
                        })
                    }
                    _ => PlatformInput::MouseMove(MouseMoveEvent {
                        position,
                        pressed_button: held,
                        modifiers,
                    }),
                };
                window.dispatch_event(input, cx);
                Some(())
            });
        }
    })
    .detach();
}

/// Fetches today's picture, trying again for a while in case the network
/// comes up after the greeter, and shows the newest kept if it is new.
fn fetch_bing(cx: &mut App, fetch: BingFetch, screens: Rc<Screens>, blur: Blur) {
    const ATTEMPTS: u32 = 6;
    const PAUSE: Duration = Duration::from_secs(20);
    cx.spawn(async move |cx| {
        for attempt in 1..=ATTEMPTS {
            let job = fetch.clone();
            let fetched = cx
                .background_executor()
                .spawn(async move { bing::fetch(&job.url, &job.dir, &job.today) })
                .await;
            match fetched {
                Ok(added) => {
                    for path in &added {
                        log::info!("kept Bing's picture {}", path.display());
                    }
                    let shown = screens.shown.borrow().clone();
                    let latest = bing::latest(&fetch.dir);
                    if let Some(latest) = latest
                        && shown.as_ref().is_none_or(|(path, _)| *path != latest)
                    {
                        let blur = shown.map_or(blur, |(_, blur)| blur);
                        cx.update(|cx| load_backdrops(cx, latest, screens, blur));
                    }
                    return;
                }
                Err(error) => log::warn!("Bing's picture, attempt {attempt}: {error:#}"),
            }
            cx.background_executor().timer(PAUSE).await;
        }
    })
    .detach();
}

fn open_screen(
    cx: &mut App,
    greeter: &Entity<Greeter>,
    output: usize,
    monitor: &Monitor,
    scale: f32,
    over_session: bool,
) -> Result<(Entity<Screen>, AnyWindowHandle, Option<u32>)> {
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
        // The lock's windows lie over the session's, out of its window
        // manager's hands.
        kind: if over_session {
            WindowKind::PopUp
        } else {
            WindowKind::Normal
        },
        is_movable: false,
        is_resizable: false,
        is_minimizable: false,
        window_background: WindowBackgroundAppearance::Opaque,
        app_id: Some("gpui-greeter".into()),
        // GPUI draws the motion of a window without the keyboard at most
        // 30 times a second, and the lock's never have it: its supervisor
        // holds the keyboard. Nor do the monitors without the form.
        inactive_frame_interval: None,
        ..Default::default()
    };
    let (handle, screen) = gpui_kit::open_window(options, cx, |window, cx| {
        cx.new(|cx| Screen::new(greeter.clone(), output, monitor.clone(), window, cx))
    })?;
    let xid = handle.update(cx, |_, window, _| xid(window)).ok().flatten();
    Ok((screen, handle, xid))
}

/// The X window GPUI made for `window`.
pub fn xid(window: &Window) -> Option<u32> {
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
    *screens.shown.borrow_mut() = Some((path.clone(), blur));
    let jobs: Vec<((u32, u32), f32)> = screens
        .windows
        .iter()
        .map(|(_, monitor)| {
            let (width, height) = (monitor.rect.width, monitor.rect.height);
            let frame = (width as f32 / FRAME.0).min(height as f32 / FRAME.1);
            ((width, height), blur.frame_px * frame)
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

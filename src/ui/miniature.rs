//! A look drawn small inside another window, for the settings app's
//! preview: the greeter's own model and drawing, laid out in a monitor's
//! points and shrunk to the box it is given. Nothing in it can be clicked,
//! and its power buttons would only say what they would do.

use std::borrow::Cow;
use std::collections::HashSet;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui_kit::prelude::*;
use gpui_kit::{App, Entity, Pixels, Size, Task, Window, div};

use super::looks::{self, Looks, Prepared, SvgAssets};
use super::model::{Greeter, Setup};
use super::screen::Screen;
use super::{Blur, wallpaper_blur};
use crate::config::{Config, Theme};
use crate::i18n;
use crate::power::Power;
use crate::scenery;
use crate::sessions::Session;
use crate::themes::{self, FRAME, Folder};
use crate::users::User;
use crate::wallpaper::{Backdrop, Picture};
use crate::x11::{Monitor, X11};

/// What the preview shows.
#[derive(Debug, Clone, PartialEq)]
pub struct Shot {
    /// The settings, whose `theme` is the look shown.
    pub config: Config,
    pub user: User,
    pub session: Option<Session>,
    /// The wallpaper, for a look that shows one.
    pub wallpaper: Option<PathBuf>,
    /// At rest, for a look that rests: Frost's clock alone.
    pub rest: bool,
    /// The box, in the window's pixels.
    pub size: Size<Pixels>,
    /// The device's pixels to a window pixel.
    pub scale_factor: f32,
}

impl Shot {
    fn pixels(&self) -> (u32, u32) {
        let device = |length: Pixels| (length.as_f32() * self.scale_factor).round().max(1.0) as u32;
        (device(self.size.width), device(self.size.height))
    }
}

/// What a look's pictures were drawn for: a change to any of it draws them
/// again.
#[derive(Debug, Clone, PartialEq)]
struct Drawn {
    theme: Theme,
    scenery: Option<scenery::Look>,
    scale: f32,
    pixels: (u32, u32),
}

impl Drawn {
    fn of(shot: &Shot) -> Self {
        Self {
            theme: shot.config.theme.clone(),
            scenery: scenery::Look::of(&shot.config),
            scale: shot.config.scale,
            pixels: shot.pixels(),
        }
    }
}

struct Shown {
    greeter: Entity<Greeter>,
    screen: Entity<Screen>,
    drawn: Drawn,
}

#[derive(Default)]
struct Wallpaper {
    /// The picture, its size and its blur, last asked for.
    asked: Option<(PathBuf, (u32, u32), Blur)>,
    /// The picture last decoded, kept for another blur or size.
    decoded: Option<(PathBuf, Arc<Picture>)>,
    made: Option<Backdrop>,
    requests: u64,
}

pub struct Miniature {
    x11: Rc<X11>,
    monitor: Monitor,
    svgs: SvgAssets,
    shot: Option<Shot>,
    shown: Option<Shown>,
    /// Whether the look asked for last could not be made ready.
    failed: bool,
    /// Counts the looks asked to be made ready, so that only the last is
    /// shown.
    preparing: u64,
    wallpaper: Wallpaper,
    /// The theme folders whose fonts are registered.
    fonts: HashSet<String>,
    _awake: Task<()>,
}

impl Miniature {
    /// A preview of the looks as `monitor` shows them.
    pub fn new(x11: Rc<X11>, monitor: Monitor, svgs: SvgAssets, cx: &mut Context<Self>) -> Self {
        Self {
            x11,
            monitor,
            svgs,
            shot: None,
            shown: None,
            failed: false,
            preparing: 0,
            wallpaper: Wallpaper::default(),
            fonts: HashSet::new(),
            _awake: Self::keep_awake(cx),
        }
    }

    /// The monitor whose points the looks are laid out in.
    pub fn monitor(&self) -> &Monitor {
        &self.monitor
    }

    /// Whether the look asked for cannot be shown.
    pub fn failed(&self) -> bool {
        self.failed
    }

    /// Shows `shot`: at once what can change in place, and the rest once
    /// its pictures are drawn, the look before showing meanwhile.
    pub fn show(&mut self, shot: Shot, cx: &mut Context<Self>) {
        if self.shot.as_ref() == Some(&shot) {
            return;
        }
        let drawn = Drawn::of(&shot);
        let only_resized = self.shot.as_ref().is_some_and(|old| {
            let before = Drawn::of(old);
            before.pixels != drawn.pixels
                && Drawn {
                    pixels: drawn.pixels,
                    ..before
                } == drawn
        });
        self.shot = Some(shot);
        match self.shown.as_ref().map(|shown| shown.drawn.clone()) {
            Some(shown) if shown == drawn => self.change_in_place(cx),
            Some(shown) if shown.theme == drawn.theme => {
                self.change_in_place(cx);
                self.prepare(drawn, only_resized, cx);
            }
            _ => self.prepare(drawn, false, cx),
        }
        self.show_wallpaper(cx);
        cx.notify();
    }

    /// What changes without drawing pictures again: the words, the
    /// settings the look reads as it draws, the user, rest and the box.
    fn change_in_place(&mut self, cx: &mut Context<Self>) {
        let (Some(shot), Some(shown)) = (self.shot.clone(), self.shown.as_ref()) else {
            return;
        };
        shown
            .screen
            .update(cx, |screen, cx| screen.set_inset(shot.size, cx));
        shown.greeter.update(cx, |greeter, cx| {
            greeter.config = Rc::new(shot.config.clone());
            greeter.strings = i18n::strings(shot.config.language);
            if greeter.users.first() != Some(&shot.user) {
                greeter.users = vec![shot.user.clone()];
            }
            if shot.rest && !greeter.resting {
                greeter.rest(cx);
            } else if !shot.rest && greeter.resting {
                greeter.wake();
            }
            cx.notify();
        });
    }

    /// Draws the look's pictures in the background, after a pause when the
    /// box is being resized.
    fn prepare(&mut self, drawn: Drawn, pause: bool, cx: &mut Context<Self>) {
        let Some(shot) = self.shot.clone() else {
            return;
        };
        self.preparing += 1;
        let asked = self.preparing;
        let config = shot.config.clone();
        let pixels = drawn.pixels;
        let monitor = &self.monitor;
        let per_point =
            pixels.0 as f32 / monitor.rect.width.max(1) as f32 * monitor.pixels_per_point();
        cx.spawn(async move |this, cx| {
            if pause {
                cx.background_executor()
                    .timer(Duration::from_millis(250))
                    .await;
                let current = this.read_with(cx, |this, _| this.preparing == asked);
                if !matches!(current, Ok(true)) {
                    return;
                }
            }
            let prepared = cx
                .background_executor()
                .spawn(async move {
                    let folder = match &config.theme {
                        Theme::Folder(name) => {
                            Some(Arc::new(themes::load_configured(&config.themes.dir, name)?))
                        }
                        _ => None,
                    };
                    let screens = [(pixels, per_point)];
                    Some(
                        looks::start_sized(&config, folder.as_ref(), &screens)
                            .finish(None)
                            .0,
                    )
                })
                .await;
            let _ = this.update(cx, |this, cx| this.prepared(asked, drawn, prepared, cx));
        })
        .detach();
    }

    fn prepared(
        &mut self,
        asked: u64,
        drawn: Drawn,
        prepared: Option<Prepared>,
        cx: &mut Context<Self>,
    ) {
        if asked != self.preparing {
            return;
        }
        let Some(shot) = self.shot.clone() else {
            return;
        };
        let Some(prepared) = prepared else {
            self.failed = true;
            self.shown = None;
            cx.notify();
            return;
        };
        self.failed = false;
        if let Some(folder) = &prepared.folder {
            self.register(folder, cx);
        }
        let theme = shot.config.theme.clone();
        let monitors = [self.monitor.clone()];
        let looks = Looks::new(
            &shot.config,
            &monitors,
            theme.clone(),
            prepared,
            self.svgs.clone(),
            true,
        );
        match &mut self.shown {
            Some(shown) if shown.drawn.theme == theme => {
                shown.drawn = drawn;
                shown.greeter.update(cx, |greeter, cx| {
                    greeter.looks = looks;
                    cx.notify();
                });
            }
            _ => {
                let setup = Setup {
                    config: shot.config.clone(),
                    setting: theme,
                    looks,
                    users: vec![shot.user.clone()],
                    sessions: shot.session.clone().into_iter().collect(),
                    monitors: monitors.to_vec(),
                    form: 0,
                    x11: self.x11.clone(),
                    power: Power { dry_run: true },
                    preview: true,
                    hostname: None,
                    lock_screen: false,
                    supervisor: None,
                };
                let greeter = cx.new(|cx| Greeter::new(setup, cx));
                // Set, not woken or put to rest, so that nothing animates.
                greeter.update(cx, |greeter, _| {
                    greeter.resting = shot.rest && rests(greeter);
                });
                let monitor = self.monitor.clone();
                let screen = cx.new(|cx| Screen::inset(greeter.clone(), monitor, shot.size, cx));
                if let Some(backdrop) = self.wallpaper.made.clone() {
                    let request = self.wallpaper.requests;
                    screen.update(cx, |screen, cx| screen.set_backdrop(backdrop, request, cx));
                }
                self.shown = Some(Shown {
                    greeter,
                    screen,
                    drawn,
                });
                // A theme folder's wallpaper blur is known only now.
                self.show_wallpaper(cx);
            }
        }
        cx.notify();
    }

    /// Makes the wallpaper for the look and the box, in the background,
    /// unless the one asked for last is it.
    fn show_wallpaper(&mut self, cx: &mut Context<Self>) {
        let Some(shot) = self.shot.clone() else {
            return;
        };
        let folder = self
            .shown
            .as_ref()
            .filter(|shown| shown.drawn.theme == shot.config.theme)
            .and_then(|shown| shown.greeter.read(cx).folder.clone());
        let blur = wallpaper_blur(&shot.config.theme, folder.as_deref(), &shot.config);
        let (Some(blur), Some(path)) = (blur, shot.wallpaper.clone()) else {
            return;
        };
        let pixels = shot.pixels();
        let asked = (path.clone(), pixels, blur);
        if self.wallpaper.asked.as_ref() == Some(&asked) {
            return;
        }
        self.wallpaper.asked = Some(asked);
        self.wallpaper.requests += 1;
        let request = self.wallpaper.requests;
        let decoded = self
            .wallpaper
            .decoded
            .clone()
            .filter(|(decoded, _)| *decoded == path)
            .map(|(_, picture)| picture);
        cx.spawn(async move |this, cx| {
            let made = cx
                .background_executor()
                .spawn(async move {
                    let picture = match decoded {
                        Some(picture) => picture,
                        None => Arc::new(Picture::open(&path)?),
                    };
                    let frame = (pixels.0 as f32 / FRAME.0).min(pixels.1 as f32 / FRAME.1);
                    let backdrop =
                        Backdrop::new(&picture, pixels, blur.frame_px * frame, blur.brightness)?;
                    anyhow::Ok((path, picture, backdrop))
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                let (path, picture, backdrop) = match made {
                    Ok(made) => made,
                    Err(error) => {
                        log::warn!("cannot show the wallpaper: {error:#}");
                        return;
                    }
                };
                this.wallpaper.decoded = Some((path, picture));
                if request != this.wallpaper.requests {
                    return;
                }
                this.wallpaper.made = Some(backdrop.clone());
                if let Some(shown) = &this.shown {
                    shown
                        .screen
                        .update(cx, |screen, cx| screen.set_backdrop(backdrop, request, cx));
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Hands GPUI a theme folder's fonts, once, and the SVGs it tints.
    fn register(&mut self, folder: &Folder, cx: &mut App) {
        looks::add_svgs(&self.svgs, folder);
        if !self.fonts.insert(folder.name.clone()) {
            return;
        }
        let fonts: Vec<Cow<'static, [u8]>> = folder
            .fonts
            .iter()
            .map(|font| Cow::Owned(font.bytes.to_vec()))
            .collect();
        if !fonts.is_empty()
            && let Err(error) = cx.text_system().add_fonts(fonts)
        {
            log::error!("theme {}: cannot load its fonts: {error:#}", folder.name);
        }
    }

    /// Keeps the form from giving way to the clock while nobody types into
    /// it, as nobody does into a preview.
    fn keep_awake(cx: &mut Context<Self>) -> Task<()> {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(3)).await;
                let kept = this.update(cx, |this, cx| {
                    let resting = this.shot.as_ref().is_some_and(|shot| shot.rest);
                    if let Some(shown) = &this.shown
                        && !resting
                    {
                        shown
                            .greeter
                            .update(cx, |greeter, _| greeter.last_input = Instant::now());
                    }
                });
                if kept.is_err() {
                    break;
                }
            }
        })
    }
}

/// Whether the greeter's look has a rest to show, on the login screen.
fn rests(greeter: &Greeter) -> bool {
    match &greeter.folder {
        Some(folder) => folder.rest,
        None => *greeter.theme() == Theme::Frost,
    }
}

impl Miniature {
    /// Whether the look shown has a rest to show.
    pub fn shows_rest(&self, cx: &App) -> bool {
        self.shown
            .as_ref()
            .is_some_and(|shown| rests(shown.greeter.read(cx)))
    }
}

impl Render for Miniature {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let size = self.shot.as_ref().map(|shot| shot.size).unwrap_or_default();
        div()
            .w(size.width)
            .h(size.height)
            .relative()
            .overflow_hidden()
            .bg(gpui_kit::black())
            .when_some(self.shown.as_ref(), |preview, shown| {
                preview.child(shown.screen.clone())
            })
            .child(div().id("preview-glass").absolute().inset_0().occlude())
    }
}

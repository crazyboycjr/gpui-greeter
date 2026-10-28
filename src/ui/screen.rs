//! The window over one monitor: it draws the theme for that monitor and
//! passes keys and pointer movement to the shared greeter.

use std::collections::HashSet;
use std::time::{Duration, Instant};

use gpui_kit::prelude::*;
use gpui_kit::{Entity, FocusHandle, KeyDownEvent, MouseMoveEvent, Subscription, Window, div, px};

use super::model::Greeter;
use super::theme::folder::{self, MonitorPictures, Pace, Timeline};
use super::theme::{self, DESIGN_REM, design_unit};
use crate::wallpaper::Backdrop;
use crate::x11::Monitor;

pub struct Screen {
    greeter: Entity<Greeter>,
    output: usize,
    monitor: Monitor,
    backdrops: Backdrops,
    folder: FolderScreen,
    focus: FocusHandle,
    _observing: Subscription,
}

/// Frost's wallpaper on this monitor, and the one it replaces while it
/// fades in.
#[derive(Default)]
pub struct Backdrops {
    pub current: Option<Backdrop>,
    pub previous: Option<Backdrop>,
    /// Counts the wallpapers that arrived, so that each new one fades in.
    pub arrivals: usize,
    /// The request the current wallpaper answered.
    request: u64,
}

/// What a window keeps for a theme folder between frames.
struct FolderScreen {
    timeline: Timeline,
    hovered: HashSet<u64>,
    /// When a redraw is already asked for.
    wake: Option<Instant>,
    /// Since when nothing has been changing but what runs forever.
    settled_since: Option<Instant>,
    /// The greeter's count of theme switches this was kept for.
    theme_changes: u32,
}

impl FolderScreen {
    fn new() -> Self {
        Self {
            timeline: Timeline::new(Instant::now()),
            hovered: HashSet::new(),
            wake: None,
            settled_since: None,
            theme_changes: 0,
        }
    }
}

impl Screen {
    pub fn new(
        greeter: Entity<Greeter>,
        output: usize,
        monitor: Monitor,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        let observing = cx.observe(&greeter, |_, _, cx| cx.notify());
        Self {
            greeter,
            output,
            monitor,
            backdrops: Backdrops::default(),
            folder: FolderScreen::new(),
            focus,
            _observing: observing,
        }
    }

    /// Shows `backdrop`, unless one asked for later is already shown.
    pub fn set_backdrop(&mut self, backdrop: Backdrop, request: u64, cx: &mut Context<Self>) {
        if request < self.backdrops.request {
            return;
        }
        self.backdrops.request = request;
        self.backdrops.previous = self.backdrops.current.replace(backdrop);
        self.backdrops.arrivals += 1;
        let arrivals = self.backdrops.arrivals;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_secs(1)).await;
            let _ = this.update(cx, |this, _| {
                if this.backdrops.arrivals == arrivals {
                    this.backdrops.previous = None;
                }
            });
        })
        .detach();
        cx.notify();
    }

    pub fn has_backdrop(&self) -> bool {
        self.backdrops.current.is_some()
    }

    /// Since when a theme folder's window has shown nothing but what moves
    /// forever, for a preview's screenshot.
    pub fn settled_since(&self) -> Option<Instant> {
        self.folder.settled_since
    }

    /// The pointer entered or left a layer whose look follows it.
    pub fn hover(&mut self, key: u64, hovered: bool, cx: &mut Context<Self>) {
        let folder = &mut self.folder;
        let changed = if hovered {
            folder.hovered.insert(key)
        } else {
            folder.hovered.remove(&key)
        };
        if changed {
            cx.notify();
        }
    }

    fn key(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.greeter
            .update(cx, |greeter, cx| greeter.key(event, cx));
        cx.stop_propagation();
    }

    fn pointer(&mut self, _: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        let output = self.output;
        self.greeter
            .update(cx, |greeter, cx| greeter.pointer_on(output, cx));
    }

    /// Draws the window again at `at`, unless an earlier redraw is due.
    fn wake_at(&mut self, at: Instant, cx: &mut Context<Self>) {
        let folder = &mut self.folder;
        if folder.wake.is_some_and(|pending| pending <= at) {
            return;
        }
        folder.wake = Some(at);
        cx.spawn(async move |this, cx| {
            let wait = at.saturating_duration_since(Instant::now());
            cx.background_executor().timer(wait).await;
            let _ = this.update(cx, |this, cx| {
                if this.folder.wake == Some(at) {
                    this.folder.wake = None;
                }
                cx.notify();
            });
        })
        .detach();
    }
}

impl Render for Screen {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let viewport = window.viewport_size();
        let unit = design_unit(viewport.width.as_f32(), viewport.height.as_f32());
        window.set_rem_size(px(DESIGN_REM * unit));
        let now = Instant::now();
        let screen = cx.entity();
        let pace = Pace::default();
        let mut pending = false;
        let on_screen = self.greeter.read(cx).on_screen;
        let content = {
            let greeter = self.greeter.read(cx);
            let assets = greeter.looks.screen(greeter.theme(), self.output);
            let view = theme::View {
                greeter,
                handle: &self.greeter,
                has_form: greeter.form == self.output,
                monitor: &self.monitor,
                scenery: assets.scenery.as_ref(),
                backdrops: &self.backdrops,
                unit,
                scale_factor: window.scale_factor(),
                size: (
                    viewport.width.as_f32() / unit,
                    viewport.height.as_f32() / unit,
                ),
            };
            let pictures = greeter
                .looks
                .get(greeter.theme())
                .and_then(|prepared| prepared.pictures.clone());
            match (greeter.folder.as_deref(), pictures) {
                (Some(folder), Some(pictures)) => {
                    let state = &mut self.folder;
                    if state.theme_changes != greeter.theme_changes {
                        *state = FolderScreen::new();
                        state.theme_changes = greeter.theme_changes;
                    }
                    state.timeline.observe(greeter.state(), now);
                    pending = assets.drawn.is_none();
                    let empty = MonitorPictures::default();
                    let frame = folder::Frame {
                        folder,
                        pictures: &pictures,
                        drawn: assets.drawn.as_deref().unwrap_or(&empty),
                        timeline: &state.timeline,
                        hovered: &state.hovered,
                        screen,
                        pace: &pace,
                    };
                    folder::render(&view, &frame)
                }
                _ => theme::render(&view),
            }
        };
        {
            let state = &mut self.folder;
            if pace.settling.get() || pending {
                state.settled_since = None;
            } else if state.settled_since.is_none() {
                state.settled_since = Some(now);
            }
        }
        // Off the screen, nothing moves until the greeter's VT is back.
        if on_screen && pace.smooth.get() {
            window.request_animation_frame();
        } else if let Some(at) = pace.wake.get().filter(|_| on_screen) {
            self.wake_at(at, cx);
        }
        div()
            .id("screen")
            .size_full()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::key))
            .on_mouse_move(cx.listener(Self::pointer))
            .child(content)
    }
}

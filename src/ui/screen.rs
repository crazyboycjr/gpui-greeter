//! The window over one monitor: it draws the theme for that monitor and
//! passes keys and pointer movement to the shared greeter.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui_kit::prelude::*;
use gpui_kit::{Entity, FocusHandle, KeyDownEvent, MouseMoveEvent, Subscription, Window, div, px};

use super::model::Greeter;
use super::theme::folder::{self, MonitorPictures, Pace, Pictures, Timeline};
use super::theme::{self, DESIGN_REM, design_unit};
use crate::scenery::Scenery;
use crate::wallpaper::Backdrop;
use crate::x11::Monitor;

pub struct Screen {
    greeter: Entity<Greeter>,
    output: usize,
    monitor: Monitor,
    scenery: Option<Arc<Scenery>>,
    backdrops: Backdrops,
    folder: Option<FolderScreen>,
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
    pictures: Arc<Pictures>,
    drawn: Arc<MonitorPictures>,
    timeline: Timeline,
    hovered: HashSet<u64>,
    /// When a redraw is already asked for.
    wake: Option<Instant>,
    /// Since when nothing has been changing but what runs forever.
    settled_since: Option<Instant>,
}

impl Screen {
    pub fn new(
        greeter: Entity<Greeter>,
        output: usize,
        monitor: Monitor,
        scenery: Option<Arc<Scenery>>,
        pictures: Option<(Arc<Pictures>, Arc<MonitorPictures>)>,
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
            scenery,
            backdrops: Backdrops::default(),
            folder: pictures.map(|(pictures, drawn)| FolderScreen {
                pictures,
                drawn,
                timeline: Timeline::new(Instant::now()),
                hovered: HashSet::new(),
                wake: None,
                settled_since: None,
            }),
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
        self.folder.as_ref().and_then(|folder| folder.settled_since)
    }

    /// The pointer entered or left a layer whose look follows it.
    pub fn hover(&mut self, key: u64, hovered: bool, cx: &mut Context<Self>) {
        let Some(folder) = self.folder.as_mut() else {
            return;
        };
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
        let Some(folder) = self.folder.as_mut() else {
            return;
        };
        if folder.wake.is_some_and(|pending| pending <= at) {
            return;
        }
        folder.wake = Some(at);
        cx.spawn(async move |this, cx| {
            let wait = at.saturating_duration_since(Instant::now());
            cx.background_executor().timer(wait).await;
            let _ = this.update(cx, |this, cx| {
                if let Some(folder) = this.folder.as_mut()
                    && folder.wake == Some(at)
                {
                    folder.wake = None;
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
        let content = {
            let greeter = self.greeter.read(cx);
            let view = theme::View {
                greeter,
                handle: &self.greeter,
                has_form: greeter.form == self.output,
                monitor: &self.monitor,
                scenery: self.scenery.as_ref(),
                backdrops: &self.backdrops,
                unit,
                scale_factor: window.scale_factor(),
                size: (
                    viewport.width.as_f32() / unit,
                    viewport.height.as_f32() / unit,
                ),
            };
            match (self.folder.as_mut(), greeter.folder.as_deref()) {
                (Some(state), Some(folder)) => {
                    state.timeline.observe(greeter.state(), now);
                    let frame = folder::Frame {
                        folder,
                        pictures: &state.pictures,
                        drawn: &state.drawn,
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
        if let Some(state) = self.folder.as_mut() {
            if pace.settling.get() {
                state.settled_since = None;
            } else if state.settled_since.is_none() {
                state.settled_since = Some(now);
            }
        }
        if pace.smooth.get() {
            window.request_animation_frame();
        } else if let Some(at) = pace.wake.get() {
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

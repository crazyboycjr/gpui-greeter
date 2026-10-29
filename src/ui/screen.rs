//! The window over one monitor: it draws the theme for that monitor and
//! passes keys and pointer movement to the shared greeter. Inset in another
//! window, it draws the same, shrunk to a box, for the settings' preview.

use std::cell::RefCell;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, App, Bounds, ElementId, Entity, FocusHandle, GlobalElementId, InspectorElementId,
    KeyDownEvent, LayoutId, MouseDownEvent, MouseMoveEvent, Pixels, RenderImage, Size,
    Subscription, Window, div, px,
};

use super::model::Greeter;
use super::theme::folder::{self, MonitorPictures, Pace, Timeline};
use super::theme::{self, DESIGN_REM};
use crate::wallpaper::Backdrop;
use crate::x11::Monitor;

pub struct Screen {
    greeter: Entity<Greeter>,
    output: usize,
    monitor: Monitor,
    backdrops: Backdrops,
    folder: FolderScreen,
    /// The slides this window has drawn, which GPUI keeps as textures until
    /// they are dropped.
    slides: Vec<Arc<RenderImage>>,
    focus: FocusHandle,
    /// The box it is drawn in, inside another window, rather than over a
    /// window of its own.
    inset: Option<Size<Pixels>>,
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
            slides: Vec::new(),
            focus,
            inset: None,
            _observing: observing,
        }
    }

    /// `monitor`'s screen drawn in a box of `size` inside another window,
    /// which keeps the keyboard.
    pub fn inset(
        greeter: Entity<Greeter>,
        monitor: Monitor,
        size: Size<Pixels>,
        cx: &mut Context<Self>,
    ) -> Self {
        let observing = cx.observe(&greeter, |_, _, cx| cx.notify());
        Self {
            greeter,
            output: 0,
            monitor,
            backdrops: Backdrops::default(),
            folder: FolderScreen::new(),
            slides: Vec::new(),
            focus: cx.focus_handle(),
            inset: Some(size),
            _observing: observing,
        }
    }

    pub fn set_inset(&mut self, size: Size<Pixels>, cx: &mut Context<Self>) {
        if self.inset != Some(size) {
            self.inset = Some(size);
            cx.notify();
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

    fn pointer(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        let output = self.output;
        self.greeter.update(cx, |greeter, cx| {
            greeter.pointer_on(output, event.position, cx)
        });
    }

    /// A press at rest only wakes the screen; nothing under the pointer
    /// gets it.
    fn press(&mut self, _: &MouseDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.greeter.update(cx, |greeter, cx| greeter.press(cx)) {
            cx.stop_propagation();
        }
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
        let scale = self.greeter.read(cx).scale();
        let (viewport, unit) = match self.inset {
            // The monitor's points, shrunk to the box.
            Some(size) => (
                size,
                size.width.as_f32() / self.monitor.rect.width.max(1) as f32
                    * self.monitor.pixels_per_point()
                    * scale,
            ),
            None => {
                let unit = self.monitor.pixels_per_point() * scale / window.scale_factor();
                window.set_rem_size(px(DESIGN_REM * unit));
                (window.viewport_size(), unit)
            }
        };
        let now = Instant::now();
        let screen = cx.entity();
        let pace = Pace::default();
        let slides = RefCell::new(Vec::new());
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
                        slides: &slides,
                    };
                    folder::render(&view, &frame)
                }
                _ => theme::render(&view),
            }
        };
        {
            let greeter = self.greeter.read(cx);
            for image in slides.into_inner() {
                if !self.slides.contains(&image) {
                    self.slides.push(image);
                }
            }
            self.slides.retain(|image| {
                let kept = greeter.slides.holds(image);
                if !kept {
                    let _ = window.drop_image(image.clone());
                }
                kept
            });
        }
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
        if let Some(size) = self.inset {
            let inset = div()
                .w(size.width)
                .h(size.height)
                .relative()
                .overflow_hidden()
                .child(content);
            return WithRem {
                rem: px(DESIGN_REM * unit),
                child: inset.into_any_element(),
            }
            .into_any_element();
        }
        div()
            .id("screen")
            .size_full()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::key))
            .on_mouse_move(cx.listener(Self::pointer))
            .capture_any_mouse_down(cx.listener(Self::press))
            .child(content)
            .into_any_element()
    }
}

/// Lays out and paints its child with a rem size of its own, as though in
/// a window of its own: an inset screen's points are smaller than its
/// window's.
struct WithRem {
    rem: Pixels,
    child: AnyElement,
}

impl IntoElement for WithRem {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl Element for WithRem {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let child = &mut self.child;
        let id = window.with_rem_size(Some(self.rem), |window| child.request_layout(window, cx));
        (id, ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        let child = &mut self.child;
        window.with_rem_size(Some(self.rem), |window| child.prepaint(window, cx));
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        let child = &mut self.child;
        window.with_rem_size(Some(self.rem), |window| child.paint(window, cx));
    }
}

//! The window over one monitor: it draws the theme for that monitor and
//! passes keys and pointer movement to the shared greeter.

use std::sync::Arc;
use std::time::Duration;

use gpui_kit::prelude::*;
use gpui_kit::{Entity, FocusHandle, KeyDownEvent, MouseMoveEvent, Subscription, Window, div, px};

use super::model::Greeter;
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

impl Screen {
    pub fn new(
        greeter: Entity<Greeter>,
        output: usize,
        monitor: Monitor,
        scenery: Option<Arc<Scenery>>,
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
}

impl Render for Screen {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let viewport = window.viewport_size();
        let unit = design_unit(viewport.width.as_f32(), viewport.height.as_f32());
        window.set_rem_size(px(DESIGN_REM * unit));
        let greeter = self.greeter.read(cx);
        let view = theme::View {
            greeter,
            handle: &self.greeter,
            has_form: greeter.form == self.output,
            monitor: &self.monitor,
            scenery: self.scenery.as_ref(),
            backdrops: &self.backdrops,
            unit,
            size: (
                viewport.width.as_f32() / unit,
                viewport.height.as_f32() / unit,
            ),
        };
        div()
            .id("screen")
            .size_full()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::key))
            .on_mouse_move(cx.listener(Self::pointer))
            .child(theme::render(&view))
    }
}

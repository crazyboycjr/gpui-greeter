//! The three looks, each drawn from the shared model for one monitor's
//! window. Lengths are the mockups' CSS pixels, written with `u`, so that
//! each window's rem size carries its monitor's pixel density.

pub mod folder;
mod frost;
mod neon;
mod paper;

use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnimationExt as _, AnyElement, App, Context, Entity, Hsla, IntoElement, ObjectFit,
    ParentElement, Rems, Styled, StyledImage as _, Window, div, img, relative, rems, rgb,
};

use std::sync::Arc;

use super::model::Greeter;
use super::screen::Backdrops;
use super::text::label;
use crate::config::Theme;
use crate::scenery::Scenery;
use crate::x11::Monitor;

/// The rem size the mockups' pixels are measured against.
pub const DESIGN_REM: f32 = 16.0;

/// The mockups' frame, in CSS pixels. Each window lays it out at the
/// largest size that fits, so a theme keeps its proportions on any monitor,
/// whatever its resolution and density.
pub const DESIGN_FRAME: (f32, f32) = (1600.0, 900.0);

/// How many of a window's pixels make one of the mockups' CSS pixels,
/// for a window of `width` by `height`.
pub fn design_unit(width: f32, height: f32) -> f32 {
    (width / DESIGN_FRAME.0).min(height / DESIGN_FRAME.1)
}

/// A length in the mockups' CSS pixels.
pub fn u(css_px: f32) -> Rems {
    rems(css_px / DESIGN_REM)
}

pub fn color(hex: u32, alpha: f32) -> Hsla {
    let mut color: Hsla = rgb(hex).into();
    color.a = alpha;
    color
}

/// One window's view of the greeter.
pub struct View<'a> {
    pub greeter: &'a Greeter,
    pub handle: &'a Entity<Greeter>,
    /// Whether this monitor shows the login form, or the backdrop alone.
    pub has_form: bool,
    pub monitor: &'a Monitor,
    /// Paper's and Neon's drawn background, once drawn for this monitor.
    pub scenery: Option<&'a Arc<Scenery>>,
    pub backdrops: &'a Backdrops,
    /// Window pixels per CSS pixel.
    pub unit: f32,
    /// Device pixels per window pixel.
    pub scale_factor: f32,
    /// The window's size in CSS pixels.
    pub size: (f32, f32),
}

impl View<'_> {
    /// A handler that acts on the greeter, for clicks and other events.
    pub fn on<E: ?Sized>(
        &self,
        act: impl Fn(&mut Greeter, &mut Context<Greeter>) + 'static,
    ) -> impl Fn(&E, &mut Window, &mut App) + 'static {
        let handle = self.handle.clone();
        move |_, _, cx| handle.update(cx, |greeter, cx| act(greeter, cx))
    }

    pub fn px(&self, css_px: f32) -> gpui_kit::Pixels {
        gpui_kit::px(css_px * self.unit)
    }
}

/// The scenery's picture, filling the window under everything else.
fn picture(scenery: &Scenery) -> impl IntoElement {
    img(scenery.picture.clone())
        .absolute()
        .inset_0()
        .size_full()
        .object_fit(ObjectFit::Fill)
}

pub fn render(view: &View) -> AnyElement {
    match view.greeter.theme() {
        Theme::Paper => paper::render(view, false),
        Theme::PaperNight => paper::render(view, true),
        Theme::Frost => frost::render(view),
        Theme::Neon => neon::render(view),
        // A theme folder that failed to load gave way to Paper already.
        Theme::Folder(_) => paper::render(view, false),
    }
}

/// How a field draws what has been typed.
struct FieldLook {
    family: &'static str,
    weight: gpui_kit::FontWeight,
    size: f32,
    color: Hsla,
    /// CSS letter-spacing of the dots, in ems.
    tracking: f32,
    placeholder: Hsla,
    placeholder_tracking: f32,
    caret: Hsla,
    /// Whether the text sits in the middle of the field, rather than at its
    /// start.
    centered: bool,
}

/// Dots for a password, the text of a visible answer, or the placeholder
/// (PAM's own question once it asks something else), with a caret.
fn field_text(view: &View, look: &FieldLook, placeholder: &str) -> impl IntoElement {
    let greeter = view.greeter;
    let typed = match greeter.visible_answer() {
        Some(answer) => Some(answer.to_string()),
        None => (greeter.typed_len() > 0).then(|| "\u{2022}".repeat(greeter.typed_len())),
    };
    let placeholder = greeter
        .question
        .as_ref()
        .map(|question| question.prompt.trim().trim_end_matches(':').to_string())
        .unwrap_or_else(|| placeholder.to_string());
    // The caret restarts its blink with each key, so it shows while typing,
    // and goes once the session starts.
    let welcome = greeter.phase == super::model::Phase::Welcome;
    let caret = div()
        .w(u(1.5))
        .h(u(look.size * 1.15))
        .when(!welcome, |caret| caret.bg(look.caret))
        .flex_shrink_0()
        .with_animation(
            ("caret", greeter.typed_len()),
            super::motion::blink(1060),
            |caret, t| caret.opacity(super::motion::blink_opacity(t)),
        );
    let caret_after = typed.is_some();
    let (text, color, tracking) = match typed {
        Some(text) => (text, look.color, look.tracking),
        None => (placeholder, look.placeholder, look.placeholder_tracking),
    };
    let text = label(text)
        .font(look.family, look.weight)
        .size(look.size)
        .color(color)
        .tracking_em(tracking);
    let line = if look.centered {
        // The text alone is centred, as an input centres it; the caret
        // hangs beside it.
        let hanger = div()
            .absolute()
            .top_0()
            .bottom_0()
            .flex()
            .items_center()
            .child(caret);
        let hanger = if caret_after {
            hanger.left(relative(1.))
        } else {
            hanger.right(relative(1.))
        };
        div().relative().child(text).child(hanger)
    } else if caret_after {
        div().flex().items_center().child(text).child(caret)
    } else {
        div().flex().items_center().child(caret).child(text)
    };
    div()
        .flex()
        .items_center()
        .when(look.centered, |field| field.justify_center().px(u(2.)))
        .min_w_0()
        .overflow_hidden()
        .child(line)
}

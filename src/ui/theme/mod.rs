//! The three looks, each drawn from the shared model for one monitor's
//! window. Lengths are points, as a Mac lays out its screen, written with
//! `u`, so that each window's rem size carries its monitor's pixels per
//! point.

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

/// The rem size points are measured against.
pub const DESIGN_REM: f32 = 16.0;

/// A length in points.
pub fn u(points: f32) -> Rems {
    rems(points / DESIGN_REM)
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
    /// Whether the look first showed here before what this monitor draws
    /// for it was done, which then fades in over the plain ground.
    pub late: bool,
    pub backdrops: &'a Backdrops,
    /// Window pixels per point.
    pub unit: f32,
    /// Device pixels per window pixel.
    pub scale_factor: f32,
    /// The window's size in points.
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

    pub fn px(&self, points: f32) -> gpui_kit::Pixels {
        gpui_kit::px(points * self.unit)
    }
}

/// The scenery's picture, filling the window under everything else; one
/// drawn after its look first showed fades in.
fn picture(view: &View, scenery: &Scenery) -> AnyElement {
    let picture = img(scenery.picture.clone())
        .absolute()
        .inset_0()
        .size_full()
        .object_fit(ObjectFit::Fill);
    if !view.late {
        return picture.into_any_element();
    }
    div()
        .absolute()
        .inset_0()
        .child(picture)
        .with_animation(
            ("scenery", scenery.picture.id.0),
            super::motion::fade(),
            |layer, t| layer.opacity(t),
        )
        .into_any_element()
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
    /// What has been typed: its dots, or an answer that may show.
    color: Hsla,
    /// CSS letter-spacing of the placeholder and of an answer that shows,
    /// in ems.
    tracking: f32,
    placeholder: Hsla,
    caret: Hsla,
    /// Whether the text sits in the middle of the field, rather than at its
    /// start.
    centered: bool,
}

/// The caret's width, in points.
const CARET: f32 = 1.5;

/// Dots for a password, the text of a visible answer, or the placeholder
/// (PAM's own question once it asks something else), with a caret.
fn field_text(view: &View, look: &FieldLook, placeholder: &str) -> impl IntoElement {
    let greeter = view.greeter;
    let answer = greeter.visible_answer();
    let dots = answer.is_none() && greeter.typed_len() > 0;
    let placeholder = greeter
        .question
        .as_ref()
        .map(|question| question.prompt.trim().trim_end_matches(':').to_string())
        .unwrap_or_else(|| placeholder.to_string());
    // The caret restarts its blink with each key, so it shows while typing,
    // and goes once the session starts.
    let welcome = greeter.phase == super::model::Phase::Welcome;
    let caret = super::motion::blinking(
        ("caret", greeter.typed_len()),
        std::time::Duration::from_millis(1060),
        div()
            .w(u(CARET))
            .h(u(look.size * 1.15))
            .when(!welcome, |caret| caret.bg(look.caret))
            .flex_shrink_0(),
    );
    let caret_after = answer.is_some() || dots;
    let text = match answer {
        Some(answer) => label(answer.to_string())
            .typed(0..answer.len())
            .color(look.color)
            .tracking_em(look.tracking),
        None if dots => label("").dots(greeter.typed_len()).color(look.color),
        None => label(placeholder)
            .color(look.placeholder)
            .tracking_em(look.tracking),
    };
    let text = text.font(look.family, look.weight).size(look.size);
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
        // The dots start where the placeholder does, after the caret.
        div()
            .flex()
            .items_center()
            .when(dots, |line| line.pl(u(CARET)))
            .child(text)
            .child(caret)
    } else {
        div().flex().items_center().child(caret).child(text)
    };
    // Once what is typed outgrows the field, its end stays in sight, as a
    // text field scrolls: the spare room goes to spacers beside it, and
    // what overflows does so at the start.
    let spacer = || div().flex_grow_1();
    div()
        .flex()
        .items_center()
        .justify_end()
        .when(look.centered, |field| field.px(u(2.)).child(spacer()))
        .min_w_0()
        .overflow_hidden()
        .child(line.flex_shrink_0())
        .child(spacer())
}

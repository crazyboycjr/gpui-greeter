//! Frost: macOS's lock screen and login window. At rest, the wallpaper,
//! sharp, with a large clock and the date; a click or a key blurs and dims
//! it behind a round avatar, the name and a password field, with Sleep,
//! Restart and Shut Down at the bottom and the time in a corner. Nothing
//! is said until there is something to say: no hints, no buttons in the
//! field, the session chosen with Tab and the look with F9.

use gpui_kit::prelude::*;
use gpui_kit::{
    AnimationExt as _, AnyElement, BoxShadow, FontWeight, ObjectFit, div, img, linear_color_stop,
    linear_gradient, point, relative, svg,
};

use super::{FieldLook, View, color, field_text, u};
use crate::i18n;
use crate::power::Action;
use crate::ui::model::Status;
use crate::ui::motion::{Shake, fade, rise, veil};
use crate::ui::text::{Label, label};
use crate::wallpaper::Backdrop;

const GROUND: u32 = 0x140B10;
const WHITE: u32 = 0xFFFFFF;

pub fn render(view: &View) -> AnyElement {
    let greeter = view.greeter;
    let root = div()
        .size_full()
        .relative()
        .overflow_hidden()
        .bg(color(GROUND, 1.0))
        .child(backdrop(view, !greeter.resting, greeter.rest_changes));
    if !view.has_form {
        return root.into_any_element();
    }
    if greeter.resting {
        return root.child(clock_screen(view)).into_any_element();
    }
    root.child(login(view))
        .child(corner(view))
        .into_any_element()
}

/// The wallpaper and its blurred, dimmed form over it; between the two
/// the veil fades and the picture swells a little, as in the mockup. A
/// wallpaper that arrives fades in over the one before.
fn backdrop(view: &View, blurred: bool, changes: u32) -> impl IntoElement {
    let dim = view.greeter.config.frost.dim;
    let veiled = Veiled { blurred, changes };
    let shade = move |veil: f32| {
        let shade = dim.min(0.12) + (dim - dim.min(0.12)) * veil;
        div().absolute().inset_0().bg(color(0x140A0E, shade))
    };
    div()
        .absolute()
        .inset_0()
        .when_some(view.backdrops.previous.clone(), |layer, old| {
            layer.child(veiled.draw("veil-old", move |veil| pictures(old.clone(), veil)))
        })
        .when_some(view.backdrops.current.clone(), |layer, new| {
            layer.child(
                div()
                    .absolute()
                    .inset_0()
                    .child(veiled.draw("veil-new", move |veil| pictures(new.clone(), veil)))
                    .with_animation(("arrival", view.backdrops.arrivals), fade(), |layer, t| {
                        layer.opacity(t)
                    }),
            )
        })
        .child(veiled.draw("veil-shade", shade))
}

/// Draws a layer of the backdrop at the veil's current state, animating it
/// to its new state after each rest and waking.
#[derive(Clone, Copy)]
struct Veiled {
    blurred: bool,
    changes: u32,
}

impl Veiled {
    fn draw(self, id: &'static str, layer: impl Fn(f32) -> gpui_kit::Div + 'static) -> AnyElement {
        let Self { blurred, changes } = self;
        if changes == 0 {
            return layer(if blurred { 1.0 } else { 0.0 }).into_any_element();
        }
        div()
            .absolute()
            .inset_0()
            .with_animation((id, changes as usize), veil(), move |_, t| {
                layer(if blurred { t } else { 1.0 - t })
            })
            .into_any_element()
    }
}

/// The wallpaper `veil` of the way from sharp to blurred: it blurs through
/// a softer form on the way, and swells a little, as CSS's filter and
/// transform do in the mockup.
fn pictures(backdrop: Backdrop, veil: f32) -> gpui_kit::Div {
    let grow = 0.08 * veil;
    let picture = |image: std::sync::Arc<gpui_kit::RenderImage>, opacity: f32| {
        img(image)
            .absolute()
            .left(relative(-grow / 2.))
            .top(relative(-grow / 2.))
            .w(relative(1. + grow))
            .h(relative(1. + grow))
            .object_fit(ObjectFit::Cover)
            .opacity(opacity)
    };
    div()
        .absolute()
        .inset_0()
        .child(picture(backdrop.sharp, 1.0))
        .child(picture(backdrop.soft, (2.0 * veil).min(1.0)))
        .child(picture(backdrop.blurred, (2.0 * veil - 1.0).max(0.0)))
}

fn shadowed(text: Label, blur: f32, alpha: f32) -> Label {
    text.shadow(0., blur / 9., blur, color(0, alpha))
}

/// The keyboard layout and the time while signing in, quiet in the top
/// right corner, as far in from both edges.
fn corner(view: &View) -> impl IntoElement {
    let greeter = view.greeter;
    let layout = greeter
        .layouts
        .get(greeter.layout)
        .map(|layout| layout.to_uppercase())
        .unwrap_or_default();
    let words = |text: String, opacity: f32| {
        div().opacity(opacity).child(shadowed(
            label(text).font("Inter", FontWeight::SEMIBOLD).size(15.),
            6.,
            0.45,
        ))
    };
    div()
        .absolute()
        .top(u(20.))
        .right(u(20.))
        .flex()
        .items_center()
        .gap(u(12.))
        .when(!layout.is_empty(), |corner| {
            corner.child(words(layout, 0.6))
        })
        .child(words(i18n::clock(&greeter.now, false), 0.92))
}

/// At rest: the date and the clock, and nothing else.
fn clock_screen(view: &View) -> impl IntoElement {
    let greeter = view.greeter;
    // As macOS sets its lock screen on a 1440-point screen: the date's
    // capitals from 139 points down, the clock's figures from 207 to 329.
    let clock = div()
        .relative()
        .mt(u(129.))
        .flex()
        .flex_col()
        .items_center()
        .gap(u(2.))
        .child(div().opacity(0.92).child(shadowed(
            label(i18n::long_date(greeter.config.language, &greeter.now)).size(40.),
            18.,
            0.4,
        )))
        .child(shadowed(
            label(i18n::clock(&greeter.now, false))
                .font("Inter", FontWeight::EXTRA_LIGHT)
                .size(168.)
                .line_height(1.05)
                .tracking(-2.3),
            18.,
            0.4,
        ))
        .with_animation(
            ("clock-rise", greeter.rest_changes as usize),
            rise(550),
            |clock, t| clock.opacity(t).top(u(10. * (1. - t))),
        );
    div()
        .absolute()
        .inset_0()
        .flex()
        .flex_col()
        .items_center()
        .child(clock)
}

fn login(view: &View) -> impl IntoElement {
    let greeter = view.greeter;
    div()
        .absolute()
        .inset_0()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .child(form(view))
        .child(power_buttons(view))
        // The lock screen's session is the one running.
        .when(!greeter.lock_screen, |login| login.child(session(view)))
}

fn form(view: &View) -> impl IntoElement {
    let greeter = view.greeter;
    let s = greeter.strings;
    let user = greeter.user();
    // macOS's sizes: a 72-point avatar on a 1440-point screen, the name's
    // capitals 18.5 points under it, the field 18.4 under the name's
    // baseline and what it says 11 under the field.
    let avatar = div()
        .size(u(72.))
        .rounded_full()
        .overflow_hidden()
        .flex()
        .items_center()
        .justify_center()
        .bg(linear_gradient(
            145.,
            linear_color_stop(color(WHITE, 0.34), 0.),
            linear_color_stop(color(WHITE, 0.08), 1.),
        ))
        .border(u(1.))
        .border_color(color(WHITE, 0.35))
        .shadow(vec![BoxShadow {
            color: color(0, 0.25),
            offset: point(view.px(0.), view.px(6.)),
            blur_radius: view.px(24.),
            spread_radius: view.px(0.),
            inset: false,
        }])
        .child(match user.and_then(|user| user.avatar.clone()) {
            Some(path) => img(path)
                .size_full()
                .rounded_full()
                .object_fit(ObjectFit::Cover)
                .into_any_element(),
            None => label(user.map(|user| user.initial()).unwrap_or_default())
                .font("Inter", FontWeight::EXTRA_LIGHT)
                .size(34.)
                .ink()
                .into_any_element(),
        });
    let name = div()
        .id("user")
        .mt(u(15.))
        .when(greeter.users.len() > 1, |name| {
            name.cursor_pointer()
                .on_click(view.on(|greeter, cx| greeter.next_user(1, cx)))
        });
    let name = name.child(shadowed(
        label(
            user.map(|user| user.display_name.clone())
                .unwrap_or_default(),
        )
        .size(15.)
        .font("Inter", FontWeight::SEMIBOLD)
        .tracking(-0.23),
        8.,
        0.4,
    ));
    // macOS's placeholder starts 13 to 15 points in, after the caret,
    // in Medium; the dots follow it from the same place.
    let look = FieldLook {
        family: "Inter",
        weight: FontWeight::MEDIUM,
        size: 13.,
        color: color(WHITE, 1.0),
        tracking: 0.32,
        placeholder: color(WHITE, 0.6),
        placeholder_tracking: -0.08 / 13.,
        caret: color(WHITE, 0.9),
        centered: false,
    };
    let field = div()
        .relative()
        .mt(u(15.))
        .w(u(160.))
        .h(u(28.))
        .rounded(u(14.))
        .pl(u(13.))
        .pr(u(13.))
        .flex()
        .items_center()
        .bg(color(WHITE, 0.3))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .child(field_text(view, &look, s.enter_password)),
        )
        .with_animation(("shake", greeter.denials as usize), Shake::animation(), {
            let frames = Shake::frames();
            move |field, t| field.left(u(frames.sample(t)))
        });
    div()
        .relative()
        .mt(u(-24.))
        .flex()
        .flex_col()
        .items_center()
        .child(avatar)
        .child(name)
        .child(field)
        .child(div().mt(u(8.)).h(u(15.)).child(message(view)))
        .with_animation(
            ("form-rise", greeter.rest_changes as usize),
            rise(550),
            |form, t| form.opacity(t).top(u(10. * (1. - t))),
        )
}

/// What the line under the field says: nothing, as on macOS, but for a
/// refused password, PAM's word, or a power action that waits for its
/// second press.
fn message(view: &View) -> AnyElement {
    let greeter = view.greeter;
    let s = greeter.strings;
    let text = |text: &str| {
        let line = label(text.to_string())
            .font("Inter", FontWeight::SEMIBOLD)
            .size(12.)
            .color(color(WHITE, 1.0));
        shadowed(line, 8., 0.45).into_any_element()
    };
    match greeter.status() {
        Status::Notice(notice) => text(notice),
        Status::Denied(notice) => text(notice.unwrap_or(s.wrong_password)),
        Status::Failed(message) => text(message),
        Status::Confirm(action) => text(&s.confirm(greeter.action_name(action))),
        Status::Ready | Status::Checking | Status::Welcome => div().into_any_element(),
    }
}

/// Sleep, Restart and Shut Down as macOS draws its startup's buttons: line
/// icons 26 points across, their 13-point labels 13 under them, 103 points
/// apart centre to centre, the labels' baselines 64 points from the bottom.
fn power_buttons(view: &View) -> impl IntoElement {
    let greeter = view.greeter;
    let button = |action: Action, id: &'static str, icon: &'static str| {
        let armed = greeter.status() == Status::Confirm(action);
        let button = div()
            .id(id)
            .flex_shrink_0()
            .flex()
            .flex_col()
            .items_center()
            .gap(u(9.9))
            .cursor_pointer()
            .opacity(if armed { 1.0 } else { 0.85 })
            .hover(|button| button.opacity(1.0))
            .on_click(view.on(move |greeter, cx| greeter.power(action, cx)))
            .child(svg().path(icon).size(u(26.)).text_color(color(WHITE, 1.0)))
            .child(shadowed(
                label(greeter.action_name(action)).size(13.),
                8.,
                0.5,
            ));
        div().w(u(103.)).flex().justify_center().child(button)
    };
    div()
        .absolute()
        .left_0()
        .right_0()
        .bottom(u(64. - 13. * DESCENT))
        .flex()
        .justify_center()
        .child(button(Action::Suspend, "sleep", "icons/moon.svg"))
        .child(button(Action::Reboot, "restart", "icons/restart.svg"))
        .child(button(Action::PowerOff, "shutdown", "icons/power.svg"))
}

/// Inter's descent, in ems, which a label's box holds under its baseline.
const DESCENT: f32 = 0.2421875;

/// The session that starts, quiet in the bottom left corner; Tab and F2
/// choose another.
fn session(view: &View) -> impl IntoElement {
    let greeter = view.greeter;
    div()
        .absolute()
        .left(u(20.))
        .bottom(u(20.))
        .opacity(0.55)
        .child(shadowed(
            label(greeter.session_name().to_string()).size(12.),
            6.,
            0.4,
        ))
}

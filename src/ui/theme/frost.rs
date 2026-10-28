//! Frost: the wallpaper, sharp, with a large clock; a click or a key blurs
//! and dims it behind a round avatar, a frosted password pill, frosted
//! power buttons and a session chip at the bottom left.

use gpui_kit::prelude::*;
use gpui_kit::{
    AnimationExt as _, AnyElement, BoxShadow, FontWeight, Hsla, MouseButton, ObjectFit,
    Transformation, div, img, linear_color_stop, linear_gradient, point, radians, relative, svg,
};

use super::{FieldLook, View, color, field_text, u};
use crate::i18n;
use crate::power::Action;
use crate::ui::model::{Phase, Status};
use crate::ui::motion::{Nudge, Shake, fade, rise, spin, veil};
use crate::ui::text::{Label, label};
use crate::wallpaper::Backdrop;

const GROUND: u32 = 0x140B10;
const WHITE: u32 = 0xFFFFFF;

pub fn render(view: &View) -> AnyElement {
    let greeter = view.greeter;
    let blurred = !greeter.locked && greeter.phase != Phase::Welcome;
    let changes = greeter.lock_changes * 2 + u32::from(greeter.phase == Phase::Welcome);
    let root = div()
        .size_full()
        .relative()
        .overflow_hidden()
        .bg(color(GROUND, 1.0))
        .child(backdrop(view, blurred, changes));
    if !view.has_form {
        return root.into_any_element();
    }
    let content = if greeter.locked {
        clock_screen(view).into_any_element()
    } else {
        login(view).into_any_element()
    };
    root.child(content)
        .child(top_right(view))
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
/// to its new state after each locking and unlocking.
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

fn top_right(view: &View) -> impl IntoElement {
    let greeter = view.greeter;
    let layout = greeter
        .layouts
        .get(greeter.layout)
        .map(|layout| layout.to_uppercase())
        .unwrap_or_default();
    div()
        .absolute()
        .top(u(28.))
        .right(u(36.))
        .flex()
        .items_center()
        .gap(u(20.))
        .child(
            div()
                .opacity(0.85)
                .child(shadowed(label(layout).size(14.), 6., 0.45)),
        )
        .when(!greeter.locked, |row| {
            row.child(shadowed(
                label(i18n::clock(&greeter.now, false)).size(14.),
                6.,
                0.45,
            ))
        })
}

fn clock_screen(view: &View) -> impl IntoElement {
    let greeter = view.greeter;
    let s = greeter.strings;
    let clock = div()
        .relative()
        .mt(u(120.))
        .flex()
        .flex_col()
        .items_center()
        .gap(u(6.))
        .child(div().opacity(0.92).child(shadowed(
            label(i18n::long_date(greeter.config.language, &greeter.now)).size(22.),
            18.,
            0.4,
        )))
        .child(shadowed(
            label(i18n::clock(&greeter.now, false))
                .font("Inter", FontWeight::EXTRA_LIGHT)
                .size(148.)
                .line_height(1.05)
                .tracking(-2.),
            18.,
            0.4,
        ))
        .with_animation(
            ("clock-rise", greeter.lock_changes as usize),
            rise(550),
            |clock, t| clock.opacity(t).top(u(10. * (1. - t))),
        );
    let nudge = div()
        .absolute()
        .left_0()
        .right_0()
        .flex()
        .flex_col()
        .items_center()
        .gap(u(8.))
        .child(
            svg()
                .path("icons/chevron-up.svg")
                .size(u(22.))
                .text_color(color(WHITE, 1.0)),
        )
        .child(shadowed(label(s.click_to_log_in).size(15.), 8., 0.45))
        .with_animation("nudge", Nudge::animation(), |nudge, t| {
            let (lift, opacity) = Nudge::at(t);
            nudge.bottom(u(64. - lift)).opacity(opacity)
        });
    div()
        .id("clock-screen")
        .absolute()
        .inset_0()
        .flex()
        .flex_col()
        .items_center()
        .cursor_pointer()
        .on_mouse_down(
            MouseButton::Left,
            view.on(|greeter, cx| {
                greeter.unlock();
                cx.notify();
            }),
        )
        .child(clock)
        .child(nudge)
}

fn login(view: &View) -> impl IntoElement {
    let greeter = view.greeter;
    let middle = if greeter.phase == Phase::Welcome {
        welcome(view).into_any_element()
    } else {
        form(view).into_any_element()
    };
    div()
        .absolute()
        .inset_0()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .on_mouse_down(
            MouseButton::Left,
            view.on(|greeter, cx| greeter.close_menu(cx)),
        )
        .child(middle)
        .child(power_buttons(view))
        .child(session_chip(view))
}

fn frosted(element: gpui_kit::Div, alpha: f32, border: f32) -> gpui_kit::Div {
    element
        .bg(color(WHITE, alpha))
        .border(u(1.))
        .border_color(color(WHITE, border))
}

fn form(view: &View) -> impl IntoElement {
    let greeter = view.greeter;
    let s = greeter.strings;
    let user = greeter.user();
    let avatar = div()
        .size(u(128.))
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
            offset: point(view.px(0.), view.px(10.)),
            blur_radius: view.px(40.),
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
                .size(60.)
                .into_any_element(),
        });
    let name = div()
        .id("user")
        .mt(u(18.))
        .when(greeter.users.len() > 1, |name| {
            name.cursor_pointer()
                .on_click(view.on(|greeter, cx| greeter.next_user(1, cx)))
        });
    let name = name.child(shadowed(
        label(
            user.map(|user| user.display_name.clone())
                .unwrap_or_default(),
        )
        .size(21.)
        .font("Inter", FontWeight::MEDIUM)
        .tracking_em(0.02),
        10.,
        0.4,
    ));
    let round_button = |id: &'static str, icon: &'static str| {
        frosted(div(), 0.12, 0.28)
            .size(u(36.))
            .rounded_full()
            .flex()
            .items_center()
            .justify_center()
            .child(svg().path(icon).size(u(16.)).text_color(color(WHITE, 1.0)))
            .id(id)
            .cursor_pointer()
            .opacity(0.8)
            .hover(|button| button.opacity(1.0))
    };
    let back = round_button("back", "icons/chevron-left.svg")
        .on_click(view.on(|greeter, cx| greeter.lock(cx)));
    let look = FieldLook {
        family: "Inter",
        weight: FontWeight::LIGHT,
        size: 15.,
        color: color(WHITE, 1.0),
        tracking: 0.32,
        placeholder: color(WHITE, 0.58),
        placeholder_tracking: 0.06,
        caret: color(WHITE, 0.75),
        centered: true,
    };
    let checking = greeter.phase == Phase::Checking;
    let submit = div()
        .id("submit")
        .size(u(32.))
        .flex_shrink_0()
        .rounded_full()
        .bg(color(WHITE, 0.88))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .on_click(view.on(|greeter, cx| greeter.submit(cx)))
        .child(if checking {
            svg()
                .path("icons/spinner.svg")
                .size(u(16.))
                .text_color(color(0x3A1F24, 1.0))
                .with_animation("spin", spin(), |icon, t| {
                    icon.with_transformation(Transformation::rotate(radians(
                        t * std::f32::consts::TAU,
                    )))
                })
                .into_any_element()
        } else {
            svg()
                .path("icons/arrow-right.svg")
                .size(u(16.))
                .text_color(color(0x3A1F24, 1.0))
                .into_any_element()
        });
    // The field has the keyboard unless the session menu has taken it.
    let focused = greeter.menu.is_none();
    // The left padding balances the button on the right, so the text sits
    // in the middle of the pill.
    let pill = frosted(div(), 0.12, if focused { 0.46 } else { 0.22 })
        .relative()
        .w(u(300.))
        .h(u(44.))
        .rounded(u(22.))
        .pl(u(38.))
        .pr(u(6.))
        .flex()
        .items_center()
        .when(focused, |pill| {
            pill.shadow(vec![BoxShadow {
                color: color(WHITE, 0.18),
                offset: point(view.px(0.), view.px(1.)),
                blur_radius: view.px(0.),
                spread_radius: view.px(0.),
                inset: true,
            }])
        })
        .child(
            div()
                .flex_1()
                .min_w_0()
                .child(field_text(view, &look, s.password)),
        )
        .child(submit)
        .with_animation(("shake", greeter.denials as usize), Shake::animation(), {
            let frames = Shake::frames();
            move |pill, t| pill.left(u(frames.sample(t)))
        });
    let row = div()
        .mt(u(26.))
        .flex()
        .items_center()
        .gap(u(12.))
        .child(back)
        .child(pill)
        .child(div().w(u(36.)));
    div()
        .relative()
        .mt(u(-40.))
        .flex()
        .flex_col()
        .items_center()
        .child(avatar)
        .child(name)
        .child(row)
        .child(div().mt(u(16.)).h(u(20.)).child(status(view)))
        .with_animation(
            ("form-rise", greeter.lock_changes as usize),
            rise(550),
            |form, t| form.opacity(t).top(u(10. * (1. - t))),
        )
}

fn status(view: &View) -> AnyElement {
    let greeter = view.greeter;
    let s = greeter.strings;
    let text = |text: &str, color: Hsla, opacity: f32| {
        let line = label(text.to_string())
            .size(12.5)
            .font("Inter", FontWeight::LIGHT)
            .tracking_em(0.04)
            .color(color);
        div()
            .opacity(opacity)
            .child(shadowed(line, 8., 0.45))
            .into_any_element()
    };
    let (white, error) = (color(WHITE, 1.0), color(0xFFC4B8, 1.0));
    match greeter.status() {
        Status::Hint => text(s.press_enter, white, 0.7),
        Status::Notice(notice) => text(notice, white, 0.8),
        Status::Denied(notice) => text(notice.unwrap_or(s.wrong_password), error, 1.0),
        Status::Failed(message) => text(message, error, 1.0),
        Status::Confirm(action) => text(&s.confirm(greeter.action_name(action)), white, 0.9),
        Status::Checking | Status::Welcome => div().into_any_element(),
    }
}

fn welcome(view: &View) -> impl IntoElement {
    let greeter = view.greeter;
    div()
        .relative()
        .flex()
        .flex_col()
        .items_center()
        .gap(u(12.))
        .child(shadowed(
            label(greeter.strings.welcome_back)
                .size(40.)
                .font("Inter", FontWeight::LIGHT),
            16.,
            0.45,
        ))
        .child(
            div().opacity(0.8).child(shadowed(
                label(format!(
                    "{}{}",
                    greeter.strings.starting,
                    greeter.session_name()
                ))
                .size(15.),
                16.,
                0.45,
            )),
        )
        .with_animation("welcome", rise(550), |welcome, t| {
            welcome.opacity(t).top(u(10. * (1. - t)))
        })
}

fn power_buttons(view: &View) -> impl IntoElement {
    let greeter = view.greeter;
    let button = |action: Action, id: &'static str, icon: &'static str| {
        let armed = greeter.status() == Status::Confirm(action);
        div()
            .id(id)
            .flex()
            .flex_col()
            .items_center()
            .gap(u(10.))
            .cursor_pointer()
            .opacity(if armed { 1.0 } else { 0.8 })
            .hover(|button| button.opacity(1.0))
            .on_click(view.on(move |greeter, cx| greeter.power(action, cx)))
            .child(
                frosted(div(), if armed { 0.3 } else { 0.12 }, 0.3)
                    .size(u(44.))
                    .rounded_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(svg().path(icon).size(u(18.)).text_color(color(WHITE, 1.0))),
            )
            .child(shadowed(
                label(greeter.action_name(action)).size(13.),
                8.,
                0.5,
            ))
    };
    div()
        .absolute()
        .left_0()
        .right_0()
        .bottom(u(44.))
        .flex()
        .justify_center()
        .gap(u(56.))
        .child(button(Action::Suspend, "sleep", "icons/moon.svg"))
        .child(button(Action::Reboot, "restart", "icons/restart.svg"))
        .child(button(Action::PowerOff, "shutdown", "icons/power.svg"))
}

fn session_chip(view: &View) -> impl IntoElement {
    let greeter = view.greeter;
    let chip = frosted(div(), 0.12, 0.28)
        .h(u(36.))
        .px(u(14.))
        .rounded(u(18.))
        .flex()
        .items_center()
        .gap(u(8.))
        .child(
            svg()
                .path("icons/monitor.svg")
                .size(u(15.))
                .text_color(color(WHITE, 1.0)),
        )
        .child(label(greeter.session_name().to_string()).size(14.))
        .id("session")
        .cursor_pointer()
        .opacity(if greeter.menu.is_some() { 1.0 } else { 0.8 })
        .hover(|chip| chip.opacity(1.0))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(view.on(|greeter, cx| greeter.toggle_menu(cx)));
    let theme = frosted(div(), 0.12, 0.28)
        .h(u(36.))
        .px(u(14.))
        .rounded(u(18.))
        .flex()
        .items_center()
        .gap(u(8.))
        .child(
            svg()
                .path("icons/theme.svg")
                .size(u(15.))
                .text_color(color(WHITE, 1.0)),
        )
        .child(label(greeter.theme_title()).size(14.))
        .id("theme")
        .cursor_pointer()
        .opacity(0.8)
        .hover(|chip| chip.opacity(1.0))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(view.on(|greeter, cx| greeter.switch_theme(1, cx)));
    div()
        .absolute()
        .left(u(36.))
        .bottom(u(44.))
        .flex()
        .gap(u(10.))
        .when_some(greeter.menu, |anchor, highlighted| {
            anchor.child(menu(view, highlighted))
        })
        .child(chip)
        .when(greeter.looks.cycle.len() > 1, |row| row.child(theme))
}

fn menu(view: &View, highlighted: usize) -> impl IntoElement {
    let greeter = view.greeter;
    let hover = color(WHITE, 0.14);
    div()
        .absolute()
        .left_0()
        .bottom(u(52.))
        .min_w(u(190.))
        .p(u(6.))
        .rounded(u(14.))
        .border(u(1.))
        .border_color(color(WHITE, 0.25))
        .bg(color(0x28161C, 0.72))
        .flex()
        .flex_col()
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .children(greeter.sessions.iter().enumerate().map(|(index, session)| {
            let current = index == greeter.session;
            div()
                .id(("session-option", index))
                .flex()
                .justify_between()
                .gap(u(20.))
                .px(u(12.))
                .py(u(10.))
                .rounded(u(9.))
                .cursor_pointer()
                .when(index == highlighted, |option| option.bg(hover))
                .hover(|option| option.bg(hover))
                .on_click(view.on(move |greeter, cx| greeter.pick_session(index, cx)))
                .child(label(session.name.clone()).size(14.))
                .child(
                    div()
                        .opacity(0.7)
                        .child(label(if current { greeter.strings.current } else { "" }).size(12.)),
                )
        }))
}

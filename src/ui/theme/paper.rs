//! Paper: a large serif clock on warm paper, the name and an underlined
//! password field beneath it, the session at the bottom left and the power
//! actions at the bottom right. Night is the same on a dark ground. The
//! paper is a sheet on a desk, ledger paper, a drafting sheet, whose title
//! block takes the session and the power actions, or plain.

use std::time::Duration;

use gpui_kit::prelude::*;
use gpui_kit::{
    AnimationExt as _, AnyElement, FontWeight, Hsla, MouseButton, div, img, relative, svg,
};

use super::{FieldLook, View, color, field_text, picture, u};
use crate::config::PaperBackground;
use crate::i18n;
use crate::power::Action;
use crate::ui::model::Status;
use crate::ui::motion::{Pulse, Shake, rise};
use crate::ui::text::label;

pub struct Palette {
    ground: u32,
    ink: u32,
    line_alpha: f32,
    menu: u32,
    underline_alpha: f32,
    error: u32,
    error_line: u32,
    /// The drafting sheet's frame lines, and its thinner ones.
    frame: (u32, f32),
    frame_thin: (u32, f32),
}

const DAY: Palette = Palette {
    ground: 0xF3EEE4,
    ink: 0x2A2622,
    line_alpha: 0.16,
    menu: 0xFBF8F2,
    underline_alpha: 0.3,
    error: 0xA8452A,
    error_line: 0xB0472A,
    frame: (0x503E2D, 0.55),
    frame_thin: (0x503E2D, 0.25),
};

const NIGHT: Palette = Palette {
    ground: 0x1D1B18,
    ink: 0xECE5D8,
    line_alpha: 0.18,
    menu: 0x26231F,
    underline_alpha: 0.35,
    error: 0xE08A6A,
    error_line: 0xE08A6A,
    frame: (0xECE5D8, 0.5),
    frame_thin: (0xECE5D8, 0.22),
};

/// The desk's sheet of paper, cream by night too.
const SHEET: u32 = 0xF7F2E7;

const SERIF_CLOCK: &str = "Fraunces 144pt";
const SERIF_HEADING: &str = "Fraunces 44pt";
const SERIF_TEXT: &str = "Fraunces 17pt";

pub fn render(view: &View, night: bool) -> AnyElement {
    let background = view.greeter.config.paper.background;
    let desk = background == PaperBackground::Desk;
    let drafting = background == PaperBackground::Drafting;
    // What is written on the desk's sheet keeps the day's ink.
    let palette = if night && !desk { &NIGHT } else { &DAY };
    let ink = color(palette.ink, 1.0);
    let root = div()
        .size_full()
        .relative()
        .overflow_hidden()
        .bg(color(if desk { SHEET } else { palette.ground }, 1.0))
        .when_some(view.scenery, |root, scenery| root.child(picture(scenery)))
        .when(drafting, |root| root.child(zones(view, palette)));
    if !view.has_form {
        return root.into_any_element();
    }
    let greeter = view.greeter;
    root.on_mouse_down(
        MouseButton::Left,
        view.on(|greeter, cx| greeter.close_menu(cx)),
    )
    .child(
        div()
            .absolute()
            .left(u(160.))
            .top(u(72.))
            .opacity(0.5)
            .child(
                label(greeter.machine.hostname.clone())
                    .size(14.)
                    .color(ink)
                    .tracking_em(0.06),
            ),
    )
    .child(main_column(view, palette))
    .child(if drafting {
        title_block(view, palette).into_any_element()
    } else {
        bottom_bar(view, palette, if desk { 660. } else { 160. }).into_any_element()
    })
    .into_any_element()
}

/// The drafting sheet's zones: numbers along the top and bottom of its
/// frame, letters down its sides.
fn zones(view: &View, palette: &Palette) -> impl IntoElement {
    let (width, height) = view.size;
    let ink = color(palette.ink, 1.0);
    let mark = move |text: String| {
        div()
            .absolute()
            .flex()
            .items_center()
            .justify_center()
            .opacity(0.5)
            .child(
                label(text)
                    .font(SERIF_TEXT, FontWeight::NORMAL)
                    .size(8.)
                    .color(ink),
            )
    };
    let across = (width - 80.) / 10.;
    let down = (height - 80.) / 5.;
    let columns = (0..10).flat_map(move |column| {
        let left = u(40. + column as f32 * across);
        let number = (column + 1).to_string();
        [
            mark(number.clone()).left(left).top(u(32.)),
            mark(number).left(left).bottom(u(32.)),
        ]
        .map(|mark| mark.w(u(160.)).h(u(8.)))
    });
    let rows = ["A", "B", "C", "D", "E"]
        .into_iter()
        .enumerate()
        .flat_map(move |(row, letter)| {
            let top = u(40. + row as f32 * down);
            [
                mark(letter.into()).top(top).left(u(32.)),
                mark(letter.into()).top(top).right(u(32.)),
            ]
            .map(|mark| mark.w(u(8.)).h(u(down)))
        });
    div().absolute().inset_0().children(columns).children(rows)
}

fn main_column(view: &View, palette: &Palette) -> impl IntoElement {
    let greeter = view.greeter;
    let ink = color(palette.ink, 1.0);
    let clock = label(i18n::clock(&greeter.now, false))
        .font(SERIF_CLOCK, FontWeight::LIGHT)
        .size(184.)
        .line_height(0.9)
        .tracking(-5.)
        .color(ink);
    let date = div().mt(u(26.)).opacity(0.58).child(
        label(i18n::long_date(greeter.config.language, &greeter.now))
            .size(20.)
            .color(ink),
    );
    let below = if greeter.phase == crate::ui::model::Phase::Welcome {
        welcome(view, palette).into_any_element()
    } else {
        form(view, palette).into_any_element()
    };
    // The column sits a little above the middle, as the mockup's
    // translateY(-54%) puts it.
    div()
        .absolute()
        .left(u(160.))
        .top_0()
        .bottom_0()
        .pb(u(36.))
        .flex()
        .flex_col()
        .justify_center()
        .child(clock)
        .child(date)
        .child(below)
}

fn form(view: &View, palette: &Palette) -> impl IntoElement {
    let greeter = view.greeter;
    let ink = color(palette.ink, 1.0);
    let accent: Hsla = gpui_kit::rgb(greeter.config.paper.accent.0).into();
    let failed = matches!(greeter.status(), Status::Denied(_) | Status::Failed(_));
    let underline = if failed {
        color(palette.error_line, 1.0)
    } else {
        color(palette.ink, palette.underline_alpha)
    };
    let user = greeter.user();
    let name = user
        .map(|user| user.display_name.clone())
        .unwrap_or_default();
    let avatar = div()
        .size(u(34.))
        .flex_shrink_0()
        .rounded_full()
        .border(u(1.))
        .border_color(color(palette.ink, palette.line_alpha))
        .overflow_hidden()
        .flex()
        .items_center()
        .justify_center()
        .child(match user.and_then(|user| user.avatar.clone()) {
            Some(path) => img(path).size_full().rounded_full().into_any_element(),
            None => label(user.map(|user| user.initial()).unwrap_or_default())
                .font(SERIF_TEXT, FontWeight::NORMAL)
                .size(17.)
                .color(ink)
                .into_any_element(),
        });
    let who = div()
        .id("user")
        .flex()
        .items_center()
        .gap(u(12.))
        .when(greeter.users.len() > 1, |row| {
            row.cursor_pointer()
                .on_click(view.on(|greeter, cx| greeter.next_user(1, cx)))
        })
        .child(avatar)
        .child(
            label(name)
                .size(17.)
                .font("Inter", FontWeight::MEDIUM)
                .color(ink),
        );
    let look = FieldLook {
        family: "Inter",
        weight: FontWeight::NORMAL,
        size: 24.,
        color: ink,
        tracking: 0.3,
        placeholder: color(palette.ink, 0.35),
        placeholder_tracking: 0.0,
        caret: accent,
        centered: false,
    };
    let field = div()
        .relative()
        .w(u(440.))
        .pt(u(10.))
        .pb(u(12.))
        .border_b(u(1.5))
        .border_color(underline)
        .child(field_text(view, &look, greeter.strings.password))
        .with_animation(("shake", greeter.denials as usize), Shake::animation(), {
            let frames = Shake::frames();
            move |field, t| field.left(u(frames.sample(t)))
        });
    div()
        .mt(u(92.))
        .flex()
        .flex_col()
        .gap(u(18.))
        .child(who)
        .child(field)
        .child(status_line(view, palette, accent))
}

fn status_line(view: &View, palette: &Palette, accent: Hsla) -> impl IntoElement {
    let greeter = view.greeter;
    let s = greeter.strings;
    let ink = |alpha| color(palette.ink, alpha);
    let error = color(palette.error, 1.0);
    let line = div().h(u(20.)).flex().items_center().gap(u(8.));
    let text = |text: &str, color: Hsla| label(text.to_string()).size(14.).color(color);
    match greeter.status() {
        Status::Hint => line.child(text(s.press_enter, ink(0.5))),
        Status::Notice(notice) => line.child(text(notice, ink(0.6))),
        Status::Checking => line.child(text(s.checking, ink(0.6))).child(dots(ink(0.6))),
        Status::Denied(notice) => line.child(text(notice.unwrap_or(s.wrong_password), error)),
        Status::Failed(message) => line.child(text(message, error)),
        Status::Confirm(action) => {
            line.child(text(&s.confirm(greeter.action_name(action)), accent))
        }
        Status::Welcome => line,
    }
}

fn dots(color: Hsla) -> impl IntoElement {
    div().flex().children((0..3usize).map(move |index| {
        let delay = Duration::from_millis(200 * index as u64);
        div()
            .child(label("\u{00B7}").size(14.).color(color))
            .with_animation(("dot", index), Pulse::animation(), move |dot, t| {
                dot.opacity(Pulse::opacity(t, delay))
            })
    }))
}

fn welcome(view: &View, palette: &Palette) -> impl IntoElement {
    let greeter = view.greeter;
    let ink = color(palette.ink, 1.0);
    let name = greeter
        .user()
        .map(|user| user.display_name.as_str())
        .unwrap_or_default();
    div()
        .relative()
        .mt(u(92.))
        .flex()
        .flex_col()
        .gap(u(10.))
        .child(
            label(greeter.strings.welcome(name))
                .font(SERIF_HEADING, FontWeight::NORMAL)
                .size(44.)
                .color(ink),
        )
        .child(
            div().opacity(0.55).child(
                label(format!(
                    "{}{}",
                    greeter.strings.starting,
                    greeter.session_name()
                ))
                .size(15.)
                .color(ink),
            ),
        )
        .with_animation("welcome", rise(500), |welcome, t| {
            welcome.opacity(t).top(u(8. * (1. - t)))
        })
}

fn bottom_bar(view: &View, palette: &Palette, right: f32) -> impl IntoElement {
    let greeter = view.greeter;
    let s = greeter.strings;
    let ink = color(palette.ink, 1.0);
    let accent: Hsla = gpui_kit::rgb(greeter.config.paper.accent.0).into();
    let session_button = div()
        .id("session")
        .flex()
        .items_center()
        .gap(u(8.))
        .py(u(12.))
        .cursor_pointer()
        .opacity(if greeter.menu.is_some() { 1.0 } else { 0.62 })
        .hover(|button| button.opacity(1.0))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(view.on(|greeter, cx| greeter.toggle_menu(cx)))
        .child(label(s.session).size(15.).color(ink))
        .child(
            label(greeter.session_name().to_string())
                .size(15.)
                .font("Inter", FontWeight::MEDIUM)
                .color(ink),
        )
        .child(
            svg()
                .path("icons/caret-up.svg")
                .size(u(12.))
                .text_color(ink),
        );
    let power = |action: Action, id: &'static str| {
        let armed = greeter.status() == Status::Confirm(action);
        div()
            .id(id)
            .py(u(12.))
            .cursor_pointer()
            .opacity(if armed { 1.0 } else { 0.62 })
            .hover(|button| button.opacity(1.0))
            .on_click(view.on(move |greeter, cx| greeter.power(action, cx)))
            .child(
                label(greeter.action_name(action))
                    .size(15.)
                    .color(if armed { accent } else { ink }),
            )
    };
    div()
        .absolute()
        .left(u(160.))
        .right(u(right))
        .bottom(u(56.))
        .flex()
        .justify_between()
        .items_end()
        .child(
            div()
                .relative()
                .when_some(greeter.menu, |anchor, highlighted| {
                    anchor.child(
                        menu(view, &MenuLook::bar(palette), highlighted, accent)
                            .left(u(-12.))
                            .bottom(u(52.)),
                    )
                })
                .child(session_button),
        )
        .child(
            div()
                .flex()
                .gap(u(36.))
                .child(power(Action::Suspend, "sleep"))
                .child(power(Action::Reboot, "restart"))
                .child(power(Action::PowerOff, "shutdown")),
        )
}

/// How the session menu looks: rounded over the bar, square on the
/// drafting sheet.
struct MenuLook {
    background: Hsla,
    border: Hsla,
    radius: f32,
    min_width: f32,
    option_padding: (f32, f32),
    option_radius: f32,
    gap: f32,
    name_size: f32,
    mark_size: f32,
    ink: Hsla,
}

impl MenuLook {
    fn bar(palette: &Palette) -> Self {
        Self {
            background: color(palette.menu, 1.0),
            border: color(palette.ink, palette.line_alpha),
            radius: 10.,
            min_width: 200.,
            option_padding: (12., 10.),
            option_radius: 6.,
            gap: 24.,
            name_size: 15.,
            mark_size: 13.,
            ink: color(palette.ink, 1.0),
        }
    }

    fn sheet(palette: &Palette) -> Self {
        Self {
            border: color(palette.frame.0, palette.frame.1),
            radius: 0.,
            option_padding: (10., 9.),
            option_radius: 0.,
            gap: 20.,
            name_size: 14.,
            mark_size: 12.,
            ..Self::bar(palette)
        }
    }
}

fn menu(view: &View, look: &MenuLook, highlighted: usize, accent: Hsla) -> gpui_kit::Div {
    let greeter = view.greeter;
    let hover = color(0x7F6E5A, 0.12);
    let ink = look.ink;
    div()
        .absolute()
        .min_w(u(look.min_width))
        .p(u(6.))
        .rounded(u(look.radius))
        .bg(look.background)
        .border(u(1.))
        .border_color(look.border)
        .flex()
        .flex_col()
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .children(greeter.sessions.iter().enumerate().map(|(index, session)| {
            let current = index == greeter.session;
            div()
                .id(("session-option", index))
                .flex()
                .justify_between()
                .items_center()
                .gap(u(look.gap))
                .px(u(look.option_padding.0))
                .py(u(look.option_padding.1))
                .rounded(u(look.option_radius))
                .cursor_pointer()
                .when(index == highlighted, |option| option.bg(hover))
                .hover(|option| option.bg(hover))
                .on_click(view.on(move |greeter, cx| greeter.pick_session(index, cx)))
                .child(label(session.name.clone()).size(look.name_size).color(ink))
                .child(
                    label(if current { greeter.strings.current } else { "" })
                        .size(look.mark_size)
                        .color(accent),
                )
        }))
}

/// The drafting sheet's title block, in its bottom right corner: the
/// project and the host above, the session and the power actions below,
/// each under its caption.
fn title_block(view: &View, palette: &Palette) -> impl IntoElement {
    const COLUMN: f32 = 140.;
    let greeter = view.greeter;
    let s = greeter.strings;
    let ink = color(palette.ink, 1.0);
    let accent: Hsla = gpui_kit::rgb(greeter.config.paper.accent.0).into();
    let thin = color(palette.frame_thin.0, palette.frame_thin.1);
    let hover = color(0x786046, 0.08);
    let cell = |columns: f32, caption: String| {
        div()
            .w(u(COLUMN * columns))
            .px(u(14.))
            .py(u(10.))
            .flex()
            .flex_col()
            .gap(u(4.))
            .border_color(thin)
            .child(
                div()
                    .opacity(0.5)
                    .child(label(caption).size(10.).tracking_em(0.08).color(ink)),
            )
    };
    let entry = |caption: &str, value: String| {
        cell(2., caption.to_string()).child(
            label(value)
                .font(SERIF_TEXT, FontWeight::NORMAL)
                .size(20.)
                .color(ink),
        )
    };
    let session = cell(1., s.session.to_string())
        .id("session")
        .border_r(u(1.))
        .cursor_pointer()
        .hover(|cell| cell.bg(hover))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(view.on(|greeter, cx| greeter.toggle_menu(cx)))
        .child(
            label(greeter.session_name().to_string())
                .size(15.)
                .font("Inter", FontWeight::MEDIUM)
                .color(ink),
        );
    let keys = greeter.power_keys();
    let actions = keys.iter().enumerate().map(|(index, &(key, action))| {
        let armed = greeter.status() == Status::Confirm(action);
        cell(1., key.to_uppercase())
            .id(key)
            .when(index + 1 < keys.len(), |cell| cell.border_r(u(1.)))
            .cursor_pointer()
            .hover(|cell| cell.bg(hover))
            .on_click(view.on(move |greeter, cx| greeter.power(action, cx)))
            .child(
                label(greeter.action_name(action))
                    .size(15.)
                    .color(if armed { accent } else { ink }),
            )
    });
    div()
        .absolute()
        .right(u(40.))
        .bottom(u(40.))
        .w(u(COLUMN * 4. + 1.))
        .border_l(u(1.))
        .border_t(u(1.))
        .border_color(color(palette.frame.0, palette.frame.1))
        .bg(color(palette.ground, 1.0))
        .flex()
        .flex_col()
        .child(
            div()
                .flex()
                .child(
                    entry(s.project, "gpui-greeter".into())
                        .border_r(u(1.))
                        .border_b(u(1.)),
                )
                .child(entry(s.host, greeter.machine.hostname.clone()).border_b(u(1.))),
        )
        .child(
            div()
                .flex()
                .child(
                    div()
                        .relative()
                        .flex()
                        .when_some(greeter.menu, |anchor, highlighted| {
                            anchor.child(
                                menu(view, &MenuLook::sheet(palette), highlighted, accent)
                                    .left(u(-1.))
                                    .bottom(relative(1.)),
                            )
                        })
                        .child(session),
                )
                .children(actions),
        )
}

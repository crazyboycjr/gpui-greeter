//! Paper: a large serif clock on warm paper and beneath it the avatar,
//! beside the name, an underlined password field and the status; the
//! session at the bottom left and the power actions at the bottom right. Night is the same on a dark ground. The
//! paper is a sheet on a desk, ledger paper, a drafting sheet, whose title
//! block takes the session and the power actions, or plain.

use std::time::Duration;

use gpui_kit::prelude::*;
use gpui_kit::{
    AnimationExt as _, AnyElement, FontWeight, Hsla, MouseButton, deferred, div, img, relative, svg,
};

use super::{FieldLook, View, color, field_text, picture, u};
use crate::config::PaperBackground;
use crate::i18n;
use crate::power::Action;
use crate::ui::model::Status;
use crate::ui::motion::{Pulse, Shake};
use crate::ui::text::label;

pub struct Palette {
    ground: u32,
    ink: u32,
    line_alpha: f32,
    menu: u32,
    error: u32,
    error_line: u32,
    /// The drafting sheet's frame lines, and its thinner ones.
    frame: (u32, f32),
    frame_thin: (u32, f32),
    /// How far the accent is drawn toward the ink, so that it reads on a
    /// dark ground as well as on paper.
    accent_lift: f32,
    /// The accent's opacity where it only marks the form at rest: the
    /// avatar's ring and the field's underline before anything is typed.
    accent_quiet: f32,
}

const DAY: Palette = Palette {
    ground: 0xF3EEE4,
    ink: 0x2A2622,
    line_alpha: 0.16,
    menu: 0xFBF8F2,
    error: 0xA8452A,
    error_line: 0xB0472A,
    frame: (0x503E2D, 0.55),
    frame_thin: (0x503E2D, 0.25),
    accent_lift: 0.0,
    accent_quiet: 0.5,
};

const NIGHT: Palette = Palette {
    ground: 0x1D1B18,
    ink: 0xECE5D8,
    line_alpha: 0.18,
    menu: 0x26231F,
    error: 0xE08A6A,
    error_line: 0xE08A6A,
    frame: (0xECE5D8, 0.5),
    frame_thin: (0xECE5D8, 0.22),
    accent_lift: 0.3,
    accent_quiet: 0.9,
};

/// What a control lit by the pointer, the menu's highlighted session or an
/// armed cell of the title block lays under itself.
const LIT_ALPHA: f32 = 0.12;
const CELL_LIT_ALPHA: f32 = 0.08;

impl Palette {
    /// The settings' accent as this palette shows it.
    fn accent(&self, view: &View) -> Hsla {
        let accent = view.greeter.config.paper.accent.0;
        color(mix(accent, self.ink, self.accent_lift), 1.0)
    }
}

/// The colour `t` of the way from `from` to `to`, both 0xRRGGBB.
fn mix(from: u32, to: u32, t: f32) -> u32 {
    [16, 8, 0].into_iter().fold(0, |rgb, shift| {
        let (a, b) = ((from >> shift) & 0xFF, (to >> shift) & 0xFF);
        let channel = a as f32 + (b as f32 - a as f32) * t;
        rgb | (channel.round().clamp(0.0, 255.0) as u32) << shift
    })
}

/// The desk's sheet of paper, cream by night too.
const SHEET: u32 = 0xF7F2E7;

const SERIF_CLOCK: &str = "Fraunces 144pt";
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
    // At rest, which only the lock screen has, the clock and the paper
    // show alone, until a click or a key.
    let resting = greeter.resting;
    root.on_mouse_down(
        MouseButton::Left,
        view.on(move |greeter, cx| {
            if resting {
                greeter.wake();
                cx.notify();
            } else {
                greeter.close_menu(cx);
            }
        }),
    )
    .child(
        div()
            .absolute()
            .left(u(160.))
            .top(u(72.))
            .opacity(0.68)
            .child(
                label(greeter.machine.hostname.clone())
                    .font("Inter", FontWeight::LIGHT)
                    .size(12.)
                    .color(ink)
                    .tracking_em(0.1),
            ),
    )
    .child(main_column(view, palette))
    .child(if drafting {
        title_block(view, palette).into_any_element()
    } else if resting {
        div().into_any_element()
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
    // Figures as tall as macOS's lock screen clock's on a 27-inch display.
    let clock = label(i18n::clock(&greeter.now, false))
        .font(SERIF_CLOCK, FontWeight::LIGHT)
        .size(168.)
        .line_height(0.9)
        .tracking(-4.6)
        .color(ink);
    let date = div().mt(u(22.)).opacity(0.68).child(
        label(i18n::long_date(greeter.config.language, &greeter.now))
            .font("Inter", FontWeight::LIGHT)
            .size(20.)
            .tracking_em(0.02)
            .color(ink),
    );
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
        // At rest the form keeps its room, so the clock stays put as it
        // shows.
        .child(form(view, palette).when(greeter.resting, |form| form.opacity(0.)))
}

fn form(view: &View, palette: &Palette) -> gpui_kit::Div {
    let greeter = view.greeter;
    let ink = color(palette.ink, 1.0);
    let accent = palette.accent(view);
    let failed = matches!(greeter.status(), Status::Denied(_) | Status::Failed(_));
    let underline = if failed {
        color(palette.error_line, 1.0)
    } else if greeter.typed_len() > 0 {
        accent
    } else {
        accent.opacity(palette.accent_quiet)
    };
    let user = greeter.user();
    let name = user
        .map(|user| user.display_name.clone())
        .unwrap_or_default();
    let switches = greeter.users.len() > 1;
    // As large as macOS's on a 27-inch display, beside the name, the field
    // and the status, which stand as tall.
    let avatar = div()
        .id("avatar")
        .size(u(72.))
        .flex_shrink_0()
        .rounded_full()
        .border(u(1.))
        .border_color(accent.opacity(palette.accent_quiet))
        .overflow_hidden()
        .flex()
        .items_center()
        .justify_center()
        .when(switches, |avatar| {
            avatar
                .cursor_pointer()
                .hover(move |avatar| avatar.border_color(accent))
                .on_click(view.on(|greeter, cx| greeter.next_user(1, cx)))
        })
        .child(match user.and_then(|user| user.avatar.clone()) {
            Some(path) => img(path).size_full().rounded_full().into_any_element(),
            None => label(user.map(|user| user.initial()).unwrap_or_default())
                .font("Inter", FontWeight::LIGHT)
                .size(30.)
                .color(ink)
                .ink()
                .into_any_element(),
        });
    let who = div()
        .id("user")
        .text_color(ink)
        .when(switches, |name| {
            name.cursor_pointer()
                .hover(move |name| name.text_color(accent))
                .on_click(view.on(|greeter, cx| greeter.next_user(1, cx)))
        })
        .child(
            label(name)
                .size(15.)
                .font("Inter", FontWeight::MEDIUM)
                .tracking_em(0.02)
                .inherit_color(),
        );
    let look = FieldLook {
        family: "Inter",
        weight: FontWeight::LIGHT,
        size: 13.,
        color: ink,
        tracking: 0.32,
        placeholder: color(palette.ink, 0.55),
        placeholder_tracking: 0.06,
        caret: accent,
        centered: false,
    };
    // macOS's password field: 160 by 28 points.
    let field = div()
        .relative()
        .w(u(160.))
        .pt(u(6.))
        .pb(u(6.))
        .border_b(u(1.))
        .border_color(underline)
        .child(field_text(view, &look, greeter.strings.password))
        .with_animation(("shake", greeter.denials as usize), Shake::animation(), {
            let frames = Shake::frames();
            move |field, t| field.left(u(frames.sample(t)))
        });
    div()
        .mt(u(56.))
        .flex()
        .items_center()
        .gap(u(16.))
        .child(avatar)
        .child(
            div()
                .flex()
                .flex_col()
                .gap(u(6.))
                .child(who)
                .child(field)
                .child(status_line(view, palette, accent)),
        )
}

fn status_line(view: &View, palette: &Palette, accent: Hsla) -> impl IntoElement {
    let greeter = view.greeter;
    let s = greeter.strings;
    let ink = |alpha| color(palette.ink, alpha);
    let error = color(palette.error, 1.0);
    let line = div().h(u(15.)).flex().items_center().gap(u(6.));
    let text = |text: &str, color: Hsla| {
        label(text.to_string())
            .font("Inter", FontWeight::LIGHT)
            .size(12.)
            .tracking_em(0.04)
            .color(color)
    };
    match greeter.status() {
        Status::Hint => line.child(text(s.press_enter, ink(0.68))),
        Status::Notice(notice) => line.child(text(notice, ink(0.68))),
        Status::Checking => line.child(text(s.checking, ink(0.68))).child(dots(accent)),
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
            .child(
                label("\u{00B7}")
                    .font("Inter", FontWeight::LIGHT)
                    .size(12.)
                    .color(color),
            )
            .with_animation(("dot", index), Pulse::animation(), move |dot, t| {
                dot.opacity(Pulse::opacity(t, delay))
            })
    }))
}

fn bottom_bar(view: &View, palette: &Palette, right: f32) -> impl IntoElement {
    let greeter = view.greeter;
    let s = greeter.strings;
    let ink = color(palette.ink, 1.0);
    let accent = palette.accent(view);
    let open = greeter.menu.is_some();
    let session_button = control(
        div().id("session").flex().items_center().gap(u(6.)),
        ink,
        accent,
        open,
    )
    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
    .on_click(view.on(|greeter, cx| greeter.toggle_menu(cx)))
    .child(
        label(s.session)
            .font("Inter", FontWeight::LIGHT)
            .size(13.)
            .tracking_em(0.04)
            .inherit_color(),
    )
    .child(
        label(greeter.session_name().to_string())
            .size(13.)
            .font("Inter", FontWeight::MEDIUM)
            .tracking_em(0.04)
            .inherit_color(),
    )
    .child(
        svg()
            .path("icons/caret-up.svg")
            .size(u(10.))
            .text_color(accent),
    );
    let power = |action: Action, id: &'static str| {
        let armed = greeter.status() == Status::Confirm(action);
        control(div().id(id), ink, accent, armed)
            .on_click(view.on(move |greeter, cx| greeter.power(action, cx)))
            .child(
                label(greeter.action_name(action))
                    .font("Inter", FontWeight::LIGHT)
                    .size(13.)
                    .tracking_em(0.04)
                    .inherit_color(),
            )
    };
    div()
        .absolute()
        .left(u(160.))
        .right(u(right))
        .bottom(u(48.))
        .flex()
        .justify_between()
        .items_end()
        .child(
            div()
                .flex()
                .gap(u(28.))
                // The lock screen's session is the one running.
                .when(!greeter.lock_screen, |left| {
                    left.child(
                        div()
                            .relative()
                            .when_some(greeter.menu, |anchor, highlighted| {
                                anchor.child(deferred(
                                    menu(view, &MenuLook::bar(palette), highlighted, accent)
                                        .left(u(-10.))
                                        .bottom(u(36.)),
                                ))
                            })
                            .child(session_button),
                    )
                })
                .when(greeter.looks.cycle.len() > 1, |left| {
                    left.child(theme_button(view, ink, accent))
                }),
        )
        .child(
            div()
                .flex()
                .gap(u(28.))
                .child(power(Action::Suspend, "sleep"))
                .child(power(Action::Reboot, "restart"))
                .child(power(Action::PowerOff, "shutdown")),
        )
}

/// One of the bar's words that act: quiet ink, and the accent under the
/// pointer or while `lit`, as an open menu's control or an armed action is.
fn control(
    button: gpui_kit::Stateful<gpui_kit::Div>,
    ink: Hsla,
    accent: Hsla,
    lit: bool,
) -> gpui_kit::Stateful<gpui_kit::Div> {
    button
        .py(u(8.))
        .cursor_pointer()
        .text_color(if lit { accent } else { ink })
        .opacity(if lit { 1.0 } else { 0.7 })
        .hover(move |button| button.opacity(1.0).text_color(accent))
}

/// "Theme Paper": switches to the next look, as F9 does.
fn theme_button(view: &View, ink: Hsla, accent: Hsla) -> impl IntoElement {
    let greeter = view.greeter;
    control(
        div().id("theme").flex().items_center().gap(u(6.)),
        ink,
        accent,
        false,
    )
    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
    .on_click(view.on(|greeter, cx| greeter.switch_theme(1, cx)))
    .child(
        label(greeter.strings.theme)
            .font("Inter", FontWeight::LIGHT)
            .size(13.)
            .tracking_em(0.04)
            .inherit_color(),
    )
    .child(
        label(greeter.theme_title())
            .size(13.)
            .font("Inter", FontWeight::MEDIUM)
            .tracking_em(0.04)
            .inherit_color(),
    )
}

/// How the session menu looks: rounded over the bar, square on the
/// drafting sheet. It is painted after everything else, over the borders
/// that GPUI draws after a box's children.
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
            radius: 8.,
            min_width: 160.,
            option_padding: (10., 6.),
            option_radius: 5.,
            gap: 20.,
            name_size: 13.,
            mark_size: 11.,
            ink: color(palette.ink, 1.0),
        }
    }

    fn sheet(palette: &Palette) -> Self {
        Self {
            border: color(palette.frame.0, palette.frame.1),
            radius: 0.,
            option_padding: (9., 6.),
            option_radius: 0.,
            gap: 16.,
            ..Self::bar(palette)
        }
    }
}

fn menu(view: &View, look: &MenuLook, highlighted: usize, accent: Hsla) -> gpui_kit::Div {
    let greeter = view.greeter;
    let hover = accent.opacity(LIT_ALPHA);
    let ink = look.ink;
    div()
        .absolute()
        .min_w(u(look.min_width))
        .p(u(5.))
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
                .child(
                    label(session.name.clone())
                        .font("Inter", FontWeight::LIGHT)
                        .size(look.name_size)
                        .color(ink),
                )
                .child(
                    label(if current { greeter.strings.current } else { "" })
                        .font("Inter", FontWeight::LIGHT)
                        .size(look.mark_size)
                        .color(accent),
                )
        }))
}

/// The drafting sheet's title block, in its bottom right corner: the
/// project and the host above, the session and the power actions below,
/// each under its caption.
fn title_block(view: &View, palette: &Palette) -> impl IntoElement {
    const COLUMN: f32 = 112.;
    let greeter = view.greeter;
    let s = greeter.strings;
    let ink = color(palette.ink, 1.0);
    let accent = palette.accent(view);
    let thin = color(palette.frame_thin.0, palette.frame_thin.1);
    let hover = accent.opacity(CELL_LIT_ALPHA);
    let cell = |columns: f32, caption: String| {
        div()
            .w(u(COLUMN * columns))
            .px(u(10.))
            .py(u(7.))
            .flex()
            .flex_col()
            .gap(u(3.))
            .border_color(thin)
            .child(
                div()
                    .opacity(0.68)
                    .child(label(caption).size(10.).tracking_em(0.12).color(ink)),
            )
    };
    let entry = |caption: &str, value: String| {
        cell(2., caption.to_string()).child(
            label(value)
                .font(SERIF_TEXT, FontWeight::NORMAL)
                .size(15.)
                .color(ink),
        )
    };
    let session = cell(1., s.session.to_string())
        .id("session")
        .border_r(u(1.))
        .cursor_pointer()
        .when(greeter.menu.is_some(), |cell| cell.bg(hover))
        .hover(|cell| cell.bg(hover))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(view.on(|greeter, cx| greeter.toggle_menu(cx)))
        .child(
            div()
                .flex()
                .items_center()
                .gap(u(6.))
                .child(
                    label(greeter.session_name().to_string())
                        .size(13.)
                        .font("Inter", FontWeight::MEDIUM)
                        .color(ink),
                )
                .child(
                    svg()
                        .path("icons/caret-up.svg")
                        .size(u(10.))
                        .text_color(accent),
                ),
        );
    let keys = greeter.power_keys();
    // The lock screen's session is the one running, not chosen there: the
    // power actions take its cell's room.
    let columns = if greeter.lock_screen {
        4. / keys.len().max(1) as f32
    } else {
        1.
    };
    let actions = keys.iter().enumerate().map(|(index, &(key, action))| {
        let armed = greeter.status() == Status::Confirm(action);
        cell(columns, key.to_uppercase())
            .id(key)
            .when(index + 1 < keys.len(), |cell| cell.border_r(u(1.)))
            .cursor_pointer()
            .when(armed, |cell| cell.bg(hover))
            .hover(|cell| cell.bg(hover))
            .on_click(view.on(move |greeter, cx| greeter.power(action, cx)))
            .child(
                label(greeter.action_name(action))
                    .font("Inter", FontWeight::LIGHT)
                    .size(13.)
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
                .child(if greeter.looks.cycle.len() > 1 {
                    // The theme takes the project's place, as a control.
                    entry(s.theme, greeter.theme_title())
                        .id("theme")
                        .border_r(u(1.))
                        .border_b(u(1.))
                        .cursor_pointer()
                        .hover(|cell| cell.bg(hover))
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_click(view.on(|greeter, cx| greeter.switch_theme(1, cx)))
                        .into_any_element()
                } else {
                    entry(s.project, "gpui-greeter".into())
                        .border_r(u(1.))
                        .border_b(u(1.))
                        .into_any_element()
                })
                .child(entry(s.host, greeter.machine.hostname.clone()).border_b(u(1.))),
        )
        .child(
            div()
                .flex()
                .when(!greeter.lock_screen, |row| {
                    row.child(
                        div()
                            .relative()
                            .flex()
                            .when_some(greeter.menu, |anchor, highlighted| {
                                anchor.child(deferred(
                                    menu(view, &MenuLook::sheet(palette), highlighted, accent)
                                        .left(u(-1.))
                                        .bottom(relative(1.)),
                                ))
                            })
                            .child(session),
                    )
                })
                .children(actions),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mixes_each_channel_on_its_own() {
        assert_eq!(mix(0xB0612E, 0xECE5D8, 0.0), 0xB0612E);
        assert_eq!(mix(0xB0612E, 0xECE5D8, 1.0), 0xECE5D8);
        assert_eq!(mix(0x00FF00, 0xFF0000, 0.5), 0x808000);
    }
}

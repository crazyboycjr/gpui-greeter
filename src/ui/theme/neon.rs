//! Neon: a drawn background (a city at night, a circuit board, a synthwave
//! horizon or a faint grid) under a vignette, scanlines and a slow scan
//! beam; the hostname over a terminal-style login panel with corner brackets
//! and a status line, and F-key buttons along the bottom.

use std::rc::Rc;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use gpui_kit::prelude::*;
use gpui_kit::{
    Animation, AnimationExt as _, AnyElement, App, Bounds, BoxShadow, FontWeight, Hsla,
    MouseButton, ObjectFit, Pixels, RenderImage, Rgba, SharedString, TransformationMatrix, Window,
    canvas, div, fill, img, linear_color_stop, linear_gradient, point, px, radians, relative, size,
    svg,
};
use image::RgbaImage;

use super::{FieldLook, View, color, field_text, picture, u};
use crate::config::NeonBackground;
use crate::i18n;
use crate::power::Action;
use crate::scenery::{Floor, Motion, Pulse, Scenery};
use crate::system;
use crate::ui::model::{Phase, Status};
use crate::ui::motion::{Beacon, Glitch, beam, blink, blink_opacity, fill as fill_animation, run};
use crate::ui::text::label;
use crate::wallpaper::render_image;

const GROUND: u32 = 0x06070A;
const INK: u32 = 0xD7F6FB;
const BRIGHT: u32 = 0xEAFCFF;
const ERROR: u32 = 0xFF5C7A;
const MONO: &str = "JetBrains Mono";

struct Palette {
    accent: Hsla,
    glow: Hsla,
    line: Hsla,
    edge: Hsla,
}

impl Palette {
    fn new(view: &View) -> Self {
        let accent = view.greeter.config.neon.accent.0;
        let denied = matches!(view.greeter.phase, Phase::Denied { .. });
        Self {
            accent: color(accent, 1.0),
            glow: color(accent, 0.45),
            line: if denied {
                color(ERROR, 0.55)
            } else {
                color(accent, 0.32)
            },
            edge: if denied {
                color(ERROR, 1.0)
            } else {
                color(accent, 1.0)
            },
        }
    }
}

pub fn render(view: &View) -> AnyElement {
    let palette = Palette::new(view);
    let accent = view.greeter.config.neon.accent.0;
    let backdrop = match (view.greeter.config.neon.background, view.scenery) {
        (NeonBackground::Grid, _) => grid(view, color(accent, 0.055)).into_any_element(),
        (_, Some(scenery)) => div()
            .absolute()
            .inset_0()
            .child(picture(scenery))
            .child(motion(view, scenery))
            .into_any_element(),
        (_, None) => div().into_any_element(),
    };
    let root = div()
        .size_full()
        .relative()
        .overflow_hidden()
        .bg(color(GROUND, 1.0))
        .child(backdrop)
        .child(
            img(vignette())
                .absolute()
                .inset_0()
                .size_full()
                .object_fit(ObjectFit::Fill),
        )
        .child(scan_beam(view, color(accent, 0.05)))
        .when(view.greeter.config.neon.scanlines, |root| {
            root.child(scanlines(view))
        });
    if !view.has_form {
        return root.into_any_element();
    }
    // At rest, which only the lock screen has, the terminal waits for a
    // click or a key.
    let resting = view.greeter.resting;
    root.when(resting, |root| {
        root.on_mouse_down(
            MouseButton::Left,
            view.on(|greeter, cx| {
                greeter.wake();
                cx.notify();
            }),
        )
    })
    .child(top_left(view, &palette))
    .child(top_right(view, &palette))
    .child(center(view, &palette))
    .child(bottom_left(view, &palette))
    .when(!resting, |root| root.child(keys(view, &palette)))
    .into_any_element()
}

fn text(text: impl Into<gpui_kit::SharedString>, size: f32) -> crate::ui::text::Label {
    label(text)
        .font(MONO, FontWeight::NORMAL)
        .size(size)
        .color(color(INK, 1.0))
}

/// Lines every 56 pixels, one pixel thick.
fn grid(view: &View, line: Hsla) -> impl IntoElement {
    let (step, width) = (view.px(56.), view.px(1.));
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let mut x = bounds.left() + step - width;
            while x < bounds.right() {
                window.paint_quad(fill(
                    Bounds::new(point(x, bounds.top()), size(width, bounds.size.height)),
                    line,
                ));
                x += step;
            }
            let mut y = bounds.top() + step - width;
            while y < bounds.bottom() {
                window.paint_quad(fill(
                    Bounds::new(point(bounds.left(), y), size(bounds.size.width, width)),
                    line,
                ));
                y += step;
            }
        },
    )
    .absolute()
    .inset_0()
}

/// A dark line every three pixels, counted up from the bottom.
fn scanlines(view: &View) -> impl IntoElement {
    let (step, height) = (view.px(3.), view.px(1.));
    let shade = color(0, 0.22);
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let mut y = bounds.bottom() - height;
            while y > bounds.top() - height {
                window.paint_quad(fill(
                    Bounds::new(point(bounds.left(), y), size(bounds.size.width, height)),
                    shade,
                ));
                y -= step;
            }
        },
    )
    .absolute()
    .inset_0()
}

/// What moves in the drawn background, over its picture.
fn motion(view: &View, scenery: &Arc<Scenery>) -> AnyElement {
    let (id, animation) = match &scenery.motion {
        Motion::Still => return div().into_any_element(),
        Motion::Beacons(_) => ("beacons", Beacon::animation()),
        Motion::Pulses(_) => ("pulses", run(Duration::from_secs_f64(Pulse::SECONDS))),
        Motion::Floor(_) => ("floor", run(Duration::from_secs_f64(Floor::SECONDS))),
    };
    let (unit, accent) = (view.unit, view.greeter.config.neon.accent.0);
    let scenery = scenery.clone();
    moving(id, animation, move |t, bounds, window, cx| {
        let at =
            |(x, y): (f64, f64)| bounds.origin + point(px(x as f32 * unit), px(y as f32 * unit));
        match &scenery.motion {
            Motion::Still => {}
            Motion::Beacons(tips) => {
                let red = color(0xFF3B5C, Beacon::opacity(t));
                let side = size(px(5. * unit), px(5. * unit));
                for &tip in tips {
                    window.paint_quad(fill(Bounds::new(at(tip), side), red));
                }
            }
            Motion::Pulses(pulses) => {
                for pulse in pulses {
                    paint_pulse(
                        pulse,
                        f64::from(t),
                        unit,
                        &at,
                        color(accent, 1.0),
                        window,
                        cx,
                    );
                }
            }
            Motion::Floor(floor) => {
                for band in floor.bands(f64::from(t)) {
                    let (top, bottom) = (at((0.0, band.top)).y, at((0.0, band.bottom)).y);
                    let line = Bounds::new(
                        point(bounds.left(), top),
                        size(bounds.size.width, bottom - top),
                    );
                    window.paint_quad(fill(line, floor_line(accent, band.shade)));
                }
            }
        }
    })
}

/// A layer over the window that paints itself afresh at each step of a
/// looping `animation`.
fn moving(
    id: &'static str,
    animation: Animation,
    paint: impl Fn(f32, Bounds<Pixels>, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let paint = Rc::new(paint);
    div()
        .absolute()
        .inset_0()
        .with_animation(id, animation, move |layer, t| {
            let paint = paint.clone();
            layer.child(
                canvas(
                    |_, _, _| {},
                    move |bounds, _, window, cx| paint(t, bounds, window, cx),
                )
                .absolute()
                .inset_0(),
            )
        })
        .into_any_element()
}

/// A crosswise floor line: the accent, under the shade that darkens the
/// floor toward the horizon. The rest of the floor has the shade drawn in.
fn floor_line(accent: u32, shade: f64) -> Hsla {
    let (line, dark) = (gpui_kit::rgb(accent), gpui_kit::rgb(Floor::SHADE.0));
    let shade = shade as f32;
    let mix = |line: f32, dark: f32| line + (dark - line) * shade;
    Rgba {
        r: mix(line.r, dark.r),
        g: mix(line.g, dark.g),
        b: mix(line.b, dark.b),
        a: Floor::LINE_ALPHA as f32,
    }
    .into()
}

/// The dashes of light on a trace `run` of the way through their loop: each
/// part of a dash on a straight stretch is a glowing stroke, turned to lie
/// along it. `at` places a point of the frame in the window.
fn paint_pulse(
    pulse: &Pulse,
    run: f64,
    unit: f32,
    at: &dyn Fn((f64, f64)) -> gpui_kit::Point<Pixels>,
    accent: Hsla,
    window: &mut Window,
    cx: &mut App,
) {
    let stretches: Vec<_> = pulse
        .points
        .windows(2)
        .map(|ends| {
            let length = (ends[1].0 - ends[0].0).hypot(ends[1].1 - ends[0].1);
            (ends[0], ends[1], length)
        })
        .collect();
    let length: f64 = stretches.iter().map(|stretch| stretch.2).sum();
    let start = Pulse::PERIOD * ((run + pulse.lead / Pulse::SECONDS) % 1.0);
    let mut dash = start - Pulse::PERIOD;
    while dash < length {
        let mut along = 0.0;
        for &(from, to, stretch) in &stretches {
            let (first, last) = (dash.max(along), (dash + Pulse::DASH).min(along + stretch));
            if last > first {
                let point_at = |distance: f64| {
                    let share = (distance - along) / stretch;
                    (
                        from.0 + (to.0 - from.0) * share,
                        from.1 + (to.1 - from.1) * share,
                    )
                };
                let (start, end) = (point_at(first), point_at(last));
                let middle = at(((start.0 + end.0) / 2.0, (start.1 + end.1) / 2.0));
                let angle = (to.1 - from.1).atan2(to.0 - from.0);
                paint_glow(last - first, middle, angle, unit, accent, window, cx);
            }
            along += stretch;
        }
        dash += Pulse::PERIOD;
    }
}

/// One straight part of a dash, `length` points long, centred on
/// `middle` and turned by `angle`.
fn paint_glow(
    length: f64,
    middle: gpui_kit::Point<Pixels>,
    angle: f64,
    unit: f32,
    accent: Hsla,
    window: &mut Window,
    cx: &mut App,
) {
    let halves = ((length * 2.0).round() as usize).min(GLOW_HALVES);
    if halves == 0 {
        return;
    }
    let (key, svg) = &glows()[halves];
    let extent = size(
        px((halves as f32 / 2.0 + 2.0 * GLOW_MARGIN) * unit),
        px(2.0 * GLOW_MARGIN * unit),
    );
    let bounds = Bounds::new(
        middle - point(extent.width / 2.0, extent.height / 2.0),
        extent,
    );
    let scale = window.scale_factor();
    let turn = TransformationMatrix::unit()
        .translate(middle.scale(scale))
        .rotate(radians(angle as f32))
        .translate(middle.scale(-scale));
    let _ = window.paint_svg(bounds, key.clone(), Some(svg.as_bytes()), turn, accent, cx);
}

/// How far a dash's glow reaches past its stroke, in points: its
/// round cap and three deviations of its blur.
const GLOW_MARGIN: f32 = 11.0;
/// The longest part of a dash, in half pixels.
const GLOW_HALVES: usize = (Pulse::DASH * 2.0) as usize;

/// The glowing dash of each length, in half pixels: the stroke and its
/// blur as the mockup's glow filter merges them, as a mask to tint.
fn glows() -> &'static [(SharedString, String)] {
    static GLOWS: OnceLock<Vec<(SharedString, String)>> = OnceLock::new();
    GLOWS.get_or_init(|| {
        (0..=GLOW_HALVES)
            .map(|halves| {
                let length = halves as f32 / 2.0;
                let (width, height) = (length + 2.0 * GLOW_MARGIN, 2.0 * GLOW_MARGIN);
                let svg = format!(
                    r##"<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}"><filter id="glow" filterUnits="userSpaceOnUse" x="0" y="0" width="{width}" height="{height}"><feGaussianBlur stdDeviation="3" result="blur"/><feMerge><feMergeNode in="blur"/><feMergeNode in="SourceGraphic"/></feMerge></filter><path d="M{GLOW_MARGIN} {GLOW_MARGIN}h{length}" stroke="#fff" stroke-width="{}" stroke-linecap="round" filter="url(#glow)"/></svg>"##,
                    Pulse::WIDTH
                );
                (format!("gpui-greeter/pulse-{halves}").into(), svg)
            })
            .collect()
    })
}

/// The darkening towards the edges: an ellipse about a point a little above
/// the middle, clear inside and the ground's colour at the corners. GPUI has
/// no radial gradients, so it is a small picture, stretched.
fn vignette() -> Arc<RenderImage> {
    static VIGNETTE: OnceLock<Arc<RenderImage>> = OnceLock::new();
    VIGNETTE
        .get_or_init(|| {
            const SIDE: u32 = 256;
            let (cx, cy) = (0.5, 0.46);
            // CSS's farthest-corner ellipse: the side distances, grown to reach a corner.
            let (rx, ry) = (
                0.5 * std::f32::consts::SQRT_2,
                0.54 * std::f32::consts::SQRT_2,
            );
            let image = RgbaImage::from_fn(SIDE, SIDE, |x, y| {
                let (px, py) = (
                    (x as f32 + 0.5) / SIDE as f32,
                    (y as f32 + 0.5) / SIDE as f32,
                );
                let t = (((px - cx) / rx).powi(2) + ((py - cy) / ry).powi(2)).sqrt();
                // Clear in the middle, .45 at 60% of the way out, .9 at the corners.
                let alpha = if t < 0.6 {
                    0.75 * t
                } else {
                    0.45 + (t - 0.6) * 1.125
                }
                .min(0.9);
                image::Rgba([6, 7, 10, (alpha * 255.0).round() as u8])
            });
            render_image(image)
        })
        .clone()
}

fn scan_beam(view: &View, tint: Hsla) -> impl IntoElement {
    let height = view.size.1;
    let transparent = color(view.greeter.config.neon.accent.0, 0.0);
    div()
        .absolute()
        .left_0()
        .right_0()
        .h(u(160.))
        .flex()
        .flex_col()
        .child(div().flex_1().bg(linear_gradient(
            180.,
            linear_color_stop(transparent, 0.),
            linear_color_stop(tint, 1.),
        )))
        .child(div().flex_1().bg(linear_gradient(
            180.,
            linear_color_stop(tint, 0.),
            linear_color_stop(transparent, 1.),
        )))
        .with_animation("beam", beam(), move |beam, t| {
            beam.top(u(-160. + t * (height + 320.)))
        })
}

fn top_left(view: &View, palette: &Palette) -> impl IntoElement {
    let machine = &view.greeter.machine;
    let place = [
        Some(machine.seat.clone()),
        machine.vt.clone(),
        Some("x11".to_string()),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" \u{00B7} ");
    div()
        .absolute()
        .left(u(64.))
        .top(u(52.))
        .flex()
        .flex_col()
        .gap(u(6.))
        .child(
            div()
                .flex()
                .items_center()
                .gap(u(8.))
                .child(
                    svg()
                        .path("icons/corner.svg")
                        .size(u(9.))
                        .text_color(palette.accent),
                )
                .child(
                    text(
                        if view.greeter.lock_screen {
                            "lock"
                        } else {
                            "greetd"
                        },
                        13.,
                    )
                    .font(MONO, FontWeight::BOLD)
                    .color(palette.accent)
                    .tracking_em(0.14),
                ),
        )
        .child(div().opacity(0.55).child(text(place, 13.)))
}

fn top_right(view: &View, palette: &Palette) -> impl IntoElement {
    let now = &view.greeter.now;
    div()
        .absolute()
        .right(u(64.))
        .top(u(44.))
        .flex()
        .flex_col()
        .items_end()
        .gap(u(4.))
        .child(
            text(i18n::clock(now, true), 40.)
                .font(MONO, FontWeight::MEDIUM)
                .tracking_em(0.04)
                .color(palette.accent)
                .shadow(0., 0., 18., palette.glow),
        )
        .child(
            div()
                .opacity(0.6)
                .child(text(i18n::iso_date(now), 13.).tracking_em(0.1)),
        )
}

fn center(view: &View, palette: &Palette) -> impl IntoElement {
    let greeter = view.greeter;
    let machine = &greeter.machine;
    let hostname = machine.hostname.clone();
    let glow = palette.glow;
    let host = div().relative().with_animation(
        ("glitch", greeter.denials as usize),
        Glitch::animation(),
        move |host, t| {
            let (dx, dy, split) = if t >= 1.0 {
                (0., 0., 0.)
            } else {
                Glitch::pose(t)
            };
            let mut name = text(hostname.clone(), 92.)
                .font(MONO, FontWeight::EXTRA_BOLD)
                .line_height(0.9)
                .tracking(-1.)
                .color(color(BRIGHT, 1.0))
                .shadow(0., 0., 26., glow);
            if split != 0.0 {
                name = name.shadow(split, 0., 0., color(0xFF2E63, 1.0)).shadow(
                    -split,
                    0.,
                    0.,
                    color(0x45F0FF, 1.0),
                );
            }
            host.left(u(dx)).top(u(dy)).child(name)
        },
    );
    let system = div()
        .opacity(0.5)
        .flex()
        .flex_col()
        .items_end()
        .child(text(machine.os.clone(), 12.).line_height(1.6))
        .child(text(machine.kernel.clone(), 12.).line_height(1.6));
    let column = div()
        .w(u(760.))
        .flex()
        .flex_col()
        .gap(u(26.))
        .child(
            div()
                .flex()
                .items_end()
                .justify_between()
                .child(host)
                .child(system),
        )
        .child(div().h(u(1.)).bg(linear_gradient(
            90.,
            linear_color_stop(palette.accent, 0.),
            linear_color_stop(color(greeter.config.neon.accent.0, 0.0), 1.),
        )))
        .child(if greeter.resting {
            resting_line(view, palette).into_any_element()
        } else {
            panel(view, palette).into_any_element()
        });
    // A little above the middle, as the mockup's translate(-50%, -54%).
    div()
        .absolute()
        .inset_0()
        .pb(u(32.))
        .flex()
        .items_center()
        .justify_center()
        .child(column)
}

/// What stands in for the login panel at rest: a status line saying how to
/// wake the terminal.
fn resting_line(view: &View, palette: &Palette) -> impl IntoElement {
    div()
        .px(u(32.))
        .flex()
        .items_center()
        .gap(u(16.))
        .child(
            text("[ LOCKED ]", 14.)
                .font(MONO, FontWeight::BOLD)
                .color(palette.accent)
                .tracking_em(0.04),
        )
        .child(
            div()
                .opacity(0.8)
                .child(text(view.greeter.strings.neon.wake, 14.).tracking_em(0.04)),
        )
        .child(
            div()
                .child(text("\u{258C}", 14.).color(palette.accent))
                .with_animation("blink", blink(1100), |cursor, t| {
                    cursor.opacity(blink_opacity(t))
                }),
        )
}

fn panel(view: &View, palette: &Palette) -> impl IntoElement {
    let greeter = view.greeter;
    let s = &greeter.strings.neon;
    let corner = |top: bool, left: bool| {
        let bracket = div().absolute().size(u(18.)).border_color(palette.edge);
        let bracket = if top {
            bracket.top(u(-1.)).border_t(u(2.))
        } else {
            bracket.bottom(u(-1.)).border_b(u(2.))
        };
        if left {
            bracket.left(u(-1.)).border_l(u(2.))
        } else {
            bracket.right(u(-1.)).border_r(u(2.))
        }
    };
    let session = div()
        .id("session")
        .flex()
        .items_center()
        .gap(u(10.))
        .px(u(10.))
        .py(u(6.))
        .border(u(1.))
        .border_color(palette.line)
        .cursor_pointer()
        .hover(|button| button.bg(color(greeter.config.neon.accent.0, 0.12)))
        .on_click(view.on(|greeter, cx| {
            greeter.next_session();
            cx.notify();
        }))
        .child(
            div()
                .opacity(0.55)
                .child(text(s.session, 12.).tracking_em(0.08)),
        )
        .child(
            text(greeter.session_name().to_string(), 12.)
                .font(MONO, FontWeight::BOLD)
                .tracking_em(0.08),
        )
        .child(text("F2", 12.).color(palette.accent).tracking_em(0.08));
    // The theme, a line like the session's, in Neon's lower case.
    let theme = div()
        .id("theme")
        .flex()
        .items_center()
        .gap(u(10.))
        .px(u(10.))
        .py(u(6.))
        .border(u(1.))
        .border_color(palette.line)
        .cursor_pointer()
        .hover(|button| button.bg(color(greeter.config.neon.accent.0, 0.12)))
        .on_click(view.on(|greeter, cx| greeter.switch_theme(1, cx)))
        .child(
            div()
                .opacity(0.55)
                .child(text(s.theme, 12.).tracking_em(0.08)),
        )
        .child(
            text(greeter.theme_title().to_lowercase(), 12.)
                .font(MONO, FontWeight::BOLD)
                .tracking_em(0.08),
        )
        .child(text("F9", 12.).color(palette.accent).tracking_em(0.08));
    let header = div()
        .flex()
        .justify_between()
        .items_center()
        .mb(u(22.))
        .child(
            text(
                if greeter.lock_screen {
                    "\u{258D}AUTH // unlock"
                } else {
                    "\u{258D}AUTH // login"
                },
                12.,
            )
            .color(palette.accent)
            .tracking_em(0.12),
        )
        .child(
            div()
                .flex()
                .gap(u(10.))
                .when(greeter.looks.cycle.len() > 1, |controls| {
                    controls.child(theme)
                })
                // The lock screen's session is the one running.
                .when(!greeter.lock_screen, |controls| controls.child(session)),
        );
    let look = FieldLook {
        family: MONO,
        weight: FontWeight::NORMAL,
        size: 20.,
        color: color(BRIGHT, 1.0),
        tracking: 0.32,
        placeholder: color(0xBEE6F0, 0.32),
        placeholder_tracking: 0.06,
        caret: palette.accent,
        centered: false,
    };
    let row = |name: &str, value: AnyElement| {
        div()
            .flex()
            .items_center()
            .child(
                div()
                    .w(u(150.))
                    .flex_shrink_0()
                    .opacity(0.5)
                    .child(text(format!("> {name}"), 18.)),
            )
            .child(div().flex_1().min_w_0().child(value))
    };
    let user = greeter
        .user()
        .map(|user| user.name.clone())
        .unwrap_or_default();
    let fields = div()
        .flex()
        .flex_col()
        .gap(u(18.))
        .child(row(
            s.user,
            text(user, 18.)
                .font(MONO, FontWeight::MEDIUM)
                .into_any_element(),
        ))
        .child(row(
            s.password,
            div()
                .py(u(4.))
                .child(field_text(view, &look, s.placeholder))
                .into_any_element(),
        ));
    div()
        .relative()
        .pt(u(26.))
        .px(u(32.))
        .pb(u(22.))
        .bg(color(0x080D11, 0.84))
        .border(u(1.))
        .border_color(palette.line)
        .child(corner(true, true))
        .child(corner(true, false))
        .child(corner(false, true))
        .child(corner(false, false))
        .child(header)
        .child(fields)
        .child(status_line(view, palette))
}

fn status_line(view: &View, palette: &Palette) -> impl IntoElement {
    let greeter = view.greeter;
    let s = &greeter.strings.neon;
    let error = color(ERROR, 1.0);
    let key = |action| {
        greeter
            .power_keys()
            .iter()
            .find(|&&(_, shown)| shown == action)
            .map_or(String::new(), |(key, _)| key.to_uppercase())
    };
    let (tag, tag_color, message): (&str, Hsla, String) = match greeter.status() {
        Status::Hint => ("READY", palette.accent, s.ready.into()),
        Status::Notice(notice) => ("READY", palette.accent, notice.into()),
        Status::Checking => ("AUTH", palette.accent, s.verifying.into()),
        Status::Denied(notice) => ("DENIED", error, notice.unwrap_or(s.denied).into()),
        Status::Failed(message) => ("ERROR", error, message.into()),
        Status::Confirm(action) => (
            "CONFIRM",
            palette.accent,
            format!("{}{}{}", s.confirm.0, key(action), s.confirm.1),
        ),
        Status::Welcome => (
            "GRANTED",
            palette.accent,
            format!("{}{} \u{2026}", s.starting, greeter.session_name()),
        ),
    };
    let line = div()
        .mt(u(24.))
        .pt(u(14.))
        .border_t(u(1.))
        .border_dashed()
        .border_color(palette.line)
        .flex()
        .items_center()
        .gap(u(16.))
        .child(
            text(format!("[ {tag} ]"), 14.)
                .font(MONO, FontWeight::BOLD)
                .color(tag_color)
                .tracking_em(0.04),
        )
        .child(
            div()
                .opacity(0.8)
                .child(text(message, 14.).tracking_em(0.04)),
        );
    match greeter.status() {
        Status::Checking => line.child(
            div()
                .flex_1()
                .h(u(6.))
                .border(u(1.))
                .border_color(palette.line)
                .child(
                    div()
                        .h_full()
                        .bg(palette.accent)
                        .shadow(vec![BoxShadow {
                            color: palette.glow,
                            offset: point(view.px(0.), view.px(0.)),
                            blur_radius: view.px(10.),
                            spread_radius: view.px(0.),
                            inset: false,
                        }])
                        .with_animation("fill", fill_animation(), |bar, t| bar.w(relative(t))),
                ),
        ),
        Status::Hint => line.child(
            div()
                .child(text("\u{258C}", 14.).color(palette.accent))
                .with_animation("blink", blink(1100), |cursor, t| {
                    cursor.opacity(blink_opacity(t))
                }),
        ),
        _ => line,
    }
}

fn bottom_left(view: &View, palette: &Palette) -> impl IntoElement {
    let greeter = view.greeter;
    let rect = view.monitor.rect;
    let display = match view.monitor.refresh {
        Some(rate) => format!(
            "display {}\u{00D7}{} @ {}Hz",
            rect.width,
            rect.height,
            rate.round()
        ),
        None => format!("display {}\u{00D7}{}", rect.width, rect.height),
    };
    let (dot, net) = if greeter.online {
        (palette.accent, "net online")
    } else {
        (color(ERROR, 1.0), "net offline")
    };
    let uptime = greeter
        .uptime
        .map(system::format_uptime)
        .unwrap_or_default();
    div()
        .absolute()
        .left(u(64.))
        .bottom(u(52.))
        .flex()
        .flex_col()
        .gap(u(6.))
        .opacity(0.6)
        .child(
            div()
                .flex()
                .gap(u(8.))
                .child(text("\u{25CF}", 12.).color(dot))
                .child(text(net, 12.).tracking_em(0.06)),
        )
        .child(text(display, 12.).tracking_em(0.06))
        .child(text(format!("uptime {uptime}"), 12.).tracking_em(0.06))
}

fn keys(view: &View, palette: &Palette) -> impl IntoElement {
    let greeter = view.greeter;
    let s = &greeter.strings.neon;
    let accent_bg = color(greeter.config.neon.accent.0, 0.12);
    let layout = greeter
        .layouts
        .get(greeter.layout)
        .cloned()
        .unwrap_or_else(|| "us".into());
    let key = |id: &'static str, name: &'static str, what: String| {
        div()
            .id(id)
            .flex()
            .items_center()
            .gap(u(8.))
            .px(u(14.))
            .py(u(10.))
            .border(u(1.))
            .border_color(palette.line)
            .bg(color(GROUND, 0.6))
            .cursor_pointer()
            .hover(|key| key.bg(accent_bg))
            .child(text(name, 13.).color(palette.accent))
            .child(text(what, 13.))
    };
    let armed = |action| greeter.status() == Status::Confirm(action);
    div()
        .absolute()
        .right(u(64.))
        .bottom(u(48.))
        .flex()
        .gap(u(12.))
        .when(!greeter.lock_screen, |keys| {
            keys.child(
                key("f2", "F2", s.session.into()).on_click(view.on(|greeter, cx| {
                    greeter.next_session();
                    cx.notify();
                })),
            )
        })
        .child(
            key("f3", "F3", format!("{} {layout}", s.layout)).on_click(view.on(|greeter, cx| {
                greeter.next_layout();
                cx.notify();
            })),
        )
        .child(
            key("f11", "F11", s.reboot.into())
                .when(armed(Action::Reboot), |key| key.bg(accent_bg))
                .on_click(view.on(|greeter, cx| greeter.power(Action::Reboot, cx))),
        )
        .child(
            key("f12", "F12", s.poweroff.into())
                .when(armed(Action::PowerOff), |key| key.bg(accent_bg))
                .on_click(view.on(|greeter, cx| greeter.power(Action::PowerOff, cx))),
        )
}

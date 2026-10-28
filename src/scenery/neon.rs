//! Neon's backgrounds. The skyline and the circuit board grow from the
//! mockup's seeds by its very steps, number for number, so they come out as
//! it draws them; the horizon is its sky, sun, mountains and floor.

use std::fmt::Write as _;

use super::random::Mulberry32;
use super::{Frame, MOCKUP, Motion, hex, over};
use crate::config::NeonBackground;

/// The page under every background.
const GROUND: u32 = 0x06070A;

pub fn svg(background: NeonBackground, accent: u32, frame: &Frame) -> (String, Motion) {
    match background {
        NeonBackground::Skyline => skyline_svg(accent, frame),
        NeonBackground::Circuit => circuit_svg(accent, frame),
        NeonBackground::Horizon => horizon_svg(accent, frame),
        NeonBackground::Grid => (
            format!(
                r#"{}<rect width="100%" height="100%" fill="{}"/></svg>"#,
                frame.svg(),
                hex(GROUND)
            ),
            Motion::Still,
        ),
    }
}

/// `n.toFixed(1)`, as the mockup writes its numbers.
fn fixed(n: f64) -> String {
    // Rust rounds an exact tie to even, JavaScript away from zero; a tie at
    // one decimal is an odd number of quarters.
    let quarters = n * 4.0;
    let n = if quarters.fract() == 0.0 && quarters % 2.0 != 0.0 {
        n + 0.05f64.copysign(n)
    } else {
        n
    };
    format!("{n:.1}")
}

/// One row of buildings, as SVG path data.
#[derive(Default)]
struct Buildings {
    outline: String,
    windows: String,
    antennas: String,
    tips: String,
}

struct Skyline {
    far: Buildings,
    mid: Buildings,
    near: Buildings,
    stars: String,
    signs: String,
    /// The antennas' tips, every row's, as the mockup draws them all.
    beacons: Vec<(f64, f64)>,
}

/// The mockup's `skyline`, across a frame `width` wide and down to the
/// mockup's 900, with the sky above the buildings `sky` deep.
fn skyline(random: &mut Mulberry32, width: f64, sky: f64) -> Skyline {
    let mut beacons = Vec::new();
    let mut row = |tops: (f64, f64), widths: (f64, f64), lit: f64| {
        let mut row = Buildings::default();
        let mut x = -20.0;
        while x < width + 20.0 {
            let w = random.between(widths.0, widths.1);
            let top = random.between(tops.0, tops.1);
            let _ = write!(
                row.outline,
                "M{} 900V{}h{}V900Z",
                fixed(x),
                fixed(top),
                fixed(w)
            );
            if random.next() < 0.35 {
                let step_width = w * random.between(0.3, 0.7);
                let step_x = x + (w - step_width) * random.next();
                let step_height = random.between(10.0, 46.0);
                let _ = write!(
                    row.outline,
                    "M{} {}v{}h{}v{}Z",
                    fixed(step_x),
                    fixed(top),
                    fixed(-step_height),
                    fixed(step_width),
                    fixed(step_height)
                );
            }
            if random.next() < 0.2 {
                let antenna_x = x + w * random.between(0.25, 0.75);
                let height = random.between(26.0, 70.0);
                let _ = write!(
                    row.antennas,
                    "M{} {}v{}",
                    fixed(antenna_x),
                    fixed(top),
                    fixed(-height)
                );
                let tip = (antenna_x - 2.5, top - height - 2.5);
                let _ = write!(row.tips, "M{} {}h5v5h-5Z", fixed(tip.0), fixed(tip.1));
                beacons.push(tip);
            }
            let mut window_y = top + 14.0;
            while window_y < 894.0 {
                let mut window_x = x + 7.0;
                while window_x < x + w - 9.0 {
                    if random.next() < lit {
                        let _ = write!(
                            row.windows,
                            "M{} {}h4v6h-4Z",
                            fixed(window_x),
                            fixed(window_y)
                        );
                    }
                    window_x += 11.0;
                }
                window_y += 14.0;
            }
            x += w + if random.next() < 0.3 {
                random.between(4.0, 14.0)
            } else {
                0.0
            };
        }
        row
    };
    let far = row((540.0, 690.0), (40.0, 110.0), 0.16);
    let mid = row((600.0, 770.0), (55.0, 140.0), 0.22);
    let near = row((720.0, 850.0), (80.0, 190.0), 0.1);
    let mut stars = String::new();
    for _ in 0..90 {
        let (x, y) = (random.next() * width, random.next() * sky);
        let _ = write!(stars, "M{} {}h1.6v1.6h-1.6Z", fixed(x), fixed(y));
    }
    let mut signs = String::new();
    for _ in 0..9 {
        let x = random.next() * (width - 20.0);
        let (y, height) = (random.between(740.0, 860.0), random.between(18.0, 42.0));
        let _ = write!(signs, "M{} {}h3v{}h-3Z", fixed(x), fixed(y), fixed(height));
    }
    Skyline {
        far,
        mid,
        near,
        stars,
        signs,
        beacons,
    }
}

/// A city at night under a hazy sky: three rows of buildings, the nearer
/// darker, their windows lit in the accent, glowing magenta signs and stars.
/// On a taller frame the city keeps to the bottom. The glow covers just the
/// band where the glowing windows and signs lie, not twice their width as in
/// the mockup, which only costs time.
fn skyline_svg(accent: u32, frame: &Frame) -> (String, Motion) {
    let down = frame.height - MOCKUP.1;
    let city = skyline(&mut Mulberry32::new(7), frame.width, 470.0 + down);
    let accent_hex = hex(accent);
    let haze = hex(over(accent, 0.09, GROUND));
    let svg = format!(
        r##"{open}
<defs>
<linearGradient id="sky" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#04050a"/><stop offset=".45" stop-color="#070a12"/><stop offset=".78" stop-color="{haze}"/><stop offset="1" stop-color="#05070b"/></linearGradient>
<filter id="neon-glow" filterUnits="userSpaceOnUse" x="0" y="580" width="{width}" height="330"><feGaussianBlur stdDeviation="2.2" result="b"/><feMerge><feMergeNode in="b"/><feMergeNode in="SourceGraphic"/></feMerge></filter>
</defs>
<rect width="{width}" height="{height}" fill="url(#sky)"/>
<path d="{stars}" fill="#ffffff" fill-opacity=".45"/>
<g transform="translate(0 {down})">
<path d="{far}" fill="#0c121b"/>
<path d="{far_windows}" fill="{accent_hex}" fill-opacity=".22"/>
<path d="{mid}" fill="#080c13"/>
<path d="{mid_windows}" fill="{accent_hex}" fill-opacity=".42" filter="url(#neon-glow)"/>
<path d="{mid_antennas}" stroke="#4a5866" stroke-width="1.5"/>
<path d="{near}" fill="#030509"/>
<path d="{near_windows}" fill="{accent_hex}" fill-opacity=".65" filter="url(#neon-glow)"/>
<path d="{signs}" fill="#ff4fd8" fill-opacity=".8" filter="url(#neon-glow)"/>
</g>
</svg>"##,
        open = frame.svg(),
        width = frame.width,
        height = frame.height,
        stars = city.stars,
        far = city.far.outline,
        far_windows = city.far.windows,
        mid = city.mid.outline,
        mid_windows = city.mid.windows,
        mid_antennas = city.mid.antennas,
        near = city.near.outline,
        near_windows = city.near.windows,
        signs = city.signs,
    );
    let beacons = city
        .beacons
        .into_iter()
        .map(|(x, y)| (x, y + down))
        .collect();
    (svg, Motion::Beacons(beacons))
}

/// Light running along a trace: a short dash every `PERIOD` of its length,
/// moving from its start to its end.
#[derive(Debug, Clone, PartialEq)]
pub struct Pulse {
    pub points: Vec<(f64, f64)>,
    /// How far into its run the pulse starts, in seconds.
    pub lead: f64,
}

impl Pulse {
    /// The length of a dash, and of a dash and the gap after it.
    pub const DASH: f64 = 18.0;
    pub const PERIOD: f64 = 438.0;
    /// How long a dash takes to run one period.
    pub const SECONDS: f64 = 3.6;
    pub const WIDTH: f64 = 2.4;
}

struct Board {
    traces: String,
    pads: String,
    vias: String,
    chips: String,
    pins: String,
    pulses: Vec<Pulse>,
}

/// The mockup's `circuit`, across a frame `width` by `height`, its traces
/// kept out of the middle where the login panel sits.
fn circuit(random: &mut Mulberry32, width: f64, height: f64) -> Board {
    const UNIT: i64 = 16;
    const DIRECTIONS: [(i64, i64); 8] = [
        (1, 0),
        (0, 1),
        (-1, 0),
        (0, -1),
        (1, 1),
        (1, -1),
        (-1, 1),
        (-1, -1),
    ];
    let (middle_x, middle_y) = (width / 2.0, height / 2.0);
    let blocked = |x: i64, y: i64| {
        let (x, y) = (x as f64, y as f64);
        x > middle_x - 460.0 && x < middle_x + 460.0 && y > middle_y - 300.0 && y < middle_y + 300.0
    };
    let outside = |x: i64, y: i64| {
        let (x, y) = (x as f64, y as f64);
        x < -20.0 || x > width + 20.0 || y < -20.0 || y > height + 20.0
    };
    let dot = |x: i64, y: i64, radius: f64| {
        format!(
            "M{} {y}a{radius} {radius} 0 1 0 {} 0a{radius} {radius} 0 1 0 {} 0",
            x as f64 - radius,
            2.0 * radius,
            -2.0 * radius
        )
    };
    // Math.round: halves round up.
    let on_grid = |at: f64| ((at / UNIT as f64 + 0.5).floor() as i64) * UNIT;
    let mut board = Board {
        traces: String::new(),
        pads: String::new(),
        vias: String::new(),
        chips: String::new(),
        pins: String::new(),
        pulses: Vec::new(),
    };
    let mut made = 0;
    while made < 58 {
        let mut x = on_grid(random.next() * width);
        let mut y = on_grid(random.next() * height);
        if blocked(x, y) {
            continue;
        }
        made += 1;
        let mut path = format!("M{x} {y}");
        let mut points = vec![(x, y)];
        board.vias.push_str(&dot(x, y, 2.6));
        let mut direction = (random.next() * 4.0).floor() as usize;
        let steps = 3 + (random.next() * 5.0).floor() as usize;
        for _ in 0..steps {
            let length = (2 + (random.next() * 9.0).floor() as i64) * UNIT;
            let (dx, dy) = DIRECTIONS[direction];
            let (next_x, next_y) = (x + dx * length, y + dy * length);
            if blocked(next_x, next_y) || outside(next_x, next_y) {
                break;
            }
            (x, y) = (next_x, next_y);
            let _ = write!(path, "L{x} {y}");
            points.push((x, y));
            direction = if random.next() < 0.65 {
                if direction < 4 {
                    (direction + if random.next() < 0.5 { 1 } else { 3 }) % 4
                } else {
                    (random.next() * 4.0).floor() as usize
                }
            } else {
                4 + (random.next() * 4.0).floor() as usize
            };
        }
        board.pads.push_str(&dot(x, y, 4.2));
        board.traces.push_str(&path);
        if made % 8 == 0 {
            let lead = (random.next() * Pulse::SECONDS * 100.0).round() / 100.0;
            board.pulses.push(Pulse {
                points: points
                    .into_iter()
                    .map(|(x, y)| (x as f64, y as f64))
                    .collect(),
                lead,
            });
        }
    }
    let chips = [
        (96.0, 96.0, 150.0, 96.0),
        (width - 270.0, 120.0, 170.0, 110.0),
        (120.0, height - 140.0, 130.0, 84.0),
        (width - 250.0, height - 130.0, 150.0, 90.0),
    ];
    for (x, y, w, h) in chips {
        let _ = write!(board.chips, "M{x} {y}h{w}v{h}h{}Z", -w);
        let mut pin = x + 12.0;
        while pin < x + w - 6.0 {
            let _ = write!(board.pins, "M{pin} {}v9M{pin} {}v9", y - 9.0, y + h);
            pin += 14.0;
        }
    }
    board
}

/// A circuit board: traces between vias and pads, four chips with their
/// pins, and light running along a few traces.
fn circuit_svg(accent: u32, frame: &Frame) -> (String, Motion) {
    let board = circuit(&mut Mulberry32::new(11), frame.width, frame.height);
    let accent_hex = hex(accent);
    let svg = format!(
        r##"{open}
<rect width="{width}" height="{height}" fill="{ground}"/>
<path d="{chips}" fill="#0a1016" stroke="{accent_hex}" stroke-opacity=".32" stroke-width="1.2"/>
<path d="{pins}" stroke="{accent_hex}" stroke-opacity=".3" stroke-width="2"/>
<path d="{traces}" fill="none" stroke="{accent_hex}" stroke-opacity=".2" stroke-width="1.6" stroke-linejoin="round"/>
<path d="{vias}" fill="{accent_hex}" fill-opacity=".35"/>
<path d="{pads}" fill="#06070a" stroke="{accent_hex}" stroke-opacity=".55" stroke-width="1.4"/>
</svg>"##,
        open = frame.svg(),
        width = frame.width,
        height = frame.height,
        ground = hex(GROUND),
        chips = board.chips,
        pins = board.pins,
        traces = board.traces,
        vias = board.vias,
        pads = board.pads,
    );
    // The mockup's glow filter takes its region from the trace's box: a
    // trace that runs straight has no area, and its pulse never shows.
    let pulses = board
        .pulses
        .into_iter()
        .filter(|pulse| {
            let spread = |axis: fn(&(f64, f64)) -> f64| {
                let values = pulse.points.iter().map(axis);
                values.clone().fold(f64::MIN, f64::max) - values.fold(f64::MAX, f64::min)
            };
            spread(|point| point.0) > 0.0 && spread(|point| point.1) > 0.0
        })
        .collect();
    (svg, Motion::Pulses(pulses))
}

/// A synthwave sunset: a sky deepening to a glow at the horizon, a striped
/// sun behind wireframe mountains, and a grid floor running toward the
/// viewer, darkened toward the horizon.
fn horizon_svg(accent: u32, frame: &Frame) -> (String, Motion) {
    let floor = Floor::under(frame);
    let (width, height) = (frame.width, frame.height);
    let accent_hex = hex(accent);
    let glow = hex(over(accent, 0.35, GROUND));
    let (sun_left, sun_top) = (width / 2.0 + 260.0, floor.horizon - 220.0);
    let stripes: String = [(0.0, 0.52), (0.56, 0.64), (0.69, 0.76), (0.82, 1.0)]
        .iter()
        .map(|(from, to)| {
            format!(
                r##"<rect x="{sun_left}" y="{}" width="300" height="{}" fill="#fff"/>"##,
                sun_top + 300.0 * from,
                300.0 * (to - from)
            )
        })
        .collect();
    let ridge = |points: &[(f64, f64)]| {
        let mut path = String::new();
        for (index, (x, y)) in points.iter().enumerate() {
            let _ = write!(
                path,
                "{}{} {}",
                if index == 0 { 'M' } else { 'L' },
                x * width / MOCKUP.0,
                y - 600.0 + floor.horizon
            );
        }
        path + "Z"
    };
    let west = ridge(&[
        (0.0, 600.0),
        (90.0, 548.0),
        (170.0, 572.0),
        (260.0, 506.0),
        (330.0, 552.0),
        (420.0, 488.0),
        (520.0, 560.0),
        (600.0, 530.0),
        (680.0, 574.0),
        (760.0, 600.0),
    ]);
    let east = ridge(&[
        (840.0, 600.0),
        (930.0, 562.0),
        (1000.0, 584.0),
        (1090.0, 520.0),
        (1170.0, 566.0),
        (1260.0, 532.0),
        (1350.0, 576.0),
        (1440.0, 540.0),
        (1520.0, 566.0),
        (1600.0, 548.0),
        (1600.0, 600.0),
    ]);
    let svg = format!(
        r##"{open}
<defs>
<linearGradient id="sky" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#04030a"/><stop offset=".38" stop-color="#0c0718"/><stop offset=".58" stop-color="#1b0c2c"/><stop offset=".666" stop-color="{glow}"/><stop offset=".667" stop-color="#06040c"/><stop offset="1" stop-color="#050309"/></linearGradient>
<linearGradient id="sun" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#ffd36e"/><stop offset=".45" stop-color="#ff7a59"/><stop offset="1" stop-color="#ff3d9a"/></linearGradient>
<linearGradient id="shade" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="{shade}" stop-opacity="{shade_alpha}"/><stop offset="{shade_reach}" stop-color="{shade}" stop-opacity="0"/></linearGradient>
<mask id="stripes" maskUnits="userSpaceOnUse" x="0" y="0" width="{width}" height="{height}">{stripes}</mask>
</defs>
<rect width="{width}" height="{height}" fill="url(#sky)"/>
<circle cx="{sun_x}" cy="{sun_y}" r="150" fill="url(#sun)" mask="url(#stripes)"/>
<g fill="#090514" stroke="{accent_hex}" stroke-opacity=".55" stroke-width="1.4"><path d="{west}"/><path d="{east}"/></g>
<path d="M0 {horizon}H{width}" stroke="{accent_hex}" stroke-opacity=".9" stroke-width="1.5"/>
<path d="{rails}" fill="{accent_hex}" fill-opacity="{line_alpha}"/>
<rect y="{horizon}" width="{width}" height="{depth}" fill="url(#shade)"/>
</svg>"##,
        open = frame.svg(),
        sun_x = sun_left + 150.0,
        sun_y = sun_top + 150.0,
        horizon = floor.horizon,
        depth = floor.depth,
        rails = floor.rails(width),
        line_alpha = Floor::LINE_ALPHA,
        shade = hex(Floor::SHADE.0),
        shade_alpha = Floor::SHADE.1,
        shade_reach = Floor::SHADE.2,
    );
    (svg, Motion::Floor(floor))
}

/// The horizon's floor: a grid of 64-pixel cells on a plane hinged at the
/// horizon and tilted 72° toward the viewer, who looks from 260 pixels away
/// at the horizon's height. The mockup's CSS puts the eye at the middle of
/// the floor instead, which turns the plane up out of sight; at the horizon
/// it shows the floor the CSS describes.
#[derive(Debug, Clone, PartialEq)]
pub struct Floor {
    /// Where the floor meets the sky, down the frame.
    pub horizon: f64,
    /// How far the floor reaches below the horizon.
    pub depth: f64,
}

/// A crosswise line of the floor on screen, and how much the shade toward
/// the horizon darkens it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Band {
    pub top: f64,
    pub bottom: f64,
    pub shade: f64,
}

impl Floor {
    /// How long the grid takes to run one cell toward the viewer.
    pub const SECONDS: f64 = 1.1;
    pub const LINE_ALPHA: f64 = 0.55;
    /// The shade's colour, its opacity at the horizon and how far down the
    /// floor it fades out.
    pub const SHADE: (u32, f64, f64) = (0x06040C, 0.9, 0.45);
    const CELL: f64 = 64.0;
    const LINE: f64 = 2.0;
    const TILT: f64 = 72.0;
    const EYE: f64 = 260.0;
    /// The plane's length down from the hinge, as the CSS sizes it.
    const LENGTH: f64 = 900.0;

    fn under(frame: &Frame) -> Self {
        let horizon = frame.height * 2.0 / 3.0;
        Self {
            horizon,
            depth: frame.height - horizon,
        }
    }

    /// How much nearer than the hinge a point `along` the plane comes:
    /// lengths there look this many times longer.
    fn nearness(along: f64) -> f64 {
        1.0 / (1.0 - along * Self::TILT.to_radians().sin() / Self::EYE)
    }

    /// How far below the horizon a point `along` the plane shows, if it is
    /// still in front of the viewer.
    fn drop(along: f64) -> Option<f64> {
        let nearness = Self::nearness(along);
        (nearness > 0.0).then(|| along * Self::TILT.to_radians().cos() * nearness)
    }

    /// How far along the plane its lowest visible point lies.
    fn reach(&self) -> f64 {
        let (sin, cos) = Self::TILT.to_radians().sin_cos();
        (self.depth / (cos + self.depth * sin / Self::EYE)).min(Self::LENGTH)
    }

    /// The lengthwise lines, which do not move: each widens from the
    /// horizon toward the viewer, as SVG path data in the frame.
    fn rails(&self, width: f64) -> String {
        let middle = width / 2.0;
        let spread = Self::nearness(self.reach());
        let mut path = String::new();
        // The plane is three frames wide, centred, its lines from its left edge.
        let mut left = -1.5 * width;
        while left < 1.5 * width {
            let right = left + Self::LINE;
            let bottom = (middle + left * spread, middle + right * spread);
            let seen =
                (middle + right).max(bottom.1) > 0.0 && (middle + left).min(bottom.0) < width;
            if seen {
                let _ = write!(
                    path,
                    "M{} {}H{}L{} {}H{}Z",
                    middle + left,
                    self.horizon,
                    middle + right,
                    bottom.1,
                    self.horizon + self.depth,
                    bottom.0
                );
            }
            left += Self::CELL;
        }
        path
    }

    /// The crosswise lines `run` of the way through a cell's run toward
    /// the viewer.
    pub fn bands(&self, run: f64) -> Vec<Band> {
        let (_, shade_alpha, shade_reach) = Self::SHADE;
        let mut bands = Vec::new();
        let mut along = Self::CELL * run;
        while let Some(top) = Self::drop(along).filter(|&top| top < self.depth) {
            let bottom =
                Self::drop(along + Self::LINE).map_or(self.depth, |bottom| bottom.min(self.depth));
            let middle = (top + bottom) / 2.0;
            bands.push(Band {
                top: self.horizon + top,
                bottom: self.horizon + bottom,
                shade: shade_alpha * (1.0 - middle / (shade_reach * self.depth)).max(0.0),
            });
            along += Self::CELL;
        }
        bands
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fingerprint(text: &str) -> u64 {
        super::super::stored::fingerprint(&[text.as_bytes()])
    }

    #[test]
    fn the_skyline_is_the_mockups() {
        let city = skyline(&mut Mulberry32::new(7), MOCKUP.0, 470.0);
        let rows: String = [&city.far, &city.mid, &city.near]
            .iter()
            .map(|row| [&*row.outline, &row.windows, &row.antennas, &row.tips].concat())
            .collect();
        assert_eq!(
            fingerprint(&[rows, city.stars, city.signs].concat()),
            0x6808_ED92_2B9C_6E18
        );
        assert_eq!(city.beacons.len(), 8);
    }

    #[test]
    fn the_circuit_is_the_mockups() {
        let board = circuit(&mut Mulberry32::new(11), MOCKUP.0, MOCKUP.1);
        let drawn = [
            board.traces,
            board.pads,
            board.vias,
            board.chips,
            board.pins,
        ]
        .concat();
        assert_eq!(fingerprint(&drawn), 0x381F_887B_2350_31DF);
        let leads: Vec<f64> = board.pulses.iter().map(|pulse| pulse.lead).collect();
        assert_eq!(leads, [2.06, 3.54, 0.36, 1.86, 3.08, 2.44, 3.53]);
        assert_eq!(
            board.pulses[1].points,
            [
                (320.0, 128.0),
                (208.0, 128.0),
                (112.0, 32.0),
                (208.0, 128.0)
            ]
        );
    }

    #[test]
    fn straight_traces_show_no_pulse() {
        let (_, motion) = circuit_svg(0x45F0FF, &Frame::new((1600, 900), 1.0));
        let Motion::Pulses(pulses) = motion else {
            panic!("the circuit moves by its pulses");
        };
        assert_eq!(pulses.len(), 5);
    }

    #[test]
    fn the_floor_runs_from_the_horizon_and_spreads_toward_the_viewer() {
        let floor = Floor::under(&Frame::new((1600, 900), 1.0));
        assert_eq!((floor.horizon, floor.depth), (600.0, 300.0));
        let bands = floor.bands(0.0);
        assert_eq!(bands[0].top, 600.0);
        assert!(
            bands
                .windows(3)
                .all(|three| { three[2].top - three[1].top > three[1].top - three[0].top })
        );
        assert!(bands.iter().all(|band| band.bottom <= 900.0));
        assert!(bands[0].shade > 0.85 && bands.last().unwrap().shade == 0.0);
        // A whole run brings the next line to where the first one was.
        let later = floor.bands(1.0);
        assert!((later[0].top - bands[1].top).abs() < 1e-9);
    }

    #[test]
    fn numbers_are_written_as_javascript_writes_them() {
        assert_eq!(fixed(0.25), "0.3");
        assert_eq!(fixed(-2.75), "-2.8");
        assert_eq!(fixed(549.26), "549.3");
        assert_eq!(fixed(-20.0), "-20.0");
    }
}

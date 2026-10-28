//! Paper's backgrounds: the desk, drawn as the mockup's SVG, and the ledger
//! and drafting sheets, whose CSS rules are laid on whole pixels as a
//! browser lays them, so that they stay crisp.

use std::fmt::Write as _;

use super::{Frame, hex};
use crate::config::PaperBackground;

/// The ground under every sheet, by day and by night.
pub const GROUND: (u32, u32) = (0xF3EEE4, 0x1D1B18);

pub fn svg(background: PaperBackground, night: bool, frame: &Frame) -> String {
    match background {
        PaperBackground::Desk => desk(night, frame),
        PaperBackground::Ledger => ledger(night, frame),
        PaperBackground::Drafting => drafting(night, frame),
        PaperBackground::Plain => sheet(night, frame).finish(),
    }
}

/// A flat lay seen from above: a light oak desk (walnut by night) under a
/// sheet of paper that holds the clock and the form, with a pencil, a cup of
/// coffee, a fountain pen, paper clips, a sticky note and a succulent. On a
/// wider or taller window the sheet grows and the things on the desk keep to
/// its right side.
fn desk(night: bool, frame: &Frame) -> String {
    let (wood, grain, grain_opacity) = if night {
        ("#3a2c22", "#000000", 0.18)
    } else {
        ("#d8c6ab", "#6e4a28", 0.07)
    };
    let (width, height) = (frame.width, frame.height);
    let (wider, taller) = frame.growth();
    let (sheet_width, sheet_height) = (990.0 + wider, 1060.0 + taller);
    let (sheet_top, half) = (-80.0 - taller / 2.0, taller / 2.0);
    let (grain_x, grain_y) = (width / 1600.0, height / 900.0);
    let ring_x = 938.0 + wider;
    let open = frame.svg();
    format!(
        r##"{open}
<defs>
<filter id="lift" x="-20%" y="-20%" width="140%" height="140%"><feDropShadow dx="0" dy="10" stdDeviation="12" flood-color="#2e1f12" flood-opacity=".26"/></filter>
<filter id="lift-sm" x="-30%" y="-30%" width="160%" height="160%"><feDropShadow dx="1" dy="4" stdDeviation="3.5" flood-color="#2e1f12" flood-opacity=".3"/></filter>
<radialGradient id="coffee" cx=".42" cy=".4" r=".62"><stop offset="0" stop-color="#94623d"/><stop offset=".5" stop-color="#623c22"/><stop offset="1" stop-color="#3b2314"/></radialGradient>
<radialGradient id="cupInside" cx=".5" cy=".5" r=".5"><stop offset=".78" stop-color="#ffffff"/><stop offset="1" stop-color="#e9e2d7"/></radialGradient>
<linearGradient id="window" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#fffaf0" stop-opacity=".26"/><stop offset=".5" stop-color="#fffaf0" stop-opacity="0"/></linearGradient>
<linearGradient id="penBody" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#2f4a48"/><stop offset=".45" stop-color="#1c2e2d"/><stop offset="1" stop-color="#111b1a"/></linearGradient>
</defs>
<rect width="{width}" height="{height}" fill="{wood}"/>
<g fill="none" stroke="{grain}" stroke-opacity="{grain_opacity}" stroke-width="2" transform="scale({grain_x} {grain_y})">
<path d="M0 62 C300 48 620 80 980 58 S1400 44 1600 70"/>
<path d="M0 150 C260 170 640 132 1000 160 S1380 176 1600 146"/>
<path d="M0 244 C380 228 700 262 1040 240 S1420 228 1600 252"/>
<path d="M0 338 C300 356 660 322 1020 346 S1360 360 1600 330"/>
<path d="M0 430 C340 414 720 448 1060 424 S1400 412 1600 440"/>
<path d="M0 522 C280 540 640 506 1000 532 S1380 548 1600 516"/>
<path d="M0 616 C360 600 700 634 1040 610 S1420 598 1600 624"/>
<path d="M0 708 C300 726 680 690 1020 716 S1380 730 1600 702"/>
<path d="M0 800 C340 784 720 818 1060 794 S1400 782 1600 808"/>
<path d="M0 888 C280 904 660 872 1000 894 S1380 908 1600 882"/>
</g>
<g transform="translate(0 {half}) rotate(-1.1 560 450)">
<rect x="58" y="{sheet_top}" width="{sheet_width}" height="{sheet_height}" fill="#f7f2e7" filter="url(#lift)"/>
<circle cx="{ring_x}" cy="176" r="58" fill="none" stroke="#8a5b33" stroke-opacity=".1" stroke-width="5" stroke-dasharray="250 40 60 14"/>
</g>
<g transform="translate({wider} {half})">
<g transform="translate(1078 300) rotate(-7)" filter="url(#lift-sm)">
<polygon points="-10,0 10,0 0,-50" fill="#ecd3a6"/>
<polygon points="-3.4,-33 3.4,-33 0,-50" fill="#3b3a38"/>
<rect x="-10" y="0" width="20" height="320" fill="#e9b646"/>
<rect x="-3" y="0" width="6" height="320" fill="#f2c869"/>
<rect x="-10" y="320" width="20" height="24" fill="#c4c2bc"/>
<rect x="-10" y="344" width="20" height="26" rx="5" fill="#e3908a"/>
</g>
<g transform="translate(1340 236)">
<circle r="128" fill="#f7f3ec" filter="url(#lift)"/>
<circle r="98" fill="none" stroke="#e8e0d4" stroke-width="2"/>
<g transform="rotate(32)"><rect x="70" y="-15" width="58" height="30" rx="15" fill="#fbfaf7" stroke="#e6ddd0" stroke-width="2"/></g>
<circle r="80" fill="url(#cupInside)" filter="url(#lift-sm)"/>
<circle r="64" fill="url(#coffee)"/>
<ellipse cx="-18" cy="-20" rx="20" ry="11" fill="#c08a5c" fill-opacity=".35" transform="rotate(-30 -18 -20)"/>
<g transform="rotate(-128)"><ellipse cx="0" cy="104" rx="11" ry="16" fill="#cfd3d6"/><rect x="-3" y="116" width="6" height="64" rx="3" fill="#cfd3d6"/></g>
</g>
<g transform="translate(1166 598) rotate(27)" filter="url(#lift-sm)">
<polygon points="-58,0 -26,-7 -26,7" fill="#d6b36a"/>
<line x1="-52" y1="0" x2="-30" y2="0" stroke="#7a6232" stroke-width="1.4"/>
<rect x="-28" y="-9" width="38" height="18" rx="7" fill="#151d1c"/>
<rect x="6" y="-12" width="200" height="24" rx="12" fill="url(#penBody)"/>
<rect x="196" y="-13" width="9" height="26" fill="#c9a55b"/>
<rect x="204" y="-13" width="130" height="26" rx="13" fill="url(#penBody)"/>
<rect x="222" y="-18" width="98" height="6" rx="3" fill="#c9a55b"/>
</g>
<g fill="none" stroke="#9aa4aa" stroke-width="3.2" stroke-linecap="round" filter="url(#lift-sm)">
<path transform="translate(1150 102) rotate(-14)" d="M8 0 H54 A11 11 0 0 1 54 22 H4 A8 8 0 0 1 4 6 H46"/>
<path transform="translate(1214 150) rotate(22)" d="M8 0 H54 A11 11 0 0 1 54 22 H4 A8 8 0 0 1 4 6 H46"/>
</g>
<g transform="translate(1122 712) rotate(-6)" filter="url(#lift-sm)">
<rect width="178" height="166" fill="#f4d47e"/>
<rect width="178" height="24" fill="#ecc866"/>
<g fill="none" stroke="#6a5424" stroke-opacity=".45" stroke-width="2.2" stroke-linecap="round">
<path d="M22 62 q14 -8 28 0 t28 0 t28 0 t26 -2"/>
<path d="M22 96 q14 -8 28 0 t28 0 t30 0"/>
<path d="M22 130 q14 -8 28 0 t26 0"/>
</g>
</g>
<g transform="translate(1506 842)">
<circle r="104" fill="#c27a50" filter="url(#lift)"/>
<circle r="88" fill="#a5603d"/>
<circle r="82" fill="#4a3a2d"/>
<g fill="#7d9f69">
<ellipse rx="17" ry="50" cy="-40"/>
<ellipse rx="17" ry="50" cy="-40" transform="rotate(40)"/>
<ellipse rx="17" ry="50" cy="-40" transform="rotate(80)"/>
<ellipse rx="17" ry="50" cy="-40" transform="rotate(120)"/>
<ellipse rx="17" ry="50" cy="-40" transform="rotate(160)"/>
<ellipse rx="17" ry="50" cy="-40" transform="rotate(200)"/>
<ellipse rx="17" ry="50" cy="-40" transform="rotate(240)"/>
<ellipse rx="17" ry="50" cy="-40" transform="rotate(280)"/>
<ellipse rx="17" ry="50" cy="-40" transform="rotate(320)"/>
</g>
<g fill="#97b884">
<ellipse rx="12" ry="32" cy="-24" transform="rotate(20)"/>
<ellipse rx="12" ry="32" cy="-24" transform="rotate(92)"/>
<ellipse rx="12" ry="32" cy="-24" transform="rotate(164)"/>
<ellipse rx="12" ry="32" cy="-24" transform="rotate(236)"/>
<ellipse rx="12" ry="32" cy="-24" transform="rotate(308)"/>
</g>
<circle r="9" fill="#b3cf9f"/>
</g>
</g>
<rect width="{width}" height="{height}" fill="url(#window)"/>
</svg>"##
    )
}

/// Ledger paper: a blue rule every 36 pixels from the top of the writing, a
/// double rule under the header and a red double margin.
fn ledger(night: bool, frame: &Frame) -> String {
    let (rule, header, margin) = if night {
        ((0x8CAAC8, 0.14), (0x8CAAC8, 0.3), (0xE07864, 0.4))
    } else {
        ((0x4A6F96, 0.17), (0x4A6F96, 0.32), (0xB84A40, 0.42))
    };
    let mut sheet = sheet(night, frame);
    let rules: Vec<f64> = (0..)
        .map(|n| 152.0 + 35.0 + 36.0 * f64::from(n))
        .take_while(|&y| y < frame.height)
        .collect();
    sheet.rows(&rules, 1.0, rule);
    sheet.double_rule(Axis::Rows, (112.0, 118.0), header);
    sheet.double_rule(Axis::Columns, (124.0, 130.0), margin);
    sheet.finish()
}

/// A drafting sheet: a fine grid with a coarser one over it, and a double
/// frame. Its zone letters and title block are the window's to write.
fn drafting(night: bool, frame: &Frame) -> String {
    let ink = if night { 0xECE5D8 } else { 0x786046 };
    let border = if night { 0xECE5D8 } else { 0x503E2D };
    let (minor, major) = if night { (0.04, 0.09) } else { (0.06, 0.13) };
    let (frame_alpha, thin_alpha) = if night { (0.5, 0.22) } else { (0.55, 0.25) };
    let lines = |from: f64, step: f64, end: f64| -> Vec<f64> {
        (0..)
            .map(|n| from + step * f64::from(n))
            .take_while(|&at| at < end)
            .collect()
    };
    let mut sheet = sheet(night, frame);
    sheet.columns(&lines(8.0, 16.0, frame.width), 1.0, (ink, minor));
    sheet.rows(&lines(8.0, 16.0, frame.height), 1.0, (ink, minor));
    sheet.columns(&lines(40.0, 80.0, frame.width), 1.0, (ink, major));
    sheet.rows(&lines(40.0, 80.0, frame.height), 1.0, (ink, major));
    sheet.outline(32.0, 1.5, (border, frame_alpha));
    sheet.outline(40.0, 1.0, (border, thin_alpha));
    sheet.finish()
}

fn sheet(night: bool, frame: &Frame) -> Sheet {
    let (width, height) = frame.pixels;
    let mut svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}">"#
    );
    let ground = if night { GROUND.1 } else { GROUND.0 };
    let _ = write!(
        svg,
        r#"<rect width="{width}" height="{height}" fill="{}"/>"#,
        hex(ground)
    );
    Sheet { svg, frame: *frame }
}

#[derive(Clone, Copy)]
enum Axis {
    Rows,
    Columns,
}

/// A sheet ruled in CSS pixels and drawn in the window's own, with each
/// line on whole pixels.
struct Sheet {
    svg: String,
    frame: Frame,
}

impl Sheet {
    /// The pixels a CSS band from `start` to `end` covers, as a browser
    /// samples a gradient's hard stops: those whose centres lie in it, and
    /// at least one.
    fn band(&self, start: f64, end: f64) -> (f64, f64) {
        let scale = self.frame.scale;
        let first = (start * scale - 0.5).ceil();
        (first, (end * scale - 0.5).ceil().max(first + 1.0))
    }

    /// A CSS border's width in whole pixels, at least one.
    fn border_width(&self, width: f64) -> f64 {
        (width * self.frame.scale).floor().max(1.0)
    }

    fn edge(&self, at: f64) -> f64 {
        (at * self.frame.scale).round()
    }

    fn rows(&mut self, tops: &[f64], height: f64, color: (u32, f64)) {
        let width = self.frame.pixels.0;
        let mut path = String::new();
        for &top in tops {
            let (start, end) = self.band(top, top + height);
            let _ = write!(path, "M0 {start}h{width}V{end}h-{width}Z");
        }
        self.fill(&path, color);
    }

    fn columns(&mut self, lefts: &[f64], width: f64, color: (u32, f64)) {
        let height = self.frame.pixels.1;
        let mut path = String::new();
        for &left in lefts {
            let (start, end) = self.band(left, left + width);
            let _ = write!(path, "M{start} 0v{height}H{end}v-{height}Z");
        }
        self.fill(&path, color);
    }

    /// A box with one-pixel CSS borders on two opposite sides, from `span.0`
    /// to `span.1` across the sheet, running its full length.
    fn double_rule(&mut self, axis: Axis, span: (f64, f64), color: (u32, f64)) {
        let thickness = self.border_width(1.0);
        let (start, end) = (self.edge(span.0), self.edge(span.1));
        let (width, height) = self.frame.pixels;
        let mut path = String::new();
        for at in [start, end - thickness] {
            let _ = match axis {
                Axis::Rows => write!(path, "M0 {at}h{width}v{thickness}h-{width}Z"),
                Axis::Columns => write!(path, "M{at} 0v{height}h{thickness}v-{height}Z"),
            };
        }
        self.fill(&path, color);
    }

    /// A border of CSS `width` around a box `inset` from every side.
    fn outline(&mut self, inset: f64, width: f64, color: (u32, f64)) {
        let thickness = self.border_width(width);
        let (left, top) = (self.edge(inset), self.edge(inset));
        let right = self.edge(self.frame.width - inset);
        let bottom = self.edge(self.frame.height - inset);
        let path = format!(
            "M{left} {top}H{right}V{bottom}H{left}Z M{} {}V{}H{}V{}Z",
            left + thickness,
            top + thickness,
            bottom - thickness,
            right - thickness,
            top + thickness
        );
        let _ = write!(
            self.svg,
            r#"<path d="{path}" fill="{}" fill-opacity="{}" fill-rule="evenodd"/>"#,
            hex(color.0),
            color.1
        );
    }

    fn fill(&mut self, path: &str, (color, alpha): (u32, f64)) {
        let _ = write!(
            self.svg,
            r#"<path d="{path}" fill="{}" fill-opacity="{alpha}"/>"#,
            hex(color)
        );
    }

    fn finish(mut self) -> String {
        self.svg.push_str("</svg>");
        self.svg
    }
}

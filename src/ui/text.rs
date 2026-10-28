//! Text set the way the mockups set it: with CSS letter-spacing, which
//! GPUI's text has no notion of, and with soft text shadows, drawn as faint
//! copies of the glyphs around them.

use std::sync::Arc;

use gpui_kit::{
    App, Bounds, Element, ElementId, FontFallbacks, FontFeatures, FontStyle, FontWeight,
    GlobalElementId, Hsla, InspectorElementId, IntoElement, LayoutId, Pixels, Point, ShapedLine,
    SharedString, Style, TextRun, Window, font, point, px, size,
};
use smallvec::SmallVec;

use super::theme::DESIGN_REM;

/// Families that draw what the bundled fonts lack, Chinese above all,
/// after a theme's own.
fn fallbacks(first: &[Arc<str>]) -> FontFallbacks {
    let greeters = ["Noto Sans CJK SC", "Noto Sans SC", "DejaVu Sans"];
    FontFallbacks::from_fonts(
        first
            .iter()
            .map(|family| family.to_string())
            .chain(greeters.map(String::from))
            .collect(),
    )
}

pub fn label(text: impl Into<SharedString>) -> Label {
    Label {
        text: text.into(),
        family: "Inter".into(),
        weight: FontWeight::NORMAL,
        italic: false,
        features: FontFeatures::default(),
        fallback: None,
        size: 16.0,
        color: gpui_kit::white(),
        tracking: Tracking::Px(0.0),
        line_height: None,
        shadows: SmallVec::new(),
    }
}

/// A single line of text. Every length is in the mockups' CSS pixels,
/// which follow the window's rem size.
pub struct Label {
    text: SharedString,
    family: SharedString,
    weight: FontWeight,
    italic: bool,
    features: FontFeatures,
    fallback: Option<Arc<[Arc<str>]>>,
    size: f32,
    color: Hsla,
    tracking: Tracking,
    line_height: Option<f32>,
    shadows: SmallVec<[Shadow; 2]>,
}

#[derive(Clone, Copy)]
enum Tracking {
    Px(f32),
    Em(f32),
}

#[derive(Clone, Copy)]
struct Shadow {
    offset: (f32, f32),
    blur: f32,
    color: Hsla,
}

impl Label {
    pub fn font(mut self, family: &'static str, weight: FontWeight) -> Self {
        self.family = family.into();
        self.weight = weight;
        self
    }

    /// A family named at run time, such as a theme's own font.
    pub fn family(mut self, family: impl Into<SharedString>, weight: FontWeight) -> Self {
        self.family = family.into();
        self.weight = weight;
        self
    }

    pub fn italic(mut self, italic: bool) -> Self {
        self.italic = italic;
        self
    }

    /// OpenType features, such as tabular figures.
    pub fn features(mut self, features: FontFeatures) -> Self {
        self.features = features;
        self
    }

    /// Families that draw what the font lacks, ahead of the greeter's own.
    pub fn fallback(mut self, families: Arc<[Arc<str>]>) -> Self {
        self.fallback = Some(families);
        self
    }

    pub fn size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }

    pub fn color(mut self, color: impl Into<Hsla>) -> Self {
        self.color = color.into();
        self
    }

    /// CSS `letter-spacing` in pixels.
    pub fn tracking(mut self, px: f32) -> Self {
        self.tracking = Tracking::Px(px);
        self
    }

    /// CSS `letter-spacing` in ems.
    pub fn tracking_em(mut self, em: f32) -> Self {
        self.tracking = Tracking::Em(em);
        self
    }

    /// CSS `line-height` as a multiple of the size; the font's own height
    /// otherwise.
    pub fn line_height(mut self, factor: f32) -> Self {
        self.line_height = Some(factor);
        self
    }

    /// CSS `text-shadow`.
    pub fn shadow(mut self, x: f32, y: f32, blur: f32, color: impl Into<Hsla>) -> Self {
        self.shadows.push(Shadow {
            offset: (x, y),
            blur,
            color: color.into(),
        });
        self
    }

    fn tracking_px(&self) -> f32 {
        match self.tracking {
            Tracking::Px(px) => px,
            Tracking::Em(em) => em * self.size,
        }
    }
}

pub struct Layout {
    line: ShapedLine,
    /// Window pixels per CSS pixel.
    unit: f32,
}

impl IntoElement for Label {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl Element for Label {
    type RequestLayoutState = Layout;
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Layout) {
        let unit = window.rem_size().as_f32() / DESIGN_REM;
        let font_size = px(self.size * unit);
        let run = TextRun {
            len: self.text.len(),
            font: gpui_kit::Font {
                weight: self.weight,
                style: if self.italic {
                    FontStyle::Italic
                } else {
                    FontStyle::Normal
                },
                features: self.features.clone(),
                fallbacks: Some(fallbacks(self.fallback.as_deref().unwrap_or_default())),
                ..font(self.family.clone())
            },
            color: self.color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let line = window
            .text_system()
            .shape_line(self.text.clone(), font_size, &[run], None);
        let chars = self.text.chars().count() as f32;
        let width = line.width + px(self.tracking_px() * unit * chars);
        let height = match self.line_height {
            Some(factor) => font_size * factor,
            None => line.ascent + line.descent,
        };
        let style = Style {
            size: size(width.into(), height.into()),
            flex_shrink: 0.0,
            ..Style::default()
        };
        (window.request_layout(style, [], cx), Layout { line, unit })
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut Layout,
        _: &mut Window,
        _: &mut App,
    ) {
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        layout: &mut Layout,
        _: &mut (),
        window: &mut Window,
        _: &mut App,
    ) {
        let Layout { line, unit } = layout;
        let unit = *unit;
        let font_size = line.font_size;
        let baseline =
            bounds.origin.y + (bounds.size.height - line.ascent - line.descent) / 2.0 + line.ascent;
        let tracking = self.tracking_px() * unit;
        let glyphs: Vec<(Point<Pixels>, _, _)> = line
            .runs
            .iter()
            .flat_map(|run| run.glyphs.iter().map(move |glyph| (run.font_id, glyph)))
            .map(|(font_id, glyph)| {
                let before = self.text[..glyph.index].chars().count() as f32;
                let origin = point(
                    bounds.origin.x + glyph.position.x + px(tracking * before),
                    baseline,
                );
                (origin, font_id, glyph.id)
            })
            .collect();

        for shadow in &self.shadows {
            let offset = point(px(shadow.offset.0 * unit), px(shadow.offset.1 * unit));
            for (spread, weight) in blur_samples(shadow.blur * unit) {
                let color = shadow.color.opacity(weight);
                for (origin, font_id, glyph_id) in &glyphs {
                    let _ = window.paint_glyph(
                        *origin + offset + spread,
                        *font_id,
                        *glyph_id,
                        font_size,
                        color,
                    );
                }
            }
        }
        for (origin, font_id, glyph_id) in &glyphs {
            let _ = window.paint_glyph(*origin, *font_id, *glyph_id, font_size, self.color);
        }
    }
}

/// Offsets and weights that spread copies of a glyph like a Gaussian blur
/// of CSS radius `blur` (a standard deviation of half of it), weighing in
/// at about the shadow's full opacity in all.
fn blur_samples(blur: f32) -> SmallVec<[(Point<Pixels>, f32); 25]> {
    let mut samples = SmallVec::new();
    if blur < 1.0 {
        samples.push((point(px(0.0), px(0.0)), 1.0));
        return samples;
    }
    let sigma = blur / 2.0;
    samples.push((point(px(0.0), px(0.0)), 1.0));
    for (ring, radius) in [(0, 0.6 * sigma), (1, 1.3 * sigma), (2, 2.1 * sigma)] {
        let weight = (-(radius * radius) / (2.0 * sigma * sigma)).exp();
        for step in 0..8 {
            let angle = std::f32::consts::TAU * (step as f32 + ring as f32 * 0.33) / 8.0;
            samples.push((
                point(px(radius * angle.cos()), px(radius * angle.sin())),
                weight,
            ));
        }
    }
    let total: f32 = samples.iter().map(|(_, weight)| weight).sum();
    for (_, weight) in &mut samples {
        *weight /= total;
    }
    samples
}

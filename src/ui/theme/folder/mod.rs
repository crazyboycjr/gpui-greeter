//! Theme folders on screen: each layer of a `themes::Folder` built into
//! GPUI elements for one monitor's window, with the look its state and its
//! conditions give it at this moment.

mod look;
mod paint;
pub mod pictures;

use std::collections::HashSet;
use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui_kit::prelude::*;
use gpui_kit::{
    AnimationExt as _, AnyElement, BoxShadow, Corners, Div, Entity, FontFeatures, FontWeight, Hsla,
    MouseButton, Rems, Rgba, Transformation, div, point, radians, relative, size, svg,
};

pub use look::{Pace, Timeline};
pub use pictures::{MonitorPictures, Pictures};

use super::{DESIGN_FRAME, View, u};
use crate::config::Language;
use crate::i18n;
use crate::system;
use crate::themes::{
    Action, Align, Anchor, Binding, Caret, Case, Color, FRAME, Fill, Fit, Flow, Folder, Justify,
    Kind, Layer, Length, Picture, Props, Repeat, State, TextAlign, Tiles, format_time, tiles,
    uptime,
};
use crate::ui::model::{Phase, Status};
use crate::ui::motion::fade;
use crate::ui::screen::Screen;
use crate::ui::text::{Label, label};
use crate::wallpaper::Backdrop;
use look::{Flags, Look};

/// The CSS pixels of the built-in looks' frame that make one frame pixel.
const CSS: f32 = DESIGN_FRAME.0 / FRAME.0;
/// Text is this size, in frame pixels, unless a theme says otherwise.
const TEXT_SIZE: f32 = 38.4;

/// What a window needs to draw a theme folder.
pub struct Frame<'a> {
    pub folder: &'a Folder,
    pub pictures: &'a Pictures,
    pub drawn: &'a MonitorPictures,
    pub timeline: &'a Timeline,
    pub hovered: &'a HashSet<u64>,
    pub screen: Entity<Screen>,
    pub pace: &'a Pace,
}

/// The key under which GPUI's asset source holds a theme's SVG.
pub fn asset_key(theme: &str, path: &str) -> String {
    format!("theme/{theme}/{path}")
}

pub fn render(view: &View, frame: &Frame) -> AnyElement {
    let greeter = view.greeter;
    let builder = Builder {
        view,
        frame,
        now: Instant::now(),
        utc_offset: greeter.now.offset().seconds(),
        language: greeter.config.language,
    };
    let statics = frame
        .timeline
        .changes()
        .iter()
        .map(|_| frame.folder.text.clone())
        .collect();
    let ctx = Ctx {
        statics: Rc::new(statics),
        zoom: 1.0,
        flow: Flow::Stack,
        item: None,
        focused: false,
        owner: None,
        delay: Duration::ZERO,
    };
    let mut root = div()
        .size_full()
        .relative()
        .overflow_hidden()
        .bg(hsla(frame.folder.background));
    if greeter.resting {
        root = root.on_mouse_down(
            MouseButton::Left,
            view.on(|greeter, cx| {
                greeter.wake();
                cx.notify();
            }),
        );
    }
    for &index in &frame.folder.roots {
        let shows = match frame.folder.layers[index].monitors {
            crate::themes::Monitors::Form => view.has_form,
            crate::themes::Monitors::All => true,
            crate::themes::Monitors::Others => !view.has_form,
        };
        if shows {
            root = root.children(builder.layer(index, &ctx));
        }
    }
    root.into_any_element()
}

fn hsla(color: Color) -> Hsla {
    Rgba {
        r: color.r,
        g: color.g,
        b: color.b,
        a: color.a,
    }
    .into()
}

fn cased(text: &str, case: Option<Case>) -> String {
    match case {
        Some(Case::Upper) => text.to_uppercase(),
        Some(Case::Lower) => text.to_lowercase(),
        _ => text.to_string(),
    }
}

/// `text` in its lines, each in `case` and with the ranges of it that typed
/// characters fill, of those `typed` gives for the whole. Each piece takes
/// its case on its own, so the ranges hold where a case changes a length.
fn lines(
    text: &str,
    typed: &[Range<usize>],
    case: Option<Case>,
) -> Vec<(String, Vec<Range<usize>>)> {
    let mut pieces = Vec::new();
    let mut at = 0;
    for range in typed {
        pieces.push((at..range.start, false));
        pieces.push((range.clone(), true));
        at = range.end;
    }
    pieces.push((at..text.len(), false));
    let mut lines = Vec::new();
    let (mut line, mut ranges) = (String::new(), Vec::new());
    for (range, is_typed) in pieces {
        for (number, part) in text.get(range).unwrap_or_default().split('\n').enumerate() {
            if number > 0 {
                lines.push((std::mem::take(&mut line), std::mem::take(&mut ranges)));
            }
            let start = line.len();
            line.push_str(&cased(part, case));
            if is_typed && line.len() > start {
                ranges.push(start..line.len());
            }
        }
    }
    lines.push((line, ranges));
    lines
}

/// What a layer takes from the one it sits in.
#[derive(Clone)]
struct Ctx {
    /// The parent's properties in each state of the timeline, for the text
    /// style its layers inherit.
    statics: Rc<Vec<Props>>,
    /// How much the layers around it scale it.
    zoom: f32,
    /// How the parent lays it out.
    flow: Flow,
    /// The session a `for_each` layer around it stands for.
    item: Option<usize>,
    /// Whether it is in the focused tiles pane.
    focused: bool,
    /// The nearest layer around it with an action, whose pointer, armed
    /// and fired conditions it shares, by its key and index.
    owner: Option<(u64, usize)>,
    /// How much longer its transitions wait, for staggered layers.
    delay: Duration,
}

/// One drawing of a layer: copies and `for_each` draw it several times.
#[derive(Default)]
struct Instance {
    number: usize,
    item: Option<usize>,
    dx: f32,
    dy: f32,
    turn: f32,
    delay: Duration,
}

struct Builder<'a> {
    view: &'a View<'a>,
    frame: &'a Frame<'a>,
    now: Instant,
    utc_offset: i32,
    language: Language,
}

impl Builder<'_> {
    /// A length in frame pixels, scaled by `zoom`.
    fn len(&self, frame_px: f32, zoom: f32) -> Rems {
        u(frame_px * zoom * CSS)
    }

    fn length(&self, length: Length, zoom: f32) -> gpui_kit::DefiniteLength {
        match length {
            Length::Px(px) => self.len(px, zoom).into(),
            Length::Percent(share) => relative(share),
        }
    }

    fn layer(&self, index: usize, ctx: &Ctx) -> Vec<AnyElement> {
        let layer = &self.frame.folder.layers[index];
        let instances: Vec<Instance> = match layer.repeat {
            Repeat::Once => vec![Instance::default()],
            Repeat::Sessions => (0..self.view.greeter.sessions.len())
                .map(|item| Instance {
                    number: item,
                    item: Some(item),
                    ..Instance::default()
                })
                .collect(),
            Repeat::Copies {
                count,
                x,
                y,
                rotate,
                delay,
            } => (0..count)
                .map(|number| Instance {
                    number,
                    dx: x * number as f32,
                    dy: y * number as f32,
                    turn: rotate * number as f32,
                    delay: delay * number as u32,
                    ..Instance::default()
                })
                .collect(),
        };
        instances
            .iter()
            .filter_map(|instance| self.instance(index, ctx, instance))
            .collect()
    }

    /// The conditions a layer is under; the pointer, armed and fired ones
    /// are those of the nearest layer with an action, itself or `owner`.
    fn flags(
        &self,
        layer: &Layer,
        key: u64,
        item: Option<usize>,
        focused: bool,
        owner: Option<(u64, usize)>,
    ) -> Flags {
        let greeter = self.view.greeter;
        let (source, source_key) = match owner {
            Some((owner_key, index)) if layer.action.is_none() => {
                (&self.frame.folder.layers[index], owner_key)
            }
            _ => (layer, key),
        };
        let fired = greeter.fired.is_some_and(|(action, at)| {
            // The owner's actions light the layer, and so do its own.
            let lit = [source, layer]
                .into_iter()
                .find(|by| by.fired_by.contains(&action) && at.elapsed() < by.fired_for);
            if let Some(by) = lit {
                self.frame.pace.wake_at(at + by.fired_for);
                self.frame.pace.settling.set(true);
            }
            lit.is_some()
        });
        let armed = match (
            greeter.armed_action(),
            source.action.and_then(|a| a.power()),
        ) {
            (Some(armed), Some(own)) => armed == own,
            (Some(_), None) => true,
            (None, _) => false,
        };
        Flags {
            locked: greeter.lock_screen,
            current: item == Some(greeter.session),
            focused,
            armed,
            fired,
            hover: self.frame.hovered.contains(&source_key),
        }
    }

    /// The layer's properties in each state of the timeline.
    fn statics(&self, layer: &Layer, flags: Flags, parent: &[Props]) -> Vec<Props> {
        let failures = self.view.greeter.failures;
        self.frame
            .timeline
            .changes()
            .iter()
            .zip(parent.iter())
            .map(|((state, _), parent)| {
                let mut props = look::props_at(layer, *state, flags, self.language, failures);
                props.inherit(parent);
                props
            })
            .collect()
    }

    fn look(&self, layer: &Layer, statics: &[Props], delay: Duration) -> Look {
        let timeline = self.frame.timeline;
        let pace = self.frame.pace;
        let mut look = look::look(layer, statics, timeline, self.now, delay, pace);
        look::moved(
            layer,
            &mut look,
            timeline,
            self.now,
            self.utc_offset,
            delay,
            pace,
        );
        look
    }

    fn instance(&self, index: usize, ctx: &Ctx, instance: &Instance) -> Option<AnyElement> {
        let layer = &self.frame.folder.layers[index];
        let key = ((index as u64) << 24) | instance.number as u64;
        let item = instance.item.or(ctx.item);
        let flags = self.flags(layer, key, item, ctx.focused, ctx.owner);
        let owner = if layer.action.is_some() {
            Some((key, index))
        } else {
            ctx.owner
        };
        let statics = self.statics(layer, flags, &ctx.statics);
        let delay = ctx.delay + instance.delay;
        let mut look = self.look(layer, &statics, delay);
        look.x += instance.dx;
        look.y += instance.dy;
        look.rotate += instance.turn;
        let alpha = look.opacity * look.shown;
        if look.shown <= 0.0 || alpha < 0.002 {
            return None;
        }
        let tinted = matches!(layer.kind, Kind::Svg { tint: true, .. });
        let zoom = ctx.zoom * if tinted { 1.0 } else { look.scale };
        let inner_ctx = Ctx {
            statics: Rc::new(statics),
            zoom,
            flow: match layer.kind {
                Kind::Box { flow, .. } => flow,
                _ => Flow::Stack,
            },
            item,
            focused: flags.focused,
            owner,
            delay,
        };
        let inner = match &layer.kind {
            Kind::Box { flow, clips } => self.boxed(layer, &look, &inner_ctx, *flow, *clips),
            Kind::Text => self.text(&look, zoom, item),
            Kind::Field { .. } => self.field(layer, &look, zoom),
            Kind::Image(picture) => self.image(layer, &look, *picture, zoom)?,
            Kind::Svg { picture, tint } => self.svg(layer, &look, *picture, *tint, zoom)?,
            Kind::Wallpaper { picture, .. } => self.wallpaper(layer, &look, *picture, zoom),
            Kind::Tiles(spec) => self.tiles(layer, spec, &look, &inner_ctx),
        };
        let parent_len = |frame_px: f32| self.len(frame_px, ctx.zoom);
        let mut inner = inner;
        if alpha < 1.0 {
            inner = inner.opacity(alpha);
        }
        let in_flow = ctx.flow != Flow::Stack;
        if in_flow || layer.anchor != Anchor::Fill {
            inner = inner
                .relative()
                .left(parent_len(look.x))
                .top(parent_len(look.y));
            inner = if in_flow && layer.grow > 0.0 {
                inner.flex_grow(layer.grow)
            } else {
                inner.flex_shrink_0()
            };
        }
        let element = if layer.action.is_some() || layer.watches_hover() {
            self.interactive(inner, layer, key, item)
        } else {
            inner.into_any_element()
        };
        let element = if look.clip.iter().any(|side| *side > 0.0) {
            paint::clip(element, look.clip).into_any_element()
        } else {
            element
        };
        Some(if in_flow {
            element
        } else {
            self.anchored(layer, element, &look, ctx.zoom)
        })
    }

    /// Places a layer in a stack: against its anchor, or over the parent.
    fn anchored(&self, layer: &Layer, element: AnyElement, look: &Look, zoom: f32) -> AnyElement {
        let len = |frame_px: f32| self.len(frame_px, zoom);
        if layer.anchor == Anchor::Fill {
            let [top, right, bottom, left] = layer.inset;
            return div()
                .absolute()
                .top(len(top + look.y))
                .bottom(len(bottom - look.y))
                .left(len(left + look.x))
                .right(len(right - look.x))
                .child(element)
                .into_any_element();
        }
        let (across, down) = layer.anchor.fractions();
        let wrapper = div().absolute().inset_0().flex();
        let wrapper = match across {
            a if a < 0.25 => wrapper.justify_start(),
            a if a < 0.75 => wrapper.justify_center(),
            _ => wrapper.justify_end(),
        };
        let wrapper = match down {
            d if d < 0.25 => wrapper.items_start(),
            d if d < 0.75 => wrapper.items_center(),
            _ => wrapper.items_end(),
        };
        wrapper.child(element).into_any_element()
    }

    fn interactive(&self, inner: Div, layer: &Layer, key: u64, item: Option<usize>) -> AnyElement {
        let mut element = inner.id(("layer", key as usize));
        if layer.watches_hover() || layer.action.is_some() {
            let screen = self.frame.screen.clone();
            element = element.on_hover(move |hovered, _, cx| {
                let hovered = *hovered;
                screen.update(cx, |screen, cx| screen.hover(key, hovered, cx));
            });
        }
        if let Some(action) = layer.action {
            element = element
                .cursor_pointer()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(
                    self.view
                        .on(move |greeter, cx| greeter.run(action, item, cx)),
                );
        }
        element.into_any_element()
    }

    /// Sets a layer's size from its width and height, or for pictures from
    /// the one given and their shape, or over its parent when it fills it.
    fn sized(
        &self,
        element: Div,
        layer: &Layer,
        look: &Look,
        zoom: f32,
        natural: Option<(f32, f32)>,
    ) -> Div {
        let (mut width, mut height) = (look.width, look.height);
        if let Some((natural_w, natural_h)) = natural.filter(|_| layer.anchor != Anchor::Fill) {
            let aspect = if natural_h > 0.0 {
                natural_w / natural_h
            } else {
                1.0
            };
            match (width, height) {
                (None, None) => {
                    width = Some(Length::Px(natural_w));
                    height = Some(Length::Px(natural_h));
                }
                (Some(Length::Px(w)), None) => height = Some(Length::Px(w / aspect)),
                (None, Some(Length::Px(h))) => width = Some(Length::Px(h * aspect)),
                _ => {}
            }
        }
        let mut element = element;
        if layer.anchor == Anchor::Fill {
            element = element.size_full();
        }
        if let Some(width) = width {
            element = element.w(self.length(width, zoom));
        }
        if let Some(height) = height {
            element = element.h(self.length(height, zoom));
        }
        element
    }

    fn boxed(&self, layer: &Layer, look: &Look, ctx: &Ctx, flow: Flow, clips: bool) -> Div {
        let props = &look.props;
        let zoom = ctx.zoom;
        let len = |frame_px: f32| self.len(frame_px, zoom);
        let mut element = match flow {
            Flow::Row => div().flex().flex_row(),
            Flow::Column => div().flex().flex_col(),
            Flow::Stack => div().relative(),
        };
        if let Some(gap) = props.gap {
            element = element.gap(len(gap));
        }
        element = match props.align {
            Some(Align::Start) => element.items_start(),
            Some(Align::Center) => element.items_center(),
            Some(Align::End) => element.items_end(),
            Some(Align::Stretch) => element.items_stretch(),
            Some(Align::Baseline) => element.items_baseline(),
            None => element,
        };
        element = match props.justify {
            Some(Justify::Start) => element.justify_start(),
            Some(Justify::Center) => element.justify_center(),
            Some(Justify::End) => element.justify_end(),
            Some(Justify::Between) => element.justify_between(),
            Some(Justify::Around) => element.justify_around(),
            None => element,
        };
        if let Some([top, right, bottom, left]) = props.padding {
            element = element
                .pt(len(top))
                .pr(len(right))
                .pb(len(bottom))
                .pl(len(left));
        }
        if look.fill.a > 0.0 {
            element = element.bg(hsla(look.fill));
        }
        if let Some([top, right, bottom, left]) = props.border {
            element = element
                .border_t(len(top))
                .border_r(len(right))
                .border_b(len(bottom))
                .border_l(len(left))
                .border_color(hsla(look.border_color));
            if props.dashed == Some(true) {
                element = element.border_dashed();
            }
        }
        if let Some([tl, tr, br, bl]) = props.radius {
            element = element
                .rounded_tl(len(tl))
                .rounded_tr(len(tr))
                .rounded_br(len(br))
                .rounded_bl(len(bl));
        }
        if let Some(Some(shadow)) = props.box_shadow {
            let px = |frame_px: f32| self.view.px(frame_px * zoom * CSS);
            element = element.shadow(vec![BoxShadow {
                color: hsla(shadow.color),
                offset: point(px(shadow.x), px(shadow.y)),
                blur_radius: px(shadow.blur),
                spread_radius: px(shadow.spread),
                inset: shadow.inset,
            }]);
        }
        if clips {
            element = element.overflow_hidden();
        }
        for (place, &child) in layer.children.iter().enumerate() {
            let child_ctx = Ctx {
                delay: ctx.delay + layer.stagger * place as u32,
                ..ctx.clone()
            };
            element = element.children(self.layer(child, &child_ctx));
        }
        self.sized(element, layer, look, ctx.zoom, None)
    }

    fn label(&self, text: String, props: &Props, color: Color, zoom: f32) -> Label {
        let size = props.size.unwrap_or(TEXT_SIZE);
        let family = props.font.as_deref().unwrap_or("Inter").to_string();
        let mut label = label(text)
            .family(family, FontWeight(props.weight.unwrap_or(400.0)))
            .italic(props.italic == Some(true))
            .size(size * zoom * CSS)
            .color(hsla(color))
            .tracking(props.tracking.unwrap_or(0.0) * zoom * CSS);
        if let Some(factor) = props.line_height {
            label = label.line_height(factor);
        }
        if let Some(Some(shadow)) = props.shadow {
            let css = |frame_px: f32| frame_px * zoom * CSS;
            label = label.shadow(
                css(shadow.x),
                css(shadow.y),
                css(shadow.blur),
                hsla(shadow.color),
            );
        }
        if let Some(features) = &props.features {
            label = label.features(FontFeatures(Arc::new(features.to_vec())));
        }
        if let Some(families) = &props.fallback {
            label = label.fallback(families.clone());
        }
        label
    }

    fn text(&self, look: &Look, zoom: f32, item: Option<usize>) -> Div {
        let props = &look.props;
        let (text, typed) = props
            .text
            .as_ref()
            .map(|template| template.render_typed(&self.filler(item)))
            .unwrap_or_default();
        let align = props.text_align.unwrap_or(TextAlign::Start);
        let lines: Vec<Label> = lines(&text, &typed, props.case)
            .into_iter()
            .map(|(line, typed)| {
                let label = self.label(line, props, look.color, zoom);
                typed.into_iter().fold(label, Label::typed)
            })
            .collect();
        let element = if lines.len() == 1 {
            let element = div().flex();
            match align {
                TextAlign::Start => element,
                TextAlign::Center => element.justify_center(),
                TextAlign::End => element.justify_end(),
            }
        } else {
            let element = div().flex().flex_col();
            match align {
                TextAlign::Start => element.items_start(),
                TextAlign::Center => element.items_center(),
                TextAlign::End => element.items_end(),
            }
        };
        let element = element.children(lines);
        self.sized_text(element, look, zoom)
    }

    fn sized_text(&self, element: Div, look: &Look, zoom: f32) -> Div {
        let mut element = element;
        if let Some(width) = look.width {
            element = element.w(self.length(width, zoom));
        }
        if let Some(height) = look.height {
            element = element.h(self.length(height, zoom));
        }
        element
    }

    fn field(&self, layer: &Layer, look: &Look, zoom: f32) -> Div {
        let Kind::Field {
            mask,
            max,
            blink,
            caret_width,
        } = &layer.kind
        else {
            return div();
        };
        let greeter = self.view.greeter;
        let props = &look.props;
        let size = props.size.unwrap_or(TEXT_SIZE);
        let typed = match greeter.visible_answer() {
            Some(answer) => Some(answer.to_string()),
            None => (greeter.typed_len() > 0).then(|| mask.repeat(greeter.typed_len().min(*max))),
        };
        let caret_shape = match props.caret {
            Some(shape) => shape,
            None if greeter.phase == Phase::Welcome => Caret::Hidden,
            None => Caret::Bar,
        };
        let caret_on = if blink.is_zero() {
            true
        } else {
            let since = greeter.last_input.elapsed().as_nanos();
            let period = blink.as_nanos().max(1);
            let phase = since % period;
            let half = period / 2;
            let wait = if phase < half {
                half - phase
            } else {
                period - phase
            };
            self.frame
                .pace
                .wake_at(self.now + Duration::from_nanos(wait as u64));
            phase < half
        };
        let caret_height = self.len(size * 1.15, zoom);
        let caret = (caret_shape != Caret::Hidden).then(|| {
            let width = self.len(
                caret_width.unwrap_or(match caret_shape {
                    Caret::Block => 0.6 * size,
                    _ => 3.6,
                }),
                zoom,
            );
            let bar = div().w(width).h(caret_height);
            let bar = if caret_on {
                bar.bg(hsla(look.caret_color))
            } else {
                bar
            };
            (width, bar)
        });
        let shown = match &typed {
            Some(text) => {
                let text = cased(text, props.case);
                let len = text.len();
                self.label(text, props, look.color, zoom).typed(0..len)
            }
            None => {
                let placeholder = props
                    .placeholder
                    .as_ref()
                    .map(|template| template.render(&self.filler(None)))
                    .unwrap_or_default();
                let placeholder = cased(&placeholder, props.case);
                self.label(placeholder, props, look.placeholder_color, zoom)
            }
        };
        // The text gives the field its baseline, the same empty or typed;
        // the caret stands beside it, out of the flow, and only keeps the
        // field as tall as itself.
        let mut text = div()
            .relative()
            .flex()
            .items_center()
            .min_h(caret_height)
            .child(shown.strut());
        if let Some((width, bar)) = caret {
            let holder = div()
                .absolute()
                .top_0()
                .bottom_0()
                .flex()
                .items_center()
                .child(bar);
            text = if typed.is_some() {
                text.pr(width).child(holder.right_0())
            } else {
                text.pl(width).child(holder.left_0())
            };
        }
        let line = div().flex().items_center().min_w_0().overflow_hidden();
        let line = match props.text_align {
            Some(TextAlign::Center) => line.justify_center(),
            Some(TextAlign::End) => line.justify_end(),
            _ => line,
        };
        self.sized_text(line.child(text), look, zoom)
    }

    fn corners(&self, props: &Props, zoom: f32) -> Corners<gpui_kit::Pixels> {
        let [tl, tr, br, bl] = props.radius.unwrap_or([0.0; 4]);
        let px = |frame_px: f32| self.view.px(frame_px * zoom * CSS);
        Corners {
            top_left: px(tl),
            top_right: px(tr),
            bottom_right: px(br),
            bottom_left: px(bl),
        }
    }

    fn image(&self, layer: &Layer, look: &Look, picture: Picture, zoom: f32) -> Option<Div> {
        let file = look.props.file?;
        let image = self.frame.pictures.images.get(&file)?.clone();
        let natural = self.frame.folder.assets[file].natural_size();
        let corners = self.corners(&look.props, zoom);
        let element = div().child(paint::picture(image, picture.fit, picture.focus, corners));
        Some(self.sized(element, layer, look, zoom, Some(natural)))
    }

    fn svg(
        &self,
        layer: &Layer,
        look: &Look,
        picture: Picture,
        tint: bool,
        zoom: f32,
    ) -> Option<Div> {
        let file = look.props.file?;
        let asset = &self.frame.folder.assets[file];
        let natural = asset.natural_size();
        let natural = (natural.0, natural.1);
        let element = if tint {
            let turn = Transformation::rotate(radians(look.rotate.to_radians()))
                .with_scaling(size(look.scale, look.scale));
            div().child(
                svg()
                    .path(asset_key(&self.frame.folder.name, &asset.path))
                    .size_full()
                    .text_color(hsla(look.color))
                    .with_transformation(turn),
            )
        } else {
            let unit = self.view.unit * self.view.scale_factor;
            let aspect = asset.aspect();
            let span = |length: Option<Length>| match length {
                Some(Length::Px(px)) => Some(px * zoom * CSS * unit),
                _ => None,
            };
            let pixels = match (span(look.width), span(look.height)) {
                (Some(width), Some(height)) => (width, height),
                (Some(width), None) => (width, width / aspect),
                (None, Some(height)) => (height * aspect, height),
                (None, None) => (self.view.size.0 * unit, self.view.size.1 * unit),
            };
            let image = self.frame.drawn.svg(file, pixels)?.clone();
            let corners = self.corners(&look.props, zoom);
            div().child(paint::picture(image, Fit::Stretch, picture.focus, corners))
        };
        Some(self.sized(element, layer, look, zoom, Some(natural)))
    }

    fn wallpaper(&self, layer: &Layer, look: &Look, picture: Picture, zoom: f32) -> Div {
        let backdrops = self.view.backdrops;
        let veil = look.blurred;
        let corners = self.corners(&look.props, zoom);
        let pictures = move |backdrop: &Backdrop| {
            let layer = |image: Arc<gpui_kit::RenderImage>, opacity: f32| {
                div()
                    .absolute()
                    .inset_0()
                    .opacity(opacity)
                    .child(paint::picture(image, picture.fit, picture.focus, corners))
            };
            div()
                .absolute()
                .inset_0()
                .child(layer(backdrop.sharp.clone(), 1.0))
                .when(veil > 0.0, |stack| {
                    stack
                        .child(layer(backdrop.soft.clone(), (2.0 * veil).min(1.0)))
                        .child(layer(backdrop.blurred.clone(), (2.0 * veil - 1.0).max(0.0)))
                })
        };
        let mut element = div().relative().overflow_hidden();
        if let Some(old) = &backdrops.previous {
            element = element.child(pictures(old));
        }
        if let Some(new) = &backdrops.current {
            element = element.child(
                div()
                    .absolute()
                    .inset_0()
                    .child(pictures(new))
                    .with_animation(("arrival", backdrops.arrivals), fade(), |layer, t| {
                        layer.opacity(t)
                    }),
            );
        }
        self.sized(element, layer, look, zoom, None)
    }

    /// The tiles layer's area, in frame pixels.
    fn tiles_area(&self, layer: &Layer) -> (f32, f32) {
        let px = |length: Option<Length>| match length {
            Some(Length::Px(px)) => Some(px),
            _ => None,
        };
        match (px(layer.base.width), px(layer.base.height)) {
            (Some(width), Some(height)) if layer.anchor != Anchor::Fill => (width, height),
            _ => {
                let (width, height) = (self.view.size.0 / CSS, self.view.size.1 / CSS);
                let [top, right, bottom, left] = layer.inset;
                (width - left - right, height - top - bottom)
            }
        }
    }

    fn tiles(&self, layer: &Layer, spec: &Tiles, look: &Look, ctx: &Ctx) -> Div {
        let Some((layout, focus)) = self.tiles_now() else {
            return div();
        };
        let Some(state) = self.view.greeter.tiles.as_ref() else {
            return div();
        };
        let (width, height) = self.tiles_area(layer);
        let area = tiles::Rect {
            x: 0.0,
            y: 0.0,
            width,
            height,
        };
        let focused_at = state
            .order
            .iter()
            .position(|&pane| pane == focus)
            .unwrap_or(0);
        let gap = look.props.gap.unwrap_or(0.0);
        let rects = tiles::arrange(
            area,
            state.order.len(),
            focused_at,
            layout,
            state.ratio,
            gap,
        );
        let mut element = div().relative();
        for (place, &pane) in state.order.iter().enumerate() {
            if let Some(rect) = rects.get(place).copied().flatten() {
                element = element.children(self.pane(pane, rect, pane == focus, layout, spec, ctx));
            }
        }
        self.sized(element, layer, look, ctx.zoom, None)
    }

    fn pane(
        &self,
        index: usize,
        rect: tiles::Rect,
        focused: bool,
        layout: tiles::Layout,
        spec: &Tiles,
        ctx: &Ctx,
    ) -> Option<AnyElement> {
        let layer = &self.frame.folder.layers[index];
        let pane = layer.pane.as_ref()?;
        let key = (index as u64) << 24;
        let flags = self.flags(layer, key, ctx.item, focused, ctx.owner);
        let statics = self.statics(layer, flags, &ctx.statics);
        let look = self.look(layer, &statics, ctx.delay);
        if look.shown <= 0.0 || look.opacity * look.shown < 0.002 {
            return None;
        }
        let border = spec.border;
        let largest = if layout == tiles::Layout::Full {
            pane.max_scale_full
        } else {
            pane.max_scale
        };
        let room = (
            rect.width - 2.0 * (border + pane.padding),
            rect.height - 2.0 * (border + pane.padding),
        );
        let zoom = match pane.content {
            Some((width, height)) => {
                let scale = (room.0 / width)
                    .min(room.1 / height)
                    .min(largest)
                    .max(pane.min_scale);
                ctx.zoom * scale
            }
            None => ctx.zoom,
        };
        let inner_ctx = Ctx {
            statics: Rc::new(statics),
            zoom,
            flow: Flow::Stack,
            item: ctx.item,
            focused,
            owner: if layer.action.is_some() {
                Some((key, index))
            } else {
                ctx.owner
            },
            delay: ctx.delay,
        };
        let mut content = match pane.content {
            Some((width, height)) => div()
                .relative()
                .flex_shrink_0()
                .w(self.len(width, zoom))
                .h(self.len(height, zoom)),
            None => div().relative().size_full(),
        };
        for (place, &child) in layer.children.iter().enumerate() {
            let child_ctx = Ctx {
                delay: inner_ctx.delay + layer.stagger * place as u32,
                ..inner_ctx.clone()
            };
            content = content.children(self.layer(child, &child_ctx));
        }
        let len = |frame_px: f32| self.len(frame_px, ctx.zoom);
        let mut element = div()
            .absolute()
            .left(len(rect.x))
            .top(len(rect.y))
            .w(len(rect.width))
            .h(len(rect.height))
            .overflow_hidden()
            .flex()
            .items_center()
            .justify_center()
            .border(len(border))
            .border_color(hsla(if focused {
                spec.focused
            } else {
                spec.unfocused
            }))
            .child(content);
        if look.fill.a > 0.0 {
            element = element.bg(hsla(look.fill));
        }
        let alpha = look.opacity * look.shown;
        if alpha < 1.0 {
            element = element.opacity(alpha);
        }
        Some(if layer.action.is_some() || layer.watches_hover() {
            self.interactive(element, layer, key, ctx.item)
        } else {
            element.into_any_element()
        })
    }

    fn filler(&self, item: Option<usize>) -> Filler<'_> {
        Filler {
            builder: self,
            item,
            in_title: false,
        }
    }

    /// The tiles' layout and focused pane as shown now.
    fn tiles_now(&self) -> Option<(tiles::Layout, usize)> {
        let folder = self.frame.folder;
        let state = self.view.greeter.tiles.as_ref()?;
        let layer = &folder.layers[folder.tiles?];
        let current = self
            .frame
            .timeline
            .current()
            .map_or(State::Idle, |(state, _)| state);
        let flags = Flags {
            locked: self.view.greeter.lock_screen,
            ..Flags::default()
        };
        let props = look::overridden(
            layer,
            current,
            flags,
            self.language,
            self.view.greeter.failures,
        );
        let layout = props.layout.unwrap_or_else(|| state.layout(folder));
        let focus = props
            .focus
            .filter(|pane| state.order.contains(pane))
            .unwrap_or(state.focus);
        Some((layout, focus))
    }
}

/// Fills templates in from the greeter, the system and the theme.
struct Filler<'a> {
    builder: &'a Builder<'a>,
    item: Option<usize>,
    /// Filling a pane's title, which may not name `{tiles.title}` again.
    in_title: bool,
}

impl Fill for Filler<'_> {
    fn string(&self, key: &str) -> Option<&crate::themes::Template> {
        self.builder
            .frame
            .folder
            .strings
            .get(self.builder.language, key)
    }

    fn binding(&self, binding: &Binding, arg: Option<&str>, out: &mut String) {
        let builder = self.builder;
        let greeter = builder.view.greeter;
        let language = builder.language;
        let s = greeter.strings;
        let user = greeter.user();
        match binding {
            Binding::Time(names) => out.push_str(&format_time(
                &greeter.now,
                arg.unwrap_or("%H:%M"),
                names.unwrap_or(language),
            )),
            Binding::Date(names) => {
                let names = names.unwrap_or(language);
                match arg {
                    Some(format) => out.push_str(&format_time(&greeter.now, format, names)),
                    None => out.push_str(&i18n::long_date(names, &greeter.now)),
                }
            }
            Binding::User => out.push_str(user.map_or("", |user| user.name.as_str())),
            Binding::UserName => out.push_str(user.map_or("", |user| user.display_name.as_str())),
            Binding::UserInitial => {
                out.push_str(&user.map(|user| user.initial()).unwrap_or_default())
            }
            Binding::Host => out.push_str(&greeter.machine.hostname),
            Binding::Session => out.push_str(greeter.session_name()),
            Binding::Theme => out.push_str(&builder.frame.folder.title),
            Binding::Prompt => match &greeter.question {
                Some(question) => out.push_str(question.prompt.trim().trim_end_matches(':')),
                None => out.push_str(s.password),
            },
            Binding::Message => match &greeter.phase {
                Phase::Denied {
                    notice: Some(notice),
                } => out.push_str(notice),
                Phase::Failed(message) => out.push_str(message),
                _ => out.push_str(greeter.notice.as_deref().unwrap_or_default()),
            },
            Binding::Status => out.push_str(&match greeter.status() {
                Status::Hint => s.press_enter.to_string(),
                Status::Notice(notice) => notice.to_string(),
                Status::Checking => s.checking.to_string(),
                Status::Denied(notice) => notice.unwrap_or(s.wrong_password).to_string(),
                Status::Failed(message) => message.to_string(),
                Status::Confirm(action) => s.confirm(greeter.action_name(action)),
                Status::Welcome => s.welcome(user.map_or("", |user| user.display_name.as_str())),
            }),
            Binding::Os => out.push_str(&greeter.facts.os),
            Binding::Arch => out.push_str(&greeter.facts.arch),
            Binding::Kernel => out.push_str(&greeter.machine.kernel),
            Binding::Uptime => {
                if let Some(up) = greeter.uptime {
                    out.push_str(&match arg {
                        Some(_) => system::format_uptime(up),
                        None => uptime(up, language),
                    });
                }
            }
            Binding::Cpu => out.push_str(&greeter.facts.cpu),
            Binding::Gpu => out.push_str(&greeter.facts.gpu),
            Binding::Memory => out.push_str(&greeter.memory),
            Binding::Packages => out.push_str(&greeter.facts.packages),
            Binding::Shell => {
                if let Some(user) = user {
                    let shell = greeter.facts.shells.get(&user.shell);
                    out.push_str(shell.map_or_else(
                        || user.shell.rsplit('/').next().unwrap_or_default(),
                        String::as_str,
                    ));
                }
            }
            Binding::Display => {
                let rect = builder.view.monitor.rect;
                out.push_str(&format!("{}\u{00D7}{}", rect.width, rect.height));
                if let Some(rate) = builder.view.monitor.refresh {
                    out.push_str(&format!(" @ {}Hz", rate.round()));
                }
            }
            Binding::Layout => out.push_str(
                greeter
                    .layouts
                    .get(greeter.layout)
                    .map_or("", String::as_str),
            ),
            Binding::Vt => out.push_str(greeter.machine.vt.as_deref().unwrap_or_default()),
            Binding::Seat => out.push_str(&greeter.machine.seat),
            Binding::Network => out.push_str(match (greeter.online, language) {
                (true, Language::English) => "online",
                (false, Language::English) => "offline",
                (true, Language::Chinese) => "在线",
                (false, Language::Chinese) => "离线",
            }),
            Binding::Typed => out.push_str(
                &arg.unwrap_or("\u{2022}")
                    .repeat(greeter.typed_count().min(256)),
            ),
            Binding::Sent => out.push_str(
                &arg.unwrap_or("\u{2022}")
                    .repeat(greeter.sent_len().min(256)),
            ),
            Binding::TilesTitle if !self.in_title => {
                if let Some((_, focus)) = builder.tiles_now()
                    && let Some(pane) = &builder.frame.folder.layers[focus].pane
                {
                    let title = Filler {
                        in_title: true,
                        ..*self
                    };
                    out.push_str(&pane.title.render(&title));
                }
            }
            Binding::TilesTitle => {}
            Binding::TilesLayout => {
                if let Some((layout, _)) = builder.tiles_now() {
                    out.push_str(layout.title());
                }
            }
            Binding::Item => {
                if let Some(session) = self.item.and_then(|item| greeter.sessions.get(item)) {
                    out.push_str(&session.name);
                }
            }
            Binding::ItemNumber => {
                if let Some(item) = self.item {
                    out.push_str(&(item + 1).to_string());
                }
            }
            Binding::ArmedKey => {
                if let Some(armed) = greeter.armed_action()
                    && let Some(chord) = builder.frame.folder.keys.key_for(Action::Power(armed))
                {
                    out.push_str(&chord.label());
                }
            }
            Binding::ArmedAction => {
                if let Some(armed) = greeter.armed_action() {
                    out.push_str(&Action::Power(armed).name());
                }
            }
            Binding::String(key) => out.push_str(built_in(s, key)),
        }
    }
}

/// One of the greeter's own words, by the name themes use for it.
fn built_in(s: &i18n::Strings, key: &str) -> &'static str {
    match key {
        "password" => s.password,
        "press_enter" => s.press_enter,
        "checking" => s.checking,
        "wrong_password" => s.wrong_password,
        "welcome_back" => s.welcome_back,
        "starting" => s.starting,
        "session" => s.session,
        "current" => s.current,
        "sleep" => s.sleep,
        "restart" => s.restart,
        "shut_down" => s.shut_down,
        "click_to_log_in" => s.click_to_log_in,
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_keep_where_typed_characters_stand() {
        // "ﬁle: " is 7 bytes and "FILE: " 6.
        let text = "ﬁle: ***\nend •";
        let shown: Vec<(String, Vec<String>)> = lines(text, &[7..10, 15..18], Some(Case::Upper))
            .into_iter()
            .map(|(line, typed)| {
                let typed = typed
                    .into_iter()
                    .map(|range| line[range].to_string())
                    .collect();
                (line, typed)
            })
            .collect();
        let typed = |text: &str| vec![text.to_string()];
        assert_eq!(
            shown,
            [
                ("FILE: ***".to_string(), typed("***")),
                ("END •".to_string(), typed("•")),
            ]
        );
        assert_eq!(lines("", &[], None), [(String::new(), vec![])]);
    }
}

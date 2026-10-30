//! A theme's layers as the greeter draws them: what each is, where it sits,
//! how it looks at rest and in each state, and what moves it.

use std::sync::Arc;
use std::time::Duration;

use super::color::Color;
use super::keys::Action;
use super::motion::{Motion, Transition};
use super::template::Template;
use super::tiles;
use crate::config::Language;

/// Where the login stands, as a theme's looks change with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum State {
    /// Before the first key, in a theme with a rest state.
    Rest,
    /// Ready, with nothing typed.
    Idle,
    Typing,
    Checking,
    /// A wrong password or a failure, until the next key.
    Error,
    Welcome,
}

impl State {
    pub const ALL: [State; 6] = [
        State::Rest,
        State::Idle,
        State::Typing,
        State::Checking,
        State::Error,
        State::Welcome,
    ];

    pub fn name(self) -> &'static str {
        match self {
            State::Rest => "rest",
            State::Idle => "idle",
            State::Typing => "typing",
            State::Checking => "checking",
            State::Error => "error",
            State::Welcome => "welcome",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|state| state.name() == name)
    }
}

/// When a layer's `on` table applies. They apply in this order, so a later
/// one wins where two set the same thing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Condition {
    Language(LanguageCondition),
    /// The greeter is the lock screen of a running session.
    Locked,
    /// A password was refused since the last rest, and the login has not
    /// succeeded.
    Failed,
    /// A `for_each` layer stands for the chosen session.
    Current,
    /// The tiles pane the layer is in has the focus.
    Focused,
    State(StateCondition),
    /// A power action waits for its second press; on a layer whose action
    /// is a power action, that one.
    Armed,
    /// The layer's action ran a moment ago.
    Fired,
    Hover,
}

/// `Language` and `State` in an order, for sorting conditions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct LanguageCondition(pub u8);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct StateCondition(pub u8);

impl Condition {
    pub fn language(language: Language) -> Self {
        Condition::Language(LanguageCondition(match language {
            Language::English => 0,
            Language::Chinese => 1,
        }))
    }

    pub fn state(state: State) -> Self {
        Condition::State(StateCondition(
            State::ALL.iter().position(|s| *s == state).unwrap_or(0) as u8,
        ))
    }

    pub fn as_state(self) -> Option<State> {
        match self {
            Condition::State(StateCondition(index)) => State::ALL.get(index as usize).copied(),
            _ => None,
        }
    }

    pub fn as_language(self) -> Option<Language> {
        match self {
            Condition::Language(LanguageCondition(0)) => Some(Language::English),
            Condition::Language(_) => Some(Language::Chinese),
            _ => None,
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        if let Some(state) = State::parse(name) {
            return Some(Self::state(state));
        }
        Some(match name {
            "en" => Self::language(Language::English),
            "zh-CN" => Self::language(Language::Chinese),
            "locked" => Condition::Locked,
            "failed" => Condition::Failed,
            "current" => Condition::Current,
            "focused" => Condition::Focused,
            "armed" => Condition::Armed,
            "fired" => Condition::Fired,
            "hover" => Condition::Hover,
            _ => return None,
        })
    }
}

/// A length in frame pixels, or a share of the parent's.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Length {
    Px(f32),
    Percent(f32),
}

impl Length {
    pub fn mix(self, to: Length, t: f32) -> Length {
        match (self, to) {
            (Length::Px(a), Length::Px(b)) => Length::Px(a + (b - a) * t),
            (Length::Percent(a), Length::Percent(b)) => Length::Percent(a + (b - a) * t),
            _ if t > 0.0 => to,
            _ => self,
        }
    }
}

/// Which point of the parent a layer is placed against, and the same point
/// of the layer; `Fill` covers the parent, less `inset`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Anchor {
    TopLeft,
    Top,
    TopRight,
    Left,
    Center,
    Right,
    BottomLeft,
    Bottom,
    BottomRight,
    Fill,
}

impl Anchor {
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "top-left" => Anchor::TopLeft,
            "top" => Anchor::Top,
            "top-right" => Anchor::TopRight,
            "left" => Anchor::Left,
            "center" => Anchor::Center,
            "right" => Anchor::Right,
            "bottom-left" => Anchor::BottomLeft,
            "bottom" => Anchor::Bottom,
            "bottom-right" => Anchor::BottomRight,
            "fill" => Anchor::Fill,
            _ => return None,
        })
    }

    /// Where along each axis the anchor lies: 0 at the start, 1 at the end.
    pub fn fractions(self) -> (f32, f32) {
        match self {
            Anchor::TopLeft | Anchor::Fill => (0.0, 0.0),
            Anchor::Top => (0.5, 0.0),
            Anchor::TopRight => (1.0, 0.0),
            Anchor::Left => (0.0, 0.5),
            Anchor::Center => (0.5, 0.5),
            Anchor::Right => (1.0, 0.5),
            Anchor::BottomLeft => (0.0, 1.0),
            Anchor::Bottom => (0.5, 1.0),
            Anchor::BottomRight => (1.0, 1.0),
        }
    }
}

/// How a box lays out the layers in it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flow {
    /// Each placed by its anchor.
    Stack,
    Row,
    Column,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Start,
    Center,
    End,
    Stretch,
    Baseline,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Justify {
    Start,
    Center,
    End,
    Between,
    Around,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextAlign {
    Start,
    Center,
    End,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Case {
    AsWritten,
    Upper,
    Lower,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Caret {
    Bar,
    Block,
    Hidden,
}

/// What a field shows for each character of a password.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mask {
    /// A dot, as macOS draws one.
    Dots,
    /// This text.
    Text(Arc<str>),
}

/// How a picture fills its box.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fit {
    /// All of it, as large as fits.
    Contain,
    /// The whole box, cropped about `focus`.
    Cover,
    /// The whole box, stretched.
    Stretch,
}

/// Which monitors a layer on the screen shows on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Monitors {
    /// The one with the login form.
    Form,
    All,
    Others,
}

/// CSS `text-shadow`, in frame pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shadow {
    pub x: f32,
    pub y: f32,
    pub blur: f32,
    pub color: Color,
}

/// CSS `box-shadow`, in frame pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoxShadow {
    pub x: f32,
    pub y: f32,
    pub blur: f32,
    pub spread: f32,
    pub color: Color,
    pub inset: bool,
}

/// One of the theme's files, by its place in `Folder::assets`.
pub type AssetId = usize;

/// Everything about a layer's look that may change with a condition. Unset
/// fields take the layer's base value, then the inherited or default one.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Props {
    pub visible: Option<bool>,
    pub opacity: Option<f32>,
    pub x: Option<f32>,
    pub y: Option<f32>,
    pub width: Option<Length>,
    pub height: Option<Length>,
    pub scale: Option<f32>,
    pub rotate: Option<f32>,
    /// Shares of the box cut away from its top, right, bottom and left.
    pub clip: Option<[f32; 4]>,

    pub font: Option<Arc<str>>,
    /// Families that draw what `font` lacks, ahead of the greeter's own.
    pub fallback: Option<Arc<[Arc<str>]>>,
    pub size: Option<f32>,
    pub weight: Option<f32>,
    pub italic: Option<bool>,
    pub color: Option<Color>,
    pub tracking: Option<f32>,
    pub line_height: Option<f32>,
    pub case: Option<Case>,
    pub features: Option<Arc<[(String, u32)]>>,
    pub shadow: Option<Option<Shadow>>,
    pub text_align: Option<TextAlign>,

    pub text: Option<Template>,
    pub file: Option<AssetId>,

    pub fill: Option<Color>,
    pub border: Option<[f32; 4]>,
    pub border_color: Option<Color>,
    pub dashed: Option<bool>,
    pub radius: Option<[f32; 4]>,
    pub padding: Option<[f32; 4]>,
    pub gap: Option<f32>,
    pub align: Option<Align>,
    pub justify: Option<Justify>,
    pub box_shadow: Option<Option<BoxShadow>>,

    pub caret: Option<Caret>,
    pub caret_color: Option<Color>,
    pub placeholder: Option<Template>,
    pub placeholder_color: Option<Color>,

    pub blurred: Option<f32>,

    pub layout: Option<tiles::Layout>,
    /// A pane of the tiles layer, by layer index.
    pub focus: Option<usize>,
}

macro_rules! merge {
    ($into:ident, $from:ident: $($field:ident),* $(,)?) => {
        $(if $from.$field.is_some() {
            $into.$field = $from.$field.clone();
        })*
    };
}

impl Props {
    /// Sets every field `other` sets.
    pub fn apply(&mut self, other: &Props) {
        merge!(self, other:
            visible, opacity, x, y, width, height, scale, rotate, clip,
            font, fallback, size, weight, italic, color, tracking, line_height, case, features,
            shadow, text_align, text, file, fill, border, border_color, dashed, radius, padding, gap,
            align, justify, box_shadow, caret, caret_color, placeholder, placeholder_color,
            blurred, layout, focus,
        );
    }

    /// Takes the text style a child inherits from `parent` where it sets
    /// none of its own.
    pub fn inherit(&mut self, parent: &Props) {
        macro_rules! inherit {
            ($($field:ident),*) => {
                $(if self.$field.is_none() {
                    self.$field = parent.$field.clone();
                })*
            };
        }
        inherit!(
            font,
            fallback,
            size,
            weight,
            italic,
            color,
            tracking,
            line_height,
            case,
            features,
            shadow,
            text_align
        );
    }
}

/// A layer's `on.<condition>` table.
#[derive(Debug, Clone, PartialEq)]
pub struct Override {
    pub condition: Condition,
    pub props: Props,
    /// How the layer changes on entering this state.
    pub transition: Option<Transition>,
    /// What runs while in this state.
    pub motions: Vec<Motion>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Kind {
    Box {
        flow: Flow,
        /// Whether what overflows the box is cut off.
        clips: bool,
    },
    Text,
    Field {
        /// What each typed character shows as.
        mask: Mask,
        /// The most characters shown.
        max: usize,
        /// A caret's blink, on for its first half; steady when zero.
        blink: Duration,
        /// In frame pixels; by default a bar is 3.6 wide and a block 0.6 em.
        caret_width: Option<f32>,
    },
    Image {
        picture: Picture,
        /// Pictures shown in turn, in place of `file`.
        slideshow: Option<Slideshow>,
    },
    Svg {
        picture: Picture,
        /// Draws the SVG's shape in the layer's colour, which can turn and
        /// change with states; otherwise in its own colours.
        tint: bool,
    },
    Wallpaper {
        picture: Picture,
        /// How far the blurred form is blurred, in frame pixels.
        blur: f32,
        /// The wallpaper folder's pictures in turn, from the one picked.
        slideshow: Option<Slideshow>,
    },
    Tiles(Tiles),
}

impl Kind {
    pub fn name(&self) -> &'static str {
        match self {
            Kind::Box { .. } => "box",
            Kind::Text => "text",
            Kind::Field { .. } => "field",
            Kind::Image { .. } => "image",
            Kind::Svg { .. } => "svg",
            Kind::Wallpaper { .. } => "wallpaper",
            Kind::Tiles(_) => "tiles",
        }
    }

    pub fn slideshow(&self) -> Option<&Slideshow> {
        match self {
            Kind::Image { slideshow, .. } | Kind::Wallpaper { slideshow, .. } => slideshow.as_ref(),
            _ => None,
        }
    }
}

/// Pictures a layer shows in turn, each for `interval`, the next fading in
/// over `crossfade`.
#[derive(Debug, Clone, PartialEq)]
pub struct Slideshow {
    /// Pictures in the theme folder, in order; a wallpaper layer's is
    /// empty, as it shows the wallpaper folder's.
    pub files: Vec<String>,
    pub interval: Duration,
    pub crossfade: Duration,
    /// The first slide, then all of them in a new order each round.
    pub shuffle: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Picture {
    pub fit: Fit,
    /// The point a cover crop keeps, as shares of the width and height.
    pub focus: (f32, f32),
}

/// A tiling container: its children are panes.
#[derive(Debug, Clone, PartialEq)]
pub struct Tiles {
    /// What `tile.next-layout` cycles through.
    pub layouts: Vec<tiles::Layout>,
    pub ratio: f32,
    pub ratio_step: f32,
    /// Pane borders, in frame pixels.
    pub border: f32,
    pub focused: Color,
    pub unfocused: Color,
}

/// What a pane of a tiles layer holds: its content, laid out on a box of
/// `content` frame pixels and scaled to fit the pane less `padding`, or on
/// the pane itself.
#[derive(Debug, Clone, PartialEq)]
pub struct Pane {
    pub title: Template,
    /// `None` lays the content out on the whole pane, unscaled.
    pub content: Option<(f32, f32)>,
    pub padding: f32,
    pub min_scale: f32,
    pub max_scale: f32,
    /// The largest scale when the layout is Full.
    pub max_scale_full: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Repeat {
    Once,
    /// Once for each session, with `{item}` its name.
    Sessions,
    /// `count` copies, each moved, turned and delayed a step further.
    Copies {
        count: usize,
        x: f32,
        y: f32,
        rotate: f32,
        delay: Duration,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Layer {
    pub id: Option<Arc<str>>,
    /// The line of its `[[layer]]` in theme.toml.
    pub line: usize,
    pub kind: Kind,
    pub parent: Option<usize>,
    pub children: Vec<usize>,
    pub anchor: Anchor,
    /// For `Fill`: top, right, bottom and left, in frame pixels.
    pub inset: [f32; 4],
    /// In a row or a column, how much of the spare room it takes.
    pub grow: f32,
    pub monitors: Monitors,
    pub base: Props,
    /// In the order they apply.
    pub overrides: Vec<Override>,
    pub transition: Transition,
    pub motions: Vec<Motion>,
    pub action: Option<Action>,
    /// The actions that light up `fired`: the layer's own, and those its
    /// `fired_by` adds.
    pub fired_by: Vec<Action>,
    pub fired_for: Duration,
    pub repeat: Repeat,
    /// Each child's transitions wait this much longer than the one before.
    pub stagger: Duration,
    pub pane: Option<Pane>,
}

impl Layer {
    /// A name for messages: its id, or its line.
    pub fn label(&self) -> String {
        match &self.id {
            Some(id) => format!("layer {id:?}"),
            None => format!("the layer on line {}", self.line),
        }
    }

    pub fn tiles(&self) -> Option<&Tiles> {
        match &self.kind {
            Kind::Tiles(tiles) => Some(tiles),
            _ => None,
        }
    }

    /// Whether any of its looks depends on the pointer.
    pub fn watches_hover(&self) -> bool {
        self.overrides
            .iter()
            .any(|o| o.condition == Condition::Hover)
    }
}

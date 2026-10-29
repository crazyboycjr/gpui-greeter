//! The settings app's colours and type: warm paper by day and a warm night,
//! following the desktop, with Paper's terracotta for the accent; Inter at
//! macOS's sizes, Chinese in Noto Sans CJK SC. gpui-kit's components take
//! the same colours through its theme.

use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::component::{Theme, ThemeConfig, ThemeConfigColors, ThemeMode};
use gpui_kit::{
    App, Font, FontFallbacks, FontFeatures, FontStyle, FontWeight, Hsla, SharedString,
    WindowAppearance, rgb,
};

/// Body text, as macOS sets its settings.
pub const TEXT: f32 = 13.0;
/// Notes under the controls.
pub const NOTE: f32 = 12.0;
/// What gpui-kit sizes its controls by: a small control is 1.5 of these,
/// 22.5 high, with 13-point text, as macOS's regular controls are.
const COMPONENT_REM: f32 = 15.0;

#[derive(Clone, Copy)]
pub struct Palette {
    pub dark: bool,
    /// Where the controls lie.
    pub ground: Hsla,
    /// The sidebar's and the bars' ground.
    pub panel: Hsla,
    pub text: Hsla,
    pub muted: Hsla,
    /// Hairlines between rows and around the preview.
    pub rule: Hsla,
    /// Controls' edges.
    pub border: Hsla,
    pub accent: Hsla,
    pub on_accent: Hsla,
    /// The sidebar's row picked, and a row under the pointer.
    pub picked: Hsla,
    pub hover: Hsla,
    pub danger: Hsla,
}

fn hsla(hex: u32) -> Hsla {
    rgb(hex).into()
}

impl Palette {
    pub fn of(appearance: WindowAppearance) -> Self {
        match appearance {
            WindowAppearance::Dark | WindowAppearance::VibrantDark => Self {
                dark: true,
                ground: hsla(0x221E1B),
                panel: hsla(0x1B1816),
                text: hsla(0xEDE5D9),
                muted: hsla(0xA89C8E),
                rule: hsla(0x35302A),
                border: hsla(0x4A4239),
                accent: hsla(0xD9844F),
                on_accent: hsla(0x1B1816),
                picked: hsla(0x3A322B),
                hover: hsla(0x2C2722),
                danger: hsla(0xE0705C),
            },
            _ => Self {
                dark: false,
                ground: hsla(0xF7F3EC),
                panel: hsla(0xEFE9DF),
                text: hsla(0x2B2520),
                muted: hsla(0x6F6357),
                rule: hsla(0xE2D9CB),
                border: hsla(0xD3C8B7),
                accent: hsla(0xB0612E),
                on_accent: hsla(0xFFFFFF),
                picked: hsla(0xE3D9CA),
                hover: hsla(0xE9E1D4),
                danger: hsla(0xB3412F),
            },
        }
    }
}

/// Inter, with Chinese from Noto Sans CJK SC.
pub fn font(weight: FontWeight) -> Font {
    Font {
        family: "Inter".into(),
        features: FontFeatures::default(),
        fallbacks: Some(FontFallbacks::from_fonts(vec![
            "Noto Sans CJK SC".into(),
            "Noto Sans SC".into(),
            "DejaVu Sans".into(),
        ])),
        weight,
        style: FontStyle::Normal,
    }
}

/// JetBrains Mono, for a command to copy.
pub fn mono() -> Font {
    Font {
        family: "JetBrains Mono".into(),
        features: FontFeatures(Arc::new(vec![("calt".into(), 0), ("liga".into(), 0)])),
        ..font(FontWeight::NORMAL)
    }
}

fn hex(color: Hsla) -> Option<SharedString> {
    let rgba = color.to_rgb();
    let channel = |value: f32| (value * 255.0).round() as u32;
    Some(
        format!(
            "#{:02X}{:02X}{:02X}",
            channel(rgba.r),
            channel(rgba.g),
            channel(rgba.b)
        )
        .into(),
    )
}

fn hex_alpha(color: Hsla, alpha: f32) -> Option<SharedString> {
    let rgba = color.to_rgb();
    let channel = |value: f32| (value * 255.0).round() as u32;
    Some(
        format!(
            "#{:02X}{:02X}{:02X}{:02X}",
            channel(rgba.r),
            channel(rgba.g),
            channel(rgba.b),
            channel(alpha)
        )
        .into(),
    )
}

/// Gives gpui-kit's components the palette: flat, square-ish corners, no
/// shadows, a focus shown by the border alone.
pub fn apply(palette: &Palette, cx: &mut App) {
    let p = palette;
    let mut colors = ThemeConfigColors::default();
    colors.background = hex(p.ground);
    colors.foreground = hex(p.text);
    colors.border = hex(p.border);
    colors.input = hex(p.border);
    colors.muted = hex(p.panel);
    colors.muted_foreground = hex(p.muted);
    colors.primary = hex(p.accent);
    colors.primary_hover = hex(p.ground.blend(p.accent.opacity(0.88)));
    colors.primary_active = hex(p.text.blend(p.accent.opacity(0.85)));
    colors.primary_foreground = hex(p.on_accent);
    colors.secondary = hex(p.panel);
    colors.secondary_hover = hex(p.hover);
    colors.secondary_active = hex(p.picked);
    colors.secondary_foreground = hex(p.text);
    colors.accent = hex(p.hover);
    colors.accent_foreground = hex(p.text);
    colors.popover = hex(p.ground);
    colors.popover_foreground = hex(p.text);
    colors.ring = hex(p.accent);
    colors.caret = hex(p.accent);
    colors.selection = hex_alpha(p.accent, 0.3);
    colors.list_active = hex_alpha(p.accent, 0.14);
    colors.list_active_border = hex(p.accent);
    colors.slider_bar = hex(p.accent);
    colors.slider_thumb = hex(if p.dark { p.text } else { p.on_accent });
    colors.switch = hex(p.border);
    colors.switch_thumb = hex(if p.dark { p.text } else { p.on_accent });
    colors.scrollbar = hex_alpha(p.ground, 0.0);
    colors.scrollbar_thumb = hex_alpha(p.muted, 0.4);
    colors.scrollbar_thumb_hover = hex_alpha(p.muted, 0.7);
    colors.danger = hex(p.danger);
    colors.link = hex(p.accent);
    colors.window_border = hex(p.border);
    colors.overlay = hex_alpha(p.ground, 0.0);
    let config = ThemeConfig {
        name: "gpui-greeter".into(),
        mode: if p.dark {
            ThemeMode::Dark
        } else {
            ThemeMode::Light
        },
        font_family: Some("Inter".into()),
        mono_font_family: Some("JetBrains Mono".into()),
        font_size: Some(COMPONENT_REM),
        radius: Some(5),
        radius_lg: Some(6),
        shadow: Some(false),
        colors,
        ..ThemeConfig::default()
    };
    Theme::update(cx, |theme| {
        theme.apply_config(&Rc::new(config));
        theme.focus_ring = false;
    });
}

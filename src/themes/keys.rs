//! What keys and clicks do in a theme: actions, the chords that run them,
//! and the keys every theme starts with.

use crate::power;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Submit,
    NextSession,
    PreviousSession,
    /// The session at this place in the list, from 1.
    Session(usize),
    /// The session a `for_each = "session"` layer stands for.
    Pick,
    NextUser,
    PreviousUser,
    NextKeyboardLayout,
    /// The next look the greeter offers, or the one before.
    NextTheme,
    PreviousTheme,
    /// Back to rest, clearing the field; only clears it in a theme without
    /// a rest state.
    Rest,
    Wake,
    Clear,
    Power(power::Action),
    Tile(TileAction),
    /// Does nothing: unbinds a key.
    Nothing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TileAction {
    FocusNext,
    FocusPrevious,
    SwapMaster,
    NextLayout,
    Shrink,
    Expand,
    Full,
}

const NAMES: [(&str, Action); 24] = [
    ("submit", Action::Submit),
    ("next-session", Action::NextSession),
    ("previous-session", Action::PreviousSession),
    ("pick", Action::Pick),
    ("next-user", Action::NextUser),
    ("previous-user", Action::PreviousUser),
    ("next-keyboard-layout", Action::NextKeyboardLayout),
    ("next-theme", Action::NextTheme),
    ("previous-theme", Action::PreviousTheme),
    ("rest", Action::Rest),
    ("wake", Action::Wake),
    ("clear", Action::Clear),
    ("suspend", Action::Power(power::Action::Suspend)),
    ("reboot", Action::Power(power::Action::Reboot)),
    ("poweroff", Action::Power(power::Action::PowerOff)),
    ("tile.focus-next", Action::Tile(TileAction::FocusNext)),
    (
        "tile.focus-previous",
        Action::Tile(TileAction::FocusPrevious),
    ),
    ("tile.swap-master", Action::Tile(TileAction::SwapMaster)),
    ("tile.next-layout", Action::Tile(TileAction::NextLayout)),
    ("tile.shrink", Action::Tile(TileAction::Shrink)),
    ("tile.expand", Action::Tile(TileAction::Expand)),
    ("tile.full", Action::Tile(TileAction::Full)),
    ("none", Action::Nothing),
    ("session:N", Action::Session(0)),
];

impl Action {
    pub fn parse(text: &str) -> Result<Self, String> {
        if let Some(number) = text.strip_prefix("session:") {
            return number
                .parse::<usize>()
                .ok()
                .filter(|&n| (1..=99).contains(&n))
                .map(Action::Session)
                .ok_or_else(|| format!("{text:?}: the session's number runs from 1"));
        }
        NAMES
            .iter()
            .find(|(name, _)| *name == text && !name.ends_with(":N"))
            .map(|(_, action)| *action)
            .ok_or_else(|| {
                let names: Vec<&str> = NAMES.iter().map(|(name, _)| *name).collect();
                format!(
                    "unknown action {text:?}; the actions are {}",
                    names.join(", ")
                )
            })
    }

    /// The action's name as themes write it.
    pub fn name(&self) -> String {
        match self {
            Action::Session(n) => format!("session:{n}"),
            action => NAMES
                .iter()
                .find(|(_, known)| known == action)
                .map(|(name, _)| (*name).to_string())
                .unwrap_or_default(),
        }
    }

    pub fn power(&self) -> Option<power::Action> {
        match self {
            Action::Power(action) => Some(*action),
            _ => None,
        }
    }
}

/// A key with the modifiers held, as `[keys]` writes it: `f10`,
/// `shift-tab`, `ctrl-u` or `mod-j`, where `mod` is Alt or Super.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chord {
    /// GPUI's name for the key: `enter`, `escape`, `f10`, `j`.
    pub key: String,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub super_key: bool,
    /// Alt or Super, either one.
    pub any_mod: bool,
}

/// The modifiers held with a key.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Held {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub super_key: bool,
}

impl Chord {
    pub fn parse(text: &str) -> Result<Self, String> {
        let lower = text.trim().to_lowercase();
        let mut chord = Chord {
            key: String::new(),
            ctrl: false,
            alt: false,
            shift: false,
            super_key: false,
            any_mod: false,
        };
        let mut parts: Vec<&str> = lower.split('-').collect();
        // `ctrl--` is Ctrl with the minus key.
        if lower.ends_with("--") {
            parts.truncate(parts.len().saturating_sub(2));
            parts.push("-");
        }
        let Some((key, modifiers)) = parts.split_last() else {
            return Err(format!("{text:?} names no key"));
        };
        for modifier in modifiers {
            let flag = match *modifier {
                "ctrl" | "control" => &mut chord.ctrl,
                "alt" => &mut chord.alt,
                "shift" => &mut chord.shift,
                "super" | "win" | "logo" => &mut chord.super_key,
                "mod" => &mut chord.any_mod,
                other => return Err(format!("{text:?}: unknown modifier {other:?}")),
            };
            *flag = true;
        }
        chord.key = match *key {
            "return" | "enter" => "enter".into(),
            "esc" | "escape" => "escape".into(),
            "" => return Err(format!("{text:?} names no key")),
            key if key.chars().count() == 1 || is_named_key(key) => key.into(),
            key => return Err(format!("{text:?}: unknown key {key:?}")),
        };
        Ok(chord)
    }

    pub fn matches(&self, key: &str, held: Held) -> bool {
        let alt_super = if self.any_mod {
            held.alt || held.super_key
        } else {
            held.alt == self.alt && held.super_key == self.super_key
        };
        self.key == key && held.ctrl == self.ctrl && held.shift == self.shift && alt_super
    }

    /// How the chord reads on screen: "F12", "Shift+Tab".
    pub fn label(&self) -> String {
        let mut label = String::new();
        for (on, name) in [
            (self.ctrl, "Ctrl+"),
            (self.alt, "Alt+"),
            (self.any_mod, "Mod+"),
            (self.super_key, "Super+"),
            (self.shift, "Shift+"),
        ] {
            if on {
                label.push_str(name);
            }
        }
        let key = if self.key.starts_with('f') && self.key.len() > 1 {
            self.key.to_uppercase()
        } else {
            let mut chars = self.key.chars();
            chars
                .next()
                .map(|first| first.to_uppercase().chain(chars).collect())
                .unwrap_or_default()
        };
        label.push_str(&key);
        label
    }
}

fn is_named_key(key: &str) -> bool {
    const NAMED: [&str; 14] = [
        "tab",
        "space",
        "backspace",
        "delete",
        "insert",
        "home",
        "end",
        "pageup",
        "pagedown",
        "up",
        "down",
        "left",
        "right",
        "menu",
    ];
    NAMED.contains(&key)
        || key
            .strip_prefix('f')
            .and_then(|number| number.parse::<u8>().ok())
            .is_some_and(|number| (1..=24).contains(&number))
}

/// The keys a theme answers to: every theme's own, as `[keys]` changes them.
#[derive(Debug, Clone)]
pub struct Keymap {
    bindings: Vec<(Chord, Action)>,
}

impl Default for Keymap {
    fn default() -> Self {
        let mut keymap = Keymap {
            bindings: Vec::new(),
        };
        for (chord, action) in [
            ("enter", Action::Submit),
            ("tab", Action::NextSession),
            ("shift-tab", Action::PreviousSession),
            ("escape", Action::Rest),
            ("ctrl-u", Action::Clear),
            ("f2", Action::NextSession),
            ("f3", Action::NextKeyboardLayout),
            ("f9", Action::NextTheme),
            ("shift-f9", Action::PreviousTheme),
            ("f10", Action::Power(power::Action::Suspend)),
            ("f11", Action::Power(power::Action::Reboot)),
            ("f12", Action::Power(power::Action::PowerOff)),
            ("up", Action::PreviousUser),
            ("down", Action::NextUser),
        ] {
            if let Ok(chord) = Chord::parse(chord) {
                keymap.bind(chord, action);
            }
        }
        keymap
    }
}

impl Keymap {
    /// Makes `chord` run `action`, in place of what it ran before.
    pub fn bind(&mut self, chord: Chord, action: Action) {
        self.bindings.retain(|(bound, _)| *bound != chord);
        self.bindings.push((chord, action));
    }

    pub fn lookup(&self, key: &str, held: Held) -> Option<Action> {
        self.bindings
            .iter()
            .find(|(chord, _)| chord.matches(key, held))
            .map(|(_, action)| *action)
    }

    /// The first key that runs `action`.
    pub fn key_for(&self, action: Action) -> Option<&Chord> {
        self.bindings
            .iter()
            .find(|(_, bound)| *bound == action)
            .map(|(chord, _)| chord)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_chords() {
        let chord = Chord::parse("Shift-Tab").unwrap();
        assert!(chord.shift && chord.key == "tab");
        assert_eq!(Chord::parse("Return").unwrap().key, "enter");
        assert_eq!(Chord::parse("ctrl--").unwrap().key, "-");
        assert_eq!(Chord::parse("f12").unwrap().label(), "F12");
        assert_eq!(Chord::parse("mod-return").unwrap().label(), "Mod+Enter");
        for wrong in ["", "hyper-j", "f99", "banana", "ctrl-"] {
            assert!(Chord::parse(wrong).is_err(), "{wrong}");
        }
    }

    #[test]
    fn mod_is_alt_or_super() {
        let focus = Chord::parse("mod-j").unwrap();
        let held = |alt, super_key| Held {
            alt,
            super_key,
            ..Held::default()
        };
        assert!(focus.matches("j", held(true, false)));
        assert!(focus.matches("j", held(false, true)));
        assert!(!focus.matches("j", held(false, false)));
        assert!(!Chord::parse("j").unwrap().matches("j", held(true, false)));
    }

    #[test]
    fn themes_rebind_and_unbind() {
        let mut keys = Keymap::default();
        let none = Held::default();
        assert_eq!(
            keys.lookup("f12", none),
            Some(Action::Power(power::Action::PowerOff))
        );
        keys.bind(Chord::parse("f12").unwrap(), Action::Nothing);
        keys.bind(
            Chord::parse("mod-1").unwrap(),
            Action::parse("session:1").unwrap(),
        );
        assert_eq!(keys.lookup("f12", none), Some(Action::Nothing));
        let alt = Held { alt: true, ..none };
        assert_eq!(keys.lookup("1", alt), Some(Action::Session(1)));
        assert_eq!(
            keys.key_for(Action::Power(power::Action::Reboot))
                .map(Chord::label),
            Some("F11".into())
        );
    }

    #[test]
    fn names_actions_both_ways() {
        for name in [
            "submit",
            "tile.swap-master",
            "poweroff",
            "session:3",
            "none",
        ] {
            assert_eq!(Action::parse(name).unwrap().name(), name);
        }
        assert!(Action::parse("session:N").is_err());
        assert!(Action::parse("session:0").is_err());
        assert!(Action::parse("reboot-now").is_err());
    }
}

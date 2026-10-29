//! Text with bindings, such as `{time}`, `{user}@{host}` or `{t.hint}`,
//! filled in from the greeter and the system each time a layer is drawn.

use std::ops::Range;
use std::sync::Arc;
use std::time::Duration;

use jiff::Zoned;

use crate::config::Language;

#[derive(Debug, Clone, PartialEq)]
pub struct Template {
    pieces: Vec<Piece>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Piece {
    Text(Arc<str>),
    Binding {
        binding: Binding,
        /// What follows a colon: a time format, or the glyph `{typed}`
        /// draws with.
        arg: Option<Arc<str>>,
    },
}

/// What a binding stands for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Binding {
    /// The time, with its names in this language whatever the greeter
    /// speaks, if one is given.
    Time(Option<Language>),
    Date(Option<Language>),
    /// The login name.
    User,
    /// The full name, or the login name without one.
    UserName,
    UserInitial,
    Host,
    Session,
    Theme,
    /// PAM's question, such as "Password", without its colon.
    Prompt,
    /// What PAM or greetd last said, if anything.
    Message,
    /// The greeter's own sentence for where the login stands.
    Status,
    Os,
    Arch,
    Kernel,
    Uptime,
    Cpu,
    Gpu,
    Memory,
    Packages,
    Shell,
    /// This monitor's mode, such as "3840×2160 @ 60Hz".
    Display,
    /// The keyboard layout, such as "us".
    Layout,
    Vt,
    Seat,
    Network,
    /// What has been typed, as one glyph a character.
    Typed,
    /// What was last sent, as one glyph a character.
    Sent,
    TilesTitle,
    TilesLayout,
    Item,
    ItemNumber,
    ArmedKey,
    ArmedAction,
    /// `{t.key}`: one of the theme's strings, or one of the greeter's own.
    String(Arc<str>),
}

const NAMES: [(&str, Binding); 37] = [
    ("time", Binding::Time(None)),
    ("time.en", Binding::Time(Some(Language::English))),
    ("time.zh-CN", Binding::Time(Some(Language::Chinese))),
    ("date", Binding::Date(None)),
    ("date.en", Binding::Date(Some(Language::English))),
    ("date.zh-CN", Binding::Date(Some(Language::Chinese))),
    ("user", Binding::User),
    ("user.name", Binding::UserName),
    ("user.initial", Binding::UserInitial),
    ("host", Binding::Host),
    ("session", Binding::Session),
    ("theme", Binding::Theme),
    ("prompt", Binding::Prompt),
    ("message", Binding::Message),
    ("status", Binding::Status),
    ("os", Binding::Os),
    ("arch", Binding::Arch),
    ("kernel", Binding::Kernel),
    ("uptime", Binding::Uptime),
    ("cpu", Binding::Cpu),
    ("gpu", Binding::Gpu),
    ("memory", Binding::Memory),
    ("packages", Binding::Packages),
    ("shell", Binding::Shell),
    ("display", Binding::Display),
    ("layout", Binding::Layout),
    ("vt", Binding::Vt),
    ("seat", Binding::Seat),
    ("network", Binding::Network),
    ("typed", Binding::Typed),
    ("sent", Binding::Sent),
    ("tiles.title", Binding::TilesTitle),
    ("tiles.layout", Binding::TilesLayout),
    ("item", Binding::Item),
    ("item.number", Binding::ItemNumber),
    ("armed.key", Binding::ArmedKey),
    ("armed.action", Binding::ArmedAction),
];

/// What fills a template in.
pub trait Fill {
    fn binding(&self, binding: &Binding, arg: Option<&str>, out: &mut String);
    fn string(&self, key: &str) -> Option<&Template>;
}

/// How deep strings may name other strings.
const DEPTH: usize = 4;

impl Template {
    pub fn literal(text: &str) -> Self {
        Self {
            pieces: vec![Piece::Text(text.into())],
        }
    }

    pub fn parse(text: &str) -> Result<Self, String> {
        let mut pieces = Vec::new();
        let mut literal = String::new();
        let mut chars = text.char_indices().peekable();
        while let Some((at, c)) = chars.next() {
            match c {
                '{' if chars.next_if(|&(_, next)| next == '{').is_some() => literal.push('{'),
                '}' if chars.next_if(|&(_, next)| next == '}').is_some() => literal.push('}'),
                '}' => return Err("a lone \"}\"; write \"}}\" for the character".into()),
                '{' => {
                    let rest = &text[at + 1..];
                    let end = rest.find('}').ok_or_else(|| {
                        format!("\"{{\" without its \"}}\" in {text:?}; write \"{{{{\" for the character")
                    })?;
                    let inside = &rest[..end];
                    if !literal.is_empty() {
                        pieces.push(Piece::Text(std::mem::take(&mut literal).into()));
                    }
                    pieces.push(binding(inside)?);
                    for _ in 0..inside.chars().count() + 1 {
                        chars.next();
                    }
                }
                c => literal.push(c),
            }
        }
        if !literal.is_empty() {
            pieces.push(Piece::Text(literal.into()));
        }
        Ok(Self { pieces })
    }

    pub fn is_empty(&self) -> bool {
        self.pieces.is_empty()
    }

    pub fn bindings(&self) -> impl Iterator<Item = &Binding> {
        self.pieces.iter().filter_map(|piece| match piece {
            Piece::Binding { binding, .. } => Some(binding),
            Piece::Text(_) => None,
        })
    }

    pub fn render(&self, fill: &impl Fill) -> String {
        self.render_typed(fill).0
    }

    /// The text, and the byte ranges of it that `{typed}` and `{sent}`
    /// filled in, in order.
    pub fn render_typed(&self, fill: &impl Fill) -> (String, Vec<Range<usize>>) {
        let mut out = String::new();
        let mut typed = Vec::new();
        self.render_into(fill, &mut out, &mut typed, 0);
        (out, typed)
    }

    fn render_into(
        &self,
        fill: &impl Fill,
        out: &mut String,
        typed: &mut Vec<Range<usize>>,
        depth: usize,
    ) {
        for piece in &self.pieces {
            match piece {
                Piece::Text(text) => out.push_str(text),
                Piece::Binding {
                    binding: binding @ Binding::String(key),
                    ..
                } => match fill.string(key) {
                    Some(string) if depth < DEPTH => {
                        string.render_into(fill, out, typed, depth + 1)
                    }
                    Some(_) => {}
                    // Not the theme's: one of the greeter's own words.
                    None => fill.binding(binding, None, out),
                },
                Piece::Binding { binding, arg } => {
                    let start = out.len();
                    fill.binding(binding, arg.as_deref(), out);
                    if matches!(binding, Binding::Typed | Binding::Sent) && out.len() > start {
                        typed.push(start..out.len());
                    }
                }
            }
        }
    }
}

fn binding(inside: &str) -> Result<Piece, String> {
    let (name, arg) = match inside.split_once(':') {
        Some((name, arg)) => (name.trim(), Some(arg)),
        None => (inside.trim(), None),
    };
    let binding = if let Some(key) = name.strip_prefix("t.") {
        if key.is_empty() {
            return Err("{t.} names no string".into());
        }
        Binding::String(key.into())
    } else {
        NAMES
            .iter()
            .find(|(known, _)| *known == name)
            .map(|(_, binding)| binding.clone())
            .ok_or_else(|| format!("unknown binding {{{name}}}"))?
    };
    match (&binding, arg) {
        (_, None) => {}
        (Binding::Time(_) | Binding::Date(_), Some(format)) => check_format(format)?,
        (Binding::Uptime, Some("short")) => {}
        (Binding::Typed | Binding::Sent, Some(glyph)) if !glyph.is_empty() => {}
        (_, Some(arg)) => return Err(format!("{{{name}}} takes no \":{arg}\"")),
    }
    Ok(Piece::Binding {
        binding,
        arg: arg.map(Into::into),
    })
}

const SPECIFIERS: &str = "YymdeHIMSpAaBbju%";

fn check_format(format: &str) -> Result<(), String> {
    let mut chars = format.chars();
    while let Some(c) = chars.next() {
        if c != '%' {
            continue;
        }
        let mut spec = chars.next();
        if spec == Some('-') {
            spec = chars.next();
        }
        match spec {
            Some(spec) if SPECIFIERS.contains(spec) => {}
            Some(spec) => {
                return Err(format!(
                    "%{spec} is not a time format; use %Y %y %m %d %e %H %I %M %S %p %A %a %B %b %j %u"
                ));
            }
            None => return Err("a time format ends in a lone %".into()),
        }
    }
    Ok(())
}

/// `now` written by a strftime-style `format`, with names in `language`:
/// `%-d` drops a leading zero, `%A` is "Wednesday" or "星期三".
pub fn format_time(now: &Zoned, format: &str, language: Language) -> String {
    const DAYS: [&str; 7] = [
        "Sunday",
        "Monday",
        "Tuesday",
        "Wednesday",
        "Thursday",
        "Friday",
        "Saturday",
    ];
    const MONTHS: [&str; 12] = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];
    const ZH_DAYS: [&str; 7] = ["日", "一", "二", "三", "四", "五", "六"];
    const ZH_MONTHS: [&str; 12] = [
        "一", "二", "三", "四", "五", "六", "七", "八", "九", "十", "十一", "十二",
    ];
    let chinese = language == Language::Chinese;
    let weekday = now.weekday().to_sunday_zero_offset() as usize % 7;
    let month = (now.month() as usize).clamp(1, 12) - 1;
    let hour12 = match now.hour() % 12 {
        0 => 12,
        hour => hour,
    };
    let mut out = String::new();
    let mut chars = format.chars();
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        let mut spec = chars.next();
        let padded = spec != Some('-');
        if !padded {
            spec = chars.next();
        }
        let number = |out: &mut String, value: i64, width: usize| {
            if padded {
                out.push_str(&format!("{value:0width$}"));
            } else {
                out.push_str(&value.to_string());
            }
        };
        match spec {
            Some('Y') => out.push_str(&now.year().to_string()),
            Some('y') => number(&mut out, i64::from(now.year()).rem_euclid(100), 2),
            Some('m') => number(&mut out, now.month().into(), 2),
            Some('d') => number(&mut out, now.day().into(), 2),
            Some('e') => out.push_str(&format!("{:>2}", now.day())),
            Some('H') => number(&mut out, now.hour().into(), 2),
            Some('I') => number(&mut out, hour12.into(), 2),
            Some('M') => number(&mut out, now.minute().into(), 2),
            Some('S') => number(&mut out, now.second().into(), 2),
            Some('j') => number(&mut out, now.day_of_year().into(), 3),
            Some('u') => out.push_str(&now.weekday().to_monday_one_offset().to_string()),
            Some('p') if chinese => out.push_str(if now.hour() < 12 { "上午" } else { "下午" }),
            Some('p') => out.push_str(if now.hour() < 12 { "AM" } else { "PM" }),
            Some('A') if chinese => {
                out.push_str("星期");
                out.push_str(ZH_DAYS[weekday]);
            }
            Some('A') => out.push_str(DAYS[weekday]),
            Some('a') if chinese => {
                out.push('周');
                out.push_str(ZH_DAYS[weekday]);
            }
            Some('a') => out.push_str(&DAYS[weekday][..3]),
            Some('B') if chinese => {
                out.push_str(ZH_MONTHS[month]);
                out.push('月');
            }
            Some('B') => out.push_str(MONTHS[month]),
            Some('b') if chinese => out.push_str(&format!("{}月", month + 1)),
            Some('b') => out.push_str(&MONTHS[month][..3]),
            Some('%') => out.push('%'),
            Some(other) => {
                out.push('%');
                out.push(other);
            }
            None => out.push('%'),
        }
    }
    out
}

/// How long the machine has been up, as fastfetch says it: "2 hours, 5
/// mins", or "2 小时 5 分钟".
pub fn uptime(uptime: Duration, language: Language) -> String {
    let seconds = uptime.as_secs();
    let (days, hours, minutes) = (seconds / 86_400, seconds / 3600 % 24, seconds / 60 % 60);
    let mut parts = Vec::new();
    let chinese = language == Language::Chinese;
    for (count, one, many, zh) in [
        (days, "day", "days", "天"),
        (hours, "hour", "hours", "小时"),
        (minutes, "min", "mins", "分钟"),
    ] {
        if count > 0 {
            parts.push(if chinese {
                format!("{count} {zh}")
            } else {
                format!("{count} {}", if count == 1 { one } else { many })
            });
        }
    }
    if parts.is_empty() {
        return if chinese {
            format!("{seconds} 秒")
        } else {
            format!("{seconds} secs")
        };
    }
    parts.join(if chinese { " " } else { ", " })
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Sample;

    impl Fill for Sample {
        fn binding(&self, binding: &Binding, arg: Option<&str>, out: &mut String) {
            match binding {
                Binding::User => out.push_str("alice"),
                Binding::Host => out.push_str("example-host"),
                Binding::Typed => out.push_str(&arg.unwrap_or("•").repeat(3)),
                _ => out.push('?'),
            }
        }

        fn string(&self, key: &str) -> Option<&Template> {
            static HINT: std::sync::OnceLock<Template> = std::sync::OnceLock::new();
            static LOOP: std::sync::OnceLock<Template> = std::sync::OnceLock::new();
            match key {
                "hint" => Some(HINT.get_or_init(|| Template::parse("Log in to {host}").unwrap())),
                "loop" => Some(LOOP.get_or_init(|| Template::parse("again {t.loop}").unwrap())),
                _ => None,
            }
        }
    }

    #[test]
    fn fills_bindings_and_strings() {
        let line = Template::parse("{user}@{host}: {t.hint} {{x}} {typed:*}").unwrap();
        assert_eq!(
            line.render(&Sample),
            "alice@example-host: Log in to example-host {x} ***"
        );
        let looping = Template::parse("{t.loop}").unwrap().render(&Sample);
        assert_eq!(looping.matches("again").count(), DEPTH);
    }

    #[test]
    fn says_where_typed_characters_stand() {
        let (text, typed) = Template::parse("{t.hint}: {typed:*} {user} {typed}")
            .unwrap()
            .render_typed(&Sample);
        assert_eq!(text, "Log in to example-host: *** alice •••");
        let pieces: Vec<&str> = typed.iter().map(|range| &text[range.clone()]).collect();
        assert_eq!(pieces, ["***", "•••"]);
    }

    #[test]
    fn refuses_what_it_cannot_fill() {
        for wrong in [
            "{tim}",
            "{user",
            "a}b",
            "{host:x}",
            "{time:%Q}",
            "{t.}",
            "{typed:}",
            "{date.fr:%a}",
        ] {
            assert!(Template::parse(wrong).is_err(), "{wrong}");
        }
    }

    #[test]
    fn dates_can_keep_one_language() {
        let bar = Template::parse("{date.en:%a %b %-d} {time.zh-CN:%p} {date}").unwrap();
        let bindings: Vec<&Binding> = bar.bindings().collect();
        assert_eq!(
            bindings,
            [
                &Binding::Date(Some(Language::English)),
                &Binding::Time(Some(Language::Chinese)),
                &Binding::Date(None),
            ]
        );
    }

    fn wednesday() -> Zoned {
        "2026-10-28T09:04:05[America/Los_Angeles]".parse().unwrap()
    }

    #[test]
    fn writes_dates_in_each_language() {
        let now = wednesday();
        assert_eq!(
            format_time(&now, "%a %b %-d %Y %H:%M:%S", Language::English),
            "Wed Oct 28 2026 09:04:05"
        );
        assert_eq!(
            format_time(&now, "%A, %-d %B", Language::English),
            "Wednesday, 28 October"
        );
        assert_eq!(
            format_time(&now, "%Y年%-m月%-d日 %a", Language::Chinese),
            "2026年10月28日 周三"
        );
        assert_eq!(
            format_time(&now, "%A %B %p", Language::Chinese),
            "星期三 十月 上午"
        );
        assert_eq!(format_time(&now, "%-I:%M %p", Language::English), "9:04 AM");
    }

    #[test]
    fn says_uptime_as_fetch_does() {
        let minutes = |m: u64| Duration::from_secs(m * 60);
        assert_eq!(uptime(minutes(2), Language::English), "2 mins");
        assert_eq!(uptime(minutes(65), Language::English), "1 hour, 5 mins");
        assert_eq!(
            uptime(minutes(60 * 49 + 1), Language::English),
            "2 days, 1 hour, 1 min"
        );
        assert_eq!(uptime(minutes(65), Language::Chinese), "1 小时 5 分钟");
        assert_eq!(
            uptime(Duration::from_secs(40), Language::English),
            "40 secs"
        );
    }
}

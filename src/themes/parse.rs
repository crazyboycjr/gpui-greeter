//! Reading theme.toml: every key checked against what the format knows,
//! every problem named with its line, into the layers the greeter draws.

use std::collections::HashMap;
use std::ops::Range;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use toml_edit::{Document, Item, TableLike};

use super::color::Color;
use super::facts::Uses;
use super::files::{self, Asset, AssetKind};
use super::keys::{Action, Chord, Keymap};
use super::layer::*;
use super::motion::{Animated, Easing, Keyframe, Motion, Transition};
use super::template::{Binding, Template};
use super::tiles;
use super::{FORMAT, Folder, MANIFEST, Problem, Severity, Strings};

/// The greeter's own words a theme may use as `{t.key}`.
pub const BUILT_IN_STRINGS: [&str; 12] = [
    "password",
    "press_enter",
    "checking",
    "wrong_password",
    "welcome_back",
    "starting",
    "session",
    "current",
    "sleep",
    "restart",
    "shut_down",
    "click_to_log_in",
];

const MAX_MANIFEST: u64 = 1 << 20;
const MAX_LAYERS: usize = 2_000;
const MAX_DEPTH: usize = 32;
const LONGEST: Duration = Duration::from_secs(60);

/// Collects problems, with lines from spans of the manifest's text.
struct Reader<'a> {
    text: &'a str,
    problems: &'a mut Vec<Problem>,
}

impl Reader<'_> {
    fn line(&self, span: Option<Range<usize>>) -> Option<usize> {
        span.map(|span| {
            let end = span.start.min(self.text.len());
            self.text[..end].matches('\n').count() + 1
        })
    }

    fn report(&mut self, severity: Severity, span: Option<Range<usize>>, message: String) {
        let line = self.line(span);
        self.problems.push(Problem {
            severity,
            file: MANIFEST.into(),
            line,
            message,
        });
    }

    fn error(&mut self, span: Option<Range<usize>>, message: impl Into<String>) {
        self.report(Severity::Error, span, message.into());
    }

    fn warn(&mut self, span: Option<Range<usize>>, message: impl Into<String>) {
        self.report(Severity::Warning, span, message.into());
    }

    fn file_error(&mut self, file: &str, message: String) {
        self.problems.push(Problem {
            severity: Severity::Error,
            file: file.into(),
            line: None,
            message,
        });
    }
}

/// A table's keys, read one by one; what is left unread is unknown.
struct Fields<'t> {
    table: &'t dyn TableLike,
    /// Where the table starts, for problems with no better place.
    span: Option<Range<usize>>,
    /// What the table is, for messages: "layer \"clock\"".
    what: String,
    used: Vec<String>,
}

fn item_span(item: &Item) -> Option<Range<usize>> {
    match item {
        Item::Value(value) => value.span(),
        Item::Table(table) => table.span(),
        Item::ArrayOfTables(array) => array.span(),
        Item::None => None,
    }
}

fn as_number(item: &Item) -> Option<f32> {
    item.as_integer()
        .map(|n| n as f32)
        .or_else(|| item.as_float().map(|n| n as f32))
        .filter(|n| n.is_finite())
}

fn value_number(value: &toml_edit::Value) -> Option<f32> {
    value
        .as_integer()
        .map(|n| n as f32)
        .or_else(|| value.as_float().map(|n| n as f32))
        .filter(|n| n.is_finite())
}

impl<'t> Fields<'t> {
    fn new(table: &'t dyn TableLike, span: Option<Range<usize>>, what: String) -> Self {
        Self {
            table,
            span,
            what,
            used: Vec::new(),
        }
    }

    fn item(&mut self, key: &str) -> Option<&'t Item> {
        self.used.push(key.to_string());
        self.table.get(key).filter(|item| !item.is_none())
    }

    fn has(&self, key: &str) -> bool {
        self.table.contains_key(key)
    }

    fn span(&self, key: &str) -> Option<Range<usize>> {
        self.table
            .get_key_value(key)
            .and_then(|(key, item)| key.span().or_else(|| item_span(item)))
            .or_else(|| self.span.clone())
    }

    fn wrong(&self, r: &mut Reader, key: &str, expected: &str) {
        r.error(
            self.span(key),
            format!("{}: `{key}` should be {expected}", self.what),
        );
    }

    fn number(&mut self, r: &mut Reader, key: &str) -> Option<f32> {
        let item = self.item(key)?;
        let number = as_number(item);
        if number.is_none() {
            self.wrong(r, key, "a number");
        }
        number
    }

    fn number_in(&mut self, r: &mut Reader, key: &str, range: Range<f32>) -> Option<f32> {
        let number = self.number(r, key)?;
        if !(range.start..=range.end).contains(&number) {
            self.wrong(
                r,
                key,
                &format!("between {} and {}", range.start, range.end),
            );
            return None;
        }
        Some(number)
    }

    fn integer(&mut self, r: &mut Reader, key: &str, range: Range<i64>) -> Option<i64> {
        let item = self.item(key)?;
        match item.as_integer() {
            Some(n) if (range.start..=range.end).contains(&n) => Some(n),
            _ => {
                self.wrong(
                    r,
                    key,
                    &format!("a whole number from {} to {}", range.start, range.end),
                );
                None
            }
        }
    }

    fn boolean(&mut self, r: &mut Reader, key: &str) -> Option<bool> {
        let item = self.item(key)?;
        let value = item.as_bool();
        if value.is_none() {
            self.wrong(r, key, "true or false");
        }
        value
    }

    fn string(&mut self, r: &mut Reader, key: &str) -> Option<&'t str> {
        let item = self.item(key)?;
        let value = item.as_str();
        if value.is_none() {
            self.wrong(r, key, "a string");
        }
        value
    }

    /// A string that must be one of `names`.
    fn choice<T: Copy>(&mut self, r: &mut Reader, key: &str, names: &[(&str, T)]) -> Option<T> {
        let text = self.string(r, key)?;
        let found = names
            .iter()
            .find(|(name, _)| *name == text)
            .map(|(_, value)| *value);
        if found.is_none() {
            let options: Vec<String> = names.iter().map(|(name, _)| format!("{name:?}")).collect();
            self.wrong(r, key, &format!("one of {}", options.join(", ")));
        }
        found
    }

    fn color(&mut self, r: &mut Reader, key: &str) -> Option<Color> {
        let text = self.string(r, key)?;
        match Color::parse(text) {
            Ok(color) => Some(color),
            Err(message) => {
                r.error(self.span(key), format!("{}: `{key}`: {message}", self.what));
                None
            }
        }
    }

    /// Milliseconds, up to a minute.
    fn millis(&mut self, r: &mut Reader, key: &str) -> Option<Duration> {
        let ms = self.number(r, key)?;
        if !(0.0..=LONGEST.as_millis() as f32).contains(&ms) {
            self.wrong(r, key, "milliseconds from 0 to 60000");
            return None;
        }
        Some(Duration::from_secs_f32(ms / 1000.0))
    }

    fn length(&mut self, r: &mut Reader, key: &str) -> Option<Length> {
        let item = self.item(key)?;
        if let Some(px) = as_number(item) {
            return Some(Length::Px(px));
        }
        let percent = item
            .as_str()
            .and_then(|text| text.strip_suffix('%'))
            .and_then(|number| number.trim().parse::<f32>().ok())
            .filter(|n| n.is_finite());
        if percent.is_none() {
            self.wrong(
                r,
                key,
                "frame pixels, or a share of the parent like \"100%\"",
            );
        }
        percent.map(|percent| Length::Percent(percent / 100.0))
    }

    /// A number or an array of numbers.
    fn numbers(&mut self, r: &mut Reader, key: &str) -> Option<Vec<f32>> {
        let item = self.item(key)?;
        if let Some(n) = as_number(item) {
            return Some(vec![n]);
        }
        let numbers = item
            .as_array()
            .map(|array| array.iter().map(value_number).collect::<Option<Vec<f32>>>());
        match numbers {
            Some(Some(numbers)) if (1..=4).contains(&numbers.len()) => Some(numbers),
            _ => {
                self.wrong(r, key, "a number, or an array of one to four numbers");
                None
            }
        }
    }

    /// Top, right, bottom and left, written as CSS writes padding.
    fn sides(&mut self, r: &mut Reader, key: &str) -> Option<[f32; 4]> {
        let n = self.numbers(r, key)?;
        Some(match n[..] {
            [all] => [all; 4],
            [vertical, horizontal] => [vertical, horizontal, vertical, horizontal],
            [top, horizontal, bottom] => [top, horizontal, bottom, horizontal],
            [top, right, bottom, left] => [top, right, bottom, left],
            _ => return None,
        })
    }

    /// Top left, top right, bottom right and bottom left.
    fn corners(&mut self, r: &mut Reader, key: &str) -> Option<[f32; 4]> {
        let n = self.numbers(r, key)?;
        Some(match n[..] {
            [all] => [all; 4],
            [a, b] => [a, b, a, b],
            [a, b, c] => [a, b, c, b],
            [a, b, c, d] => [a, b, c, d],
            _ => return None,
        })
    }

    fn pair(&mut self, r: &mut Reader, key: &str) -> Option<(f32, f32)> {
        let n = self.numbers(r, key)?;
        match n[..] {
            [a, b] => Some((a, b)),
            _ => {
                self.wrong(r, key, "two numbers, [x, y]");
                None
            }
        }
    }

    fn template(&mut self, r: &mut Reader, key: &str) -> Option<Template> {
        let text = self.string(r, key)?;
        match Template::parse(text) {
            Ok(template) => Some(template),
            Err(message) => {
                r.error(self.span(key), format!("{}: `{key}`: {message}", self.what));
                None
            }
        }
    }

    fn strings(&mut self, r: &mut Reader, key: &str) -> Option<Vec<&'t str>> {
        let item = self.item(key)?;
        let strings = item.as_array().and_then(|array| {
            array
                .iter()
                .map(|value| value.as_str())
                .collect::<Option<Vec<_>>>()
        });
        if strings.is_none() {
            self.wrong(r, key, "an array of strings");
        }
        strings
    }

    fn table(
        &mut self,
        r: &mut Reader,
        key: &str,
    ) -> Option<(&'t dyn TableLike, Option<Range<usize>>)> {
        let item = self.item(key)?;
        match item.as_table_like() {
            Some(table) => Some((table, item_span(item).or_else(|| self.span(key)))),
            None => {
                self.wrong(r, key, "a table");
                None
            }
        }
    }

    /// Reports the keys nobody read, with the nearest known name.
    fn finish(self, r: &mut Reader) {
        for (key, item) in self.table.iter() {
            if self.used.iter().any(|used| used == key) {
                continue;
            }
            let span = self
                .table
                .get_key_value(key)
                .and_then(|(key, _)| key.span())
                .or_else(|| item_span(item));
            let hint = self
                .used
                .iter()
                .filter(|known| distance(known, key) <= 2)
                .min_by_key(|known| distance(known, key))
                .map(|known| format!("; did you mean `{known}`?"))
                .unwrap_or_default();
            r.error(span, format!("{}: unknown key `{key}`{hint}", self.what));
        }
    }
}

/// Levenshtein distance, for suggestions.
fn distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut previous = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let current = row[j + 1];
            row[j + 1] = (previous + usize::from(ca != *cb))
                .min(row[j] + 1)
                .min(current + 1);
            previous = current;
        }
    }
    row[b.len()]
}

/// The files a theme refers to, each loaded once.
struct Assets<'a> {
    dir: &'a Path,
    assets: Vec<Asset>,
    by_path: HashMap<String, Option<AssetId>>,
}

impl Assets<'_> {
    fn get(&mut self, r: &mut Reader, span: Option<Range<usize>>, path: &str) -> Option<AssetId> {
        if let Some(found) = self.by_path.get(path) {
            return *found;
        }
        let loaded = match files::load_asset(self.dir, path) {
            Ok(asset) => {
                self.assets.push(asset);
                Some(self.assets.len() - 1)
            }
            Err(message) => {
                if files::resolve(self.dir, path).is_ok() {
                    r.file_error(path, message);
                } else {
                    r.error(span, message);
                }
                None
            }
        };
        self.by_path.insert(path.to_string(), loaded);
        loaded
    }
}

pub fn folder(dir: &Path, name: &str, problems: &mut Vec<Problem>) -> Option<Folder> {
    let text = match read_manifest(dir) {
        Ok(text) => text,
        Err(message) => {
            problems.push(Problem {
                severity: Severity::Error,
                file: MANIFEST.into(),
                line: None,
                message,
            });
            return None;
        }
    };
    let document = match Document::parse(text.as_str()) {
        Ok(document) => document,
        Err(error) => {
            let line = error
                .span()
                .map(|span| text[..span.start.min(text.len())].matches('\n').count() + 1);
            problems.push(Problem {
                severity: Severity::Error,
                file: MANIFEST.into(),
                line,
                message: error.message().to_string(),
            });
            return None;
        }
    };
    let mut r = Reader {
        text: &text,
        problems,
    };
    let mut assets = Assets {
        dir,
        assets: Vec::new(),
        by_path: HashMap::new(),
    };
    let mut top = Fields::new(document.as_table(), Some(0..0), "theme.toml".into());

    if let Some(format) = top.integer(&mut r, "format", 1..i64::MAX)
        && format > FORMAT
    {
        r.error(
            top.span("format"),
            format!("format {format} is newer than this greeter reads ({FORMAT})"),
        );
        return None;
    }
    if !top.has("format") {
        r.warn(
            Some(0..0),
            "theme.toml: say which format it is written in: format = 1",
        );
    }
    let title = top
        .string(&mut r, "name")
        .map(str::to_string)
        .unwrap_or_else(|| name.to_string());
    top.string(&mut r, "description");
    let background = top.color(&mut r, "background").unwrap_or(Color::BLACK);
    let rest = top.boolean(&mut r, "rest").unwrap_or(false);
    let rest_after = top
        .number_in(&mut r, "rest_after", 0.0..86_400.0)
        .unwrap_or(60.0);
    let welcome = top
        .number_in(&mut r, "welcome", 300.0..5_000.0)
        .unwrap_or(1_200.0);

    let mut fonts = Vec::new();
    let fonts_span = top.span("fonts");
    for path in top.strings(&mut r, "fonts").unwrap_or_default() {
        match files::load_font(dir, path) {
            Ok(font) => fonts.push(font),
            Err(message) => r.error(fonts_span.clone(), message),
        }
    }

    let mut text_style = Props::default();
    if let Some((table, span)) = top.table(&mut r, "text") {
        let mut fields = Fields::new(table, span, "[text]".into());
        read_text_style(&mut fields, &mut r, &mut text_style);
        fields.finish(&mut r);
    }

    let strings = read_strings(&mut top, &mut r);
    let mut keys = Keymap::default();
    let mut key_actions = Vec::new();
    if let Some((table, span)) = top.table(&mut r, "keys") {
        for (chord, item) in table.iter() {
            let key_span = table
                .get_key_value(chord)
                .and_then(|(key, _)| key.span())
                .or_else(|| span.clone());
            let action = item.as_str().map(Action::parse);
            match (Chord::parse(chord), action) {
                (Ok(chord), Some(Ok(action))) => {
                    keys.bind(chord, action);
                    key_actions.push((action, key_span));
                }
                (Err(message), _) => r.error(key_span, format!("[keys]: {message}")),
                (_, Some(Err(message))) => r.error(key_span, format!("[keys]: {message}")),
                (_, None) => r.error(key_span, format!("[keys]: {chord:?} should name an action")),
            }
        }
    }

    let layers = read_layers(&mut top, &mut r, &mut assets);
    top.finish(&mut r);
    let mut layers = layers?;
    let roots: Vec<usize> = (0..layers.len())
        .filter(|&i| layers[i].parent.is_none())
        .collect();
    for i in 0..layers.len() {
        if let Some(parent) = layers[i].parent {
            layers[parent].children.push(i);
        }
    }

    let tiles = check_tiles(&layers, &mut r);
    let wallpaper = layers.iter().find_map(|layer| match layer.kind {
        Kind::Wallpaper { blur, .. } => Some(blur),
        _ => None,
    });
    let mut uses = Uses::default();
    check_layers(
        &layers,
        &assets.assets,
        tiles.is_some(),
        &strings,
        &mut uses,
        &mut r,
    );
    for (action, span) in key_actions {
        if matches!(action, Action::Tile(_)) && tiles.is_none() {
            r.error(
                span.clone(),
                format!("[keys]: {} needs a tiles layer", action.name()),
            );
        }
        if action == Action::Pick {
            r.error(span, "[keys]: pick is for clicks on a for_each layer");
        }
    }
    check_strings(&strings, &mut uses, tiles.is_some(), &mut r);
    check_fonts(&layers, &text_style, &fonts, fonts_span, &mut r);

    Some(Folder {
        name: name.to_string(),
        title,
        dir: dir.to_path_buf(),
        background,
        rest,
        rest_after: (rest_after > 0.0).then(|| Duration::from_secs_f32(rest_after)),
        welcome: Duration::from_secs_f32(welcome / 1000.0),
        text: text_style,
        strings,
        keys,
        layers,
        roots,
        fonts,
        assets: assets.assets,
        tiles,
        wallpaper,
        uses,
    })
}

fn read_manifest(dir: &Path) -> Result<String, String> {
    let path =
        files::resolve(dir, MANIFEST).map_err(|_| format!("no {MANIFEST} in {}", dir.display()))?;
    let length = std::fs::metadata(&path)
        .map_err(|error| error.to_string())?
        .len();
    if length > MAX_MANIFEST {
        return Err(format!("{MANIFEST} is larger than 1 MiB"));
    }
    std::fs::read_to_string(&path).map_err(|error| format!("cannot read it: {error}"))
}

fn read_strings(top: &mut Fields, r: &mut Reader) -> Strings {
    let mut strings = Strings::default();
    let Some((table, span)) = top.table(r, "strings") else {
        return strings;
    };
    let mut languages = Fields::new(table, span, "[strings]".into());
    for (language, map) in [
        ("en", &mut strings.english),
        ("zh-CN", &mut strings.chinese),
    ] {
        let Some((table, span)) = languages.table(r, language) else {
            continue;
        };
        let mut fields = Fields::new(table, span, format!("[strings.{language}]"));
        let keys: Vec<String> = table.iter().map(|(key, _)| key.to_string()).collect();
        for key in keys {
            let valid = key
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'));
            if !valid {
                r.error(
                    fields.span(&key),
                    format!("[strings.{language}]: {key:?} should be letters, digits, _ - ."),
                );
            }
            if let Some(template) = fields.template(r, &key) {
                map.insert(key.as_str().into(), template);
            }
        }
        fields.finish(r);
    }
    languages.finish(r);
    strings
}

const KINDS: [&str; 7] = ["box", "text", "field", "image", "svg", "wallpaper", "tiles"];

fn read_layers(top: &mut Fields, r: &mut Reader, assets: &mut Assets) -> Option<Vec<Layer>> {
    let Some(item) = top.item("layer") else {
        r.error(Some(0..0), "theme.toml has no [[layer]]");
        return None;
    };
    let Some(tables) = item.as_array_of_tables() else {
        r.error(
            item_span(item),
            "`layer` should be written as [[layer]] tables",
        );
        return None;
    };
    if tables.len() > MAX_LAYERS {
        r.error(item_span(item), format!("more than {MAX_LAYERS} layers"));
        return None;
    }
    // Every id first, so that a tiles layer can name a pane that follows it.
    let mut ids: HashMap<String, usize> = HashMap::new();
    for (index, table) in tables.iter().enumerate() {
        if let Some(id) = table.get("id").and_then(|item| item.as_str())
            && ids.insert(id.to_string(), index).is_some()
        {
            r.error(
                table.get("id").and_then(item_span),
                format!("layer id {id:?} is taken by an earlier layer"),
            );
        }
    }
    let mut layers: Vec<Layer> = Vec::with_capacity(tables.len());
    for (index, table) in tables.iter().enumerate() {
        let line = r.line(table.span()).unwrap_or(0);
        let layer = read_layer(table, index, line, &ids, &layers, r, assets);
        layers.push(layer);
    }
    Some(layers)
}

fn read_layer(
    table: &toml_edit::Table,
    index: usize,
    line: usize,
    ids: &HashMap<String, usize>,
    earlier: &[Layer],
    r: &mut Reader,
    assets: &mut Assets,
) -> Layer {
    let id = table
        .get("id")
        .and_then(|item| item.as_str())
        .map(Arc::<str>::from);
    let what = match &id {
        Some(id) => format!("layer {id:?}"),
        None => format!("the layer on line {line}"),
    };
    let mut f = Fields::new(table, table.span(), what.clone());
    f.string(r, "id");

    let parent = f
        .string(r, "parent")
        .and_then(|parent| match ids.get(parent) {
            Some(&at) if at < index => Some(at),
            Some(_) => {
                r.error(
                    f.span("parent"),
                    format!("{what}: its parent {parent:?} must come before it"),
                );
                None
            }
            None => {
                r.error(
                    f.span("parent"),
                    format!("{what}: no layer {parent:?} to sit in"),
                );
                None
            }
        });
    let depth =
        std::iter::successors(parent, |&at| earlier.get(at).and_then(|layer| layer.parent)).count();
    if depth >= MAX_DEPTH {
        r.error(
            f.span("parent"),
            format!("{what}: layers nest deeper than {MAX_DEPTH}"),
        );
    }
    let parent_kind = parent
        .and_then(|at| earlier.get(at))
        .map(|layer| &layer.kind);
    let in_tiles = matches!(parent_kind, Some(Kind::Tiles(_)));

    let kind_name = f.string(r, "kind").unwrap_or("box");
    if !KINDS.contains(&kind_name) {
        r.error(
            f.span("kind"),
            format!(
                "{what}: unknown kind {kind_name:?}; the kinds are {}",
                KINDS.join(", ")
            ),
        );
    }
    let picture = |f: &mut Fields, r: &mut Reader| {
        let fit = f
            .choice(
                r,
                "fit",
                &[
                    ("contain", Fit::Contain),
                    ("cover", Fit::Cover),
                    ("fill", Fit::Stretch),
                ],
            )
            .unwrap_or(Fit::Contain);
        let focus = f.pair(r, "position").unwrap_or((0.5, 0.5));
        Picture {
            fit,
            focus: (focus.0.clamp(0.0, 1.0), focus.1.clamp(0.0, 1.0)),
        }
    };
    let kind = match kind_name {
        "text" => Kind::Text,
        "field" => Kind::Field {
            mask: f.string(r, "mask").unwrap_or("\u{2022}").into(),
            max: f.integer(r, "max", 0..256).unwrap_or(64) as usize,
            blink: f.millis(r, "blink").unwrap_or(Duration::from_millis(1060)),
            caret_width: f.number_in(r, "caret_width", 0.0..400.0),
        },
        "image" => Kind::Image(picture(&mut f, r)),
        "svg" => Kind::Svg {
            picture: picture(&mut f, r),
            tint: f.boolean(r, "tint").unwrap_or(false),
        },
        "wallpaper" => Kind::Wallpaper {
            picture: picture(&mut f, r),
            blur: f.number_in(r, "blur", 0.0..400.0).unwrap_or(0.0),
        },
        "tiles" => Kind::Tiles(read_tiles(&mut f, r)),
        _ => Kind::Box {
            flow: f
                .choice(
                    r,
                    "layout",
                    &[
                        ("stack", Flow::Stack),
                        ("row", Flow::Row),
                        ("column", Flow::Column),
                    ],
                )
                .unwrap_or(Flow::Stack),
            clips: f
                .choice(r, "overflow", &[("visible", false), ("hidden", true)])
                .unwrap_or(false),
        },
    };

    let anchor = f
        .string(r, "anchor")
        .and_then(|text| {
            let anchor = Anchor::parse(text);
            if anchor.is_none() {
                r.error(f.span("anchor"), format!("{what}: unknown anchor {text:?}; use top-left, top, top-right, left, center, right, bottom-left, bottom, bottom-right or fill"));
            }
            anchor
        })
        .unwrap_or(Anchor::TopLeft);
    let inset = f.sides(r, "inset").unwrap_or([0.0; 4]);
    if f.has("inset") && anchor != Anchor::Fill {
        r.warn(
            f.span("inset"),
            format!("{what}: inset applies only with anchor = \"fill\""),
        );
    }
    let grow = f.number_in(r, "grow", 0.0..1000.0).unwrap_or(0.0);
    let monitors = f
        .choice(
            r,
            "monitors",
            &[
                ("form", Monitors::Form),
                ("all", Monitors::All),
                ("others", Monitors::Others),
            ],
        )
        .unwrap_or(Monitors::Form);
    if f.has("monitors") && parent.is_some() {
        r.error(
            f.span("monitors"),
            format!("{what}: only a layer on the screen itself picks its monitors"),
        );
    }

    let action = f
        .string(r, "action")
        .and_then(|text| match Action::parse(text) {
            Ok(action) => Some(action),
            Err(message) => {
                r.error(f.span("action"), format!("{what}: {message}"));
                None
            }
        });
    let mut fired_by: Vec<Action> = action.into_iter().collect();
    for name in f.strings(r, "fired_by").unwrap_or_default() {
        match Action::parse(name) {
            Ok(other) if !fired_by.contains(&other) => fired_by.push(other),
            Ok(_) => {}
            Err(message) => r.error(f.span("fired_by"), format!("{what}: {message}")),
        }
    }
    let fired_for = f
        .millis(r, "fired_for")
        .unwrap_or(Duration::from_millis(700));

    let mut repeat = Repeat::Once;
    if let Some(each) = f.string(r, "for_each") {
        if each == "session" {
            repeat = Repeat::Sessions;
        } else {
            r.error(
                f.span("for_each"),
                format!("{what}: for_each takes \"session\""),
            );
        }
    }
    if let Some((table, span)) = f.table(r, "copies") {
        if repeat != Repeat::Once {
            r.error(
                span.clone(),
                format!("{what}: a layer has for_each or copies, not both"),
            );
        }
        let mut c = Fields::new(table, span, format!("{what}: copies"));
        repeat = Repeat::Copies {
            count: c.integer(r, "count", 1..512).unwrap_or(1) as usize,
            x: c.number(r, "x").unwrap_or(0.0),
            y: c.number(r, "y").unwrap_or(0.0),
            rotate: c.number(r, "rotate").unwrap_or(0.0),
            delay: c.millis(r, "delay").unwrap_or(Duration::ZERO),
        };
        c.finish(r);
    }
    let stagger = f.millis(r, "stagger").unwrap_or(Duration::ZERO);

    let pane = in_tiles.then(|| read_pane(&mut f, r, &what));
    if !in_tiles {
        for key in [
            "title",
            "content",
            "min_scale",
            "max_scale",
            "max_scale_full",
        ] {
            if f.has(key) {
                f.item(key);
                r.error(
                    f.span(key),
                    format!("{what}: `{key}` is for a pane, a layer in a tiles layer"),
                );
            }
        }
    }

    let transition = read_transition(&mut f, r, "transition").unwrap_or_default();
    let motions = read_motions(&mut f, r, &what);
    let mut base = Props::default();
    read_props(&mut f, r, assets, ids, &kind, &mut base, &what);
    let overrides = read_overrides(&mut f, r, assets, ids, &kind, &what);
    f.finish(r);

    // A key written but wrong is reported where it is read.
    let mentions = |key: &str| {
        table.contains_key(key)
            || table
                .get("on")
                .and_then(Item::as_table_like)
                .is_some_and(|on| {
                    on.iter().any(|(_, item)| {
                        item.as_table_like()
                            .is_some_and(|inner| inner.contains_key(key))
                    })
                })
    };
    if matches!(kind, Kind::Text) && !mentions("text") {
        r.error(table.span(), format!("{what}: a text layer needs `text`"));
    }
    if matches!(kind, Kind::Image(_) | Kind::Svg { .. }) && !mentions("file") {
        r.error(
            table.span(),
            format!("{what}: an {} layer needs `file`", kind.name()),
        );
    }

    Layer {
        id,
        line,
        kind,
        parent,
        children: Vec::new(),
        anchor,
        inset,
        grow,
        monitors,
        base,
        overrides,
        transition,
        motions,
        action,
        fired_by,
        fired_for,
        repeat,
        stagger,
        pane,
    }
}

fn read_tiles(f: &mut Fields, r: &mut Reader) -> Tiles {
    let layouts = match f.strings(r, "layouts") {
        Some(names) => names
            .into_iter()
            .filter_map(|name| match tiles::Layout::parse(name) {
                Ok(layout) => Some(layout),
                Err(message) => {
                    r.error(f.span("layouts"), message);
                    None
                }
            })
            .collect(),
        None => vec![
            tiles::Layout::Tall,
            tiles::Layout::Mirror,
            tiles::Layout::Full,
        ],
    };
    if layouts.is_empty() {
        r.error(f.span("layouts"), "a tiles layer needs at least one layout");
    }
    Tiles {
        layouts,
        ratio: f.number_in(r, "ratio", 0.1..0.9).unwrap_or(0.5),
        ratio_step: f.number_in(r, "ratio_step", 0.0..0.5).unwrap_or(0.03),
        border: f.number_in(r, "pane_border", 0.0..100.0).unwrap_or(0.0),
        focused: f
            .color(r, "focused_color")
            .unwrap_or(Color::rgba(1.0, 0.0, 0.0, 1.0)),
        unfocused: f.color(r, "unfocused_color").unwrap_or(Color::rgba(
            0xdd as f32 / 255.0,
            0xdd as f32 / 255.0,
            0xdd as f32 / 255.0,
            1.0,
        )),
    }
}

fn read_pane(f: &mut Fields, r: &mut Reader, what: &str) -> Pane {
    let fills = f.table.get("content").and_then(Item::as_str) == Some("fill");
    let content = if fills {
        f.item("content");
        None
    } else {
        let content = f.pair(r, "content");
        if content.is_none() {
            r.error(f.span.clone(), format!("{what}: a pane needs `content = [width, height]`, the size its content is laid out on, or content = \"fill\""));
        }
        Some(
            content
                .filter(|(w, h)| *w > 0.0 && *h > 0.0)
                .unwrap_or((1000.0, 1000.0)),
        )
    };
    let max_scale = f.number_in(r, "max_scale", 0.05..20.0).unwrap_or(1.0);
    Pane {
        title: f
            .template(r, "title")
            .unwrap_or_else(|| Template::literal("")),
        content,
        padding: f.number_in(r, "padding", 0.0..2000.0).unwrap_or(0.0),
        min_scale: f.number_in(r, "min_scale", 0.05..20.0).unwrap_or(0.3),
        max_scale,
        max_scale_full: f
            .number_in(r, "max_scale_full", 0.05..20.0)
            .unwrap_or(max_scale),
    }
}

fn read_transition(f: &mut Fields, r: &mut Reader, key: &str) -> Option<Transition> {
    let item = f.item(key)?;
    if let Some(ms) = as_number(item) {
        if !(0.0..=LONGEST.as_millis() as f32).contains(&ms) {
            f.wrong(r, key, "milliseconds from 0 to 60000");
            return None;
        }
        return Some(Transition {
            duration: Duration::from_secs_f32(ms / 1000.0),
            ..Transition::default()
        });
    }
    let Some(table) = item.as_table_like() else {
        f.wrong(r, key, "milliseconds, or { duration, easing, delay }");
        return None;
    };
    let mut t = Fields::new(table, item_span(item), format!("{}: {key}", f.what));
    let transition = Transition {
        duration: t.millis(r, "duration").unwrap_or(Duration::ZERO),
        easing: read_easing(&mut t, r, "easing").unwrap_or(Easing::EASE),
        delay: t.millis(r, "delay").unwrap_or(Duration::ZERO),
    };
    t.finish(r);
    Some(transition)
}

fn read_easing(f: &mut Fields, r: &mut Reader, key: &str) -> Option<Easing> {
    let text = f.string(r, key)?;
    match Easing::parse(text) {
        Ok(easing) => Some(easing),
        Err(message) => {
            r.error(f.span(key), format!("{}: {message}", f.what));
            None
        }
    }
}

fn read_motions(f: &mut Fields, r: &mut Reader, what: &str) -> Vec<Motion> {
    let Some(item) = f.item("motion") else {
        return Vec::new();
    };
    let mut tables: Vec<(&dyn TableLike, Option<Range<usize>>)> = Vec::new();
    match item {
        Item::ArrayOfTables(array) => {
            for table in array.iter() {
                tables.push((table, table.span()));
            }
        }
        Item::Value(toml_edit::Value::Array(array)) => {
            for value in array.iter() {
                match value.as_inline_table() {
                    Some(table) => tables.push((table, value.span())),
                    None => f.wrong(r, "motion", "a table, or an array of them"),
                }
            }
        }
        item => match item.as_table_like() {
            Some(table) => tables.push((table, item_span(item))),
            None => f.wrong(r, "motion", "a table, or an array of them"),
        },
    }
    tables
        .into_iter()
        .filter_map(|(table, span)| read_motion(table, span, r, what))
        .collect()
}

fn read_motion(
    table: &dyn TableLike,
    span: Option<Range<usize>>,
    r: &mut Reader,
    what: &str,
) -> Option<Motion> {
    let mut m = Fields::new(table, span.clone(), format!("{what}: motion"));
    let property = m.choice(
        r,
        "property",
        &[
            ("x", Animated::X),
            ("y", Animated::Y),
            ("rotate", Animated::Rotate),
            ("scale", Animated::Scale),
            ("opacity", Animated::Opacity),
        ],
    );
    if !m.has("property") {
        r.error(span.clone(), format!("{what}: a motion needs `property`"));
    }
    let duration = m.millis(r, "duration");
    if duration.is_none_or(|d| d.is_zero()) {
        r.error(
            span.clone(),
            format!("{what}: a motion needs a `duration` above 0"),
        );
    }
    let easing = read_easing(&mut m, r, "easing").unwrap_or(Easing::Linear);
    let mut frames = Vec::new();
    if let Some(item) = m.item("keyframes") {
        let Some(array) = item.as_array() else {
            m.wrong(
                r,
                "keyframes",
                "an array of [at, value] or [at, value, \"easing\"]",
            );
            return None;
        };
        for value in array.iter() {
            let frame = value.as_array().and_then(|frame| {
                let at = value_number(frame.get(0)?)?;
                let value = value_number(frame.get(1)?)?;
                let easing = match frame.get(2) {
                    Some(easing) => Some(Easing::parse(easing.as_str()?).ok()?),
                    None => None,
                };
                (frame.len() <= 3).then_some(Keyframe { at, value, easing })
            });
            match frame {
                Some(frame) => frames.push(frame),
                None => {
                    m.wrong(
                        r,
                        "keyframes",
                        "an array of [at, value] or [at, value, \"easing\"]",
                    );
                    return None;
                }
            }
        }
    }
    match (m.number(r, "from"), m.number(r, "to")) {
        (Some(from), Some(to)) if frames.is_empty() => {
            frames = vec![
                Keyframe {
                    at: 0.0,
                    value: from,
                    easing: None,
                },
                Keyframe {
                    at: 1.0,
                    value: to,
                    easing: None,
                },
            ];
        }
        (None, None) => {}
        _ => {
            r.error(
                span.clone(),
                format!("{what}: a motion has keyframes, or from and to"),
            );
        }
    }
    let ordered = frames.windows(2).all(|pair| pair[0].at <= pair[1].at);
    let ends =
        frames.first().is_some_and(|f| f.at == 0.0) && frames.last().is_some_and(|f| f.at == 1.0);
    if frames.len() < 2 || !ordered || !ends {
        r.error(
            span.clone(),
            format!("{what}: keyframes run in order from at 0 to at 1"),
        );
        return None;
    }
    let repeat = match m.item("repeat") {
        None => Some(1),
        Some(item) if item.as_str() == Some("infinite") => None,
        Some(item) => match item.as_integer() {
            Some(n) if (1..=10_000).contains(&n) => Some(n as u32),
            _ => {
                m.wrong(r, "repeat", "a number of runs, or \"infinite\"");
                Some(1)
            }
        },
    };
    let alternate = m.boolean(r, "alternate").unwrap_or(false);
    let delay = m.millis(r, "delay").unwrap_or(Duration::ZERO);
    let clock = match m.string(r, "sync") {
        Some("clock") => true,
        Some(other) => {
            r.error(
                m.span("sync"),
                format!("{what}: sync takes \"clock\", not {other:?}"),
            );
            false
        }
        None => false,
    };
    if clock && repeat.is_some() {
        r.error(
            m.span("sync"),
            format!("{what}: a motion synced to the clock repeats forever: repeat = \"infinite\""),
        );
    }
    m.finish(r);
    Some(Motion {
        property: property?,
        frames,
        duration: duration.filter(|d| !d.is_zero())?,
        easing,
        repeat,
        alternate,
        delay,
        clock,
    })
}

fn read_text_style(f: &mut Fields, r: &mut Reader, props: &mut Props) {
    props.font = f.string(r, "font").map(Into::into);
    props.fallback = f.item("fallback").and_then(|item| {
        let families: Option<Vec<Arc<str>>> = match item.as_str() {
            Some(family) => Some(vec![family.into()]),
            None => item
                .as_array()
                .and_then(|array| array.iter().map(|v| v.as_str().map(Into::into)).collect()),
        };
        if families.is_none() {
            f.wrong(r, "fallback", "a family, or an array of families");
        }
        families.map(Into::into)
    });
    props.size = f.number_in(r, "size", 1.0..2000.0);
    props.weight = f.item("weight").and_then(|item| {
        let weight = as_number(item)
            .filter(|w| (1.0..=1000.0).contains(w))
            .or_else(|| {
                item.as_str().and_then(|name| {
                    Some(match name {
                        "thin" => 100.0,
                        "extralight" => 200.0,
                        "light" => 300.0,
                        "regular" | "normal" => 400.0,
                        "medium" => 500.0,
                        "semibold" => 600.0,
                        "bold" => 700.0,
                        "extrabold" => 800.0,
                        "black" => 900.0,
                        _ => return None,
                    })
                })
            });
        if weight.is_none() {
            f.wrong(
                r,
                "weight",
                "100 to 900, or thin, light, regular, medium, semibold, bold, black",
            );
        }
        weight
    });
    props.italic = f.boolean(r, "italic");
    props.color = f.color(r, "color");
    props.tracking = f.number(r, "tracking");
    props.line_height = f.number_in(r, "line_height", 0.1..10.0);
    props.case = f.choice(
        r,
        "case",
        &[
            ("none", Case::AsWritten),
            ("upper", Case::Upper),
            ("lower", Case::Lower),
        ],
    );
    props.features = f.strings(r, "features").and_then(|features| {
        let parsed: Option<Vec<(String, u32)>> = features
            .iter()
            .map(|feature| {
                let (tag, value) = match feature.split_once('=') {
                    Some((tag, value)) => (tag, value.parse::<u32>().ok()?),
                    None => (*feature, 1),
                };
                (tag.len() == 4 && tag.chars().all(|c| c.is_ascii_alphanumeric()))
                    .then(|| (tag.to_string(), value))
            })
            .collect();
        if parsed.is_none() {
            f.wrong(
                r,
                "features",
                "OpenType features like [\"tnum\", \"ss01=0\"]",
            );
        }
        parsed.map(Into::into)
    });
    if let Some(item) = f.item("shadow") {
        props.shadow = if item.as_bool() == Some(false) || item.as_str() == Some("none") {
            Some(None)
        } else if let Some(table) = item.as_table_like() {
            let mut s = Fields::new(table, item_span(item), format!("{}: shadow", f.what));
            let shadow = Shadow {
                x: s.number(r, "x").unwrap_or(0.0),
                y: s.number(r, "y").unwrap_or(0.0),
                blur: s.number_in(r, "blur", 0.0..500.0).unwrap_or(0.0),
                color: s.color(r, "color").unwrap_or(Color::BLACK.with_alpha(0.5)),
            };
            s.finish(r);
            Some(Some(shadow))
        } else {
            f.wrong(r, "shadow", "{ x, y, blur, color }, or \"none\"");
            None
        };
    }
    props.text_align = f.choice(
        r,
        "text_align",
        &[
            ("start", TextAlign::Start),
            ("left", TextAlign::Start),
            ("center", TextAlign::Center),
            ("end", TextAlign::End),
            ("right", TextAlign::End),
        ],
    );
}

#[allow(clippy::too_many_arguments)]
fn read_props(
    f: &mut Fields,
    r: &mut Reader,
    assets: &mut Assets,
    ids: &HashMap<String, usize>,
    kind: &Kind,
    props: &mut Props,
    what: &str,
) {
    props.visible = f.boolean(r, "visible");
    props.opacity = f.number_in(r, "opacity", 0.0..1.0);
    props.x = f.number(r, "x");
    props.y = f.number(r, "y");
    props.width = f.length(r, "width");
    props.height = f.length(r, "height");
    props.scale = f.number_in(r, "scale", 0.0..20.0);
    props.rotate = f.number(r, "rotate");
    props.clip = f
        .sides(r, "clip")
        .map(|sides| sides.map(|side| side.clamp(0.0, 1.0)));
    read_text_style(f, r, props);
    props.text = f.template(r, "text");
    if let Some(path) = f.string(r, "file") {
        let span = f.span("file");
        props.file = assets.get(r, span, path);
    }
    props.fill = f.color(r, "fill");
    props.border = f.sides(r, "border");
    props.border_color = f.color(r, "border_color");
    props.dashed = f.choice(r, "border_style", &[("solid", false), ("dashed", true)]);
    props.radius = f.corners(r, "radius");
    props.padding = f.sides(r, "padding");
    props.gap = f.number_in(r, "gap", 0.0..4000.0);
    props.align = f.choice(
        r,
        "align",
        &[
            ("start", Align::Start),
            ("center", Align::Center),
            ("end", Align::End),
            ("stretch", Align::Stretch),
            ("baseline", Align::Baseline),
        ],
    );
    props.justify = f.choice(
        r,
        "justify",
        &[
            ("start", Justify::Start),
            ("center", Justify::Center),
            ("end", Justify::End),
            ("between", Justify::Between),
            ("around", Justify::Around),
        ],
    );
    if let Some(item) = f.item("box_shadow") {
        props.box_shadow = if item.as_str() == Some("none") || item.as_bool() == Some(false) {
            Some(None)
        } else if let Some(table) = item.as_table_like() {
            let mut s = Fields::new(table, item_span(item), format!("{what}: box_shadow"));
            let shadow = BoxShadow {
                x: s.number(r, "x").unwrap_or(0.0),
                y: s.number(r, "y").unwrap_or(0.0),
                blur: s.number_in(r, "blur", 0.0..1000.0).unwrap_or(0.0),
                spread: s.number(r, "spread").unwrap_or(0.0),
                color: s.color(r, "color").unwrap_or(Color::BLACK.with_alpha(0.5)),
                inset: s.boolean(r, "inset").unwrap_or(false),
            };
            s.finish(r);
            Some(Some(shadow))
        } else {
            f.wrong(
                r,
                "box_shadow",
                "{ x, y, blur, spread, color, inset }, or \"none\"",
            );
            None
        };
    }
    props.caret = f.choice(
        r,
        "caret",
        &[
            ("bar", Caret::Bar),
            ("block", Caret::Block),
            ("none", Caret::Hidden),
        ],
    );
    props.caret_color = f.color(r, "caret_color");
    props.placeholder = f.template(r, "placeholder");
    props.placeholder_color = f.color(r, "placeholder_color");
    props.blurred = f.number_in(r, "blurred", 0.0..1.0);
    if matches!(kind, Kind::Tiles(_)) {
        props.layout = f
            .string(r, "layout")
            .and_then(|name| match tiles::Layout::parse(name) {
                Ok(layout) => Some(layout),
                Err(message) => {
                    r.error(f.span("layout"), format!("{what}: {message}"));
                    None
                }
            });
        props.focus = f.string(r, "focus").and_then(|pane| {
            let found = ids.get(pane).copied();
            if found.is_none() {
                r.error(
                    f.span("focus"),
                    format!("{what}: no pane {pane:?} to focus"),
                );
            }
            found
        });
    }
}

fn read_overrides(
    f: &mut Fields,
    r: &mut Reader,
    assets: &mut Assets,
    ids: &HashMap<String, usize>,
    kind: &Kind,
    what: &str,
) -> Vec<Override> {
    let Some((table, _)) = f.table(r, "on") else {
        return Vec::new();
    };
    let mut overrides = Vec::new();
    for (names, item) in table.iter() {
        let span = table
            .get_key_value(names)
            .and_then(|(key, _)| key.span())
            .or_else(|| item_span(item));
        let Some(inner) = item.as_table_like() else {
            r.error(span, format!("{what}: on.{names} should be a table"));
            continue;
        };
        let conditions: Vec<Condition> = names
            .split(',')
            .map(str::trim)
            .filter_map(|name| {
                let condition = Condition::parse(name);
                if condition.is_none() {
                    r.error(span.clone(), format!("{what}: on.{name}: unknown condition; use a state (rest, idle, typing, checking, error, welcome), a language (en, zh-CN), failed, current, focused, armed, fired or hover"));
                }
                condition
            })
            .collect();
        let label = format!("{what}: on.{names}");
        let mut o = Fields::new(inner, span.clone(), label.clone());
        let transition = read_transition(&mut o, r, "transition");
        let motions = read_motions(&mut o, r, &label);
        let mut props = Props::default();
        read_props(&mut o, r, assets, ids, kind, &mut props, &label);
        o.finish(r);
        for condition in conditions {
            if (transition.is_some() || !motions.is_empty()) && condition.as_state().is_none() {
                r.error(
                    span.clone(),
                    format!("{label}: transitions and motions belong to states"),
                );
            }
            overrides.push(Override {
                condition,
                props: props.clone(),
                transition,
                motions: motions.clone(),
            });
        }
    }
    overrides.sort_by_key(|o| o.condition);
    overrides
}

/// The tiles layer, if any: one at most, on the screen itself, with panes.
fn check_tiles(layers: &[Layer], r: &mut Reader) -> Option<usize> {
    let all: Vec<usize> = (0..layers.len())
        .filter(|&i| layers[i].tiles().is_some())
        .collect();
    let &first = all.first()?;
    for &extra in &all[1..] {
        r.error(
            Some(0..0),
            format!(
                "{}: a theme has one tiles layer at most",
                layers[extra].label()
            ),
        );
    }
    let tiles = &layers[first];
    if tiles.children.is_empty() {
        r.error(
            Some(0..0),
            format!(
                "{}: a tiles layer needs panes: layers with parent = its id",
                tiles.label()
            ),
        );
    }
    let sized = tiles.anchor == Anchor::Fill && tiles.parent.is_none()
        || matches!(tiles.base.width, Some(Length::Px(_)))
            && matches!(tiles.base.height, Some(Length::Px(_)));
    if !sized {
        r.error(Some(0..0), format!("{}: a tiles layer needs anchor = \"fill\" on the screen, or a width and height in frame pixels", tiles.label()));
    }
    Some(first)
}

#[allow(clippy::too_many_arguments)]
fn check_layers(
    layers: &[Layer],
    assets: &[Asset],
    has_tiles: bool,
    strings: &Strings,
    uses: &mut Uses,
    r: &mut Reader,
) {
    for (index, layer) in layers.iter().enumerate() {
        let span = line_span(r.text, layer.line);
        let ancestors = || std::iter::successors(layer.parent, |&at| layers[at].parent);
        let in_each = layer.repeat == Repeat::Sessions
            || ancestors().any(|at| layers[at].repeat == Repeat::Sessions);
        let all_props =
            std::iter::once(&layer.base).chain(layer.overrides.iter().map(|o| &o.props));
        let templates = all_props
            .clone()
            .flat_map(|props| props.text.iter().chain(props.placeholder.iter()))
            .chain(layer.pane.iter().map(|pane| &pane.title));
        for template in templates {
            for binding in template.bindings() {
                uses.note(binding);
                match binding {
                    Binding::TilesTitle | Binding::TilesLayout if !has_tiles => {
                        r.error(
                            span.clone(),
                            format!("{}: {{tiles.*}} needs a tiles layer", layer.label()),
                        );
                    }
                    Binding::Item | Binding::ItemNumber if !in_each => {
                        r.error(
                            span.clone(),
                            format!(
                                "{}: {{item}} needs for_each on the layer or one around it",
                                layer.label()
                            ),
                        );
                    }
                    Binding::String(key) => {
                        check_string_key(strings, key, &layer.label(), span.clone(), r)
                    }
                    _ => {}
                }
            }
        }
        if layer.action == Some(Action::Pick) && !in_each {
            r.error(
                span.clone(),
                format!(
                    "{}: pick needs for_each on the layer or one around it",
                    layer.label()
                ),
            );
        }
        let tile_action = layer.action.is_some_and(|a| matches!(a, Action::Tile(_)))
            || layer.fired_by.iter().any(|a| matches!(a, Action::Tile(_)));
        if tile_action && !has_tiles {
            r.error(
                span.clone(),
                format!("{}: tile actions need a tiles layer", layer.label()),
            );
        }
        let conditions: Vec<Condition> = layer.overrides.iter().map(|o| o.condition).collect();
        if conditions.contains(&Condition::Current) && !in_each {
            r.warn(
                span.clone(),
                format!(
                    "{}: on.current never applies outside for_each",
                    layer.label()
                ),
            );
        }
        let in_pane = ancestors()
            .chain(std::iter::once(index))
            .any(|at| layers[at].pane.is_some());
        if conditions.contains(&Condition::Focused) && !in_pane {
            r.warn(
                span.clone(),
                format!("{}: on.focused applies only in a tiles pane", layer.label()),
            );
        }
        let acts = |at: usize| layers[at].action.is_some() || !layers[at].fired_by.is_empty();
        if conditions.contains(&Condition::Fired) && !acts(index) && !ancestors().any(acts) {
            r.warn(
                span.clone(),
                format!("{}: on.fired needs an action or fired_by", layer.label()),
            );
        }
        let turns = all_props
            .clone()
            .any(|props| props.rotate.is_some_and(|angle| angle != 0.0))
            || layer
                .motions
                .iter()
                .chain(layer.overrides.iter().flat_map(|o| o.motions.iter()))
                .any(|motion| motion.property == Animated::Rotate)
            || matches!(layer.repeat, Repeat::Copies { rotate, .. } if rotate != 0.0);
        let tinted = matches!(layer.kind, Kind::Svg { tint: true, .. });
        if turns && !tinted {
            r.warn(
                span.clone(),
                format!(
                    "{}: only an svg layer with tint = true turns; this one stays upright",
                    layer.label()
                ),
            );
        }
        let aspect = all_props
            .clone()
            .find_map(|props| props.file)
            .map(|file| assets[file].aspect());
        if let Kind::Svg { tint: false, .. } = layer.kind
            && static_size(layers, index, aspect).is_none()
        {
            r.error(span.clone(), format!("{}: an svg without tint is drawn once at its size: give it a width or a height in frame pixels, or anchor = \"fill\"", layer.label()));
        }
        if let Kind::Box {
            flow: Flow::Stack, ..
        } = layer.kind
            && !layer.children.is_empty()
            && layer.pane.is_none()
            && layer.anchor != Anchor::Fill
            && (layer.base.width.is_none() || layer.base.height.is_none())
        {
            r.warn(span.clone(), format!("{}: a stack box places its layers by anchor inside its own size; give it a width and a height, or anchor = \"fill\"", layer.label()));
        }
        for file in all_props.filter_map(|props| props.file) {
            let kind_matches = matches!(
                (&layer.kind, &assets[file].kind),
                (Kind::Image(_), AssetKind::Raster(_)) | (Kind::Svg { .. }, AssetKind::Svg { .. })
            );
            if !kind_matches {
                r.error(
                    span.clone(),
                    format!(
                        "{}: {} is not a file an {} layer draws",
                        layer.label(),
                        assets[file].path,
                        layer.kind.name()
                    ),
                );
            }
        }
    }
}

/// The size a layer has whatever the screen: from frame pixels, or by
/// filling a parent whose size is known, or the screen's frame; for a
/// picture of `aspect` (width over height), one side gives the other.
pub fn static_size(layers: &[Layer], index: usize, aspect: Option<f32>) -> Option<(f32, f32)> {
    let base = &layers[index].base;
    sized(layers, index, base.width, base.height, aspect)
}

/// The size `static_size` gives a layer `width` by `height`.
pub fn sized(
    layers: &[Layer],
    index: usize,
    width: Option<Length>,
    height: Option<Length>,
    aspect: Option<f32>,
) -> Option<(f32, f32)> {
    let aspect = aspect.filter(|aspect| aspect.is_finite() && *aspect > 0.0);
    match (sides(layers, index, width, height), aspect) {
        ((Some(width), Some(height)), _) => Some((width, height)),
        ((Some(width), None), Some(aspect)) => Some((width, width / aspect)),
        ((None, Some(height)), Some(aspect)) => Some((height * aspect, height)),
        _ => None,
    }
}

/// Each side of a layer's size that holds whatever the screen.
fn sides(
    layers: &[Layer],
    index: usize,
    width: Option<Length>,
    height: Option<Length>,
) -> (Option<f32>, Option<f32>) {
    let layer = &layers[index];
    let parent = match layer.parent {
        None => (Some(super::FRAME.0), Some(super::FRAME.1)),
        Some(parent) => match &layers[parent].pane {
            Some(pane) => (pane.content.map(|c| c.0), pane.content.map(|c| c.1)),
            None => {
                let base = &layers[parent].base;
                sides(layers, parent, base.width, base.height)
            }
        },
    };
    if layer.anchor == Anchor::Fill {
        let [top, right, bottom, left] = layer.inset;
        return (
            parent.0.map(|width| width - left - right),
            parent.1.map(|height| height - top - bottom),
        );
    }
    let px = |length: Option<Length>, whole: Option<f32>| match length {
        Some(Length::Px(px)) => Some(px),
        Some(Length::Percent(share)) => whole.map(|whole| whole * share),
        None => None,
    };
    (px(width, parent.0), px(height, parent.1))
}

fn line_span(text: &str, line: usize) -> Option<Range<usize>> {
    let start = text
        .split_inclusive('\n')
        .take(line.saturating_sub(1))
        .map(str::len)
        .sum::<usize>();
    Some(start..start)
}

fn check_string_key(
    strings: &Strings,
    key: &str,
    what: &str,
    span: Option<Range<usize>>,
    r: &mut Reader,
) {
    let english = strings.english.contains_key(key);
    let chinese = strings.chinese.contains_key(key);
    let built_in = BUILT_IN_STRINGS.contains(&key);
    if !english && !chinese && !built_in {
        r.error(
            span,
            format!("{what}: no string {{t.{key}}} in [strings.en]"),
        );
    } else if english && !chinese {
        r.warn(
            span,
            format!("{what}: {{t.{key}}} has no zh-CN translation; English stands in"),
        );
    } else if chinese && !english && !built_in {
        r.error(
            span,
            format!("{what}: {{t.{key}}} is in [strings.zh-CN] but not [strings.en]"),
        );
    }
}

fn check_strings(strings: &Strings, uses: &mut Uses, has_tiles: bool, r: &mut Reader) {
    for (language, map) in [("en", &strings.english), ("zh-CN", &strings.chinese)] {
        for (key, template) in map {
            for binding in template.bindings() {
                uses.note(binding);
                match binding {
                    Binding::String(inner)
                        if !strings.english.contains_key(inner.as_ref())
                            && !strings.chinese.contains_key(inner.as_ref())
                            && !BUILT_IN_STRINGS.contains(&inner.as_ref()) =>
                    {
                        r.error(
                            Some(0..0),
                            format!("[strings.{language}] {key}: no string {{t.{inner}}}"),
                        );
                    }
                    Binding::TilesTitle | Binding::TilesLayout if !has_tiles => {
                        r.error(
                            Some(0..0),
                            format!("[strings.{language}] {key}: {{tiles.*}} needs a tiles layer"),
                        );
                    }
                    _ => {}
                }
            }
        }
    }
}

/// The faces built into the greeter, by family and weight; none italic.
const BUILT_IN_FACES: [(&str, u16); 11] = [
    ("Inter", 200),
    ("Inter", 300),
    ("Inter", 400),
    ("Inter", 500),
    ("Fraunces 144pt", 300),
    ("Fraunces 44pt", 400),
    ("Fraunces 17pt", 400),
    ("JetBrains Mono", 400),
    ("JetBrains Mono", 500),
    ("JetBrains Mono", 700),
    ("JetBrains Mono", 800),
];

/// Text the greeter would draw otherwise than the theme asks, with no
/// sign of it on screen: a font the text system passes over, a family
/// neither bundled nor built in, and a weight or an italic that the family
/// lacks, which draws in its nearest face.
fn check_fonts(
    layers: &[Layer],
    text: &Props,
    fonts: &[files::FontFile],
    span: Option<Range<usize>>,
    r: &mut Reader,
) {
    let mut faces: HashMap<&str, Vec<(u16, bool)>> = HashMap::new();
    for (family, weight) in BUILT_IN_FACES {
        faces.entry(family).or_default().push((weight, false));
    }
    let mut passed_over: Vec<&str> = Vec::new();
    for font in fonts {
        for face in &font.faces {
            for family in &face.families {
                if face.has_m {
                    faces
                        .entry(family.as_str())
                        .or_default()
                        .push((face.weight, face.italic));
                } else {
                    passed_over.push(family);
                }
            }
        }
        if font.faces.iter().any(|face| !face.has_m) {
            r.warn(span.clone(), format!(
                "{} has no \"m\", and the text system takes no font without one: text in {} draws in another font; keep an \"m\" in a subset",
                font.path,
                font.families.join(", ")
            ));
        }
    }

    let mut warned: Vec<Arc<str>> = Vec::new();
    let mut named: Vec<(Arc<str>, Option<Range<usize>>)> = Vec::new();
    if let Some(font) = &text.font {
        named.push((font.clone(), Some(0..0)));
    }
    for layer in layers {
        let props = std::iter::once(&layer.base).chain(layer.overrides.iter().map(|o| &o.props));
        for font in props.filter_map(|props| props.font.clone()) {
            named.push((font, line_span(r.text, layer.line)));
        }
    }
    for (family, span) in named {
        let known = faces.contains_key(family.as_ref()) || passed_over.contains(&family.as_ref());
        if !known && !warned.contains(&family) {
            let bundled: Vec<&str> = fonts
                .iter()
                .flat_map(|font| font.families.iter().map(String::as_str))
                .collect();
            r.warn(span, format!(
                "font {family:?} is neither bundled nor built in; it is drawn only if the system has it. Bundled: {}",
                if bundled.is_empty() { "none".to_string() } else { bundled.join(", ") }
            ));
            warned.push(family);
        }
    }

    let mut checked: Vec<TextStyle> = Vec::new();
    for (index, layer) in layers.iter().enumerate() {
        if !matches!(layer.kind, Kind::Text | Kind::Field { .. }) {
            continue;
        }
        for style in text_styles(layers, index, text) {
            let Some(has) = faces.get(style.0.as_ref()) else {
                continue;
            };
            if checked.contains(&style) {
                continue;
            }
            checked.push(style.clone());
            let (family, weight, italic) = style;
            let span = line_span(r.text, layer.line);
            let slanted = italic.unwrap_or(false);
            let mut weights: Vec<u16> = has
                .iter()
                .filter(|face| face.1 == slanted)
                .map(|face| face.0)
                .collect();
            if weights.is_empty() {
                if let Some(italic) = italic {
                    let lacks = if italic {
                        "no italic, so it draws upright"
                    } else {
                        "only italics, so it draws in italic"
                    };
                    r.warn(span, format!("{}: {family} has {lacks}", layer.label()));
                    continue;
                }
                weights = has.iter().map(|face| face.0).collect();
            }
            if let Some(weight) = weight.filter(|weight| !weights.contains(weight)) {
                weights.sort_unstable();
                weights.dedup();
                let weights: Vec<String> = weights.iter().map(ToString::to_string).collect();
                r.warn(span, format!(
                    "{}: {family} has no weight {weight}, only {}, so it draws at the nearest of those",
                    layer.label(),
                    weights.join(", ")
                ));
            }
        }
    }
}

/// A family, and the weight and italic asked of it, if any.
type TextStyle = (Arc<str>, Option<u16>, Option<bool>);

/// The styles a text layer draws in, from its own, the layers it sits in
/// and `[text]`: one for its base look, and one for each `on` table, its
/// own or a layer's around it, that changes them.
fn text_styles(layers: &[Layer], index: usize, text: &Props) -> Vec<TextStyle> {
    let chain: Vec<usize> = std::iter::successors(Some(index), |&at| layers[at].parent).collect();
    let resolve = |changed: Option<(usize, &Props)>| {
        let mut style = Props::default();
        for &at in &chain {
            let mut own = layers[at].base.clone();
            if let Some((_, props)) = changed.filter(|(level, _)| *level == at) {
                own.apply(props);
            }
            style.inherit(&own);
        }
        style.inherit(text);
        (
            style.font.unwrap_or_else(|| "Inter".into()),
            style.weight.map(|weight| weight.round() as u16),
            style.italic,
        )
    };
    let mut styles = vec![resolve(None)];
    for &at in &chain {
        for change in &layers[at].overrides {
            let props = &change.props;
            if props.font.is_some() || props.weight.is_some() || props.italic.is_some() {
                let style = resolve(Some((at, props)));
                if !styles.contains(&style) {
                    styles.push(style);
                }
            }
        }
    }
    styles
}

#[cfg(test)]
mod tests {
    use super::*;

    fn theme(text: &str) -> (Option<Folder>, Vec<Problem>) {
        static FOLDERS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "gpui-greeter-parse-{}-{}",
            std::process::id(),
            FOLDERS.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(MANIFEST), text).unwrap();
        std::fs::write(
            dir.join("ring.svg"),
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100"><circle cx="50" cy="50" r="40"/></svg>"#,
        )
        .unwrap();
        std::fs::write(
            dir.join("tall.svg"),
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="67" height="104"><rect width="67" height="104"/></svg>"#,
        )
        .unwrap();
        let mut problems = Vec::new();
        let folder = folder(&dir, "sample", &mut problems);
        let _ = std::fs::remove_dir_all(dir);
        (folder, problems)
    }

    fn errors(problems: &[Problem]) -> Vec<String> {
        problems
            .iter()
            .filter(|p| p.severity == Severity::Error)
            .map(ToString::to_string)
            .collect()
    }

    #[test]
    fn reads_a_small_theme() {
        let (folder, problems) = theme(
            r##"format = 1
name = "Sample"
background = "#101010"
rest = true

[text]
font = "Inter"
size = 40

[strings.en]
hint = "Log in to {host}"
[strings.zh-CN]
hint = "登录 {host}"

[keys]
"mod-j" = "submit"

[[layer]]
id = "clock"
kind = "text"
text = "{time}"
anchor = "center"
y = -200
on.rest = { scale = 1.5, transition = { duration = 300, easing = "ease-out" } }

[[layer]]
id = "ring"
kind = "svg"
file = "ring.svg"
tint = true
width = 800
color = "#e8d5c0"
motion = { property = "rotate", from = 0, to = 360, duration = 60000, easing = "steps(60)", repeat = "infinite", sync = "clock" }
action = "next-session"
fired_by = ["previous-session", "next-session"]

[layer.on.error]
color = "#b7410e"
"##,
        );
        assert!(errors(&problems).is_empty(), "{problems:?}");
        let folder = folder.unwrap();
        assert_eq!(folder.title, "Sample");
        assert!(folder.rest);
        assert_eq!(folder.layers.len(), 2);
        let clock = &folder.layers[0];
        assert_eq!(clock.anchor, Anchor::Center);
        assert_eq!(clock.overrides[0].condition, Condition::state(State::Rest));
        assert_eq!(clock.overrides[0].props.scale, Some(1.5));
        let ring = &folder.layers[1];
        assert_eq!(ring.line, 26);
        assert!(ring.motions[0].clock);
        assert_eq!(
            ring.fired_by,
            [Action::NextSession, Action::PreviousSession],
            "fired_by adds to the layer's own action"
        );
        assert!(
            folder
                .strings
                .get(crate::config::Language::Chinese, "hint")
                .is_some()
        );
    }

    #[test]
    fn names_each_problem_with_its_line() {
        let (_, problems) = theme(
            r##"format = 1

[[layer]]
id = "a"
kind = "text"
txt = "{time}"

[[layer]]
id = "b"
parent = "c"
kind = "svg"
file = "../ring.svg"

[[layer]]
id = "c"
kind = "text"
text = "{tim}"
color = "red"
"##,
        );
        let errors = errors(&problems);
        let has = |needle: &str| errors.iter().any(|e| e.contains(needle));
        assert!(
            has("theme.toml:6: error: layer \"a\": unknown key `txt`; did you mean `text`?"),
            "{errors:#?}"
        );
        assert!(
            has("theme.toml:10: error: layer \"b\": its parent \"c\" must come before it"),
            "{errors:#?}"
        );
        assert!(
            has("theme.toml:12:") && has("without \"..\""),
            "{errors:#?}"
        );
        assert!(
            has("theme.toml:17: error: layer \"c\": `text`: unknown binding {tim}"),
            "{errors:#?}"
        );
        assert!(has("theme.toml:18:") && has("not a colour"), "{errors:#?}");
    }

    #[test]
    fn checks_what_layers_refer_to() {
        let (_, problems) = theme(
            r##"format = 1
[[layer]]
kind = "text"
text = "{tiles.title} {item} {t.nothing}"
action = "tile.focus-next"
[[layer]]
kind = "svg"
file = "ring.svg"
"##,
        );
        let errors = errors(&problems);
        let has = |needle: &str| errors.iter().any(|e| e.contains(needle));
        assert!(has("{tiles.*} needs a tiles layer"), "{errors:#?}");
        assert!(has("{item} needs for_each"), "{errors:#?}");
        assert!(has("no string {t.nothing}"), "{errors:#?}");
        assert!(has("tile actions need a tiles layer"), "{errors:#?}");
        assert!(
            has("an svg without tint is drawn once at its size"),
            "{errors:#?}"
        );
    }

    #[test]
    fn reads_tiles_and_panes() {
        let (folder, problems) = theme(
            r##"format = 1
[keys]
"mod-j" = "tile.focus-next"
[[layer]]
id = "tiles"
kind = "tiles"
anchor = "fill"
inset = [0, 0, 57.6, 0]
pane_border = 4.8
focus = "login"
on.rest = { layout = "full", focus = "clock" }
[[layer]]
id = "login"
parent = "tiles"
content = [1248, 816]
title = "gpui-greeter"
[[layer]]
id = "clock"
parent = "tiles"
content = [652.8, 307.2]
max_scale = 1.25
max_scale_full = 3.4
[[layer]]
kind = "text"
text = "{tiles.layout}"
"##,
        );
        assert!(errors(&problems).is_empty(), "{problems:#?}");
        let folder = folder.unwrap();
        assert_eq!(folder.tiles, Some(0));
        assert_eq!(folder.layers[0].children, vec![1, 2]);
        assert_eq!(folder.layers[0].base.focus, Some(1));
        assert_eq!(
            folder.layers[0].overrides[0].props.layout,
            Some(tiles::Layout::Full)
        );
        assert_eq!(folder.layers[2].pane.as_ref().unwrap().max_scale_full, 3.4);
    }

    #[test]
    fn the_built_in_faces_are_the_fonts_built_in() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets");
        let mut found = Vec::new();
        for entry in std::fs::read_dir(dir.join("fonts")).unwrap() {
            let name = entry.unwrap().file_name().into_string().unwrap();
            if !files::is_font(&name) {
                continue;
            }
            let font = files::load_font(&dir, &format!("fonts/{name}")).unwrap();
            for face in font.faces {
                assert!(face.has_m && !face.italic, "{name}");
                found.push((face.families[0].clone(), face.weight));
            }
        }
        found.sort();
        let mut built_in: Vec<(String, u16)> = BUILT_IN_FACES
            .iter()
            .map(|(family, weight)| (family.to_string(), *weight))
            .collect();
        built_in.sort();
        assert_eq!(found, built_in);
    }

    #[test]
    fn warns_of_a_face_its_family_lacks() {
        let (_, problems) = theme(
            r##"format = 1
[text]
font = "Inter"
[[layer]]
id = "heavy"
kind = "text"
text = "{time}"
weight = 600
[[layer]]
id = "slanted"
kind = "text"
text = "{date}"
italic = true
[[layer]]
id = "box"
layout = "row"
font = "JetBrains Mono"
weight = "bold"
[[layer]]
id = "code"
parent = "box"
kind = "text"
text = "{host}"
[layer.on.error]
weight = 900
[[layer]]
kind = "text"
text = "{user}"
fallback = ["Noto Serif CJK SC"]
weight = 500
"##,
        );
        assert!(errors(&problems).is_empty(), "{problems:#?}");
        let warnings: Vec<String> = problems.iter().map(ToString::to_string).collect();
        let has = |needle: &str| warnings.iter().any(|w| w.contains(needle));
        assert!(
            has("layer \"heavy\": Inter has no weight 600, only 200, 300, 400, 500"),
            "{warnings:#?}"
        );
        assert!(
            has("layer \"slanted\": Inter has no italic, so it draws upright"),
            "{warnings:#?}"
        );
        assert!(
            has("layer \"code\": JetBrains Mono has no weight 900"),
            "{warnings:#?}"
        );
        assert_eq!(warnings.len(), 3, "{warnings:#?}");
    }

    #[test]
    fn a_picture_given_one_side_takes_the_other_from_its_shape() {
        let (folder, problems) = theme(
            r##"format = 1
[[layer]]
kind = "svg"
file = "tall.svg"
width = 402
[[layer]]
kind = "svg"
file = "tall.svg"
height = 624
"##,
        );
        assert!(errors(&problems).is_empty(), "{problems:?}");
        let folder = folder.unwrap();
        let aspect = Some(folder.assets[0].aspect());
        let (width, height) = static_size(&folder.layers, 0, aspect).unwrap();
        assert_eq!(width, 402.0);
        assert!((height - 624.0).abs() < 0.01, "{height}");
        let (width, height) = static_size(&folder.layers, 1, aspect).unwrap();
        assert!((width - 402.0).abs() < 0.01, "{width}");
        assert_eq!(height, 624.0);
    }

    #[test]
    fn refuses_a_broken_manifest_with_its_line() {
        let (folder, problems) = theme("format = 1\n[[layer]]\nkind = = \"text\"\n");
        assert!(folder.is_none());
        assert_eq!(problems[0].line, Some(3), "{problems:?}");
    }
}

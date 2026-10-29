//! The settings window: the sections down the side, the look as the greeter
//! draws it above, the section's controls beneath, and Apply at the foot.
//! What is chosen stays a draft, shown in the preview at once, until Apply.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::component::button::{Button, ButtonGroup, ButtonVariants as _};
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::slider::{Slider, SliderEvent, SliderState};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{
    Disableable as _, IconName, Selectable as _, Sizable as _, h_flex, v_flex,
};
use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, ClipboardItem, Div, Entity, FontWeight, Hsla, ObjectFit, PathPromptOptions, Pixels,
    RenderImage, SharedString, Subscription, Window, div, img, px, size,
};

use super::Paths;
use super::apply::{self, Changes, Outcome};
use super::avatar::{self, AccountsService, IconsFolder};
use super::request::{MAX_PICTURE, NewPicture, Request, new_name};
use super::setting::{self, BLUR, DIM, IDLE, SCALE};
use super::style::{self, NOTE, Palette, TEXT};
use super::words::{Words, words};
use crate::bing::{self, Kept};
use crate::config::{Config, Language, NeonBackground, PaperBackground, Pick, Rgb, Theme};
use crate::sessions::{self, Session};
use crate::themes;
use crate::ui::SvgAssets;
use crate::ui::miniature::{Miniature, Shot};
use crate::users::{self, User};
use crate::wallpaper::{self, Picture, picture_format, render_image};
use crate::x11::{Monitor, X11};

const SIDEBAR: f32 = 184.0;
/// The narrowest the controls' column gets.
const COLUMN: f32 = 560.0;
const PAD: f32 = 24.0;
const BAR: f32 = 52.0;
const ROW: f32 = 40.0;
const LABEL: f32 = 132.0;
/// Wallpapers to a row, as wide as the column together, and the size they
/// are made at.
const PER_ROW: f32 = 4.0;
const GAP: f32 = 14.0;
const THUMB_PIXELS: (u32, u32) = (320, 180);
/// Bing's pictures shown at first, and how many more each Show More adds:
/// each is a large picture to make small.
const BING_PAGE: usize = 8;

/// The accents the mockups offered.
const PAPER_ACCENTS: [u32; 4] = [0xB0612E, 0x3E6E8E, 0x5B7F4F, 0x8A5BA6];
const NEON_ACCENTS: [u32; 4] = [0x45F0FF, 0x9CFF5A, 0xFFB547, 0xFF4FD8];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Look,
    Wallpaper,
    Avatar,
    General,
}

impl Section {
    const ALL: [Section; 4] = [
        Section::Look,
        Section::Wallpaper,
        Section::Avatar,
        Section::General,
    ];

    pub fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "look" => Section::Look,
            "wallpaper" => Section::Wallpaper,
            "avatar" => Section::Avatar,
            "general" => Section::General,
            _ => return None,
        })
    }

    fn title(self, words: &Words) -> &'static str {
        match self {
            Section::Look => words.look,
            Section::Wallpaper => words.wallpaper,
            Section::Avatar => words.avatar,
            Section::General => words.general,
        }
    }
}

/// What the window starts with.
pub struct Start {
    pub paths: Paths,
    pub x11: Rc<X11>,
    pub monitor: Monitor,
    pub svgs: SvgAssets,
    pub section: Option<Section>,
    pub answers: Vec<PathBuf>,
}

/// What is on disk.
struct Saved {
    config: Config,
    /// Why the settings file does not read, if it does not.
    problem: Option<String>,
    /// The wallpaper folder's pictures, by name.
    pictures: Vec<(String, PathBuf)>,
    /// Bing's pictures kept, the newest first.
    bing: Vec<Kept>,
    /// Whether the daily Bing timer is on, where it is installed.
    bing_timer: Option<bool>,
    avatar: Option<PathBuf>,
}

/// What is chosen, not yet applied.
struct Draft {
    config: Config,
    added: Vec<Added>,
    removed: HashSet<String>,
    /// The avatar, made from the picture chosen.
    avatar: Option<PathBuf>,
}

struct Added {
    /// Its name in the wallpaper folder.
    name: String,
    source: PathBuf,
}

struct LookEntry {
    theme: Theme,
    title: String,
}

enum Thumbnail {
    Loading,
    Ready(Arc<RenderImage>),
    Failed,
}

enum State {
    Ready,
    Applying,
    /// What the last action came to, and whether it went wrong.
    Note(String, bool),
    /// polkit could not ask: the command to run instead.
    Sudo(String),
}

struct Sliders {
    scale: Entity<SliderState>,
    blur: Entity<SliderState>,
    dim: Entity<SliderState>,
    idle: Entity<SliderState>,
}

pub struct Settings {
    paths: Paths,
    palette: Palette,
    x11: Rc<X11>,
    /// The ground the window was given last, as 0xRRGGBB.
    painted_ground: Option<u32>,
    words: &'static Words,
    user: User,
    session: Option<Session>,
    saved: Saved,
    draft: Draft,
    section: Section,
    /// The look picked in the list, which the preview shows.
    selected: Theme,
    looks: Vec<LookEntry>,
    preview: Entity<Miniature>,
    /// Whether the preview shows its look at rest, if chosen.
    rest: Option<bool>,
    thumbnails: HashMap<PathBuf, Thumbnail>,
    /// How many of Bing's pictures the gallery shows.
    bing_shown: usize,
    sliders: Sliders,
    state: State,
    copied: bool,
    answers: Vec<PathBuf>,
    /// The avatars made so far, each in a file of its own, as GPUI keeps
    /// pictures by their path.
    avatars_made: usize,
    title_language: Language,
    _subscriptions: Vec<Subscription>,
}

impl Settings {
    pub fn new(start: Start, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let Start {
            paths,
            x11,
            monitor,
            svgs,
            section,
            answers,
        } = start;
        let palette = Palette::of(window.appearance());
        style::apply(&palette, cx);
        let user = users::current().unwrap_or_else(|| User {
            name: "user".into(),
            display_name: "User".into(),
            avatar: None,
            shell: "/bin/sh".into(),
        });
        let saved = load(&paths, &user.name);
        forget_old_avatars(&paths.cache);
        let sessions = sessions::load(&saved.config.sessions.dir);
        let session = sessions::current(sessions.clone()).or_else(|| sessions.into_iter().next());
        let draft = Draft {
            config: saved.config.clone(),
            added: Vec::new(),
            removed: HashSet::new(),
            avatar: None,
        };
        let slider = |cx: &mut Context<Self>, (low, high): (f32, f32), step: f32, value: f32| {
            cx.new(|_| {
                SliderState::new()
                    .min(low)
                    .max(high)
                    .step(step)
                    .default_value(value)
            })
        };
        let config = &saved.config;
        let sliders = Sliders {
            scale: slider(cx, SCALE, 0.05, config.scale),
            blur: slider(cx, BLUR, 1.0, config.frost.blur.clamp(BLUR.0, BLUR.1)),
            dim: slider(cx, DIM, 0.02, config.frost.dim.clamp(DIM.0, DIM.1)),
            idle: slider(
                cx,
                (IDLE.0 as f32, IDLE.1 as f32),
                10.0,
                (config.frost.idle_seconds as f32).clamp(IDLE.0 as f32, IDLE.1 as f32),
            ),
        };
        let mut subscriptions = vec![cx.observe_window_appearance(window, |this, window, cx| {
            this.palette = Palette::of(window.appearance());
            style::apply(&this.palette, cx);
            cx.notify();
        })];
        for (slider, set) in [
            (
                &sliders.scale,
                (|config: &mut Config, value: f32| config.scale = value) as fn(&mut Config, f32),
            ),
            (&sliders.blur, |config, value| config.frost.blur = value),
            (&sliders.dim, |config, value| config.frost.dim = value),
            (&sliders.idle, |config, value| {
                config.frost.idle_seconds = value.round() as u64
            }),
        ] {
            subscriptions.push(
                cx.subscribe(slider, move |this, _, event: &SliderEvent, cx| {
                    if let SliderEvent::Change(value) = event {
                        set(&mut this.draft.config, value.start());
                        this.edited(cx);
                    }
                }),
            );
        }
        let preview = cx.new(|cx| Miniature::new(x11.clone(), monitor, svgs, cx));
        let looks = looks(&saved.config);
        let mut this = Self {
            words: words(saved.config.language),
            title_language: saved.config.language,
            selected: saved.config.theme.clone(),
            paths,
            palette,
            x11,
            painted_ground: None,
            user,
            session,
            saved,
            draft,
            section: section.unwrap_or(Section::Look),
            looks,
            preview,
            rest: None,
            thumbnails: HashMap::new(),
            bing_shown: BING_PAGE,
            sliders,
            state: State::Ready,
            copied: false,
            answers,
            avatars_made: 0,
            _subscriptions: subscriptions,
        };
        this.load_thumbnails(cx);
        this
    }

    /// Something in the draft changed: a note on the last Apply no longer
    /// holds, nor a sudo command saved before the change.
    fn edited(&mut self, cx: &mut Context<Self>) {
        if matches!(self.state, State::Note(..) | State::Sudo(_)) {
            self.state = State::Ready;
        }
        self.words = words(self.draft.config.language);
        cx.notify();
    }

    fn change_count(&self) -> usize {
        setting::changes(&self.saved.config, &self.draft.config).len()
            + self.draft.added.len()
            + self.draft.removed.len()
            + usize::from(self.draft.avatar.is_some())
            + usize::from(self.timer_change().is_some())
    }

    /// The daily Bing timer's change, where it is installed: on while the
    /// wallpaper is Bing's picture of the day, off otherwise.
    fn timer_change(&self) -> Option<bool> {
        let wanted = self.draft.config.wallpaper.pick == Pick::Bing;
        self.saved
            .bing_timer
            .filter(|on| *on != wanted)
            .map(|_| wanted)
    }

    /// The pictures Frost would choose from once applied, by name.
    fn pictures(&self) -> Vec<(String, PathBuf, bool)> {
        let mut pictures: Vec<(String, PathBuf, bool)> = self
            .saved
            .pictures
            .iter()
            .filter(|(name, _)| !self.draft.removed.contains(name))
            .map(|(name, path)| (name.clone(), path.clone(), false))
            .chain(
                self.draft
                    .added
                    .iter()
                    .map(|added| (added.name.clone(), added.source.clone(), true)),
            )
            .collect();
        pictures.sort_by(|a, b| a.0.cmp(&b.0));
        pictures
    }

    /// The look the preview shows: the one picked, but Frost for its
    /// wallpaper, and a look that shows the avatar for it.
    fn preview_look(&self) -> Theme {
        match self.section {
            Section::Wallpaper => Theme::Frost,
            Section::Avatar
                if !matches!(
                    self.selected,
                    Theme::Paper | Theme::PaperNight | Theme::Frost
                ) =>
            {
                Theme::Frost
            }
            _ => self.selected.clone(),
        }
    }

    fn preview_rest(&self) -> bool {
        self.rest.unwrap_or(self.section == Section::Wallpaper)
    }

    /// The wallpaper Frost would show today.
    fn preview_wallpaper(&self) -> Option<PathBuf> {
        let bing = &self.saved.bing;
        let newest_bing = || bing.first().map(|kept| kept.path.clone());
        match &self.draft.config.wallpaper.pick {
            Pick::Bing => return newest_bing().or_else(|| self.first_picture()),
            Pick::BingFile(name) => {
                let named = bing.iter().find(|kept| kept.name == *name);
                return named
                    .map(|kept| kept.path.clone())
                    .or_else(newest_bing)
                    .or_else(|| self.first_picture());
            }
            Pick::File(_) | Pick::Daily | Pick::Random => {}
        }
        let pictures = self.pictures();
        if pictures.is_empty() {
            return None;
        }
        let at = match &self.draft.config.wallpaper.pick {
            Pick::File(name) => pictures
                .iter()
                .position(|(picture, _, _)| picture == name)
                .unwrap_or(0),
            Pick::Daily => {
                let day = jiff::Zoned::now()
                    .date()
                    .since(jiff::civil::date(2000, 1, 1))
                    .map_or(0, |span| span.get_days().unsigned_abs() as usize);
                day % pictures.len()
            }
            Pick::Random | Pick::Bing | Pick::BingFile(_) => 0,
        };
        Some(pictures[at].1.clone())
    }

    fn first_picture(&self) -> Option<PathBuf> {
        self.pictures().into_iter().next().map(|(_, path, _)| path)
    }

    fn shot(&self, size: gpui_kit::Size<Pixels>, scale_factor: f32) -> Shot {
        let mut config = self.draft.config.clone();
        config.theme = self.preview_look();
        let mut user = self.user.clone();
        user.avatar = self
            .draft
            .avatar
            .clone()
            .or_else(|| self.saved.avatar.clone());
        Shot {
            config,
            user,
            session: self.session.clone(),
            wallpaper: self.preview_wallpaper(),
            rest: self.preview_rest(),
            size,
            scale_factor,
        }
    }

    fn load_thumbnails(&mut self, cx: &mut Context<Self>) {
        let bing = self.saved.bing.iter().take(self.bing_shown);
        let wanted: Vec<PathBuf> = self
            .pictures()
            .into_iter()
            .map(|(_, path, _)| path)
            .chain(bing.map(|kept| kept.path.clone()))
            .filter(|path| !self.thumbnails.contains_key(path))
            .collect();
        for path in wanted {
            self.thumbnails.insert(path.clone(), Thumbnail::Loading);
            cx.spawn(async move |this, cx| {
                let made = cx
                    .background_executor()
                    .spawn({
                        let path = path.clone();
                        async move {
                            let picture = Picture::open(&path)?;
                            let small = picture.cover(THUMB_PIXELS)?;
                            anyhow::Ok(render_image(small))
                        }
                    })
                    .await;
                let _ = this.update(cx, |this, cx| {
                    let thumbnail = match made {
                        Ok(image) => Thumbnail::Ready(image),
                        Err(error) => {
                            log::warn!("{}: {error:#}", path.display());
                            Thumbnail::Failed
                        }
                    };
                    this.thumbnails.insert(path, thumbnail);
                    cx.notify();
                });
            })
            .detach();
        }
    }

    /// Asks for files: the desktop's file dialog, or in tests the answers
    /// given on the command line.
    fn ask_for_files(
        &mut self,
        multiple: bool,
        prompt: &'static str,
        then: fn(&mut Self, Vec<PathBuf>, &mut Context<Self>),
        cx: &mut Context<Self>,
    ) {
        if !self.answers.is_empty() {
            let answer = self.answers.remove(0);
            then(self, vec![answer], cx);
            return;
        }
        let asked = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple,
            prompt: Some(prompt.into()),
        });
        cx.spawn(async move |this, cx| {
            let answer = asked.await;
            let _ = this.update(cx, |this, cx| match answer {
                Ok(Ok(Some(paths))) => then(this, paths, cx),
                Ok(Ok(None)) | Err(_) => {}
                Ok(Err(error)) => {
                    this.state =
                        State::Note(format!("{}{error:#}", this.words.file_dialog_failed), true);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn add_pictures(&mut self, paths: Vec<PathBuf>, cx: &mut Context<Self>) {
        let mut refused = Vec::new();
        for path in paths {
            let format = std::fs::metadata(&path)
                .ok()
                .filter(|meta| meta.is_file() && meta.len() <= MAX_PICTURE)
                .and_then(|_| sniff(&path));
            let Some(format) = format else {
                refused.push(file_name(&path));
                continue;
            };
            let original = file_name(&path);
            let taken: HashSet<String> = self
                .pictures()
                .into_iter()
                .map(|(name, _, _)| name)
                .chain(self.draft.removed.iter().cloned())
                .collect();
            let name = new_name(&original, format, |name| taken.contains(name));
            self.draft.added.push(Added { name, source: path });
        }
        self.load_thumbnails(cx);
        self.edited(cx);
        if !refused.is_empty() {
            self.state = State::Note(
                format!("{} {}", refused.join(", "), self.words.not_a_picture),
                true,
            );
        }
    }

    fn remove_picture(&mut self, name: &str, cx: &mut Context<Self>) {
        let before = self.draft.added.len();
        self.draft.added.retain(|added| added.name != name);
        if self.draft.added.len() == before {
            self.draft.removed.insert(name.to_string());
        }
        if self.draft.config.wallpaper.pick == Pick::File(name.to_string()) {
            self.draft.config.wallpaper.pick = Pick::Random;
        }
        self.edited(cx);
    }

    fn choose_avatar(&mut self, paths: Vec<PathBuf>, cx: &mut Context<Self>) {
        let Some(path) = paths.into_iter().next() else {
            return;
        };
        self.avatars_made += 1;
        let made = self.paths.cache.join(format!(
            "avatar-{}-{}.png",
            std::process::id(),
            self.avatars_made
        ));
        cx.spawn(async move |this, cx| {
            let written = cx
                .background_executor()
                .spawn({
                    let made = made.clone();
                    async move {
                        let png = avatar::square_png(&path)?;
                        if let Some(dir) = made.parent() {
                            std::fs::create_dir_all(dir)?;
                        }
                        std::fs::write(&made, png)?;
                        anyhow::Ok(())
                    }
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                match written {
                    Ok(()) => this.draft.avatar = Some(made),
                    Err(error) => {
                        this.state = State::Note(format!("{error:#}"), true);
                    }
                }
                this.edited(cx);
            });
        })
        .detach();
    }

    fn revert(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.draft = Draft {
            config: self.saved.config.clone(),
            added: Vec::new(),
            removed: HashSet::new(),
            avatar: None,
        };
        self.selected = self.saved.config.theme.clone();
        self.sync_sliders(window, cx);
        self.state = State::Ready;
        self.edited(cx);
    }

    fn sync_sliders(&self, window: &mut Window, cx: &mut Context<Self>) {
        let config = &self.draft.config;
        let values = [
            (&self.sliders.scale, config.scale),
            (&self.sliders.blur, config.frost.blur),
            (&self.sliders.dim, config.frost.dim),
            (&self.sliders.idle, config.frost.idle_seconds as f32),
        ];
        for (slider, value) in values {
            slider.update(cx, |slider, cx| slider.set_value(value, window, cx));
        }
    }

    fn apply(&mut self, cx: &mut Context<Self>) {
        if matches!(self.state, State::Applying) || self.change_count() == 0 {
            return;
        }
        self.state = State::Applying;
        self.copied = false;
        let settings = setting::changes(&self.saved.config, &self.draft.config);
        let added: Vec<(String, PathBuf)> = self
            .draft
            .added
            .iter()
            .map(|added| (added.name.clone(), added.source.clone()))
            .collect();
        let mut removed: Vec<String> = self.draft.removed.iter().cloned().collect();
        removed.sort();
        let avatar = self
            .draft
            .avatar
            .clone()
            .map(|png| (self.user.name.clone(), png));
        let bing_timer = self.timer_change();
        let paths = self.paths.clone();
        cx.spawn(async move |this, cx| {
            let outcome = cx
                .background_executor()
                .spawn(async move {
                    let mut add = Vec::new();
                    for (name, source) in added {
                        match read_picture(&source) {
                            Ok(bytes) => add.push(NewPicture { name, bytes }),
                            Err(error) => return Outcome::Failed(format!("{error:#}")),
                        }
                    }
                    let changes = Changes {
                        request: Request {
                            settings,
                            add,
                            remove: removed,
                            bing_timer,
                        },
                        avatar,
                    };
                    let accounts: Box<dyn avatar::Accounts> = match paths.elevate {
                        apply::Elevate::Pkexec => Box::new(AccountsService),
                        _ => Box::new(IconsFolder(paths.icons.clone())),
                    };
                    apply::apply(
                        &changes,
                        accounts.as_ref(),
                        &paths.elevate,
                        &paths.cache.join("request"),
                    )
                })
                .await;
            let _ = this.update(cx, |this, cx| this.applied(outcome, cx));
        })
        .detach();
        cx.notify();
    }

    fn applied(&mut self, outcome: Outcome, cx: &mut Context<Self>) {
        self.state = match outcome {
            Outcome::Done(lines) => {
                for line in &lines {
                    log::info!("{line}");
                }
                self.reload();
                self.load_thumbnails(cx);
                State::Note(self.words.applied.to_string(), false)
            }
            Outcome::Cancelled => State::Note(self.words.cancelled.to_string(), false),
            Outcome::Sudo(command) => {
                // The avatar is set by now; only root's part waits.
                self.keep_avatar();
                State::Sudo(command)
            }
            Outcome::Failed(error) => State::Note(format!("{}{error}", self.words.failed), true),
        };
        cx.notify();
    }

    /// Takes the avatar set as the one there is.
    fn keep_avatar(&mut self) {
        if let Some(avatar) = self.draft.avatar.take() {
            self.saved.avatar = Some(avatar);
        }
    }

    /// Reads what is on disk again, after applying, and drops the draft's
    /// changes that are there now.
    fn reload(&mut self) {
        self.keep_avatar();
        let avatar = self.saved.avatar.clone();
        self.saved = load(&self.paths, &self.user.name);
        self.saved.avatar = avatar;
        self.draft.config = self.saved.config.clone();
        self.draft.added.clear();
        self.draft.removed.clear();
        self.looks = looks(&self.saved.config);
        self.words = words(self.draft.config.language);
    }

    fn pick_look(&mut self, theme: Theme, cx: &mut Context<Self>) {
        self.selected = theme;
        self.rest = None;
        cx.notify();
    }

    fn open_section(&mut self, section: Section, cx: &mut Context<Self>) {
        self.section = section;
        self.rest = None;
        cx.notify();
    }
}

/// Reads the settings, the wallpaper folder, Bing's pictures, whether the
/// Bing timer is on and `user`'s avatar.
fn load(paths: &Paths, user: &str) -> Saved {
    let (config, problem) = paths.load();
    let pictures = wallpaper::candidates(&config.wallpaper.dir)
        .into_iter()
        .filter_map(|path| {
            let name = path.file_name()?.to_str()?.to_string();
            Some((name, path))
        })
        .collect();
    Saved {
        avatar: avatar::current(&paths.icons, user),
        bing: bing::kept(&config.wallpaper.bing_dir),
        bing_timer: paths.units().enabled(),
        config,
        problem,
        pictures,
    }
}

/// Removes the avatars made a day ago or more, which AccountsService has
/// copied if they were applied at all.
fn forget_old_avatars(cache: &Path) {
    let Ok(entries) = std::fs::read_dir(cache) else {
        return;
    };
    let day = std::time::Duration::from_secs(24 * 60 * 60);
    for entry in entries.flatten() {
        let name = entry.file_name();
        let made = name
            .to_str()
            .is_some_and(|name| name.starts_with("avatar-") && name.ends_with(".png"));
        let old = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .is_ok_and(|modified| modified.elapsed().is_ok_and(|age| age >= day));
        if made && old {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// Every look: the built-in ones, then the theme folders installed.
fn looks(config: &Config) -> Vec<LookEntry> {
    let built_in = [
        (Theme::Paper, "Paper"),
        (Theme::PaperNight, "Paper Night"),
        (Theme::Frost, "Frost"),
        (Theme::Neon, "Neon"),
    ];
    let mut looks: Vec<LookEntry> = built_in
        .into_iter()
        .map(|(theme, title)| LookEntry {
            theme,
            title: title.into(),
        })
        .collect();
    let mut folders: Vec<LookEntry> = std::fs::read_dir(&config.themes.dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            let manifest = entry.path().join(themes::MANIFEST);
            if !themes::valid_name(&name) || themes::RESERVED.contains(&name.as_str()) {
                return None;
            }
            let text = std::fs::read_to_string(manifest).ok()?;
            let title = toml::from_str::<toml::Table>(&text)
                .ok()
                .and_then(|table| table.get("name")?.as_str().map(str::to_string))
                .unwrap_or_else(|| name.clone());
            Some(LookEntry {
                theme: Theme::Folder(name),
                title,
            })
        })
        .collect();
    folders.sort_by_key(|look| look.title.to_lowercase());
    looks.extend(folders);
    if !looks.iter().any(|look| look.theme == config.theme) {
        looks.push(LookEntry {
            title: config.theme.name().to_string(),
            theme: config.theme.clone(),
        });
    }
    looks
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// The picture format of the file at `path`, from its first bytes.
fn sniff(path: &Path) -> Option<&'static str> {
    use std::io::Read as _;
    let mut head = [0u8; 12];
    let read = std::fs::File::open(path).ok()?.read(&mut head).ok()?;
    picture_format(&head[..read])
}

/// A picture's bytes, read as the user, for the helper.
fn read_picture(path: &Path) -> anyhow::Result<Vec<u8>> {
    use anyhow::Context as _;
    let meta =
        std::fs::metadata(path).with_context(|| format!("cannot read {}", path.display()))?;
    anyhow::ensure!(
        meta.is_file() && meta.len() <= MAX_PICTURE,
        "{} is not a picture file",
        path.display()
    );
    std::fs::read(path).with_context(|| format!("cannot read {}", path.display()))
}

// What follows draws the window.

impl Render for Settings {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.title_language != self.draft.config.language {
            self.title_language = self.draft.config.language;
            window.set_window_title(self.words.title);
        }
        let ground = gpui_kit::Rgba::from(self.palette.ground);
        let ground = [ground.r, ground.g, ground.b]
            .into_iter()
            .fold(0, |rgb, channel| {
                rgb << 8 | (channel * 255.0).round() as u32
            });
        if self.painted_ground != Some(ground)
            && let Some(xid) = crate::ui::xid(window)
        {
            self.painted_ground = Some(ground);
            if let Err(error) = self.x11.set_background(xid, ground) {
                log::warn!("cannot give the window its ground: {error:#}");
            }
        }
        // The preview as large as about half the window allows, and the
        // controls in a column as wide, both in the middle.
        let viewport = window.viewport_size();
        let monitor = self.preview.read(cx).monitor().rect;
        let aspect = monitor.height.max(1) as f32 / monitor.width.max(1) as f32;
        let room_w = (viewport.width.as_f32() - SIDEBAR - 2.0 * PAD).max(160.0);
        let room_h = ((viewport.height.as_f32() - BAR) * 0.54).max(90.0);
        let (width, height) = if room_w * aspect <= room_h {
            (room_w, room_w * aspect)
        } else {
            (room_h / aspect, room_h)
        };
        let box_size = size(px(width.floor()), px(height.floor()));
        let column = width.floor().max(COLUMN).min(room_w);
        let shot = self.shot(box_size, window.scale_factor());
        self.preview
            .update(cx, |preview, cx| preview.show(shot, cx));

        let p = self.palette;
        v_flex()
            .size_full()
            .bg(p.ground)
            .text_color(p.text)
            .font(style::font(FontWeight::NORMAL))
            .text_size(px(TEXT))
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .items_start()
                    .child(self.sidebar(cx))
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .child(self.preview_area(box_size, column, cx))
                            .child(
                                div().id("section").flex_1().min_h_0().w_full().child(
                                    v_flex()
                                        .id("section-scroll")
                                        .size_full()
                                        .overflow_y_scrollbar()
                                        .child(
                                            div()
                                                .w(px(column))
                                                .mx_auto()
                                                .pb(px(PAD))
                                                .child(self.section_body(column, aspect, cx)),
                                        ),
                                ),
                            ),
                    ),
            )
            .when_some(self.sudo_panel(cx), |root, panel| root.child(panel))
            .child(self.bottom_bar(cx))
    }
}

impl Settings {
    fn sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let p = self.palette;
        v_flex()
            .w(px(SIDEBAR))
            .h_full()
            .flex_shrink_0()
            .bg(p.panel)
            .border_r_1()
            .border_color(p.rule)
            .pt(px(PAD - 6.0))
            .px(px(10.0))
            .gap(px(2.0))
            .children(Section::ALL.into_iter().map(|section| {
                let picked = section == self.section;
                div()
                    .id(("section", section as usize))
                    .h(px(28.0))
                    .px(px(10.0))
                    .flex()
                    .items_center()
                    .rounded(px(5.0))
                    .cursor_pointer()
                    .when(picked, |row| {
                        row.bg(p.picked).font(style::font(FontWeight::MEDIUM))
                    })
                    .when(!picked, |row| row.hover(|row| row.bg(p.hover)))
                    .on_click(cx.listener(move |this, _, _, cx| this.open_section(section, cx)))
                    .child(section.title(self.words))
            }))
    }

    fn preview_area(
        &self,
        box_size: gpui_kit::Size<Pixels>,
        column: f32,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let p = self.palette;
        let w = self.words;
        let look = self.preview_look();
        let title = self
            .looks
            .iter()
            .find(|entry| entry.theme == look)
            .map_or_else(|| look.name().to_string(), |entry| entry.title.clone());
        let rests = self.preview.read(cx).shows_rest(cx);
        let rest = self.preview_rest();
        let failed = self.preview.read(cx).failed();
        let header = h_flex()
            .w_full()
            .h(px(36.0))
            .justify_between()
            .child(div().font(style::font(FontWeight::MEDIUM)).child(title))
            .when(rests, |header| {
                header.child(
                    ButtonGroup::new("rest")
                        .small()
                        .outline()
                        .child(Button::new("sign-in").label(w.sign_in).selected(!rest))
                        .child(Button::new("at-rest").label(w.at_rest).selected(rest))
                        .on_click(cx.listener(|this, clicked: &Vec<usize>, _, cx| {
                            this.rest = Some(clicked.contains(&1));
                            cx.notify();
                        })),
                )
            });
        let preview = div()
            .relative()
            .w(box_size.width)
            .h(box_size.height)
            .border_1()
            .border_color(p.rule)
            .overflow_hidden()
            .child(self.preview.clone())
            .when(failed, |preview| {
                preview.child(
                    div()
                        .absolute()
                        .inset_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_color(gpui_kit::white())
                        .child(w.preview_failed),
                )
            });
        v_flex()
            .w(px(column))
            .mx_auto()
            .flex_shrink_0()
            .items_center()
            .pt(px(PAD - 14.0))
            .pb(px(18.0))
            .child(header)
            .child(preview)
    }

    fn section_body(&mut self, column: f32, aspect: f32, cx: &mut Context<Self>) -> AnyElement {
        match self.section {
            Section::Look => self.look_section(cx).into_any_element(),
            Section::Wallpaper => {
                let width = ((column - GAP * (PER_ROW - 1.0)) / PER_ROW).floor();
                let thumb = (width, (width * aspect).round());
                self.wallpaper_section(thumb, cx).into_any_element()
            }
            Section::Avatar => self.avatar_section(cx).into_any_element(),
            Section::General => self.general_section(cx).into_any_element(),
        }
    }

    fn look_section(&mut self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let p = self.palette;
        let w = self.words;
        let default = self.draft.config.theme.clone();
        // Its first row level with the first of the options.
        let list = v_flex()
            .w(px(200.0))
            .flex_shrink_0()
            .pt(px((ROW - 28.0) / 2.0))
            .gap(px(2.0))
            .children(self.looks.iter().enumerate().map(|(index, look)| {
                let picked = look.theme == self.selected;
                let theme = look.theme.clone();
                h_flex()
                    .id(("look", index))
                    .h(px(28.0))
                    .px(px(10.0))
                    .rounded(px(5.0))
                    .justify_between()
                    .cursor_pointer()
                    .when(picked, |row| row.bg(p.picked))
                    .when(!picked, |row| row.hover(|row| row.bg(p.hover)))
                    .on_click(cx.listener(move |this, _, _, cx| this.pick_look(theme.clone(), cx)))
                    .child(div().truncate().child(look.title.clone()))
                    .when(look.theme == default, |row| {
                        row.child(
                            div()
                                .text_size(px(NOTE))
                                .text_color(p.muted)
                                .child(w.default_mark),
                        )
                    })
            }));
        let selected = self.selected.clone();
        let mut options = v_flex().flex_1().min_w_0();
        let config = &self.draft.config;
        match &selected {
            Theme::Paper | Theme::PaperNight => {
                options = options
                    .child(row(
                        &p,
                        w.background,
                        segmented(
                            "paper-background",
                            PaperBackground::ALL
                                .iter()
                                .map(|b| (w.paper_background(*b), *b == config.paper.background)),
                            cx.listener(|this, clicked: &Vec<usize>, _, cx| {
                                if let Some(&at) = clicked.first() {
                                    this.draft.config.paper.background = PaperBackground::ALL[at];
                                    this.edited(cx);
                                }
                            }),
                        ),
                    ))
                    .child(row(
                        &p,
                        w.accent,
                        swatches(
                            "paper-accent",
                            &p,
                            &PAPER_ACCENTS,
                            config.paper.accent,
                            cx.listener(|this, rgb: &Rgb, _, cx| {
                                this.draft.config.paper.accent = *rgb;
                                this.edited(cx);
                            }),
                        ),
                    ));
            }
            Theme::Frost => {
                options = options
                    .child(row(
                        &p,
                        w.blur,
                        slider_with(
                            &p,
                            &self.sliders.blur,
                            format!("{}", config.frost.blur.round()),
                        ),
                    ))
                    .child(row(
                        &p,
                        w.dim,
                        slider_with(
                            &p,
                            &self.sliders.dim,
                            format!("{}%", (config.frost.dim * 100.0).round()),
                        ),
                    ))
                    .child(row(
                        &p,
                        w.idle,
                        slider_with(
                            &p,
                            &self.sliders.idle,
                            format!("{} {}", config.frost.idle_seconds, w.seconds),
                        ),
                    ))
                    .child(note(&p, w.frost_wallpaper_note));
            }
            Theme::Neon => {
                options = options
                    .child(row(
                        &p,
                        w.background,
                        segmented(
                            "neon-background",
                            NeonBackground::ALL
                                .iter()
                                .map(|b| (w.neon_background(*b), *b == config.neon.background)),
                            cx.listener(|this, clicked: &Vec<usize>, _, cx| {
                                if let Some(&at) = clicked.first() {
                                    this.draft.config.neon.background = NeonBackground::ALL[at];
                                    this.edited(cx);
                                }
                            }),
                        ),
                    ))
                    .child(row(
                        &p,
                        w.accent,
                        swatches(
                            "neon-accent",
                            &p,
                            &NEON_ACCENTS,
                            config.neon.accent,
                            cx.listener(|this, rgb: &Rgb, _, cx| {
                                this.draft.config.neon.accent = *rgb;
                                this.edited(cx);
                            }),
                        ),
                    ))
                    .child(row(
                        &p,
                        w.scanlines,
                        Switch::new("scanlines")
                            .small()
                            .checked(config.neon.scanlines)
                            .on_click(cx.listener(|this, on: &bool, _, cx| {
                                this.draft.config.neon.scanlines = *on;
                                this.edited(cx);
                            })),
                    ));
            }
            Theme::Folder(_) => {
                let broken = self.preview.read(cx).failed();
                options = options.child(note(
                    &p,
                    if broken {
                        w.folder_broken
                    } else {
                        w.folder_note
                    },
                ));
            }
        }
        options = options
            .when(selected != default, |options| {
                options.child(
                    h_flex().pt(px(14.0)).child(
                        Button::new("use-as-default")
                            .small()
                            .label(w.use_as_default)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.draft.config.theme = this.selected.clone();
                                this.edited(cx);
                            })),
                    ),
                )
            })
            .child(note(&p, w.default_note));
        h_flex()
            .items_start()
            .gap(px(PAD))
            .child(list)
            .child(options)
    }

    fn wallpaper_section(
        &mut self,
        thumb: (f32, f32),
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let p = self.palette;
        let w = self.words;
        let pick = self.draft.config.wallpaper.pick.clone();
        let modes = [
            (
                w.one_picture,
                matches!(pick, Pick::File(_) | Pick::BingFile(_)),
            ),
            (w.random, pick == Pick::Random),
            (w.daily, pick == Pick::Daily),
            (w.bing, pick == Pick::Bing),
        ];
        let mode_note = match pick {
            Pick::File(_) | Pick::BingFile(_) => w.one_picture_note,
            Pick::Random => w.random_note,
            Pick::Daily => w.daily_note,
            Pick::Bing => w.bing_note,
        };
        let show = row(
            &p,
            w.show,
            segmented(
                "pick",
                modes.into_iter(),
                cx.listener(|this, clicked: &Vec<usize>, _, cx| {
                    let pick = match clicked.first() {
                        Some(0) => {
                            let first = this.pictures().into_iter().next();
                            let newest_bing = this.saved.bing.first();
                            first.map(|(name, _, _)| Pick::File(name)).or_else(|| {
                                newest_bing.map(|kept| Pick::BingFile(kept.name.clone()))
                            })
                        }
                        Some(1) => Some(Pick::Random),
                        Some(2) => Some(Pick::Daily),
                        Some(3) => Some(Pick::Bing),
                        _ => None,
                    };
                    if let Some(pick) = pick {
                        this.draft.config.wallpaper.pick = pick;
                        this.edited(cx);
                    }
                }),
            ),
        );
        let pictures = self.pictures();
        let grid = if pictures.is_empty() {
            div()
                .py(px(12.0))
                .text_color(p.muted)
                .child(w.no_pictures)
                .into_any_element()
        } else {
            h_flex()
                .flex_wrap()
                .gap(px(GAP))
                .pt(px(12.0))
                .children(
                    pictures
                        .into_iter()
                        .enumerate()
                        .map(|(index, (name, path, new))| {
                            self.thumbnail(index, name, &path, new, &pick, thumb, cx)
                        }),
                )
                .into_any_element()
        };
        let heading = h_flex()
            .h(px(ROW))
            .justify_between()
            .border_b_1()
            .border_color(p.rule)
            .child(
                div()
                    .font(style::font(FontWeight::MEDIUM))
                    .child(w.pictures),
            )
            .child(
                Button::new("add-pictures")
                    .small()
                    .label(w.add_pictures)
                    .on_click(cx.listener(|this, _, _, cx| {
                        let prompt = this.words.add_pictures;
                        this.ask_for_files(true, prompt, Self::add_pictures, cx);
                    })),
            );
        v_flex()
            .child(show)
            .child(note(&p, mode_note))
            .child(div().h(px(12.0)))
            .child(heading)
            .child(grid)
            .child(note(
                &p,
                format!(
                    "{} {}",
                    w.kept_in,
                    self.saved.config.wallpaper.dir.display()
                ),
            ))
            .child(self.bing_gallery(&pick, thumb, cx))
    }

    /// Bing's pictures kept, the newest first, each with its day and title;
    /// a click shows that one always.
    fn bing_gallery(
        &self,
        pick: &Pick,
        thumb: (f32, f32),
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let p = self.palette;
        let w = self.words;
        let kept = &self.saved.bing;
        let heading = h_flex()
            .h(px(ROW))
            .justify_between()
            .border_b_1()
            .border_color(p.rule)
            .child(
                div()
                    .font(style::font(FontWeight::MEDIUM))
                    .child(w.bing_pictures),
            )
            .when(!kept.is_empty(), |heading| {
                heading.child(div().text_color(p.muted).child(kept.len().to_string()))
            });
        let gallery = if kept.is_empty() {
            div()
                .py(px(12.0))
                .text_color(p.muted)
                .child(w.no_bing_pictures)
                .into_any_element()
        } else {
            h_flex()
                .flex_wrap()
                .gap(px(GAP))
                .pt(px(12.0))
                .children(
                    kept.iter()
                        .take(self.bing_shown)
                        .enumerate()
                        .map(|(index, kept)| self.bing_card(index, kept, pick, thumb, cx)),
                )
                .into_any_element()
        };
        let timer_note = match self.timer_change() {
            Some(true) => Some(w.bing_timer_on),
            Some(false) => Some(w.bing_timer_off),
            None if *pick == Pick::Bing => Some(w.bing_daily_note),
            None => None,
        };
        v_flex()
            .child(div().h(px(18.0)))
            .child(heading)
            .child(gallery)
            .when(kept.len() > self.bing_shown, |section| {
                section.child(
                    h_flex().pt(px(12.0)).child(
                        Button::new("bing-more")
                            .small()
                            .label(w.show_more)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.bing_shown += BING_PAGE;
                                this.load_thumbnails(cx);
                                cx.notify();
                            })),
                    ),
                )
            })
            .child(note(
                &p,
                format!(
                    "{} {}",
                    w.kept_in,
                    self.saved.config.wallpaper.bing_dir.display()
                ),
            ))
            .when_some(timer_note, |section, timer_note| {
                section.child(note(&p, timer_note))
            })
    }

    fn bing_card(
        &self,
        index: usize,
        kept: &Kept,
        pick: &Pick,
        (width, height): (f32, f32),
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let p = self.palette;
        let picked = *pick == Pick::BingFile(kept.name.clone());
        let name = kept.name.clone();
        let title = kept
            .about
            .as_ref()
            .map(|about| about.title.clone())
            .filter(|title| !title.is_empty());
        v_flex()
            .id(("bing", index))
            .w(px(width))
            .gap(px(3.0))
            .cursor_pointer()
            .on_click(cx.listener(move |this, _, _, cx| {
                this.draft.config.wallpaper.pick = Pick::BingFile(name.clone());
                this.edited(cx);
            }))
            .child(
                div()
                    .w(px(width))
                    .h(px(height))
                    .mb(px(2.0))
                    .rounded(px(4.0))
                    .overflow_hidden()
                    .border_2()
                    .border_color(if picked { p.accent } else { p.rule })
                    .bg(p.panel)
                    .child(self.small_picture(&kept.path)),
            )
            .child(
                div()
                    .text_size(px(NOTE))
                    .text_color(if picked { p.text } else { p.muted })
                    .child(kept.day.clone()),
            )
            .when_some(title, |card, title| {
                card.child(
                    div()
                        .truncate()
                        .text_size(px(NOTE))
                        .text_color(p.muted)
                        .child(title),
                )
            })
    }

    /// The picture at `path` made small, or what stands in for it while it
    /// is made or when it cannot be.
    fn small_picture(&self, path: &Path) -> AnyElement {
        let p = self.palette;
        match self.thumbnails.get(path) {
            Some(Thumbnail::Ready(image)) => img(image.clone())
                .size_full()
                .object_fit(ObjectFit::Cover)
                .into_any_element(),
            Some(Thumbnail::Failed) => div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .text_color(p.muted)
                .text_size(px(NOTE))
                .child("?")
                .into_any_element(),
            _ => div().size_full().bg(p.panel).into_any_element(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn thumbnail(
        &self,
        index: usize,
        name: String,
        path: &Path,
        new: bool,
        pick: &Pick,
        (width, height): (f32, f32),
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let p = self.palette;
        let w = self.words;
        let picked = *pick == Pick::File(name.clone());
        let picture = self.small_picture(path);
        let remove_name = name.clone();
        let pick_name = name.clone();
        let group: SharedString = format!("thumb-{index}").into();
        v_flex()
            .id(("thumbnail", index))
            .group(group.clone())
            .w(px(width))
            .gap(px(5.0))
            .cursor_pointer()
            .on_click(cx.listener(move |this, _, _, cx| {
                this.draft.config.wallpaper.pick = Pick::File(pick_name.clone());
                this.edited(cx);
            }))
            .child(
                div()
                    .relative()
                    .w(px(width))
                    .h(px(height))
                    .rounded(px(4.0))
                    .overflow_hidden()
                    .border_2()
                    .border_color(if picked { p.accent } else { p.rule })
                    .bg(p.panel)
                    .child(picture)
                    .child(
                        div()
                            .absolute()
                            .top(px(4.0))
                            .right(px(4.0))
                            .invisible()
                            .group_hover(group, |close| close.visible())
                            .child(
                                Button::new(("remove", index))
                                    .icon(IconName::Close)
                                    .xsmall()
                                    .tooltip(w.remove)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        cx.stop_propagation();
                                        this.remove_picture(&remove_name, cx);
                                    })),
                            ),
                    ),
            )
            .child(
                h_flex()
                    .gap(px(6.0))
                    .text_size(px(NOTE))
                    .child(
                        div()
                            .truncate()
                            .text_color(if picked { p.text } else { p.muted })
                            .child(name),
                    )
                    .when(new, |line| {
                        line.child(div().flex_shrink_0().text_color(p.accent).child(w.new_mark))
                    }),
            )
    }

    fn avatar_section(&mut self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let p = self.palette;
        let w = self.words;
        let shown = self
            .draft
            .avatar
            .clone()
            .or_else(|| self.saved.avatar.clone());
        let face = div()
            .size(px(96.0))
            .flex_shrink_0()
            .rounded_full()
            .overflow_hidden()
            .bg(p.panel)
            .border_1()
            .border_color(p.border)
            .flex()
            .items_center()
            .justify_center()
            .child(match &shown {
                Some(path) => img(path.clone())
                    .size_full()
                    .rounded_full()
                    .object_fit(ObjectFit::Cover)
                    .into_any_element(),
                None => div()
                    .text_size(px(40.0))
                    .font(style::font(FontWeight::LIGHT))
                    .text_color(p.muted)
                    .child(self.user.initial())
                    .into_any_element(),
            });
        let who = v_flex()
            .gap(px(4.0))
            .child(
                div()
                    .text_size(px(15.0))
                    .font(style::font(FontWeight::MEDIUM))
                    .child(self.user.display_name.clone()),
            )
            .when(self.user.name != self.user.display_name, |who| {
                who.child(div().text_color(p.muted).child(self.user.name.clone()))
            })
            .child(
                div().pt(px(10.0)).child(
                    Button::new("choose-avatar")
                        .small()
                        .label(w.choose_picture)
                        .on_click(cx.listener(|this, _, _, cx| {
                            let prompt = this.words.choose_picture;
                            this.ask_for_files(false, prompt, Self::choose_avatar, cx);
                        })),
                ),
            );
        v_flex()
            .pt(px(8.0))
            .child(
                h_flex()
                    .gap(px(PAD))
                    .items_center()
                    .pb(px(16.0))
                    .border_b_1()
                    .border_color(p.rule)
                    .child(face)
                    .child(who),
            )
            .when(self.draft.avatar.is_some(), |section| {
                section.child(note(&p, w.avatar_new))
            })
            .when(shown.is_none(), |section| {
                section.child(note(&p, w.no_avatar))
            })
            .child(note(&p, w.avatar_note))
    }

    fn general_section(&mut self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let p = self.palette;
        let w = self.words;
        let config = &self.draft.config;
        let language = config.language;
        v_flex()
            .child(row(
                &p,
                w.size,
                slider_with(
                    &p,
                    &self.sliders.scale,
                    format!("{}%", (config.scale * 100.0).round()),
                ),
            ))
            .child(note(&p, w.size_note))
            .child(div().h(px(12.0)))
            .child(row(
                &p,
                w.language,
                segmented(
                    "language",
                    [
                        ("English", language == Language::English),
                        ("中文", language == Language::Chinese),
                    ]
                    .into_iter(),
                    cx.listener(|this, clicked: &Vec<usize>, _, cx| {
                        let language = match clicked.first() {
                            Some(0) => Language::English,
                            Some(1) => Language::Chinese,
                            _ => return,
                        };
                        this.draft.config.language = language;
                        this.edited(cx);
                    }),
                ),
            ))
            .child(note(&p, w.language_note))
    }

    fn sudo_panel(&self, cx: &mut Context<Self>) -> Option<impl IntoElement + use<>> {
        let State::Sudo(command) = &self.state else {
            return None;
        };
        let p = self.palette;
        let w = self.words;
        let to_copy = command.clone();
        Some(
            v_flex()
                .w_full()
                .px(px(PAD))
                .py(px(14.0))
                .gap(px(8.0))
                .bg(p.panel)
                .border_t_1()
                .border_color(p.rule)
                .child(
                    div()
                        .font(style::font(FontWeight::MEDIUM))
                        .child(w.sudo_title),
                )
                .child(div().text_color(p.muted).child(w.sudo_note))
                .child(
                    h_flex()
                        .gap(px(8.0))
                        .child(
                            div()
                                .id("sudo-command")
                                .flex_1()
                                .min_w_0()
                                .px(px(10.0))
                                .py(px(5.0))
                                .rounded(px(5.0))
                                .border_1()
                                .border_color(p.border)
                                .bg(p.ground)
                                .font(style::mono())
                                .text_size(px(12.0))
                                .overflow_x_scroll()
                                .whitespace_nowrap()
                                .child(command.clone()),
                        )
                        .child(
                            Button::new("copy")
                                .small()
                                .label(if self.copied { w.copied } else { w.copy })
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    cx.write_to_clipboard(ClipboardItem::new_string(
                                        to_copy.clone(),
                                    ));
                                    this.copied = true;
                                    cx.notify();
                                })),
                        )
                        .child(Button::new("sudo-done").small().label(w.done).on_click(
                            cx.listener(|this, _, window, cx| {
                                this.reload();
                                this.sync_sliders(window, cx);
                                this.state = State::Ready;
                                this.copied = false;
                                this.load_thumbnails(cx);
                                cx.notify();
                            }),
                        )),
                ),
        )
    }

    fn bottom_bar(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let p = self.palette;
        let w = self.words;
        let count = self.change_count();
        let applying = matches!(self.state, State::Applying);
        let (status, error): (String, bool) = match &self.state {
            State::Applying => (w.applying.into(), false),
            State::Note(text, error) => (text.clone(), *error),
            _ => match &self.saved.problem {
                Some(problem) if count == 0 => (format!("{}{problem}", w.unreadable), true),
                _ if count == 0 => (w.no_changes.into(), false),
                _ => (w.unapplied(count), false),
            },
        };
        h_flex()
            .h(px(BAR))
            .flex_shrink_0()
            .px(px(PAD))
            .gap(px(10.0))
            .bg(p.panel)
            .border_t_1()
            .border_color(p.rule)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_color(if error { p.danger } else { p.muted })
                    .child(status),
            )
            .child(
                Button::new("revert")
                    .small()
                    .label(w.revert)
                    .disabled(count == 0 || applying)
                    .on_click(cx.listener(|this, _, window, cx| this.revert(window, cx))),
            )
            .child(
                Button::new("apply")
                    .small()
                    .primary()
                    .label(w.apply)
                    .disabled(count == 0 || applying)
                    .on_click(cx.listener(|this, _, _, cx| this.apply(cx))),
            )
    }
}

/// A row of the form: its label at the left, its control at the right.
fn row(p: &Palette, label: &'static str, control: impl IntoElement) -> Div {
    h_flex()
        .min_h(px(ROW))
        .py(px(6.0))
        .gap(px(16.0))
        .border_b_1()
        .border_color(p.rule)
        .child(div().w(px(LABEL)).flex_shrink_0().child(label))
        .child(h_flex().flex_1().min_w_0().justify_end().child(control))
}

/// A quiet line under the rows.
fn note(p: &Palette, text: impl Into<SharedString>) -> Div {
    div()
        .pt(px(8.0))
        .text_size(px(NOTE))
        .text_color(p.muted)
        .child(text.into())
}

/// One choice of a few, side by side.
fn segmented(
    id: &'static str,
    choices: impl Iterator<Item = (&'static str, bool)>,
    on_click: impl Fn(&Vec<usize>, &mut Window, &mut gpui_kit::App) + 'static,
) -> ButtonGroup {
    ButtonGroup::new(id)
        .small()
        .outline()
        .children(
            choices.enumerate().map(|(index, (label, picked))| {
                Button::new((id, index)).label(label).selected(picked)
            }),
        )
        .on_click(on_click)
}

/// A slider, and its value beside it.
fn slider_with(p: &Palette, state: &Entity<SliderState>, value: String) -> Div {
    h_flex()
        .gap(px(12.0))
        .child(div().w(px(220.0)).child(Slider::new(state)))
        .child(
            div()
                .w(px(56.0))
                .text_color(p.muted)
                .flex()
                .justify_end()
                .child(value),
        )
}

/// The accents offered, and the one set if it is another.
fn swatches(
    id: &'static str,
    p: &Palette,
    offered: &[u32],
    current: Rgb,
    on_pick: impl Fn(&Rgb, &mut Window, &mut gpui_kit::App) + 'static,
) -> Div {
    let on_pick = Rc::new(on_pick);
    let mut colours: Vec<u32> = offered.to_vec();
    if !colours.contains(&current.0) {
        colours.push(current.0);
    }
    h_flex()
        .gap(px(10.0))
        .children(colours.into_iter().enumerate().map(|(index, hex)| {
            let picked = hex == current.0;
            let on_pick = on_pick.clone();
            let colour: Hsla = gpui_kit::rgb(hex).into();
            div()
                .id((id, index))
                .size(px(22.0))
                .rounded_full()
                .border_2()
                .border_color(if picked {
                    p.text
                } else {
                    gpui_kit::transparent_black()
                })
                .p(px(2.0))
                .cursor_pointer()
                .on_click(move |_, window, cx| on_pick(&Rgb(hex), window, cx))
                .child(div().size_full().rounded_full().bg(colour))
        }))
}

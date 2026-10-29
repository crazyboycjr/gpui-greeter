//! What the greeter knows and does, shared by the windows on every monitor:
//! who logs in with which session, what has been typed, where the login
//! stands, which monitor shows the form, and the clock.

use std::rc::Rc;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use gpui_kit::{Context, KeyDownEvent, Task};
use jiff::Zoned;

use super::looks::Looks;
use super::theme::folder::{Slides, slides};
use crate::config::{Config, PaperBackground, Theme};
use crate::greetd::Greetd;
use crate::i18n::{self, Strings};
use crate::lock::link::{self, ToLock, ToView};
use crate::login::{self, Attempt, Outcome, Typed};
use crate::power::{Action, Power};
use crate::remembered::{self, Remembered};
use crate::sessions::Session;
use crate::system::{self, Machine};
use crate::themes::facts::{self, Facts, Uses};
use crate::themes::{self, Folder, State, TileAction, tiles};
use crate::users::{self, User};
use crate::x11::{Monitor, X11};

/// How long a power action waits for the second press that confirms it.
const CONFIRM: Duration = Duration::from_secs(4);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Phase {
    Idle,
    Checking,
    Denied { notice: Option<String> },
    Failed(String),
    Welcome,
}

/// A question PAM asked after the password, such as a one-time code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Question {
    pub prompt: String,
    pub secret: bool,
}

/// What the line under the password field says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status<'a> {
    Hint,
    Notice(&'a str),
    Checking,
    Denied(Option<&'a str>),
    Failed(&'a str),
    Confirm(Action),
    Welcome,
}

struct Armed {
    action: Action,
    at: Instant,
}

pub struct Setup {
    pub config: Config,
    /// The theme the settings name, which the one picked at the login
    /// screen is remembered against.
    pub setting: Theme,
    pub looks: Looks,
    pub users: Vec<User>,
    pub sessions: Vec<Session>,
    pub monitors: Vec<Monitor>,
    pub form: usize,
    pub x11: Rc<X11>,
    pub power: Power,
    /// A preview's greeter stays on its welcome rather than quit.
    pub preview: bool,
    /// A name to show for the machine instead of its own, in a preview.
    pub hostname: Option<String>,
    /// The lock screen's rules: every look rests until a key, the session
    /// shows but is not chosen, and the theme picked is kept apart.
    pub lock_screen: bool,
    /// The lock's supervisor, which checks the password on the lock
    /// screen; greetd does otherwise.
    pub supervisor: Option<link::Sender>,
}

/// Who checks the password.
#[derive(Clone)]
enum Checker {
    Greetd(Arc<Mutex<Option<Greetd>>>),
    Lock(link::Sender),
}

/// Where a tiles layer's panes stand: their order, the master first, which
/// has the focus, and the layout.
#[derive(Debug, Clone, PartialEq)]
pub struct TilesState {
    pub order: Vec<usize>,
    pub focus: usize,
    /// In the tiles layer's `layouts`.
    pub layout: usize,
    pub ratio: f32,
    /// Full screen over the layout, as xmonad's mod-f.
    pub full: bool,
}

impl TilesState {
    fn new(folder: &Folder) -> Option<Self> {
        let layer = &folder.layers[folder.tiles?];
        let tiles = layer.tiles()?;
        let order = layer.children.clone();
        Some(Self {
            focus: layer
                .base
                .focus
                .filter(|pane| order.contains(pane))
                .or_else(|| order.first().copied())?,
            layout: layer
                .base
                .layout
                .and_then(|layout| tiles.layouts.iter().position(|l| *l == layout))
                .unwrap_or(0),
            ratio: tiles.ratio,
            full: false,
            order,
        })
    }

    pub fn layout(&self, folder: &Folder) -> tiles::Layout {
        let layouts = folder
            .tiles
            .and_then(|at| folder.layers[at].tiles())
            .map(|tiles| tiles.layouts.as_slice())
            .unwrap_or_default();
        match layouts.get(self.layout) {
            _ if self.full => tiles::Layout::Full,
            Some(layout) => *layout,
            None => tiles::Layout::Tall,
        }
    }

    fn act(&mut self, action: TileAction, folder: &Folder) {
        let Some(tiles) = folder.tiles.and_then(|at| folder.layers[at].tiles()) else {
            return;
        };
        let count = self.order.len();
        let at = self
            .order
            .iter()
            .position(|&pane| pane == self.focus)
            .unwrap_or(0);
        if count == 0 {
            return;
        }
        match action {
            TileAction::FocusNext => self.focus = self.order[(at + 1) % count],
            TileAction::FocusPrevious => self.focus = self.order[(at + count - 1) % count],
            TileAction::SwapMaster => {
                let other = if at == 0 { 1.min(count - 1) } else { at };
                self.order.swap(0, other);
            }
            TileAction::NextLayout => {
                self.layout = (self.layout + 1) % tiles.layouts.len().max(1);
                self.full = false;
            }
            TileAction::Shrink => self.ratio = (self.ratio - tiles.ratio_step).max(0.1),
            TileAction::Expand => self.ratio = (self.ratio + tiles.ratio_step).min(0.9),
            TileAction::Full => self.full = !self.full,
        }
    }
}

pub struct Greeter {
    pub config: Rc<Config>,
    pub strings: &'static Strings,
    pub machine: Machine,
    pub users: Vec<User>,
    pub user: usize,
    pub sessions: Vec<Session>,
    pub session: usize,
    pub phase: Phase,
    pub question: Option<Question>,
    pub notice: Option<String>,
    /// The open session menu and the entry the keyboard is on.
    pub menu: Option<usize>,
    /// At rest: Frost's clock screen, or a theme's rest state, before a
    /// click or a key brings the form.
    pub resting: bool,
    /// Counts the times it came to rest and woke, so each one animates.
    pub rest_changes: u32,
    /// Counts refusals, so each one shakes the field.
    pub denials: u32,
    pub monitors: Vec<Monitor>,
    /// The monitor whose window shows the login form.
    pub form: usize,
    pub now: Zoned,
    pub online: bool,
    pub uptime: Option<Duration>,
    pub layouts: Vec<String>,
    pub layout: usize,
    /// The theme folder being shown, if the theme is one.
    pub folder: Option<Arc<Folder>>,
    /// The looks the theme key switches between.
    pub looks: Looks,
    /// Counts the switches of look, so each window starts the new one
    /// afresh.
    pub theme_changes: u32,
    pub tiles: Option<TilesState>,
    /// The theme folder's slideshows.
    pub slides: Slides,
    /// Where slideshows send the slides they want made.
    slide_requests: async_channel::Sender<slides::Request>,
    /// The last action a key or a click ran, and when.
    pub fired: Option<(themes::Action, Instant)>,
    /// Passwords refused since the last rest.
    pub failures: u32,
    pub facts: Facts,
    /// What is in use of the memory, for `{memory}`.
    pub memory: String,
    pub last_input: Instant,
    /// Whether the greeter's VT is the one on screen; while it is not, the
    /// windows are off the screen and nothing is drawn.
    pub on_screen: bool,
    pub lock_screen: bool,
    typed: Typed,
    sent_len: usize,
    armed: Option<Armed>,
    windows: Vec<Option<u32>>,
    focused: Option<u32>,
    checker: Checker,
    power: Power,
    preview: bool,
    setting: Theme,
    facts_read: Uses,
    x11: Rc<X11>,
    _clock: Task<()>,
    _slide_making: Task<()>,
}

impl Greeter {
    pub fn new(setup: Setup, cx: &mut Context<Self>) -> Self {
        let Setup {
            config,
            setting,
            looks,
            users,
            sessions,
            monitors,
            form,
            x11,
            power,
            preview,
            hostname,
            lock_screen,
            supervisor,
        } = setup;
        // The lock screen has one user and shows one session, its own.
        let (user, session) = if lock_screen {
            (0, 0)
        } else {
            let remembered = Remembered::load(remembered::LOGIN);
            let user = remembered
                .user
                .and_then(|name| users.iter().position(|user| user.name == name))
                .unwrap_or(0);
            let session = remembered
                .session
                .or_else(|| {
                    users
                        .get(user)
                        .and_then(|user| users::accounts_session(&user.name))
                })
                .and_then(|id| sessions.iter().position(|session| session.id == id))
                .unwrap_or(0);
            (user, session)
        };
        let (layouts, layout) = x11.layouts();
        let mut machine = Machine::read();
        if let Some(hostname) = hostname {
            machine.hostname = hostname;
        }
        let folder = looks
            .get(&config.theme)
            .and_then(|prepared| prepared.folder.clone());
        let uses = folder
            .as_ref()
            .map(|folder| folder.uses)
            .unwrap_or_default();
        let facts = Facts::read(uses, users.iter().map(|user| user.shell.as_str()));
        let memory = if uses.memory {
            facts::memory(&std::fs::read_to_string("/proc/meminfo").unwrap_or_default())
        } else {
            String::new()
        };
        let (slide_requests, to_make) = async_channel::unbounded();
        let slides = Slides::new(
            folder.as_deref(),
            (&config.wallpaper.dir, &config.wallpaper.pick),
            largest(&monitors),
            0,
            slide_requests.clone(),
        );
        Self {
            slides,
            slide_requests,
            _slide_making: Self::make_slides(to_make, cx),
            strings: if lock_screen {
                i18n::lock_strings(config.language)
            } else {
                i18n::strings(config.language)
            },
            resting: has_rest(&config.theme, folder.as_deref(), lock_screen),
            tiles: folder.as_deref().and_then(TilesState::new),
            folder,
            looks,
            theme_changes: 0,
            setting,
            facts_read: uses,
            fired: None,
            failures: 0,
            facts,
            memory,
            preview,
            config: Rc::new(config),
            machine,
            users,
            user,
            sessions,
            session,
            phase: Phase::Idle,
            question: None,
            notice: None,
            menu: None,
            rest_changes: 0,
            denials: 0,
            windows: vec![None; monitors.len()],
            monitors,
            form,
            now: Zoned::now(),
            online: system::online(),
            uptime: system::uptime(),
            layouts,
            layout,
            typed: Typed::new(),
            sent_len: 0,
            armed: None,
            focused: None,
            last_input: Instant::now(),
            on_screen: true,
            lock_screen,
            checker: match supervisor {
                Some(link) => Checker::Lock(link),
                None => Checker::Greetd(Arc::new(Mutex::new(None))),
            },
            power,
            x11,
            _clock: Self::clock(cx),
        }
    }

    pub fn theme(&self) -> &Theme {
        &self.config.theme
    }

    /// What the look's lengths are multiplied by: the settings' scale, and
    /// a theme folder's own.
    pub fn scale(&self) -> f32 {
        self.config.scale * self.folder.as_ref().map_or(1.0, |folder| folder.scale)
    }

    /// Where the login stands, in a theme's terms.
    pub fn state(&self) -> State {
        if self.resting {
            return State::Rest;
        }
        match self.phase {
            Phase::Welcome => State::Welcome,
            Phase::Checking => State::Checking,
            Phase::Denied { .. } | Phase::Failed(_) => State::Error,
            Phase::Idle if !self.typed.is_empty() => State::Typing,
            Phase::Idle => State::Idle,
        }
    }

    fn has_rest(&self) -> bool {
        has_rest(self.theme(), self.folder.as_deref(), self.lock_screen)
    }

    /// How many characters were sent with the last attempt.
    pub fn sent_len(&self) -> usize {
        self.sent_len
    }

    /// How many characters the field holds now.
    pub fn typed_count(&self) -> usize {
        self.typed.len()
    }

    /// The power action waiting for its second press.
    pub fn armed_action(&self) -> Option<Action> {
        self.armed.as_ref().map(|armed| armed.action)
    }

    /// Shows `state` as though the login had come to it, for previews.
    pub fn show_state(&mut self, state: State, cx: &mut Context<Self>) {
        const SENT: usize = 8;
        self.armed = None;
        self.question = None;
        self.notice = None;
        self.typed = Typed::new();
        self.resting = state == State::Rest;
        self.phase = Phase::Idle;
        match state {
            State::Rest | State::Idle => {}
            State::Typing => self.typed.push("password"),
            State::Checking => {
                self.sent_len = SENT;
                self.phase = Phase::Checking;
            }
            State::Error => {
                self.sent_len = SENT;
                self.phase = Phase::Denied { notice: None };
                self.denials += 1;
                self.failures = self.failures.max(1);
            }
            State::Welcome => {
                self.sent_len = SENT;
                self.phase = Phase::Welcome;
            }
        }
        self.last_input = Instant::now();
        cx.notify();
    }

    pub fn user(&self) -> Option<&User> {
        self.users.get(self.user)
    }

    pub fn session(&self) -> Option<&Session> {
        self.sessions.get(self.session)
    }

    pub fn session_name(&self) -> &str {
        self.session().map_or("", |session| session.name.as_str())
    }

    /// How many characters the field holds, to draw as dots: while they are
    /// being checked, the ones sent.
    pub fn typed_len(&self) -> usize {
        match self.phase {
            Phase::Checking | Phase::Welcome => self.sent_len,
            _ => self.typed.len(),
        }
    }

    /// The answer to a question that is not secret, shown as typed.
    pub fn visible_answer(&self) -> Option<&str> {
        self.question
            .as_ref()
            .filter(|question| !question.secret)
            .map(|_| self.typed.as_str())
    }

    pub fn status(&self) -> Status<'_> {
        if let Some(armed) = &self.armed {
            return Status::Confirm(armed.action);
        }
        match &self.phase {
            Phase::Idle => self.notice.as_deref().map_or(Status::Hint, Status::Notice),
            Phase::Checking => Status::Checking,
            Phase::Denied { notice } => Status::Denied(notice.as_deref()),
            Phase::Failed(message) => Status::Failed(message),
            Phase::Welcome => Status::Welcome,
        }
    }

    pub fn is_busy(&self) -> bool {
        matches!(self.phase, Phase::Checking | Phase::Welcome)
    }

    /// The function keys the look shows on its power buttons, by their
    /// names in key events.
    pub fn power_keys(&self) -> &'static [(&'static str, Action)] {
        const NEON: &[(&str, Action)] = &[("f11", Action::Reboot), ("f12", Action::PowerOff)];
        const DRAFTING: &[(&str, Action)] = &[
            ("f10", Action::Suspend),
            ("f11", Action::Reboot),
            ("f12", Action::PowerOff),
        ];
        let paper = matches!(self.theme(), Theme::Paper | Theme::PaperNight);
        match self.theme() {
            Theme::Neon => NEON,
            _ if paper && self.config.paper.background == PaperBackground::Drafting => DRAFTING,
            _ => &[],
        }
    }

    /// The name of an action in the theme's words.
    pub fn action_name(&self, action: Action) -> &'static str {
        let (s, neon) = (self.strings, *self.theme() == Theme::Neon);
        match action {
            Action::Suspend => s.sleep,
            Action::Reboot if neon => s.neon.reboot,
            Action::Reboot => s.restart,
            Action::PowerOff if neon => s.neon.poweroff,
            Action::PowerOff => s.shut_down,
        }
    }

    pub fn set_window(&mut self, output: usize, xid: Option<u32>) {
        if let Some(slot) = self.windows.get_mut(output) {
            *slot = xid;
        }
        self.keep_focus();
    }

    pub fn key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        if self.phase == Phase::Welcome {
            return;
        }
        self.last_input = Instant::now();
        if let Some(folder) = self.folder.clone() {
            self.folder_key(&folder, event, cx);
            cx.notify();
            return;
        }
        let keystroke = &event.keystroke;
        let modifiers = keystroke.modifiers;
        if let Some(index) = self.menu {
            let count = self.sessions.len().max(1);
            match keystroke.key.as_str() {
                "up" => self.menu = Some((index + count - 1) % count),
                "tab" if modifiers.shift => self.menu = Some((index + count - 1) % count),
                "down" | "tab" => self.menu = Some((index + 1) % count),
                "enter" | "space" => self.pick_session(index, cx),
                "escape" => self.menu = None,
                _ => {}
            }
            cx.notify();
            return;
        }
        // On the lock screen every key brings the form, but for the theme
        // key and Escape, which leave the rest as it is, and for Enter and
        // Space, which only wake below.
        if self.lock_screen
            && !matches!(keystroke.key.as_str(), "f9" | "escape" | "enter" | "space")
        {
            self.wake();
        }
        let power_key = self
            .power_keys()
            .iter()
            .find(|(key, _)| *key == keystroke.key)
            .map(|&(_, action)| action);
        if let Some(action) = power_key {
            self.power(action, cx);
            return;
        }
        match keystroke.key.as_str() {
            // Keys that only wake the clock screen, rather than type or
            // choose a session out of sight.
            "enter" | "space" | "tab" | "f2" if self.resting => self.wake(),
            "enter" => self.submit(cx),
            "escape" => self.escape(cx),
            "backspace" if modifiers.control => self.clear_typed(),
            "backspace" => self.typed.pop(),
            "u" if modifiers.control => self.clear_typed(),
            "f9" if modifiers.shift => self.switch_theme(-1, cx),
            "f9" => self.switch_theme(1, cx),
            "tab" if modifiers.shift => self.previous_session(),
            "tab" | "f2" => self.next_session(),
            "f3" => self.next_layout(),
            "up" => self.next_user(-1, cx),
            "down" => self.next_user(1, cx),
            _ => match &keystroke.key_char {
                Some(text) if !modifiers.control && !modifiers.alt && !modifiers.platform => {
                    let text = text.clone();
                    self.type_text(&text);
                }
                _ if self.resting => self.wake(),
                _ => {}
            },
        }
        cx.notify();
    }

    /// A key in a theme folder: its `[keys]` first, then typing.
    fn folder_key(&mut self, folder: &Folder, event: &KeyDownEvent, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;
        let modifiers = keystroke.modifiers;
        let held = themes::Held {
            ctrl: modifiers.control,
            alt: modifiers.alt,
            shift: modifiers.shift,
            super_key: modifiers.platform,
        };
        if let Some(action) = folder.keys.lookup(&keystroke.key, held) {
            self.run(action, None, cx);
            return;
        }
        match keystroke.key.as_str() {
            "backspace" if modifiers.control => self.clear_typed(),
            "backspace" => self.typed.pop(),
            _ => match &keystroke.key_char {
                Some(text) if !modifiers.control && !modifiers.alt && !modifiers.platform => {
                    let text = text.clone();
                    self.type_text(&text);
                }
                _ if self.resting => self.wake(),
                _ => {}
            },
        }
    }

    /// Runs what a theme's key or click asks for; `item` is the session a
    /// `for_each` layer stands for.
    pub fn run(&mut self, action: themes::Action, item: Option<usize>, cx: &mut Context<Self>) {
        use themes::Action as Run;
        if self.phase == Phase::Welcome {
            return;
        }
        self.last_input = Instant::now();
        // The lock screen switches looks without leaving the rest.
        let keeps_rest = matches!(action, Run::Rest | Run::Nothing)
            || self.lock_screen && matches!(action, Run::NextTheme | Run::PreviousTheme);
        if !keeps_rest {
            self.wake();
        }
        // The lock screen's session is the one running: a session key or
        // control only wakes it, and lights nothing.
        let chooses_session = matches!(
            action,
            Run::NextSession | Run::PreviousSession | Run::Session(_) | Run::Pick
        );
        if self.lock_screen && chooses_session {
            cx.notify();
            return;
        }
        self.fired = Some((action, Instant::now()));
        match action {
            Run::Submit => self.submit(cx),
            Run::NextSession => self.next_session(),
            Run::PreviousSession => self.previous_session(),
            Run::Session(number) => self.pick_session(number.saturating_sub(1), cx),
            Run::Pick => {
                if let Some(index) = item {
                    self.pick_session(index, cx);
                }
            }
            Run::NextUser => self.next_user(1, cx),
            Run::PreviousUser => self.next_user(-1, cx),
            Run::NextKeyboardLayout => self.next_layout(),
            Run::NextTheme => self.switch_theme(1, cx),
            Run::PreviousTheme => self.switch_theme(-1, cx),
            Run::Rest => self.escape(cx),
            Run::Wake | Run::Nothing => {}
            Run::Clear => self.clear_typed(),
            Run::Power(power) => self.power(power, cx),
            Run::Tile(tile) => {
                if let (Some(tiles), Some(folder)) = (self.tiles.as_mut(), self.folder.as_deref()) {
                    tiles.act(tile, folder);
                }
            }
        }
        cx.notify();
    }

    fn type_text(&mut self, text: &str) {
        self.wake();
        if self.phase == Phase::Checking {
            return;
        }
        if matches!(self.phase, Phase::Denied { .. } | Phase::Failed(_)) {
            self.phase = Phase::Idle;
        }
        self.typed.push(text);
        // The pane holding the field takes the focus, as a window does.
        if let (Some(tiles), Some(folder)) = (self.tiles.as_mut(), self.folder.as_deref())
            && let Some(pane) = field_pane(folder)
        {
            tiles.focus = pane;
        }
    }

    fn clear_typed(&mut self) {
        self.typed = Typed::new();
    }

    /// Leaves rest for the form.
    pub fn wake(&mut self) {
        if self.resting {
            self.resting = false;
            self.rest_changes += 1;
            self.last_input = Instant::now();
        }
    }

    /// Goes back to rest, forgetting what was typed.
    pub fn rest(&mut self, cx: &mut Context<Self>) {
        if self.has_rest() && !self.resting && !self.is_busy() {
            self.resting = true;
            self.rest_changes += 1;
            self.failures = 0;
            self.menu = None;
            self.clear_typed();
            self.forget_question(cx);
            if !matches!(self.phase, Phase::Idle) {
                self.phase = Phase::Idle;
            }
            cx.notify();
        }
    }

    fn escape(&mut self, cx: &mut Context<Self>) {
        if self.has_rest() && !self.resting {
            self.rest(cx);
        } else {
            self.clear_typed();
            self.forget_question(cx);
        }
    }

    pub fn toggle_menu(&mut self, cx: &mut Context<Self>) {
        if self.lock_screen {
            return;
        }
        self.menu = match self.menu {
            Some(_) => None,
            None => Some(self.session),
        };
        cx.notify();
    }

    pub fn close_menu(&mut self, cx: &mut Context<Self>) {
        if self.menu.take().is_some() {
            cx.notify();
        }
    }

    pub fn pick_session(&mut self, index: usize, cx: &mut Context<Self>) {
        if index < self.sessions.len() {
            self.session = index;
        }
        self.menu = None;
        cx.notify();
    }

    pub fn next_session(&mut self) {
        if !self.sessions.is_empty() {
            self.session = (self.session + 1) % self.sessions.len();
        }
    }

    pub fn previous_session(&mut self) {
        if !self.sessions.is_empty() {
            self.session = (self.session + self.sessions.len() - 1) % self.sessions.len();
        }
    }

    pub fn next_layout(&mut self) {
        if self.layouts.len() < 2 {
            return;
        }
        let next = (self.layout + 1) % self.layouts.len();
        match self.x11.lock_layout(next) {
            Ok(()) => self.layout = next,
            Err(error) => log::warn!("cannot switch the keyboard layout: {error:#}"),
        }
    }

    pub fn next_user(&mut self, step: isize, cx: &mut Context<Self>) {
        let count = self.users.len() as isize;
        if count < 2 || self.is_busy() {
            return;
        }
        self.user = (self.user as isize + step).rem_euclid(count) as usize;
        self.clear_typed();
        self.forget_question(cx);
        self.phase = Phase::Idle;
        cx.notify();
    }

    /// A pointer moving over another monitor brings the form along.
    pub fn pointer_on(&mut self, output: usize, cx: &mut Context<Self>) {
        self.last_input = Instant::now();
        if output != self.form && output < self.monitors.len() && self.phase != Phase::Welcome {
            self.form = output;
            self.menu = None;
            self.keep_focus();
            cx.notify();
        }
    }

    /// The first press of a power button asks for a second.
    pub fn power(&mut self, action: Action, cx: &mut Context<Self>) {
        if self.is_busy() {
            return;
        }
        let confirmed = self
            .armed
            .as_ref()
            .is_some_and(|armed| armed.action == action && armed.at.elapsed() < CONFIRM);
        if matches!(self.phase, Phase::Denied { .. } | Phase::Failed(_)) {
            self.phase = Phase::Idle;
        }
        if !confirmed {
            self.armed = Some(Armed {
                action,
                at: Instant::now(),
            });
            cx.notify();
            return;
        }
        self.armed = None;
        let power = self.power;
        let name = self.action_name(action);
        let failed = self.strings.failed;
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { power.run(action) })
                .await;
            if let Err(error) = result {
                log::error!("{name}: {error:#}");
                let _ = this.update(cx, |this, cx| {
                    this.phase = Phase::Failed(format!("{name}{failed}{error}"));
                    cx.notify();
                });
            }
        })
        .detach();
        cx.notify();
    }

    pub fn submit(&mut self, cx: &mut Context<Self>) {
        let ready = matches!(
            self.phase,
            Phase::Idle | Phase::Denied { .. } | Phase::Failed(_)
        );
        if !ready || self.typed.is_empty() {
            return;
        }
        let sent = self.typed.len();
        match self.checker.clone() {
            // The supervisor's word comes back over the link, to `checked`.
            Checker::Lock(link) => {
                let typed = self.typed.take();
                cx.background_executor()
                    .spawn(async move {
                        if let Err(error) = link.answer(typed) {
                            log::error!("cannot reach the lock's supervisor: {error}");
                        }
                    })
                    .detach();
            }
            Checker::Greetd(greetd) => {
                let (Some(user), Some(session)) = (self.user(), self.session()) else {
                    return;
                };
                let (user, cmd, env) = (
                    user.name.clone(),
                    session.command(&self.config.sessions.wrapper),
                    session.environment(),
                );
                let attempt = Attempt {
                    user,
                    typed: self.typed.take(),
                    resumes: self.question.is_some(),
                    cmd,
                    env,
                };
                cx.spawn(async move |this, cx| {
                    let outcome = cx
                        .background_executor()
                        .spawn(async move { converse(&greetd, attempt) })
                        .await;
                    let _ = this.update(cx, |this, cx| this.finish(outcome, cx));
                })
                .detach();
            }
        }
        self.sent_len = sent;
        self.phase = Phase::Checking;
        self.notice = None;
        self.armed = None;
        cx.notify();
    }

    /// The lock's supervisor's word on what was sent to it.
    pub fn checked(&mut self, said: ToView, cx: &mut Context<Self>) {
        let outcome = match said {
            ToView::Asked {
                prompt,
                secret,
                notice,
            } => Ok(Outcome::Asked {
                prompt,
                secret,
                notice,
            }),
            ToView::Denied { notice } => Ok(Outcome::Denied { notice }),
            ToView::Failed { message } => Err(Failure::Broken(anyhow::anyhow!(message))),
            ToView::Moved { .. } | ToView::Pressed { .. } | ToView::Released { .. } => return,
        };
        if self.phase == Phase::Checking {
            self.finish(outcome, cx);
        }
    }

    fn finish(&mut self, outcome: Result<Outcome, Failure>, cx: &mut Context<Self>) {
        self.question = None;
        match outcome {
            Ok(Outcome::Started) => {
                self.phase = Phase::Welcome;
                self.failures = 0;
                if self.preview {
                    cx.notify();
                    return;
                }
                self.remember();
                // greetd starts the session once the greeter has gone, so it
                // goes at once.
                cx.quit();
                // Should the toolkit hang on its way out, greetd's SIGTERM
                // after five seconds would only reach the shell around startx,
                // and leave this X server on the session's VT.
                std::thread::spawn(|| {
                    std::thread::sleep(Duration::from_secs(2));
                    std::process::exit(0);
                });
            }
            Ok(Outcome::Asked {
                prompt,
                secret,
                notice,
            }) => {
                self.question = Some(Question { prompt, secret });
                self.notice = notice;
                self.phase = Phase::Idle;
            }
            Ok(Outcome::Denied { notice }) => {
                self.phase = Phase::Denied { notice };
                self.denials += 1;
                self.failures += 1;
            }
            Err(Failure::Unreachable(error)) => {
                log::error!("{error:#}");
                self.phase = Phase::Failed(self.strings.no_greetd.to_string());
                self.denials += 1;
                self.failures += 1;
            }
            Err(Failure::Broken(error)) => {
                log::error!("{error:#}");
                self.phase = Phase::Failed(error.to_string());
                self.denials += 1;
                self.failures += 1;
            }
        }
        cx.notify();
    }

    fn remember(&self) {
        let mut remembered = Remembered::load(remembered::LOGIN);
        remembered.user = self.user().map(|user| user.name.clone());
        remembered.session = self.session().map(|session| session.id.clone());
        remembered.save(remembered::LOGIN);
    }

    /// The look the theme key picked, kept for the next start: the login
    /// screen's and the lock screen's apart.
    fn remember_theme(&self) {
        if self.preview {
            return;
        }
        let file = if self.lock_screen {
            remembered::LOCK
        } else {
            remembered::LOGIN
        };
        let mut remembered = Remembered::load(file);
        remembered.theme = Some(self.theme().name().to_string());
        remembered.theme_setting = Some(self.setting.name().to_string());
        remembered.save(file);
    }

    /// The look's name, as its control shows it.
    pub fn theme_title(&self) -> String {
        match self.theme() {
            Theme::Paper => "Paper".into(),
            Theme::PaperNight => "Paper Night".into(),
            Theme::Frost => "Frost".into(),
            Theme::Neon => "Neon".into(),
            Theme::Folder(name) => self
                .folder
                .as_ref()
                .map_or_else(|| name.clone(), |folder| folder.title.clone()),
        }
    }

    /// The theme key: the next look, or with `step` -1 the one before. A look
    /// not ready yet is made ready, and shown once it is.
    pub fn switch_theme(&mut self, step: isize, cx: &mut Context<Self>) {
        if self.is_busy() {
            return;
        }
        let Some(target) = self.looks.neighbour(self.theme(), step) else {
            return;
        };
        self.last_input = Instant::now();
        if self.looks.is_ready(&target) {
            self.apply_theme(target, cx);
        } else {
            self.looks.wanted = Some(target.clone());
            self.looks.prepare(&target, cx);
        }
        cx.notify();
    }

    /// A look is ready, or failed to be: shows it if it was waited for.
    pub fn look_ready(&mut self, theme: &Theme, cx: &mut Context<Self>) {
        if self.looks.wanted.as_ref() == Some(theme) && self.looks.is_ready(theme) {
            self.apply_theme(theme.clone(), cx);
        } else if self.looks.has_failed(theme) {
            // A theme folder that cannot be read is skipped over.
            self.looks.cycle.retain(|other| other != theme);
        }
        cx.notify();
    }

    fn apply_theme(&mut self, theme: Theme, cx: &mut Context<Self>) {
        self.looks.wanted = None;
        let folder = self
            .looks
            .get(&theme)
            .and_then(|prepared| prepared.folder.clone());
        if let Some(folder) = &folder {
            let wanted = self.facts_read.and(folder.uses);
            if wanted != self.facts_read {
                self.facts = Facts::read(wanted, self.users.iter().map(|user| user.shell.as_str()));
                self.facts_read = wanted;
            }
        }
        Rc::make_mut(&mut self.config).theme = theme;
        self.tiles = folder.as_deref().and_then(TilesState::new);
        self.folder = folder;
        self.theme_changes += 1;
        self.slides = Slides::new(
            self.folder.as_deref(),
            (&self.config.wallpaper.dir, &self.config.wallpaper.pick),
            largest(&self.monitors),
            self.theme_changes.into(),
            self.slide_requests.clone(),
        );
        self.fired = None;
        self.menu = None;
        if !self.has_rest() {
            self.resting = false;
        }
        self.remember_theme();
        self.prepare_neighbours(cx);
        cx.notify();
    }

    /// Makes the looks either side of this one ready, so the theme key
    /// shows them at once.
    pub fn prepare_neighbours(&mut self, cx: &mut Context<Self>) {
        for step in [1, -1] {
            if let Some(theme) = self.looks.neighbour(self.theme(), step) {
                self.looks.prepare(&theme, cx);
            }
        }
    }

    /// Leaves a question PAM asked, for a fresh start with the password.
    fn forget_question(&mut self, cx: &mut Context<Self>) {
        self.notice = None;
        if self.question.take().is_none() {
            return;
        }
        match self.checker.clone() {
            Checker::Lock(link) => {
                if let Err(error) = link.send(&ToLock::Cancel) {
                    log::error!("cannot reach the lock's supervisor: {error}");
                }
            }
            Checker::Greetd(greetd) => cx
                .background_executor()
                .spawn(async move {
                    let mut slot = greetd.lock().unwrap_or_else(PoisonError::into_inner);
                    if let Some(Err(error)) = slot.as_mut().map(Greetd::cancel) {
                        log::warn!("cannot cancel the login: {error:#}");
                        *slot = None;
                    }
                })
                .detach(),
        }
    }

    /// Makes the slides the slideshows ask for, one at a time, in the
    /// background, and draws them once made.
    fn make_slides(
        requests: async_channel::Receiver<slides::Request>,
        cx: &mut Context<Self>,
    ) -> Task<()> {
        cx.spawn(async move |this, cx| {
            while let Ok(request) = requests.recv().await {
                let (request, made) = cx
                    .background_executor()
                    .spawn(async move {
                        let made = request.make();
                        (request, made)
                    })
                    .await;
                let arrived = this.update(cx, |greeter, cx| {
                    greeter.slides.arrived(&request, made, Instant::now());
                    if greeter.on_screen {
                        cx.notify();
                    }
                });
                if arrived.is_err() {
                    break;
                }
            }
        })
    }

    fn clock(cx: &mut Context<Self>) -> Task<()> {
        cx.spawn(async move |this, cx| {
            loop {
                let nanos = Zoned::now().subsec_nanosecond().max(0) as u64;
                let wait = Duration::from_nanos(1_000_000_000 - nanos.min(999_999_999));
                cx.background_executor().timer(wait).await;
                if this.update(cx, |this, cx| this.tick(cx)).is_err() {
                    break;
                }
            }
        })
    }

    fn tick(&mut self, cx: &mut Context<Self>) {
        self.now = Zoned::now();
        // The supervisor starts again a view it has not heard from in a
        // while.
        if let Checker::Lock(link) = &self.checker
            && self.now.second() % 5 == 0
            && let Err(error) = link.send(&ToLock::Alive)
        {
            log::error!("cannot reach the lock's supervisor: {error}");
        }
        if self
            .armed
            .as_ref()
            .is_some_and(|armed| armed.at.elapsed() >= CONFIRM)
        {
            self.armed = None;
        }
        if self.now.second() % 10 == 0 {
            self.online = system::online();
            self.uptime = system::uptime();
            if self
                .folder
                .as_ref()
                .is_some_and(|folder| folder.uses.memory)
            {
                let meminfo = std::fs::read_to_string("/proc/meminfo").unwrap_or_default();
                self.memory = facts::memory(&meminfo);
            }
        }
        let idle = match &self.folder {
            Some(folder) => folder.rest_after,
            None => Some(Duration::from_secs(self.config.frost.idle_seconds)),
        };
        if !self.resting && idle.is_some_and(|idle| self.last_input.elapsed() >= idle) {
            self.rest(cx);
        }
        if self.on_screen {
            self.keep_focus();
            cx.notify();
        }
    }

    /// The greeter's VT went to the background, or came back. While it is
    /// away the windows are off the screen, where GPUI draws nothing for
    /// them: an X server whose VT is in the background shows no frame, and
    /// on NVIDIA each frame drawn then loses the GPU device, which GPUI
    /// would recreate again and again. Back, the windows return, and their
    /// first frame recovers the device if it was lost.
    pub fn set_on_screen(&mut self, shown: bool, cx: &mut Context<Self>) {
        if shown == self.on_screen {
            return;
        }
        self.on_screen = shown;
        if shown {
            log::info!("the greeter's VT is on screen again: drawing again");
        } else {
            log::info!("the greeter's VT went to the background: drawing stops until it is back");
        }
        for (output, xid) in self.windows.iter().enumerate() {
            let (Some(xid), Some(monitor)) = (*xid, self.monitors.get(output)) else {
                continue;
            };
            let done = if shown {
                self.x11.show(xid, monitor.rect)
            } else {
                self.x11.hide(xid)
            };
            if let Err(error) = done {
                log::warn!(
                    "cannot show or hide the window on {:?}: {error:#}",
                    monitor.rect
                );
            }
        }
        self.focused = None;
        if shown {
            self.keep_focus();
            cx.notify();
        }
    }

    /// The form's window takes the keyboard, once it is on screen. On the
    /// lock screen the supervisor holds the keyboard and hands the keys
    /// on, and the session's focus stays where it was.
    fn keep_focus(&mut self) {
        if matches!(self.checker, Checker::Lock(_)) {
            return;
        }
        let Some(xid) = self.windows.get(self.form).copied().flatten() else {
            return;
        };
        if self.focused != Some(xid) && self.x11.focus(xid) {
            self.focused = Some(xid);
        }
    }
}

/// Whether a look has a rest state, which it starts at and Escape and time
/// return to: Frost's clock screen, a theme's with `rest`, and on the lock
/// screen every built-in look's.
fn has_rest(theme: &Theme, folder: Option<&Folder>, lock_screen: bool) -> bool {
    match folder {
        Some(folder) => folder.rest,
        None => lock_screen || *theme == Theme::Frost,
    }
}

/// The largest monitor's size, in pixels.
fn largest(monitors: &[Monitor]) -> (u32, u32) {
    monitors.iter().fold((1, 1), |(width, height), monitor| {
        (
            width.max(monitor.rect.width),
            height.max(monitor.rect.height),
        )
    })
}

/// The tiles pane the password field is in, if it is in one.
fn field_pane(folder: &Folder) -> Option<usize> {
    let field = folder
        .layers
        .iter()
        .position(|layer| matches!(layer.kind, themes::Kind::Field { .. }))?;
    std::iter::successors(Some(field), |&at| folder.layers[at].parent)
        .find(|&at| folder.layers[at].pane.is_some())
}

enum Failure {
    /// greetd's socket is not there or does not answer.
    Unreachable(anyhow::Error),
    /// The conversation broke off; the next attempt starts a new one.
    Broken(anyhow::Error),
}

fn converse(greetd: &Mutex<Option<Greetd>>, attempt: Attempt) -> Result<Outcome, Failure> {
    let mut slot = greetd.lock().unwrap_or_else(PoisonError::into_inner);
    let client = match slot.as_mut() {
        Some(client) => client,
        None => slot.insert(Greetd::connect().map_err(Failure::Unreachable)?),
    };
    let outcome = login::attempt(client, attempt);
    if outcome.is_err() {
        *slot = None;
    }
    outcome.map_err(Failure::Broken)
}

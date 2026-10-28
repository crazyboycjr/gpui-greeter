//! What the greeter knows and does, shared by the windows on every monitor:
//! who logs in with which session, what has been typed, where the login
//! stands, which monitor shows the form, and the clock.

use std::rc::Rc;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use gpui_kit::{Context, KeyDownEvent, Task};
use jiff::Zoned;
use zeroize::Zeroizing;

use crate::config::{Config, PaperBackground, Theme};
use crate::greetd::Greetd;
use crate::i18n::{self, Strings};
use crate::login::{self, Attempt, Outcome};
use crate::power::{Action, Power};
use crate::remembered::Remembered;
use crate::sessions::Session;
use crate::system::{self, Machine};
use crate::users::{self, User};
use crate::x11::{Monitor, X11};

/// How long a power action waits for the second press that confirms it.
const CONFIRM: Duration = Duration::from_secs(4);
/// How long the welcome shows before the greeter makes way for the session.
const WELCOME: Duration = Duration::from_millis(1200);

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
    pub users: Vec<User>,
    pub sessions: Vec<Session>,
    pub monitors: Vec<Monitor>,
    pub form: usize,
    pub x11: Rc<X11>,
    pub power: Power,
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
    /// Frost's clock screen, before a click or a key brings the form.
    pub locked: bool,
    /// Counts lockings and unlockings, so each one animates.
    pub lock_changes: u32,
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
    typed: Typed,
    sent_len: usize,
    armed: Option<Armed>,
    windows: Vec<Option<u32>>,
    focused: Option<u32>,
    last_input: Instant,
    greetd: Arc<Mutex<Option<Greetd>>>,
    power: Power,
    x11: Rc<X11>,
    _clock: Task<()>,
}

impl Greeter {
    pub fn new(setup: Setup, cx: &mut Context<Self>) -> Self {
        let Setup {
            config,
            users,
            sessions,
            monitors,
            form,
            x11,
            power,
        } = setup;
        let remembered = Remembered::load();
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
        let (layouts, layout) = x11.layouts();
        Self {
            strings: i18n::strings(config.language),
            locked: config.theme == Theme::Frost,
            config: Rc::new(config),
            machine: Machine::read(),
            users,
            user,
            sessions,
            session,
            phase: Phase::Idle,
            question: None,
            notice: None,
            menu: None,
            lock_changes: 0,
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
            greetd: Arc::new(Mutex::new(None)),
            power,
            x11,
            _clock: Self::clock(cx),
        }
    }

    pub fn theme(&self) -> &Theme {
        &self.config.theme
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
        let keystroke = &event.keystroke;
        let modifiers = keystroke.modifiers;
        if let Some(index) = self.menu {
            let count = self.sessions.len().max(1);
            match keystroke.key.as_str() {
                "up" => self.menu = Some((index + count - 1) % count),
                "down" | "tab" => self.menu = Some((index + 1) % count),
                "enter" | "space" => self.pick_session(index, cx),
                "escape" => self.menu = None,
                _ => {}
            }
            cx.notify();
            return;
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
            // Keys that only wake the clock screen, rather than type.
            "enter" | "space" if self.locked => self.unlock(),
            "enter" => self.submit(cx),
            "escape" => self.escape(cx),
            "backspace" if modifiers.control => self.clear_typed(),
            "backspace" => self.typed.pop(),
            "u" if modifiers.control => self.clear_typed(),
            "f2" => self.next_session(),
            "f3" => self.next_layout(),
            "up" => self.next_user(-1, cx),
            "down" => self.next_user(1, cx),
            _ => match &keystroke.key_char {
                Some(text) if !modifiers.control && !modifiers.alt && !modifiers.platform => {
                    let text = text.clone();
                    self.type_text(&text);
                }
                _ if self.locked => self.unlock(),
                _ => {}
            },
        }
        cx.notify();
    }

    fn type_text(&mut self, text: &str) {
        self.unlock();
        if self.phase == Phase::Checking {
            return;
        }
        if matches!(self.phase, Phase::Denied { .. } | Phase::Failed(_)) {
            self.phase = Phase::Idle;
        }
        self.typed.push(text);
    }

    fn clear_typed(&mut self) {
        self.typed = Typed::new();
    }

    pub fn unlock(&mut self) {
        if self.locked {
            self.locked = false;
            self.lock_changes += 1;
            self.last_input = Instant::now();
        }
    }

    pub fn lock(&mut self, cx: &mut Context<Self>) {
        if *self.theme() == Theme::Frost && !self.locked && !self.is_busy() {
            self.locked = true;
            self.lock_changes += 1;
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
        if *self.theme() == Theme::Frost && !self.locked {
            self.lock(cx);
        } else {
            self.clear_typed();
            self.forget_question(cx);
        }
    }

    pub fn toggle_menu(&mut self, cx: &mut Context<Self>) {
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
        let (Some(user), Some(session)) = (self.user(), self.session()) else {
            return;
        };
        if !ready || self.typed.is_empty() {
            return;
        }
        let (user, cmd, env) = (
            user.name.clone(),
            session.command(&self.config.sessions.wrapper),
            session.environment(),
        );
        self.sent_len = self.typed.len();
        let attempt = Attempt {
            user,
            typed: self.typed.take(),
            resumes: self.question.is_some(),
            cmd,
            env,
        };
        self.phase = Phase::Checking;
        self.notice = None;
        self.armed = None;
        let greetd = self.greetd.clone();
        cx.spawn(async move |this, cx| {
            let outcome = cx
                .background_executor()
                .spawn(async move { converse(&greetd, attempt) })
                .await;
            let _ = this.update(cx, |this, cx| this.finish(outcome, cx));
        })
        .detach();
        cx.notify();
    }

    fn finish(&mut self, outcome: Result<Outcome, Failure>, cx: &mut Context<Self>) {
        self.question = None;
        match outcome {
            Ok(Outcome::Started) => {
                self.phase = Phase::Welcome;
                self.remember();
                cx.spawn(async move |_, cx| {
                    cx.background_executor().timer(WELCOME).await;
                    cx.update(|cx| cx.quit());
                })
                .detach();
                // The session waits for the greeter to go: should the toolkit
                // hang on its way out, greetd's SIGTERM after five seconds
                // would only reach the shell around startx, and leave this X
                // server on the session's VT.
                std::thread::spawn(|| {
                    std::thread::sleep(WELCOME + Duration::from_secs(2));
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
            }
            Err(Failure::Unreachable(error)) => {
                log::error!("{error:#}");
                self.phase = Phase::Failed(self.strings.no_greetd.to_string());
                self.denials += 1;
            }
            Err(Failure::Broken(error)) => {
                log::error!("{error:#}");
                self.phase = Phase::Failed(error.to_string());
                self.denials += 1;
            }
        }
        cx.notify();
    }

    fn remember(&self) {
        Remembered {
            user: self.user().map(|user| user.name.clone()),
            session: self.session().map(|session| session.id.clone()),
        }
        .save();
    }

    /// Leaves a question PAM asked, for a fresh start with the password.
    fn forget_question(&mut self, cx: &mut Context<Self>) {
        self.notice = None;
        if self.question.take().is_none() {
            return;
        }
        let greetd = self.greetd.clone();
        cx.background_executor()
            .spawn(async move {
                let mut slot = greetd.lock().unwrap_or_else(PoisonError::into_inner);
                if let Some(Err(error)) = slot.as_mut().map(Greetd::cancel) {
                    log::warn!("cannot cancel the login: {error:#}");
                    *slot = None;
                }
            })
            .detach();
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
        }
        let idle = Duration::from_secs(self.config.frost.idle_seconds);
        if !self.locked && self.last_input.elapsed() >= idle {
            self.lock(cx);
        }
        self.keep_focus();
        cx.notify();
    }

    /// The form's window takes the keyboard, once it is on screen.
    fn keep_focus(&mut self) {
        let Some(xid) = self.windows.get(self.form).copied().flatten() else {
            return;
        };
        if self.focused != Some(xid) && self.x11.focus(xid) {
            self.focused = Some(xid);
        }
    }
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

/// What has been typed into the field, which leaves no copy behind: its
/// buffer is made large enough never to grow, and one that text leaves is
/// wiped as it is dropped.
struct Typed(Zeroizing<String>);

impl Typed {
    /// Room for any password; typing beyond it is ignored.
    const CAPACITY: usize = 1024;

    fn new() -> Self {
        Self(Zeroizing::new(String::with_capacity(Self::CAPACITY)))
    }

    fn push(&mut self, text: &str) {
        if self.0.len() + text.len() <= Self::CAPACITY {
            self.0.push_str(text);
        }
    }

    fn pop(&mut self) {
        let mut kept = Self::new();
        if let Some((last, _)) = self.0.char_indices().next_back() {
            kept.0.push_str(&self.0[..last]);
        }
        *self = kept;
    }

    /// The text, for sending; the field is left empty.
    fn take(&mut self) -> Zeroizing<String> {
        std::mem::replace(self, Self::new()).0
    }

    fn len(&self) -> usize {
        self.0.chars().count()
    }

    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    fn as_str(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::Typed;

    #[test]
    fn typed_text_stays_in_its_first_buffer() {
        let mut typed = Typed::new();
        let buffer = typed.as_str().as_ptr();
        typed.push("pässwörd");
        assert_eq!(typed.as_str().as_ptr(), buffer);
        typed.pop();
        assert_eq!(typed.as_str(), "pässwör");
        assert_eq!(typed.len(), 7);
        assert_eq!(&*typed.take(), "pässwör");
        assert!(typed.is_empty());
        typed.push(&"x".repeat(Typed::CAPACITY + 1));
        assert!(typed.is_empty());
    }
}

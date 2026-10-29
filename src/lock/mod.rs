//! `gpui-greeter lock`: the screen locker for a running X session.
//!
//! The command's process is the supervisor. It covers the screen with a
//! plain window above every other, holds the keyboard and the pointer for
//! as long as the screen is locked, and checks the password with PAM. The
//! looks are drawn by its view, a second process it starts, which gets the
//! keys from it and hands it what was typed. Should the view die or hang,
//! the cover and the grabs stay, the cover takes the password itself, and
//! the view starts again: only the supervisor's own end gives the screen
//! back.

mod cover;
pub mod link;
pub mod pam;

use std::io::{Read as _, Write as _};
use std::ops::ControlFlow;
use std::os::fd::{AsRawFd as _, BorrowedFd, FromRawFd as _, OwnedFd, RawFd};
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt as _;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use anyhow::{Context as _, Result};
use rustix::event::{PollFd, PollFlags, Timespec};
use rustix::process::{Signal, set_parent_process_death_signal};
use x11rb::protocol::Event;
use x11rb::protocol::xproto::{self, KeyButMask};
use zeroize::Zeroizing;

use crate::config::{Language, Theme};
use crate::greetd::wire;
use crate::login::Typed;
use crate::x11::Monitor;
use cover::Cover;
use link::{ToLock, ToView};

/// How long the lock waits for another program to let go of the keyboard
/// and the pointer.
const GRAB_PATIENCE: Duration = Duration::from_secs(2);
/// How long a suspend xss-lock holds for the lock waits for the view to
/// show, before it goes ahead over the plain cover.
const SLEEP_LOCK_WAIT: Duration = Duration::from_secs(2);
/// The view starts again this long after it ended, and twice as long after
/// each end that came soon after its start, up to `RESTART_MOST`.
const RESTART: Duration = Duration::from_millis(250);
const RESTART_MOST: Duration = Duration::from_secs(8);
/// A view that ran this long had no trouble starting.
const STEADY: Duration = Duration::from_secs(30);
/// A view not shown this long after its start, or silent this long since,
/// is taken for stuck and started again.
const SHOW_WAIT: Duration = Duration::from_secs(20);
const SILENCE: Duration = Duration::from_secs(20);
/// How long a message to the view may wait for it to read.
const WRITE_WAIT: Duration = Duration::from_millis(500);
/// Without a view this long, the cover shows where the password goes.
const FIELD_AFTER: Duration = Duration::from_secs(2);
/// xss-lock's `--transfer-sleep-lock` hands its delay lock on suspend over
/// in this descriptor, for the locker to close once the screen is locked.
const SLEEP_LOCK_FD: &str = "XSS_SLEEP_LOCK_FD";

pub struct Options {
    pub config: PathBuf,
    pub theme: Option<Theme>,
    pub language: Option<Language>,
    pub dry_run: bool,
    pub service: pam::Service,
}

pub fn run(options: Options) -> Result<()> {
    // There the X server is XWayland, whose windows are only some of them.
    anyhow::ensure!(
        std::env::var("XDG_SESSION_TYPE").as_deref() != Ok("wayland"),
        "the screen is not locked: this is a Wayland session, and the lock is for X sessions"
    );
    anyhow::ensure!(
        options.service.file().is_some(),
        "the screen is not locked: without the PAM service {0} no password \
         could unlock it; /etc/pam.d/{0} comes with the package",
        pam::SERVICE
    );
    let user = crate::users::current()
        .context("the screen is not locked: this runs as no known account")?
        .name;
    // A lock started from a terminal outlives the terminal.
    // SAFETY: setting a signal's disposition, before any thread starts.
    unsafe { libc::signal(libc::SIGHUP, libc::SIG_IGN) };
    let sleep_lock = take_sleep_lock();
    let cover = Cover::new().context("the screen is not locked")?;
    if cover.held_elsewhere()? {
        drop(sleep_lock);
        log::info!("the screen is locked already; this lock ends with that one");
        while cover.held_elsewhere()? {
            std::thread::sleep(Duration::from_millis(500));
        }
        return Ok(());
    }
    cover.show()?;
    cover
        .grab(GRAB_PATIENCE)
        .context("the screen is not locked")?;
    cover.claim()?;
    let code = match Lock::new(cover, options, user, sleep_lock)?.run() {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("gpui-greeter: {error:#}");
            1
        }
    };
    // Out at once: a check replaced while PAM ran it may still run on
    // another thread, holding locks that exit's cleanup would wait for.
    // SAFETY: nothing is left to flush, and the view has been reaped.
    unsafe { libc::_exit(code) }
}

/// xss-lock's delay lock on suspend, which lets the suspend go ahead once
/// it is closed. The view must not keep it open.
fn take_sleep_lock() -> Option<OwnedFd> {
    let fd: RawFd = std::env::var(SLEEP_LOCK_FD).ok()?.parse().ok()?;
    // SAFETY: fcntl on a bare number, which fails unless it is open.
    if fd <= 2 || unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) } != 0 {
        return None;
    }
    // SAFETY: an open descriptor xss-lock left for this process to close.
    Some(unsafe { OwnedFd::from_raw_fd(fd) })
}

struct Lock {
    cover: Cover,
    options: Options,
    user: String,
    sleep_lock: Option<OwnedFd>,
    locked_at: Instant,
    post: Post,
    inbox: mpsc::Receiver<Mail>,
    /// Rung by `post`, to wake the loop from its poll.
    bell: UnixStream,
    view: Option<View>,
    /// How many views were started, which numbers each.
    views: u64,
    restart_at: Option<Instant>,
    restart_wait: Duration,
    /// Since when no view has been shown.
    viewless_since: Option<Instant>,
    /// Whether the cover shows where the password goes.
    field: bool,
    /// What the cover took, while no view is shown.
    typed: Typed,
    /// How many characters the cover sent to PAM.
    sent: usize,
    check: Option<Check>,
    /// How many checks were started, which numbers each.
    checks: u64,
    /// Where the pointer last moved, not yet told to the view.
    moved: Option<(i32, i32)>,
    restack: bool,
}

struct View {
    number: u64,
    child: Child,
    link: link::Sender,
    shown: Option<Shown>,
    started: Instant,
    /// When it last said anything.
    heard: Instant,
    /// The monitors it was started over.
    monitors: Vec<Monitor>,
}

struct Shown {
    windows: Vec<xproto::Window>,
    keys: xproto::Window,
}

/// PAM's conversation under way. Another may replace it while it runs on,
/// whose questions then go unanswered and whose refusal goes unshown, but
/// whose acceptance still unlocks.
struct Check {
    number: u64,
    answers: mpsc::Sender<Option<Zeroizing<String>>>,
    /// The view that sent the password, or none if the cover took it.
    view: Option<u64>,
    /// PAM waits for the answer to a question.
    asking: bool,
    /// Its question was taken back, or its view has gone.
    cancelled: bool,
}

enum Mail {
    View(u64, ToLock),
    ViewGone(u64),
    Question(u64, pam::Question),
    Verdict(u64, pam::Verdict),
}

/// Mail for the main loop, from the threads that read the view and talk
/// with PAM.
#[derive(Clone)]
struct Post {
    mail: mpsc::Sender<Mail>,
    bell: Arc<UnixStream>,
}

impl Post {
    fn send(&self, mail: Mail) {
        if self.mail.send(mail).is_ok() {
            // When the bell's buffer is full, it has rung already.
            let _ = (&*self.bell).write(&[0]);
        }
    }
}

impl Lock {
    fn new(
        cover: Cover,
        options: Options,
        user: String,
        sleep_lock: Option<OwnedFd>,
    ) -> Result<Self> {
        let (ring, bell) = UnixStream::pair()?;
        ring.set_nonblocking(true)?;
        bell.set_nonblocking(true)?;
        let (mail, inbox) = mpsc::channel();
        Ok(Self {
            cover,
            options,
            user,
            sleep_lock,
            locked_at: Instant::now(),
            post: Post {
                mail,
                bell: Arc::new(ring),
            },
            inbox,
            bell,
            view: None,
            views: 0,
            restart_at: None,
            restart_wait: RESTART,
            viewless_since: Some(Instant::now()),
            field: false,
            typed: Typed::new(),
            sent: 0,
            check: None,
            checks: 0,
            moved: None,
            restack: false,
        })
    }

    /// Holds the screen until the password is accepted. Should the X
    /// server go, the session has gone with it, and so does the lock.
    fn run(mut self) -> Result<()> {
        self.start_view();
        loop {
            if let Some(event) = self.cover.poll().context("the X server is gone")? {
                self.on_event(event);
                continue;
            }
            self.settle();
            self.wait();
            while let Ok(mail) = self.inbox.try_recv() {
                if self.on_mail(mail).is_break() {
                    self.unlock();
                    return Ok(());
                }
            }
            self.on_time();
        }
    }

    /// Sends what the events since the last wait asked for.
    fn settle(&mut self) {
        if std::mem::take(&mut self.restack) {
            let windows = self
                .view
                .as_ref()
                .and_then(|view| view.shown.as_ref())
                .map_or(&[][..], |shown| &shown.windows);
            self.cover.raise(windows);
        }
        self.tell_moved();
        if let Err(error) = self.cover.flush() {
            log::warn!("cannot write to the X server: {error:#}");
        }
    }

    /// Waits for the X server, for mail, or for the next thing due.
    fn wait(&self) {
        let due = self
            .next_due()
            .map(|at| at.saturating_duration_since(Instant::now()))
            .and_then(|wait| Timespec::try_from(wait).ok());
        let x_fd = self.cover.fd();
        let mut fds = [
            PollFd::new(&x_fd, PollFlags::IN),
            PollFd::new(&self.bell, PollFlags::IN),
        ];
        if let Err(error) = rustix::event::poll(&mut fds, due.as_ref())
            && error != rustix::io::Errno::INTR
        {
            log::warn!("cannot wait for events: {error}");
            std::thread::sleep(Duration::from_millis(50));
        }
        let mut rung = [0; 64];
        while matches!((&self.bell).read(&mut rung), Ok(read) if read > 0) {}
    }

    fn next_due(&self) -> Option<Instant> {
        let view = self.view.as_ref().map(|view| match view.shown {
            None => view.started + SHOW_WAIT,
            Some(_) => view.heard + SILENCE,
        });
        let sleep_lock = self
            .sleep_lock
            .as_ref()
            .map(|_| self.locked_at + SLEEP_LOCK_WAIT);
        let field = self
            .viewless_since
            .filter(|_| !self.field)
            .map(|since| since + FIELD_AFTER);
        [self.restart_at, view, sleep_lock, field]
            .into_iter()
            .flatten()
            .min()
    }

    fn on_event(&mut self, event: Event) {
        match event {
            Event::KeyPress(key) | Event::KeyRelease(key) => self.key(&key),
            Event::ButtonPress(button) => {
                self.tell_moved();
                self.tell_view(&ToView::Pressed {
                    x: button.root_x.into(),
                    y: button.root_y.into(),
                    button: button.detail,
                });
            }
            Event::ButtonRelease(button) => {
                self.tell_moved();
                self.tell_view(&ToView::Released {
                    x: button.root_x.into(),
                    y: button.root_y.into(),
                    button: button.detail,
                });
            }
            Event::MotionNotify(motion) => {
                self.moved = Some((motion.root_x.into(), motion.root_y.into()));
            }
            Event::MapNotify(map) => self.restack |= !self.is_ours(map.window),
            Event::ConfigureNotify(configure) if configure.window == self.cover.root() => {
                self.monitors_changed();
            }
            Event::ConfigureNotify(configure) => {
                self.restack |= !self.is_ours(configure.window);
            }
            Event::CirculateNotify(circulate) => {
                self.restack |= !self.is_ours(circulate.window);
            }
            Event::Expose(expose) if expose.count == 0 => self.draw(),
            Event::MappingNotify(_) => self.cover.reload_keymap(),
            Event::RandrScreenChangeNotify(_) | Event::RandrNotify(_) => self.monitors_changed(),
            Event::Error(error) => log::debug!("the X server refused a request: {error:?}"),
            _ => {}
        }
    }

    fn is_ours(&self, window: xproto::Window) -> bool {
        window == self.cover.window()
            || self
                .view
                .as_ref()
                .and_then(|view| view.shown.as_ref())
                .is_some_and(|shown| shown.windows.contains(&window))
    }

    /// A key, which goes to the view while it is shown, and to the cover's
    /// own field while it is not.
    fn key(&mut self, key: &xproto::KeyPressEvent) {
        if let Some(shown) = self.view.as_ref().and_then(|view| view.shown.as_ref()) {
            self.cover.forward(key, shown.keys);
        } else if key.response_type == xproto::KEY_PRESS_EVENT {
            self.take_key(key.detail, key.state);
        }
    }

    fn take_key(&mut self, code: u8, state: KeyButMask) {
        if self.check.as_ref().is_some_and(|check| !check.cancelled) {
            return;
        }
        match self.cover.keysym(code, state) {
            cover::RETURN | cover::KP_ENTER => {
                if !self.typed.is_empty() {
                    self.sent = self.typed.len();
                    let password = self.typed.take();
                    self.start_check(password, None);
                }
            }
            cover::BACKSPACE => self.typed.pop(),
            cover::ESCAPE => self.typed = Typed::new(),
            sym => {
                let held = KeyButMask::CONTROL | KeyButMask::MOD1 | KeyButMask::MOD4;
                if let Some(c) = cover::char_of(sym)
                    && u16::from(state & held) == 0
                {
                    self.typed.push(c.encode_utf8(&mut [0; 4]));
                }
            }
        }
        self.field = true;
        self.draw();
    }

    fn draw(&self) {
        match &self.check {
            Some(check) if check.view.is_none() => self.cover.draw(true, self.sent, true),
            _ => self.cover.draw(self.field, self.typed.len(), false),
        }
    }

    /// Starts PAM's conversation on a thread of its own, with `password`
    /// for its first question; `view` sent it, or the cover took it.
    fn start_check(&mut self, password: Zeroizing<String>, view: Option<u64>) {
        self.checks += 1;
        let number = self.checks;
        let (answers, questions) = mpsc::channel();
        let (service, user, post) = (
            self.options.service.clone(),
            self.user.clone(),
            self.post.clone(),
        );
        let started = std::thread::Builder::new()
            .name("pam".into())
            .spawn(move || {
                let mut ask = |question: pam::Question| {
                    post.send(Mail::Question(number, question));
                    questions.recv().ok().flatten()
                };
                let verdict = pam::authenticate(&service, &user, password, &mut ask);
                post.send(Mail::Verdict(number, verdict));
            });
        match started {
            Ok(_) => {
                self.check = Some(Check {
                    number,
                    answers,
                    view,
                    asking: false,
                    cancelled: false,
                });
            }
            Err(error) => {
                log::error!("cannot check the password: {error}");
                let message = error.to_string();
                self.tell(view, &ToView::Failed { message });
            }
        }
    }

    fn on_mail(&mut self, mail: Mail) -> ControlFlow<()> {
        let current = self.view.as_ref().map(|view| view.number);
        match mail {
            Mail::View(number, message) if current == Some(number) => self.heard(number, message),
            Mail::ViewGone(number) if current == Some(number) => self.view_ended(),
            Mail::View(..) | Mail::ViewGone(_) => {}
            Mail::Question(check, question) => self.asked(check, question),
            Mail::Verdict(check, verdict) => return self.judged(check, verdict),
        }
        ControlFlow::Continue(())
    }

    fn heard(&mut self, number: u64, message: ToLock) {
        if let Some(view) = &mut self.view {
            view.heard = Instant::now();
        }
        match message {
            ToLock::Shown { windows, keys } => {
                if let Some(view) = &mut self.view {
                    view.shown = Some(Shown { windows, keys });
                }
                log::info!("the view is shown");
                self.viewless_since = None;
                self.typed = Typed::new();
                self.field = false;
                self.draw();
                self.restack = true;
                // The screen shows the lock: a suspend may go ahead.
                self.sleep_lock = None;
            }
            ToLock::Answer { text } => {
                let text = Zeroizing::new(text);
                match &mut self.check {
                    Some(check) if check.view == Some(number) && check.asking => {
                        check.asking = false;
                        let _ = check.answers.send(Some(text));
                    }
                    Some(check) if check.view == Some(number) && !check.cancelled => {
                        log::warn!("an answer came while PAM was busy, and was dropped");
                    }
                    _ => self.start_check(text, Some(number)),
                }
            }
            ToLock::Cancel => {
                if let Some(check) = &mut self.check {
                    check.cancelled = true;
                    if std::mem::take(&mut check.asking) {
                        let _ = check.answers.send(None);
                    }
                }
            }
            ToLock::Alive => {}
        }
    }

    /// PAM asks something more. Only the view that sent the password can
    /// show the question; otherwise it goes unanswered, which refuses. A
    /// check replaced already hears nothing more either way.
    fn asked(&mut self, number: u64, question: pam::Question) {
        let current = self.view.as_ref().map(|view| view.number);
        let Some(check) = self.check.as_mut().filter(|check| check.number == number) else {
            return;
        };
        if check.cancelled || check.view.is_none() || check.view != current {
            let _ = check.answers.send(None);
            return;
        }
        check.asking = true;
        let to = check.view;
        self.tell(
            to,
            &ToView::Asked {
                prompt: question.prompt,
                secret: question.secret,
                notice: question.notice,
            },
        );
    }

    fn judged(&mut self, number: u64, verdict: pam::Verdict) -> ControlFlow<()> {
        let said = match verdict {
            pam::Verdict::Accepted => return ControlFlow::Break(()),
            pam::Verdict::Denied { notice } => {
                log::info!("PAM refused the password");
                ToView::Denied { notice }
            }
            pam::Verdict::Failed(message) => {
                log::warn!("PAM could not check the password: {message}");
                ToView::Failed { message }
            }
        };
        let Some(check) = self.check.take_if(|check| check.number == number) else {
            return ControlFlow::Continue(());
        };
        if !check.cancelled {
            self.tell(check.view, &said);
        }
        self.draw();
        ControlFlow::Continue(())
    }

    /// Tells the view numbered `to`, if it is still the one running.
    fn tell(&mut self, to: Option<u64>, message: &ToView) {
        if to.is_some() && self.view.as_ref().map(|view| view.number) == to {
            self.tell_view(message);
        }
    }

    /// Tells the view; one that does not take it in time is stuck.
    fn tell_view(&mut self, message: &ToView) {
        let Some(view) = &self.view else {
            return;
        };
        if let Err(error) = view.link.send(message) {
            log::warn!("the view does not listen ({error}); it starts again");
            self.end_view();
            self.restart_soon();
        }
    }

    fn tell_moved(&mut self) {
        if let Some((x, y)) = self.moved.take() {
            self.tell_view(&ToView::Moved { x, y });
        }
    }

    fn start_view(&mut self) {
        self.views += 1;
        match View::start(
            &self.options,
            self.views,
            self.post.clone(),
            self.cover.monitors(),
        ) {
            Ok(view) => self.view = Some(view),
            Err(error) => {
                log::error!("cannot start the view: {error:#}");
                self.restart_soon();
            }
        }
    }

    /// Starts the view again after a wait that grows while it keeps
    /// failing.
    fn restart_soon(&mut self) {
        self.restart_at = Some(Instant::now() + self.restart_wait);
        self.restart_wait = (self.restart_wait * 2).min(RESTART_MOST);
    }

    /// Ends the view, if it is still running; the cover holds the screen
    /// alone until another is shown.
    fn end_view(&mut self) {
        let Some(mut view) = self.view.take() else {
            return;
        };
        let _ = view.child.kill();
        match view.child.wait() {
            Ok(status) => log::warn!("the view ended ({status}); the cover holds the screen"),
            Err(error) => log::warn!("the view ended ({error}); the cover holds the screen"),
        }
        if let Some(check) = &mut self.check
            && check.view == Some(view.number)
        {
            check.cancelled = true;
            if std::mem::take(&mut check.asking) {
                let _ = check.answers.send(None);
            }
        }
        if view.started.elapsed() >= STEADY {
            self.restart_wait = RESTART;
        }
        self.viewless_since.get_or_insert_with(Instant::now);
        self.draw();
    }

    fn view_ended(&mut self) {
        self.end_view();
        self.restart_soon();
    }

    fn on_time(&mut self) {
        let now = Instant::now();
        if self.restart_at.is_some_and(|at| at <= now) {
            self.restart_at = None;
            self.start_view();
        }
        let stuck = self.view.as_ref().is_some_and(|view| match view.shown {
            None => now >= view.started + SHOW_WAIT,
            Some(_) => now >= view.heard + SILENCE,
        });
        if stuck {
            log::warn!("the view stopped answering; it starts again");
            self.end_view();
            self.restart_soon();
        }
        if self.sleep_lock.is_some() && now >= self.locked_at + SLEEP_LOCK_WAIT {
            self.sleep_lock = None;
        }
        if !self.field
            && self
                .viewless_since
                .is_some_and(|since| now >= since + FIELD_AFTER)
        {
            self.field = true;
            self.draw();
        }
    }

    /// The screen changed size or the monitors changed: the cover covers it
    /// again at once, and a view laid out for other monitors starts again
    /// over these.
    fn monitors_changed(&mut self) {
        if let Err(error) = self.cover.fit() {
            log::warn!("cannot fit the cover to the screen: {error:#}");
        }
        self.restack = true;
        self.draw();
        let monitors = self.cover.monitors();
        if self
            .view
            .as_ref()
            .is_some_and(|view| view.monitors != monitors)
        {
            log::info!("the monitors changed; the view starts again over them");
            self.end_view();
            self.restart_at = Some(Instant::now());
        }
    }

    /// Gives the screen back at once: the cover and the grabs go, then the
    /// view, which lies over the session until it has gone.
    fn unlock(mut self) {
        log::info!("unlocked");
        self.cover.close();
        if let Some(mut view) = self.view.take() {
            let _ = view.child.kill();
            let _ = view.child.wait();
        }
    }
}

impl View {
    fn start(options: &Options, number: u64, post: Post, monitors: Vec<Monitor>) -> Result<Self> {
        let (ours, theirs) = UnixStream::pair()?;
        ours.set_write_timeout(Some(WRITE_WAIT))?;
        let fd = theirs.as_raw_fd();
        let mut command = Command::new("/proc/self/exe");
        command
            .arg0("gpui-greeter")
            .args(["lock", "--view", &fd.to_string(), "--config"])
            .arg(&options.config)
            .stdin(Stdio::null())
            // An input method would take the password's keys for its own.
            .env_remove("XMODIFIERS")
            .env_remove(SLEEP_LOCK_FD);
        if let Some(theme) = &options.theme {
            command.args(["--theme", theme.name()]);
        }
        if let Some(language) = options.language {
            command.args(["--language", language.name()]);
        }
        if options.dry_run {
            command.arg("--dry-run");
        }
        // SAFETY: prctl and fcntl are safe to call between fork and exec.
        unsafe {
            command.pre_exec(move || {
                set_parent_process_death_signal(Some(Signal::KILL))?;
                rustix::io::fcntl_setfd(BorrowedFd::borrow_raw(fd), rustix::io::FdFlags::empty())?;
                Ok(())
            });
        }
        let mut child = command.spawn().context("cannot run the view")?;
        drop(theirs);
        let reading = ours.try_clone().and_then(|mut reader| {
            std::thread::Builder::new()
                .name("view".into())
                .spawn(move || {
                    while let Ok(message) = wire::read::<ToLock>(&mut reader) {
                        post.send(Mail::View(number, message));
                    }
                    post.send(Mail::ViewGone(number));
                })
        });
        if let Err(error) = reading {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error.into());
        }
        let now = Instant::now();
        Ok(Self {
            number,
            child,
            link: link::Sender::new(ours),
            shown: None,
            started: now,
            heard: now,
            monitors,
        })
    }
}

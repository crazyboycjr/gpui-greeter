//! `gpui-greeter lock --when-idle SECONDS`: a small process for the session
//! that locks the screen once the X server's screen saver has kept it blank
//! that long, and at once when logind asks the session to lock, as
//! `loginctl lock-session` does. It draws nothing and wakes only when the
//! screen saver or logind has something to say, or when the lock it waits
//! for is due; the lock it starts keeps the screen blank until a key or the
//! pointer lights it up.

use std::io::{Read as _, Write as _};
use std::os::fd::AsFd as _;
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt as _;
use std::process::{Child, Command};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use anyhow::{Context as _, Result};
use rustix::event::{PollFd, PollFlags, Timespec};
use x11rb::connection::{Connection as _, RequestConnection as _};
use x11rb::protocol::Event;
use x11rb::protocol::screensaver::{self, ConnectionExt as _};

use super::cover;
use crate::x11::X11;

/// How often a lock this started is looked in on, to know it ended.
const LOOK_IN: Duration = Duration::from_secs(30);

/// What logind asks of the session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Asked {
    Lock,
    Unlock,
}

/// Runs until the X server goes, locking with `gpui-greeter lock` and
/// `lock_args` once the screen has been blank for `delay`.
pub fn run(delay: Duration, lock_args: Vec<String>) -> Result<()> {
    let x11 = X11::connect()?;
    let conn = x11.conn();
    anyhow::ensure!(
        conn.extension_information(screensaver::X11_EXTENSION_NAME)?
            .is_some(),
        "the X server has no screen saver extension to tell when the screen blanks"
    );
    conn.screensaver_query_version(1, 1)?.reply()?;
    conn.screensaver_select_input(x11.root(), screensaver::Event::NOTIFY_MASK)?;
    conn.flush()?;

    // The ringing end stays open here too: should logind's thread end, the
    // bell must not read as closed, which would wake the loop forever.
    let (ring, bell) = UnixStream::pair()?;
    ring.set_nonblocking(true)?;
    bell.set_nonblocking(true)?;
    let ringer = ring.try_clone()?;
    let (asks, asked) = mpsc::channel();
    std::thread::Builder::new()
        .name("logind".into())
        .spawn(move || {
            let tell = |ask| {
                if asks.send(ask).is_ok() {
                    let _ = (&ringer).write(&[0]);
                }
            };
            if let Err(error) = follow_logind(&tell) {
                log::info!("logind's lock-session does not reach this: {error}");
            }
        })?;
    log::info!(
        "locking once the screen has been blank for {} s",
        delay.as_secs()
    );

    let mut idle = Idle {
        x11: &x11,
        lock_args,
        lock: None,
        blank_since: x11.screen_saver_on().then(Instant::now),
    };
    loop {
        // Events read while waiting for a reply wait in the connection, not
        // on its socket: each turn takes them first.
        if let Some(event) = conn.poll_for_event().context("the X server is gone")? {
            if let Event::ScreensaverNotify(saver) = event {
                let blank = matches!(
                    saver.state,
                    screensaver::State::ON | screensaver::State::CYCLE
                );
                idle.blank_since = blank.then(Instant::now);
            }
            continue;
        }
        if let Ok(ask) = asked.try_recv() {
            match ask {
                Asked::Lock => idle.lock(),
                Asked::Unlock => idle.unlock(),
            }
            continue;
        }
        if idle
            .blank_since
            .is_some_and(|since| since.elapsed() >= delay)
        {
            idle.blank_since = None;
            idle.lock();
            continue;
        }
        idle.look_in();
        let due = idle
            .blank_since
            .map(|since| (since + delay).saturating_duration_since(Instant::now()));
        let wait = match (due, idle.lock.is_some()) {
            (Some(due), true) => Some(due.min(LOOK_IN)),
            (Some(due), false) => Some(due),
            (None, true) => Some(LOOK_IN),
            (None, false) => None,
        };
        let wait = wait.and_then(|wait| Timespec::try_from(wait).ok());
        let x_fd = conn.stream().as_fd();
        let mut fds = [
            PollFd::new(&x_fd, PollFlags::IN),
            PollFd::new(&bell, PollFlags::IN),
        ];
        if let Err(error) = rustix::event::poll(&mut fds, wait.as_ref())
            && error != rustix::io::Errno::INTR
        {
            return Err(error).context("cannot wait for the X server");
        }
        let mut rung = [0; 64];
        while matches!((&bell).read(&mut rung), Ok(read) if read > 0) {}
    }
}

struct Idle<'a> {
    x11: &'a X11,
    lock_args: Vec<String>,
    /// The lock this started, while it runs.
    lock: Option<Child>,
    /// Since when the screen saver has had the screen blank.
    blank_since: Option<Instant>,
}

impl Idle<'_> {
    /// Locks the screen, unless a lock holds it already: this one's or
    /// another's.
    fn lock(&mut self) {
        self.look_in();
        if self.lock.is_some() || cover::locked(self.x11).unwrap_or(false) {
            return;
        }
        let mut command = Command::new("/proc/self/exe");
        command
            .arg0("gpui-greeter")
            .arg("lock")
            .args(&self.lock_args);
        match command.spawn() {
            Ok(child) => {
                log::info!("locking the screen");
                self.lock = Some(child);
            }
            Err(error) => log::error!("cannot start the lock: {error}"),
        }
    }

    /// logind's unlock-session ends the lock this started, as xss-lock's
    /// does.
    fn unlock(&mut self) {
        if let Some(mut lock) = self.lock.take() {
            log::info!("logind unlocked the session");
            let _ = lock.kill();
            let _ = lock.wait();
        }
    }

    /// Forgets the lock this started once it has ended.
    fn look_in(&mut self) {
        if self
            .lock
            .as_mut()
            .is_some_and(|lock| !matches!(lock.try_wait(), Ok(None)))
        {
            self.lock = None;
        }
    }
}

/// Tells of the Lock and Unlock signals logind sends the session this runs
/// in, until the system bus goes.
fn follow_logind(tell: &dyn Fn(Asked)) -> zbus::Result<()> {
    const LOGIND: &str = "org.freedesktop.login1";
    let bus = zbus::blocking::Connection::system()?;
    let manager = zbus::blocking::Proxy::new(
        &bus,
        LOGIND,
        "/org/freedesktop/login1",
        "org.freedesktop.login1.Manager",
    )?;
    let path: zbus::zvariant::OwnedObjectPath = manager
        .call("GetSessionByPID", &(std::process::id()))
        .or_else(|error| match std::env::var("XDG_SESSION_ID") {
            Ok(id) => manager.call("GetSession", &(id.as_str())),
            Err(_) => Err(error),
        })?;
    let session = zbus::blocking::Proxy::new(&bus, LOGIND, path, "org.freedesktop.login1.Session")?;
    log::info!("following logind's lock-session for {}", session.path());
    for signal in session.receive_all_signals()? {
        match signal.header().member().map(|member| member.as_str()) {
            Some("Lock") => tell(Asked::Lock),
            Some("Unlock") => tell(Asked::Unlock),
            _ => {}
        }
    }
    Ok(())
}

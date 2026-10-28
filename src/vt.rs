//! Whether the greeter's virtual terminal is the one on screen. While another
//! is, the greeter's X server cannot show anything, and on NVIDIA every
//! frame drawn loses the GPU device, so the greeter stops drawing until its
//! VT is back. logind's `Active` property of the greeter's own session says
//! so the moment it changes; the kernel's active console stands in when
//! logind cannot.

use std::fs::File;
use std::path::Path;
use std::time::Duration;

use rustix::event::{PollFd, PollFlags, Timespec};

const ACTIVE_CONSOLE: &str = "/sys/class/tty/tty0/active";

/// Reads the active console from this file instead of the kernel's, with
/// `XDG_VTNR` as the greeter's own, so that a test can play its VT being
/// switched away and back.
pub const CONSOLE_OVERRIDE: &str = "GPUI_GREETER_ACTIVE_CONSOLE";

/// How long the console is trusted to say it changed, before it is read
/// again all the same.
const RECHECK: Duration = Duration::from_secs(1);

/// Calls `on_change` from a thread of its own with whether the greeter's VT
/// is on screen: at once, and again whenever that changes.
pub fn watch(on_change: impl Fn(bool) + Send + 'static) {
    let spawned = std::thread::Builder::new()
        .name("vt".into())
        .spawn(move || {
            if let Some(file) = std::env::var_os(CONSOLE_OVERRIDE) {
                return follow_console(Path::new(&file), &on_change);
            }
            match follow_logind(&on_change) {
                Ok(()) => log::warn!("logind stopped saying whether the greeter's VT is shown"),
                Err(error) => {
                    log::info!("logind cannot say whether the greeter's VT is shown: {error}")
                }
            }
            follow_console(Path::new(ACTIVE_CONSOLE), &on_change);
        });
    if let Err(error) = spawned {
        log::warn!("cannot follow the greeter's VT: {error}");
    }
}

/// Follows the `Active` property of the logind session the greeter runs in.
fn follow_logind(on_change: &dyn Fn(bool)) -> zbus::Result<()> {
    const LOGIND: &str = "org.freedesktop.login1";
    let bus = zbus::blocking::Connection::system()?;
    let manager = zbus::blocking::Proxy::new(
        &bus,
        LOGIND,
        "/org/freedesktop/login1",
        "org.freedesktop.login1.Manager",
    )?;
    let path: zbus::zvariant::OwnedObjectPath =
        manager.call("GetSessionByPID", &(std::process::id()))?;
    let session = zbus::blocking::Proxy::new(&bus, LOGIND, path, "org.freedesktop.login1.Session")?;
    let changes = session.receive_property_changed::<bool>("Active");
    on_change(session.get_property("Active")?);
    for change in changes {
        if let Ok(active) = change.get() {
            on_change(active);
        }
    }
    Ok(())
}

/// Compares the kernel's active console with the greeter's own VT, as
/// `XDG_VTNR` names it, whenever the kernel says it changed; without a VT
/// of its own the greeter is always shown.
fn follow_console(path: &Path, on_change: &dyn Fn(bool)) {
    let Some(ours) = std::env::var("XDG_VTNR")
        .ok()
        .filter(|vt| !vt.trim().is_empty())
    else {
        return;
    };
    let mut last = None;
    loop {
        // The kernel wakes a poll of the attribute when the console changes,
        // if the same open file read it before.
        let mut file = File::open(path).ok();
        let shown = file
            .as_mut()
            .and_then(|file| std::io::read_to_string(file).ok())
            .and_then(|active| is_ours(&active, &ours));
        if shown.is_some() && shown != last {
            on_change(shown == Some(true));
            last = shown;
        }
        let timeout = Timespec::try_from(RECHECK).ok();
        let waited = file.as_ref().zip(timeout).is_some_and(|(file, timeout)| {
            let mut fds = [PollFd::new(file, PollFlags::PRI | PollFlags::ERR)];
            rustix::event::poll(&mut fds, Some(&timeout)).is_ok()
        });
        if !waited {
            std::thread::sleep(RECHECK);
        }
    }
}

/// Whether the console the kernel names, such as "tty1", is VT `ours`;
/// nothing when it names none.
fn is_ours(active: &str, ours: &str) -> Option<bool> {
    let number = active.trim().strip_prefix("tty")?;
    Some(number == ours.trim())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_the_active_console() {
        assert_eq!(is_ours("tty1\n", "1"), Some(true));
        assert_eq!(is_ours("tty1", " 1\n"), Some(true));
        assert_eq!(is_ours("tty7\n", "1"), Some(false));
        assert_eq!(is_ours("tty11\n", "1"), Some(false));
        assert_eq!(is_ours("", "1"), None);
    }
}

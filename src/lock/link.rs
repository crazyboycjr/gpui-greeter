//! What the lock's supervisor and its view tell each other, over a socket
//! pair framed as greetd's messages are.

use std::io;
use std::os::fd::{FromRawFd as _, RawFd};
use std::os::unix::net::UnixStream;
use std::sync::{Arc, Mutex, PoisonError};

use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, Zeroizing};

use crate::greetd::wire;

/// From the view to the supervisor.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToLock {
    /// The view's windows are on the screen; keys go to `keys`.
    Shown { windows: Vec<u32>, keys: u32 },
    /// What was typed: the password, or the answer to PAM's last question.
    Answer { text: String },
    /// PAM's last question is not to be answered.
    Cancel,
    /// The view still runs, as it says every few seconds.
    Alive,
}

/// From the supervisor to the view. Points are the screen's pixels.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToView {
    Moved {
        x: i32,
        y: i32,
    },
    Pressed {
        x: i32,
        y: i32,
        button: u8,
    },
    Released {
        x: i32,
        y: i32,
        button: u8,
    },
    /// PAM asks something more, such as a one-time code.
    Asked {
        prompt: String,
        secret: bool,
        notice: Option<String>,
    },
    Denied {
        notice: Option<String>,
    },
    Failed {
        message: String,
    },
}

/// The writing end of the link, shared by whoever sends.
#[derive(Clone)]
pub struct Sender(Arc<Mutex<UnixStream>>);

impl Sender {
    pub fn new(stream: UnixStream) -> Self {
        Self(Arc::new(Mutex::new(stream)))
    }

    pub fn send<T: Serialize>(&self, message: &T) -> io::Result<()> {
        let mut stream = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        wire::write(&mut *stream, message)
    }

    /// Sends what was typed, leaving no copy of it behind.
    pub fn answer(&self, mut text: Zeroizing<String>) -> io::Result<()> {
        let mut message = ToLock::Answer {
            text: std::mem::take(&mut *text),
        };
        let sent = self.send(&message);
        if let ToLock::Answer { text } = &mut message {
            text.zeroize();
        }
        sent
    }
}

/// The view's end of the link, which the supervisor leaves open for it.
pub struct ViewEnd(UnixStream);

impl ViewEnd {
    /// Takes the socket open as `fd`.
    pub fn adopt(fd: RawFd) -> anyhow::Result<Self> {
        // SAFETY: a stat of plain memory, filled in by fstat.
        let mut stat: libc::stat = unsafe { std::mem::zeroed() };
        // SAFETY: fstat only reads the descriptor, and fails if it is shut.
        let open = unsafe { libc::fstat(fd, &mut stat) } == 0;
        anyhow::ensure!(
            open && stat.st_mode & libc::S_IFMT == libc::S_IFSOCK,
            "descriptor {fd} is not the lock's socket"
        );
        // SAFETY: an open socket no one else here holds.
        Ok(Self(unsafe { UnixStream::from_raw_fd(fd) }))
    }

    /// Reads what the supervisor says, on a thread of its own, into the
    /// channel returned. When the supervisor goes, the screen is no longer
    /// locked, and the view goes too.
    pub fn listen(self) -> io::Result<(Sender, async_channel::Receiver<ToView>)> {
        let mut reader = self.0.try_clone()?;
        let (heard, said) = async_channel::unbounded();
        std::thread::Builder::new()
            .name("lock".into())
            .spawn(move || {
                while let Ok(message) = wire::read::<ToView>(&mut reader) {
                    if heard.send_blocking(message).is_err() {
                        break;
                    }
                }
                std::process::exit(0);
            })?;
        Ok((Sender::new(self.0), said))
    }
}

//! `gpui-greeter preview`: a theme folder in one window, against a fake
//! greetd in the same process, at a state of the login of the author's
//! choosing. With a screenshot asked for, it draws in an X server of its
//! own (Xvfb) with lavapipe, Mesa's Vulkan on the CPU, so no display or GPU
//! is needed, and writes the screenshot once the screen has settled.

use std::os::unix::fs::PermissionsExt as _;
use std::os::unix::net::UnixListener;
use std::os::unix::process::CommandExt as _;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::rc::Rc;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use anyhow::{Context as _, Result, bail};
use gpui_kit::{AnyWindowHandle, App, AppContext as _, Entity, Keystroke, Modifiers};
use rustix::process::{Pid, Signal, kill_process, set_parent_process_death_signal};

use super::model::Greeter;
use super::screen::Screen;
use crate::greetd::fake;
use crate::themes::{Chord, State};
use crate::x11::{Rect, X11};

/// What `preview` was asked for.
#[derive(Debug, Clone)]
pub struct Preview {
    /// The state to show; the theme's first one when `None`.
    pub state: Option<State>,
    pub size: (u32, u32),
    /// Where to write the screenshot, after which the preview ends.
    pub shot: Option<PathBuf>,
    /// Take it this long after the state is shown, rather than once the
    /// screen settles.
    pub at: Option<Duration>,
    /// Keys pressed once the state shows, in turn.
    pub presses: Vec<Chord>,
    /// Where the pointer goes then, in the window's pixels.
    pub hover: Option<(i32, i32)>,
    pub user: Option<String>,
    pub host: Option<String>,
    /// How the screenshot went, for `main` to report.
    pub outcome: Arc<Mutex<Option<Result<()>>>>,
}

/// An X server of the preview's own, which ends with it.
pub struct Xvfb {
    child: Child,
    number: u32,
    pub display: String,
}

impl Drop for Xvfb {
    fn drop(&mut self) {
        // Asked to stop, Xvfb removes its lock file and socket; killed, it
        // leaves both behind.
        let _ = kill_process(Pid::from_child(&self.child), Signal::TERM);
        let asked = Instant::now();
        while matches!(self.child.try_wait(), Ok(None)) {
            if asked.elapsed() > Duration::from_secs(3) {
                let _ = self.child.kill();
                let _ = self.child.wait();
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let lock = PathBuf::from(format!("/tmp/.X{}-lock", self.number));
        let ours = std::fs::read_to_string(&lock)
            .is_ok_and(|text| text.trim() == self.child.id().to_string());
        if ours {
            let _ = std::fs::remove_file(lock);
            let _ = std::fs::remove_file(format!("/tmp/.X11-unix/X{}", self.number));
        }
    }
}

/// Starts Xvfb with a screen of `size` on a display nobody uses.
pub fn start_xvfb((width, height): (u32, u32)) -> Result<Xvfb> {
    let free = |n: u32| {
        !Path::new(&format!("/tmp/.X11-unix/X{n}")).exists()
            && !Path::new(&format!("/tmp/.X{n}-lock")).exists()
    };
    let number = (90..400)
        .find(|&n| free(n))
        .context("no free X display number")?;
    let display = format!(":{number}");
    let mut command = Command::new("Xvfb");
    command
        .args([
            display.as_str(),
            "-screen",
            "0",
            &format!("{width}x{height}x24"),
            "-nolisten",
            "tcp",
            "-noreset",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    // SAFETY: prctl is safe to call between fork and exec. Should the
    // preview die without ending it, Xvfb is asked to stop all the same.
    unsafe {
        command.pre_exec(|| Ok(set_parent_process_death_signal(Some(Signal::TERM))?));
    }
    let child = command
        .spawn()
        .context("cannot start Xvfb; it comes with xorg-server-xvfb")?;
    let xvfb = Xvfb {
        child,
        number,
        display,
    };
    let socket = PathBuf::from(format!("/tmp/.X11-unix/X{number}"));
    let started = Instant::now();
    while !socket.exists() {
        if started.elapsed() > Duration::from_secs(10) {
            bail!("Xvfb did not start on {}", xvfb.display);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Ok(xvfb)
}

/// Points Vulkan at lavapipe when the display cannot take the GPU's
/// frames, as Xvfb cannot (it has no DRI3), unless the driver is chosen
/// already. Must run before any thread starts.
pub fn prefer_lavapipe(headless: bool) {
    let chosen = ["VK_DRIVER_FILES", "VK_ICD_FILENAMES"]
        .iter()
        .any(|name| std::env::var_os(name).is_some());
    if chosen || (!headless && has_dri3()) {
        return;
    }
    let icd = std::fs::read_dir("/usr/share/vulkan/icd.d")
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .find(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("lvp_icd") && name.ends_with(".json"))
        });
    let Some(icd) = icd else {
        log::warn!("no lavapipe (vulkan-swrast) to draw with; trying the GPU's driver");
        return;
    };
    // SAFETY: no other thread runs yet.
    unsafe {
        std::env::set_var("VK_DRIVER_FILES", &icd);
        std::env::set_var("VK_ICD_FILENAMES", &icd);
        if std::env::var_os("LP_NUM_THREADS").is_none() {
            std::env::set_var("LP_NUM_THREADS", "4");
        }
        let mesa_egl = "/usr/share/glvnd/egl_vendor.d/50_mesa.json";
        if Path::new(mesa_egl).exists()
            && std::env::var_os("__EGL_VENDOR_LIBRARY_FILENAMES").is_none()
        {
            std::env::set_var("__EGL_VENDOR_LIBRARY_FILENAMES", mesa_egl);
        }
    }
    log::info!("drawing with lavapipe ({})", icd.display());
}

fn has_dri3() -> bool {
    use x11rb::connection::RequestConnection as _;
    let Ok((conn, _)) = x11rb::connect(None) else {
        return false;
    };
    conn.extension_information("DRI3").ok().flatten().is_some()
}

/// Starts the fake greetd, which takes the password "test", on a socket of
/// its own, and points GREETD_SOCK at it. Must run before any other thread
/// starts; returns the folder to remove afterwards.
pub fn start_fake_greetd() -> Result<PathBuf> {
    let dir = std::env::temp_dir().join(format!("gpui-greeter-preview-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir)?;
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))?;
    let socket = dir.join("greetd.sock");
    let listener = UnixListener::bind(&socket)
        .with_context(|| format!("cannot listen on {}", socket.display()))?;
    // SAFETY: no other thread runs yet.
    unsafe { std::env::set_var("GREETD_SOCK", &socket) };
    std::thread::spawn(move || fake::serve(listener, "test"));
    Ok(dir)
}

/// The state shown first, so that the one asked for is reached the way a
/// login reaches it, with its transitions.
fn lead_in(state: State, has_rest: bool) -> Option<State> {
    match state {
        State::Rest => None,
        State::Idle => has_rest.then_some(State::Rest),
        State::Typing => Some(State::Idle),
        State::Checking => Some(State::Typing),
        State::Error | State::Welcome => Some(State::Checking),
    }
}

/// Shows the state asked for, presses the keys and moves the pointer as
/// asked, and, if a screenshot is wanted, takes it once the window has
/// settled, then ends the preview.
#[allow(clippy::too_many_arguments)]
pub fn drive(
    cx: &mut App,
    preview: &Preview,
    greeter: Entity<Greeter>,
    screen: Entity<Screen>,
    window: AnyWindowHandle,
    x11: Rc<X11>,
    rect: Rect,
    waits_for_wallpaper: bool,
) {
    let has_rest = greeter
        .read(cx)
        .folder
        .as_ref()
        .is_some_and(|folder| folder.rest);
    let target = preview.state;
    let lead = target.and_then(|state| lead_in(state, has_rest));
    if let Some(first) = lead.or(target) {
        greeter.update(cx, |greeter, cx| greeter.show_state(first, cx));
    }
    let (shot, at, outcome) = (preview.shot.clone(), preview.at, preview.outcome.clone());
    let (presses, hover) = (preview.presses.clone(), preview.hover);
    cx.spawn(async move |cx| {
        if let (Some(_), Some(target)) = (lead, target) {
            cx.background_executor()
                .timer(Duration::from_millis(700))
                .await;
            greeter.update(cx, |greeter, cx| greeter.show_state(target, cx));
        }
        for chord in presses {
            cx.background_executor()
                .timer(Duration::from_millis(300))
                .await;
            let keystroke = Keystroke {
                modifiers: Modifiers {
                    control: chord.ctrl,
                    alt: chord.alt || chord.any_mod,
                    shift: chord.shift,
                    platform: chord.super_key,
                    function: false,
                },
                key: chord.key,
                key_char: None,
            };
            let _ = cx.update_window(window, |_, window, cx| {
                window.dispatch_keystroke(keystroke, cx);
            });
        }
        if let Some((x, y)) = hover
            && let Err(error) = x11.move_pointer((rect.x + x, rect.y + y))
        {
            log::warn!("cannot move the pointer: {error:#}");
        }
        let Some(shot) = shot else {
            return;
        };
        // What reaches the screen trails what is drawn by about this much.
        const LATENCY: Duration = Duration::from_millis(400);
        let shown = Instant::now();
        loop {
            cx.background_executor()
                .timer(Duration::from_millis(100))
                .await;
            let (settled, backdrop) = screen.read_with(cx, |screen, _| {
                (screen.settled_since(), screen.has_backdrop())
            });
            let waited = shown.elapsed();
            let ready = match at {
                Some(at) => waited + LATENCY >= at,
                None => {
                    settled.is_some_and(|since| since.elapsed() >= Duration::from_millis(700))
                        && (backdrop || !waits_for_wallpaper || waited > Duration::from_secs(5))
                }
            };
            if ready {
                break;
            }
            if waited > Duration::from_secs(30) {
                log::warn!("the screen did not settle in 30 s; taking the screenshot as it is");
                break;
            }
        }
        cx.background_executor().timer(LATENCY).await;
        let taken = x11.capture(rect).and_then(|image| {
            image
                .save(&shot)
                .with_context(|| format!("cannot write {}", shot.display()))
        });
        *outcome.lock().unwrap_or_else(PoisonError::into_inner) = Some(taken);
        cx.update(|cx| cx.quit());
    })
    .detach();
}

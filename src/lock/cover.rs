//! The supervisor's side of the X server: one plain window over the whole
//! screen, above every other, with the keyboard and the pointer held for
//! it. It shows black, and squares for the password when it takes the
//! password itself.

use std::os::fd::{AsFd as _, BorrowedFd};
use std::time::{Duration, Instant};

use anyhow::{Context as _, Result, bail};
use x11rb::connection::{Connection as _, RequestConnection as _};
use x11rb::protocol::Event;
use x11rb::protocol::randr::{self, ConnectionExt as _};
use x11rb::protocol::screensaver::{self, ConnectionExt as _};
use x11rb::protocol::xproto::{self, ConnectionExt as _, EventMask, KeyButMask};
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;

use crate::x11::{Monitor, Rect, X11};

/// The selection a cover owns while it locks the screen, so that a second
/// lock knows.
const MARKER: &[u8] = b"_GPUI_GREETER_LOCK";

const GRAB_RETRY: Duration = Duration::from_millis(20);

/// `_NET_WM_BYPASS_COMPOSITOR`'s ask that a compositor never unredirect
/// the window. Unredirected, a full-screen window flips by itself, and
/// NVIDIA gives such flips variable refresh, which can black out a monitor
/// such as an OLED; the view's windows ask the same.
const KEEP_COMPOSITING: u32 = 2;

pub const RETURN: u32 = 0xff0d;
pub const KP_ENTER: u32 = 0xff8d;
pub const BACKSPACE: u32 = 0xff08;
pub const ESCAPE: u32 = 0xff1b;

pub struct Cover {
    x11: X11,
    window: xproto::Window,
    gc: xproto::Gcontext,
    marker: xproto::Atom,
    size: (u16, u16),
    /// The monitor that shows the password the cover takes.
    field: Rect,
    keymap: Keymap,
}

impl Cover {
    /// Connects to `$DISPLAY`'s X server and makes the cover, not yet shown.
    pub fn new() -> Result<Self> {
        let x11 = X11::connect()?;
        let conn = x11.conn();
        let root = x11.root();
        let screen = conn
            .setup()
            .roots
            .iter()
            .find(|screen| screen.root == root)
            .context("the X server lists no screen for its root window")?;
        let size = (screen.width_in_pixels, screen.height_in_pixels);
        let (black, white) = (screen.black_pixel, screen.white_pixel);
        let window = conn.generate_id()?;
        conn.create_window(
            x11rb::COPY_DEPTH_FROM_PARENT,
            window,
            root,
            0,
            0,
            size.0,
            size.1,
            0,
            xproto::WindowClass::INPUT_OUTPUT,
            x11rb::COPY_FROM_PARENT,
            &xproto::CreateWindowAux::new()
                .background_pixel(black)
                .override_redirect(1)
                .event_mask(EventMask::EXPOSURE),
        )?;
        conn.change_property8(
            xproto::PropMode::REPLACE,
            window,
            xproto::AtomEnum::WM_NAME,
            xproto::AtomEnum::STRING,
            b"gpui-greeter lock",
        )?;
        conn.change_property8(
            xproto::PropMode::REPLACE,
            window,
            xproto::AtomEnum::WM_CLASS,
            xproto::AtomEnum::STRING,
            b"gpui-greeter-lock\0gpui-greeter-lock\0",
        )?;
        let bypass = conn
            .intern_atom(false, b"_NET_WM_BYPASS_COMPOSITOR")?
            .reply()?
            .atom;
        conn.change_property32(
            xproto::PropMode::REPLACE,
            window,
            bypass,
            xproto::AtomEnum::CARDINAL,
            &[KEEP_COMPOSITING],
        )?;
        let gc = conn.generate_id()?;
        conn.create_gc(
            gc,
            window,
            &xproto::CreateGCAux::new()
                .foreground(white)
                .background(black)
                .line_width(2),
        )?;
        let marker = conn.intern_atom(false, MARKER)?.reply()?.atom;
        let keymap = Keymap::load(conn)?;
        let field = field_monitor(&x11.monitors(), size);
        Ok(Self {
            x11,
            window,
            gc,
            marker,
            size,
            field,
            keymap,
        })
    }

    pub fn window(&self) -> xproto::Window {
        self.window
    }

    pub fn root(&self) -> xproto::Window {
        self.x11.root()
    }

    /// Whether another lock holds the screen already.
    pub fn held_elsewhere(&self) -> Result<bool> {
        let owner = self
            .x11
            .conn()
            .get_selection_owner(self.marker)?
            .reply()?
            .owner;
        Ok(owner != x11rb::NONE)
    }

    /// Puts the cover over every window there is.
    pub fn show(&self) -> Result<()> {
        let conn = self.x11.conn();
        conn.map_window(self.window)?;
        conn.configure_window(
            self.window,
            &xproto::ConfigureWindowAux::new().stack_mode(xproto::StackMode::ABOVE),
        )?;
        conn.flush()?;
        Ok(())
    }

    /// Takes the keyboard and then the pointer, trying again for a while
    /// as long as another program holds them: the window manager does
    /// while the key that started the lock is down, and so does an open
    /// menu.
    pub fn grab(&self, patience: Duration) -> Result<()> {
        let conn = self.x11.conn();
        let deadline = Instant::now() + patience;
        loop {
            let status = conn
                .grab_keyboard(
                    false,
                    self.window,
                    x11rb::CURRENT_TIME,
                    xproto::GrabMode::ASYNC,
                    xproto::GrabMode::ASYNC,
                )?
                .reply()?
                .status;
            if status == xproto::GrabStatus::SUCCESS {
                break;
            }
            if Instant::now() >= deadline {
                bail!("another program, such as an open menu, holds the keyboard ({status:?})");
            }
            std::thread::sleep(GRAB_RETRY);
        }
        loop {
            let status = conn
                .grab_pointer(
                    false,
                    self.window,
                    EventMask::BUTTON_PRESS | EventMask::BUTTON_RELEASE | EventMask::POINTER_MOTION,
                    xproto::GrabMode::ASYNC,
                    xproto::GrabMode::ASYNC,
                    x11rb::NONE,
                    x11rb::NONE,
                    x11rb::CURRENT_TIME,
                )?
                .reply()?
                .status;
            if status == xproto::GrabStatus::SUCCESS {
                return Ok(());
            }
            if Instant::now() >= deadline {
                conn.ungrab_keyboard(x11rb::CURRENT_TIME)?;
                conn.flush()?;
                bail!("another program, such as an open menu, holds the pointer ({status:?})");
            }
            std::thread::sleep(GRAB_RETRY);
        }
    }

    /// Marks the screen as locked for as long as the cover lasts, and asks
    /// to hear of windows put over it, of the monitors changing and of the
    /// screen saver blanking the screen: RandR tells of outputs and modes,
    /// and the root window's ConfigureNotify of monitors set with
    /// `xrandr --setmonitor` too.
    pub fn claim(&self) -> Result<()> {
        let conn = self.x11.conn();
        conn.set_selection_owner(self.window, self.marker, x11rb::CURRENT_TIME)?;
        conn.change_window_attributes(
            self.root(),
            &xproto::ChangeWindowAttributesAux::new()
                .event_mask(EventMask::SUBSTRUCTURE_NOTIFY | EventMask::STRUCTURE_NOTIFY),
        )?;
        if conn
            .extension_information(randr::X11_EXTENSION_NAME)?
            .is_some()
        {
            conn.randr_query_version(1, 5)?.reply()?;
            conn.randr_select_input(
                self.root(),
                randr::NotifyMask::SCREEN_CHANGE
                    | randr::NotifyMask::CRTC_CHANGE
                    | randr::NotifyMask::OUTPUT_CHANGE,
            )?;
        }
        if conn
            .extension_information(screensaver::X11_EXTENSION_NAME)?
            .is_some()
        {
            conn.screensaver_select_input(self.root(), screensaver::Event::NOTIFY_MASK)?;
        }
        conn.flush()?;
        Ok(())
    }

    /// Whether the X server's screen saver has the screen blank now.
    pub fn blank(&self) -> bool {
        self.x11.screen_saver_on()
    }

    /// Takes the view's windows off the screen, where they draw nothing, or
    /// puts them back. A window gone already only answers with an error.
    pub fn show_view(&self, view: &[xproto::Window], shown: bool) {
        let conn = self.x11.conn();
        for &window in view {
            let _ = if shown {
                conn.map_window(window)
            } else {
                conn.unmap_window(window)
            };
        }
    }

    /// Puts the cover back over everything, and the view's windows over
    /// it. A view window gone already only answers with an error, which
    /// the event loop passes over.
    pub fn raise(&self, view: &[xproto::Window]) {
        let conn = self.x11.conn();
        let above = xproto::ConfigureWindowAux::new().stack_mode(xproto::StackMode::ABOVE);
        for window in std::iter::once(self.window).chain(view.iter().copied()) {
            let _ = conn.configure_window(window, &above);
        }
    }

    /// Hands a key the cover holds to the view's window `to`, as though it
    /// had been pressed there: grabs do not stop an event sent to a window,
    /// and sent with no mask it goes to the client that made the window.
    pub fn forward(&self, key: &xproto::KeyPressEvent, to: xproto::Window) {
        let event = xproto::KeyPressEvent {
            event: to,
            child: x11rb::NONE,
            ..*key
        };
        let _ = self
            .x11
            .conn()
            .send_event(false, to, EventMask::NO_EVENT, event);
    }

    /// Covers the screen again after its size or its monitors changed.
    pub fn fit(&mut self) -> Result<()> {
        let conn = self.x11.conn();
        let geometry = conn.get_geometry(self.root())?.reply()?;
        let size = (geometry.width, geometry.height);
        if size != self.size {
            self.size = size;
            conn.configure_window(
                self.window,
                &xproto::ConfigureWindowAux::new()
                    .x(0)
                    .y(0)
                    .width(u32::from(size.0))
                    .height(u32::from(size.1)),
            )?;
        }
        self.field = field_monitor(&self.x11.monitors(), size);
        Ok(())
    }

    pub fn monitors(&self) -> Vec<Monitor> {
        self.x11.monitors()
    }

    /// Shows the password the cover takes itself: a line where it goes,
    /// and a square for each character, hollow while it is checked.
    pub fn draw(&self, field: bool, count: usize, checking: bool) {
        const SLOTS: usize = 24;
        let conn = self.x11.conn();
        let _ = conn.clear_area(false, self.window, 0, 0, 0, 0);
        if !field {
            return;
        }
        let Rect {
            x,
            y,
            width,
            height,
        } = self.field;
        let side = (height / 100).clamp(6, 40) as i32;
        let span = SLOTS as i32 * side * 2;
        let left = x + (width as i32 - span) / 2;
        let top = y + (height as i32 - side) / 2;
        let rect = |x: i32, y: i32, width: i32, height: i32| xproto::Rectangle {
            x: x as i16,
            y: y as i16,
            width: width as u16,
            height: height as u16,
        };
        let line = rect(left, top + side * 2, span, (side / 8).max(1));
        let _ = conn.poly_fill_rectangle(self.window, self.gc, &[line]);
        let squares: Vec<xproto::Rectangle> = (0..count.min(SLOTS) as i32)
            .map(|at| rect(left + side / 2 + at * side * 2, top, side, side))
            .collect();
        let _ = if checking {
            conn.poly_rectangle(self.window, self.gc, &squares)
        } else {
            conn.poly_fill_rectangle(self.window, self.gc, &squares)
        };
    }

    /// The keysym of a key the cover holds, as the core keyboard mapping
    /// has it.
    pub fn keysym(&self, code: u8, state: KeyButMask) -> u32 {
        self.keymap.keysym(code, state)
    }

    /// Reads the keyboard mapping again, after it changed.
    pub fn reload_keymap(&mut self) {
        match Keymap::load(self.x11.conn()) {
            Ok(keymap) => self.keymap = keymap,
            Err(error) => log::warn!("cannot read the keyboard mapping: {error:#}"),
        }
    }

    pub fn fd(&self) -> BorrowedFd<'_> {
        self.x11.conn().stream().as_fd()
    }

    pub fn poll(&self) -> Result<Option<Event>> {
        Ok(self.x11.conn().poll_for_event()?)
    }

    pub fn flush(&self) -> Result<()> {
        self.x11.conn().flush()?;
        Ok(())
    }

    /// Gives the keyboard and the pointer back and takes the cover away.
    pub fn close(&self) {
        let conn = self.x11.conn();
        let _ = conn.ungrab_keyboard(x11rb::CURRENT_TIME);
        let _ = conn.ungrab_pointer(x11rb::CURRENT_TIME);
        let _ = conn.destroy_window(self.window);
        let _ = conn.flush();
    }
}

/// Whether a lock holds the screen of `x11`'s display.
pub fn locked(x11: &X11) -> Result<bool> {
    let conn = x11.conn();
    let marker = conn.intern_atom(false, MARKER)?.reply()?.atom;
    Ok(conn.get_selection_owner(marker)?.reply()?.owner != x11rb::NONE)
}

/// The primary monitor, else the first, else the whole screen.
fn field_monitor(monitors: &[Monitor], (width, height): (u16, u16)) -> Rect {
    monitors
        .iter()
        .find(|monitor| monitor.primary)
        .or(monitors.first())
        .map_or(
            Rect {
                x: 0,
                y: 0,
                width: width.into(),
                height: height.into(),
            },
            |monitor| monitor.rect,
        )
}

/// The core keyboard mapping, for the keys the cover takes itself.
struct Keymap {
    first: u8,
    per: usize,
    syms: Vec<u32>,
}

impl Keymap {
    fn load(conn: &RustConnection) -> Result<Self> {
        let setup = conn.setup();
        let (first, last) = (setup.min_keycode, setup.max_keycode);
        let reply = conn
            .get_keyboard_mapping(first, last - first + 1)?
            .reply()?;
        Ok(Self {
            first,
            per: reply.keysyms_per_keycode.into(),
            syms: reply.keysyms,
        })
    }

    /// The keysym of the key in its first group, at the level Shift and
    /// Caps Lock choose.
    fn keysym(&self, code: u8, state: KeyButMask) -> u32 {
        let Some(at) = code.checked_sub(self.first) else {
            return 0;
        };
        let start = usize::from(at) * self.per;
        let row = self.syms.get(start..start + self.per).unwrap_or_default();
        let lower = row.first().copied().unwrap_or(0);
        let upper = row.get(1).copied().filter(|&sym| sym != 0).unwrap_or(lower);
        let caps =
            state.contains(KeyButMask::LOCK) && char_of(lower).is_some_and(char::is_lowercase);
        if state.contains(KeyButMask::SHIFT) != caps {
            upper
        } else {
            lower
        }
    }
}

/// The character a keysym types, if it types one.
pub fn char_of(sym: u32) -> Option<char> {
    match sym {
        0x20..=0x7e | 0xa0..=0xff => char::from_u32(sym),
        0x0100_00a0..=0x0110_ffff => char::from_u32(sym - 0x0100_0000),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_type_their_level_s_character() {
        let keymap = Keymap {
            first: 8,
            per: 2,
            // a A, 1 !, ä Ä
            syms: vec![0x61, 0x41, 0x31, 0x21, 0xe4, 0xc4],
        };
        let char_at = |code, state| char_of(keymap.keysym(code, state));
        assert_eq!(char_at(8, KeyButMask::default()), Some('a'));
        assert_eq!(char_at(8, KeyButMask::SHIFT), Some('A'));
        assert_eq!(char_at(8, KeyButMask::LOCK), Some('A'));
        assert_eq!(char_at(8, KeyButMask::LOCK | KeyButMask::SHIFT), Some('a'));
        assert_eq!(char_at(9, KeyButMask::LOCK), Some('1'));
        assert_eq!(char_at(9, KeyButMask::SHIFT), Some('!'));
        assert_eq!(char_at(10, KeyButMask::LOCK), Some('Ä'));
        assert_eq!(char_of(0x0100_4e2d), Some('中'));
        assert_eq!(char_of(RETURN), None);
    }
}

//! What GPUI does not do on X11 for a greeter: list the monitors with their
//! pixel density, place a window exactly over one or take it off the
//! screen, give it the keyboard, and read and switch the keyboard layout.

use anyhow::{Context as _, Result};
use x11rb::connection::Connection;
use x11rb::protocol::dpms::ConnectionExt as _;
use x11rb::protocol::randr::{self, ConnectionExt as _};
use x11rb::protocol::xkb::{self, ConnectionExt as _};
use x11rb::protocol::xproto::{self, ConnectionExt as _};
use x11rb::rust_connection::RustConnection;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl Rect {
    fn contains(&self, other: &Rect) -> bool {
        other.x >= self.x
            && other.y >= self.y
            && other.x + other.width as i32 <= self.x + self.width as i32
            && other.y + other.height as i32 <= self.y + self.height as i32
    }

    pub fn contains_point(&self, (x, y): (i32, i32)) -> bool {
        x >= self.x
            && y >= self.y
            && x < self.x + self.width as i32
            && y < self.y + self.height as i32
    }

    fn area(&self) -> u64 {
        u64::from(self.width) * u64::from(self.height)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Monitor {
    pub rect: Rect,
    /// Its physical size as it reports it; zero when it does not.
    pub size_mm: (u32, u32),
    pub primary: bool,
    /// The refresh rate of the mode it runs, in Hz.
    pub refresh: Option<f32>,
}

/// The points to an inch a Mac lays a 27-inch display out at: a Studio
/// Display at twice its pixels, or a 4K one that looks like 2560 by 1440.
const POINTS_PER_INCH: f64 = 109.0;

impl Monitor {
    /// The scale its pixel density asks for, 1 at 96 dpi, in twelfths as
    /// GPUI rounds it. A monitor that reports no believable size gets one
    /// from its height in pixels instead.
    pub fn scale(&self) -> f32 {
        match self.dpi() {
            Some(dpi) => ((dpi / 96.0 * 12.0).round() / 12.0).max(1.0) as f32,
            None => (f64::from(self.rect.height) / 1080.0).round().max(1.0) as f32,
        }
    }

    /// How many of its pixels make a point, the looks' unit of length: as
    /// many as a Mac gives a point on a display this dense, rounded to
    /// twelfths against the slack in a reported size, and never fewer than
    /// one. A monitor that reports no believable size is laid out as 2560
    /// by 1440 points fitted to its pixels.
    pub fn pixels_per_point(&self) -> f32 {
        let per_point = match self.dpi() {
            Some(dpi) => (dpi / POINTS_PER_INCH * 12.0).round() / 12.0,
            None => (f64::from(self.rect.width) / 2560.0).min(f64::from(self.rect.height) / 1440.0),
        };
        per_point.max(1.0) as f32
    }

    /// Its pixel density, from the size it reports, if that is believable.
    fn dpi(&self) -> Option<f64> {
        let (width_mm, height_mm) = self.size_mm;
        let (width, height) = (f64::from(self.rect.width), f64::from(self.rect.height));
        let dpi = 25.4 * (width * height / (f64::from(width_mm) * f64::from(height_mm))).sqrt();
        (width_mm >= 100 && height_mm >= 60 && (72.0..=400.0).contains(&dpi)).then_some(dpi)
    }
}

/// Of overlapping monitors, as mirrored ones are, the larger stands for
/// both: a window on each would cover the other's.
fn distinct(mut monitors: Vec<Monitor>) -> Vec<Monitor> {
    monitors.sort_by_key(|monitor| std::cmp::Reverse(monitor.rect.area()));
    let mut kept: Vec<Monitor> = Vec::new();
    for monitor in monitors {
        if !kept.iter().any(|big| big.rect.contains(&monitor.rect)) {
            kept.push(monitor);
        }
    }
    kept.sort_by_key(|monitor| (monitor.rect.x, monitor.rect.y));
    kept
}

pub struct X11 {
    conn: RustConnection,
    root: xproto::Window,
}

impl X11 {
    pub fn connect() -> Result<Self> {
        let (conn, screen) = x11rb::connect(None).context("cannot connect to the X server")?;
        let root = conn.setup().roots[screen].root;
        Ok(Self { conn, root })
    }

    pub fn conn(&self) -> &RustConnection {
        &self.conn
    }

    pub fn root(&self) -> xproto::Window {
        self.root
    }

    /// Every monitor that shows something, or the whole screen when RandR
    /// cannot say.
    pub fn monitors(&self) -> Vec<Monitor> {
        match self.randr_monitors() {
            Ok(monitors) if !monitors.is_empty() => distinct(monitors),
            outcome => {
                if let Err(error) = outcome {
                    log::warn!("RandR cannot list the monitors: {error:#}");
                }
                let screen = &self.conn.setup().roots[0];
                vec![Monitor {
                    rect: Rect {
                        x: 0,
                        y: 0,
                        width: screen.width_in_pixels.into(),
                        height: screen.height_in_pixels.into(),
                    },
                    size_mm: (
                        screen.width_in_millimeters.into(),
                        screen.height_in_millimeters.into(),
                    ),
                    primary: true,
                    refresh: None,
                }]
            }
        }
    }

    fn randr_monitors(&self) -> Result<Vec<Monitor>> {
        self.conn.randr_query_version(1, 5)?.reply()?;
        let resources = self
            .conn
            .randr_get_screen_resources_current(self.root)?
            .reply()?;
        let monitors = self
            .conn
            .randr_get_monitors(self.root, true)?
            .reply()?
            .monitors;
        Ok(monitors
            .into_iter()
            .map(|monitor| {
                let output = monitor.outputs.first().and_then(|&output| {
                    self.conn
                        .randr_get_output_info(output, x11rb::CURRENT_TIME)
                        .ok()?
                        .reply()
                        .ok()
                });
                // A monitor that reports no size, as one that has dropped
                // off DisplayPort does, gets one X makes up from the
                // screen's resolution; its output tells. A monitor defined
                // by hand keeps the size it was given.
                let size_mm = match &output {
                    Some(info) if monitor.automatic => (info.mm_width, info.mm_height),
                    _ => (monitor.width_in_millimeters, monitor.height_in_millimeters),
                };
                Monitor {
                    rect: Rect {
                        x: monitor.x.into(),
                        y: monitor.y.into(),
                        width: monitor.width.into(),
                        height: monitor.height.into(),
                    },
                    size_mm,
                    primary: monitor.primary,
                    refresh: output.and_then(|info| self.refresh(info.crtc, &resources)),
                }
            })
            .collect())
    }

    fn refresh(
        &self,
        crtc: randr::Crtc,
        resources: &randr::GetScreenResourcesCurrentReply,
    ) -> Option<f32> {
        let mode = self
            .conn
            .randr_get_crtc_info(crtc, x11rb::CURRENT_TIME)
            .ok()?
            .reply()
            .ok()?
            .mode;
        let info = resources.modes.iter().find(|info| info.id == mode)?;
        let mut lines = f64::from(info.vtotal);
        if info.mode_flags.contains(randr::ModeFlag::DOUBLE_SCAN) {
            lines *= 2.0;
        }
        if info.mode_flags.contains(randr::ModeFlag::INTERLACE) {
            lines /= 2.0;
        }
        let rate = f64::from(info.dot_clock) / (f64::from(info.htotal) * lines);
        rate.is_finite()
            .then_some(rate as f32)
            .filter(|rate| *rate > 0.0)
    }

    /// Lets the monitors blank when idle but never power down, as the
    /// user's session keeps them: a DisplayPort monitor that sleeps can come
    /// back black on NVIDIA's X server.
    pub fn keep_monitors_awake(&self) {
        let disabled = self
            .conn
            .dpms_capable()
            .ok()
            .and_then(|cookie| cookie.reply().ok())
            .filter(|reply| reply.capable)
            .map(|_| self.conn.dpms_disable().map(|cookie| cookie.check()));
        if let Some(Ok(Err(error))) = disabled {
            log::warn!("cannot turn DPMS off: {error}");
        }
    }

    pub fn pointer(&self) -> Option<(i32, i32)> {
        let reply = self.conn.query_pointer(self.root).ok()?.reply().ok()?;
        Some((reply.root_x.into(), reply.root_y.into()))
    }

    /// Moves the pointer to `(x, y)` on the screen, as the mouse would.
    pub fn move_pointer(&self, (x, y): (i32, i32)) -> Result<()> {
        let clamp = |value: i32| value.clamp(i16::MIN.into(), i16::MAX.into()) as i16;
        self.conn
            .warp_pointer(x11rb::NONE, self.root, 0, 0, 0, 0, clamp(x), clamp(y))?;
        self.conn.flush()?;
        Ok(())
    }

    /// Moves `window` exactly over `rect`, which GPUI misses by a couple of
    /// pixels on X11.
    pub fn place(&self, window: u32, rect: Rect) -> Result<()> {
        let aux = xproto::ConfigureWindowAux::new()
            .x(rect.x)
            .y(rect.y)
            .width(rect.width)
            .height(rect.height)
            .stack_mode(xproto::StackMode::ABOVE);
        self.conn.configure_window(window, &aux)?;
        self.conn.flush()?;
        Ok(())
    }

    /// Takes `window` off the screen; GPUI draws no frames for it then.
    pub fn hide(&self, window: u32) -> Result<()> {
        self.conn.unmap_window(window)?;
        self.conn.flush()?;
        Ok(())
    }

    /// Puts `window` back on the screen, exactly over `rect`.
    pub fn show(&self, window: u32, rect: Rect) -> Result<()> {
        self.place(window, rect)?;
        self.conn.map_window(window)?;
        self.conn.flush()?;
        Ok(())
    }

    /// Gives `window` the keyboard, once it is on screen; false until then.
    pub fn focus(&self, window: u32) -> bool {
        let viewable = self
            .conn
            .get_window_attributes(window)
            .ok()
            .and_then(|cookie| cookie.reply().ok())
            .is_some_and(|attributes| attributes.map_state == xproto::MapState::VIEWABLE);
        viewable
            && self
                .conn
                .set_input_focus(
                    xproto::InputFocus::POINTER_ROOT,
                    window,
                    x11rb::CURRENT_TIME,
                )
                .ok()
                .and_then(|cookie| cookie.check().ok())
                .is_some()
    }

    /// The screen's pixels in `rect`, for a preview's screenshot.
    pub fn capture(&self, rect: Rect) -> Result<image::RgbImage> {
        let reply = self
            .conn
            .get_image(
                xproto::ImageFormat::Z_PIXMAP,
                self.root,
                i16::try_from(rect.x)?,
                i16::try_from(rect.y)?,
                u16::try_from(rect.width)?,
                u16::try_from(rect.height)?,
                !0,
            )?
            .reply()
            .context("the X server would not give the screen's pixels")?;
        let pixels = rect.width as usize * rect.height as usize;
        anyhow::ensure!(
            reply.depth >= 24 && reply.data.len() >= pixels * 4,
            "the X server sent {} bytes of depth {} for {}×{}",
            reply.data.len(),
            reply.depth,
            rect.width,
            rect.height
        );
        let lsb = self.conn.setup().image_byte_order == xproto::ImageOrder::LSB_FIRST;
        let mut image = image::RgbImage::new(rect.width, rect.height);
        for (out, &[a, b, c, d]) in image.pixels_mut().zip(reply.data.as_chunks::<4>().0) {
            out.0 = if lsb { [c, b, a] } else { [b, c, d] };
        }
        Ok(image)
    }

    /// The configured keyboard layouts, such as `["us", "cn"]`, and the one
    /// in use.
    pub fn layouts(&self) -> (Vec<String>, usize) {
        let names = self.rules_names().unwrap_or_default();
        let layouts: Vec<String> = names
            .get(2)
            .map(|layouts| {
                layouts
                    .split(',')
                    .filter(|l| !l.is_empty())
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        let current = self
            .group()
            .unwrap_or(0)
            .min(layouts.len().saturating_sub(1));
        (layouts, current)
    }

    /// Switches to the layout at `index` in the configured list.
    pub fn lock_layout(&self, index: usize) -> Result<()> {
        self.conn.xkb_use_extension(1, 0)?.reply()?;
        self.conn
            .xkb_latch_lock_state(
                xkb::ID::USE_CORE_KBD.into(),
                xproto::ModMask::default(),
                xproto::ModMask::default(),
                true,
                xkb::Group::from(index as u8),
                xproto::ModMask::default(),
                false,
                0,
            )?
            .check()?;
        Ok(())
    }

    fn group(&self) -> Option<usize> {
        self.conn.xkb_use_extension(1, 0).ok()?.reply().ok()?;
        let state = self
            .conn
            .xkb_get_state(xkb::ID::USE_CORE_KBD.into())
            .ok()?
            .reply()
            .ok()?;
        Some(u8::from(state.group).into())
    }

    /// `_XKB_RULES_NAMES`: rules, model, layouts, variants and options.
    fn rules_names(&self) -> Option<Vec<String>> {
        let atom = self
            .conn
            .intern_atom(true, b"_XKB_RULES_NAMES")
            .ok()?
            .reply()
            .ok()?
            .atom;
        let property = self
            .conn
            .get_property(false, self.root, atom, xproto::AtomEnum::STRING, 0, 1024)
            .ok()?
            .reply()
            .ok()?;
        Some(
            property
                .value
                .split(|&byte| byte == 0)
                .map(|field| String::from_utf8_lossy(field).into_owned())
                .collect(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn monitor(x: i32, width: u32, height: u32, size_mm: (u32, u32)) -> Monitor {
        Monitor {
            rect: Rect {
                x,
                y: 0,
                width,
                height,
            },
            size_mm,
            primary: false,
            refresh: None,
        }
    }

    #[test]
    fn scales_by_pixel_density() {
        assert_eq!(monitor(0, 3840, 2160, (597, 336)).scale(), 20.0 / 12.0);
        assert_eq!(monitor(0, 1920, 1080, (527, 296)).scale(), 1.0);
        assert_eq!(monitor(0, 2560, 1440, (597, 336)).scale(), 14.0 / 12.0);
    }

    #[test]
    fn guesses_when_the_size_is_missing_or_a_placeholder() {
        assert_eq!(monitor(0, 3840, 2160, (0, 0)).scale(), 2.0);
        assert_eq!(monitor(0, 3840, 2160, (160, 90)).scale(), 2.0);
        assert_eq!(monitor(0, 1920, 1080, (16, 9)).scale(), 1.0);
    }

    #[test]
    fn gives_a_point_the_pixels_a_mac_would() {
        // A 27-inch 4K monitor looks like 2560 by 1440, as on a Mac.
        assert_eq!(monitor(0, 3840, 2160, (590, 330)).pixels_per_point(), 1.5);
        assert_eq!(monitor(0, 3840, 2160, (597, 336)).pixels_per_point(), 1.5);
        assert_eq!(monitor(0, 5120, 2880, (597, 336)).pixels_per_point(), 2.0);
        assert_eq!(monitor(0, 2560, 1440, (597, 336)).pixels_per_point(), 1.0);
        assert_eq!(monitor(0, 3840, 2160, (697, 392)).pixels_per_point(), 1.25);
        // Never fewer than one, as a Mac never draws below its own size.
        assert_eq!(monitor(0, 1920, 1080, (527, 296)).pixels_per_point(), 1.0);
    }

    #[test]
    fn takes_a_point_from_the_pixels_when_the_size_is_missing() {
        assert_eq!(monitor(0, 3840, 2160, (0, 0)).pixels_per_point(), 1.5);
        assert_eq!(monitor(0, 3840, 2160, (160, 90)).pixels_per_point(), 1.5);
        assert_eq!(monitor(0, 2880, 1800, (0, 0)).pixels_per_point(), 1.125);
        assert_eq!(monitor(0, 1920, 1080, (16, 9)).pixels_per_point(), 1.0);
    }

    #[test]
    fn a_mirror_shares_its_monitor_window() {
        let big = monitor(0, 3840, 2160, (597, 336));
        let mirror = monitor(0, 1920, 1080, (527, 296));
        let beside = monitor(3840, 1920, 1080, (527, 296));
        let kept = distinct(vec![mirror, beside.clone(), big.clone()]);
        assert_eq!(kept, [big, beside]);
    }
}

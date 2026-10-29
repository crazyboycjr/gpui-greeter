//! gpui-greeter-bing.timer, which fetches Bing's picture of the day every
//! day as the greeter's user, so that the pictures gather on days without a
//! login screen too. It is on while the wallpaper is Bing's: the settings
//! window asks systemd whether it is, and the root helper turns it on or
//! off, which is all the helper may do with systemd.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context as _, Result, ensure};

pub const TIMER: &str = "gpui-greeter-bing.timer";
const SERVICE: &str = "gpui-greeter-bing.service";
/// Where the package installs its units.
const UNITS: &str = "/usr/lib/systemd/system";
const SYSTEMCTL: &str = "/usr/bin/systemctl";

/// Whose units: the system's, or, for a debug build's tests, those under a
/// folder standing in for `/`, which systemctl changes offline, starting
/// nothing.
#[derive(Debug, Clone, Copy)]
pub enum Units<'a> {
    System,
    Root(&'a Path),
}

impl Units<'_> {
    fn systemctl(&self) -> Command {
        let mut command = Command::new(SYSTEMCTL);
        if let Units::Root(root) = self {
            command.arg(format!("--root={}", root.display()));
        }
        command
    }

    fn timer_file(&self) -> PathBuf {
        let units = Path::new(UNITS);
        match self {
            Units::System => units.join(TIMER),
            Units::Root(root) => root
                .join(units.strip_prefix("/").unwrap_or(units))
                .join(TIMER),
        }
    }

    /// Whether the package's timer is there to turn on.
    pub fn installed(&self) -> bool {
        self.timer_file().is_file()
    }

    /// Whether the timer is on, as systemd says, if it is there.
    pub fn enabled(&self) -> Option<bool> {
        if !self.installed() {
            return None;
        }
        let output = self.systemctl().args(["is-enabled", TIMER]).output().ok()?;
        match String::from_utf8_lossy(&output.stdout).trim() {
            "enabled" => Some(true),
            "disabled" => Some(false),
            _ => None,
        }
    }

    /// Turns the timer on, from now and at every boot, and fetches today's
    /// picture at once; or turns it off. Says what it did.
    pub fn set(&self, on: bool) -> Result<String> {
        let steps: &[&[&str]] = match (self, on) {
            (Units::System, true) => &[
                &["enable", "--now", TIMER],
                &["start", "--no-block", SERVICE],
            ],
            (Units::System, false) => &[&["disable", "--now", TIMER]],
            (Units::Root(_), true) => &[&["enable", TIMER]],
            (Units::Root(_), false) => &[&["disable", TIMER]],
        };
        for step in steps {
            let output = self
                .systemctl()
                .args(*step)
                .output()
                .with_context(|| format!("cannot run {SYSTEMCTL}"))?;
            ensure!(
                output.status.success(),
                "systemctl {} failed: {}",
                step.join(" "),
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        let done = if on { "enabled" } else { "disabled" };
        Ok(format!("{done:<9}{TIMER}"))
    }
}

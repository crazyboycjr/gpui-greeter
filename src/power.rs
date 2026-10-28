//! Sleep, restart and shut down, asked of logind over the system bus.

use anyhow::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Suspend,
    Reboot,
    PowerOff,
}

impl Action {
    fn method(self) -> &'static str {
        match self {
            Action::Suspend => "Suspend",
            Action::Reboot => "Reboot",
            Action::PowerOff => "PowerOff",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Power {
    /// Only say what would be done: for trying the greeter out.
    pub dry_run: bool,
}

impl Power {
    pub fn run(self, action: Action) -> Result<()> {
        if self.dry_run {
            log::info!("dry run: logind would {}", action.method());
            return Ok(());
        }
        let bus = zbus::blocking::Connection::system()?;
        bus.call_method(
            Some("org.freedesktop.login1"),
            "/org/freedesktop/login1",
            Some("org.freedesktop.login1.Manager"),
            action.method(),
            // Not interactive: no polkit agent runs in a greeter.
            &(false,),
        )?;
        Ok(())
    }
}

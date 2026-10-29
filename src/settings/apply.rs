//! Applying what the settings app changed: the avatar through
//! AccountsService, as the user, and the rest in one run of the root
//! helper, `gpui-greeter apply-settings`, through pkexec. Without polkit,
//! the request is saved and the sudo command that applies it is given
//! instead.

use std::io::Write as _;
use std::os::unix::fs::{OpenOptionsExt as _, PermissionsExt as _};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context as _, Result};

use super::avatar::Accounts;
use super::request::Request;

/// How the helper runs as root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Elevate {
    /// pkexec, which asks polkit, whose agent asks for a password.
    Pkexec,
    /// The helper as the user, on a test folder; debug builds only.
    Direct { root: PathBuf },
    /// As if polkit could not ask, for the sudo command; debug builds only.
    Unavailable { root: PathBuf },
}

/// How applying went.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Done, a line for each change made.
    Done(Vec<String>),
    /// The password was not given.
    Cancelled,
    /// polkit could not ask; the command applies the saved request.
    Sudo(String),
    Failed(String),
}

/// What there is to apply.
pub struct Changes {
    pub request: Request,
    /// The user and the PNG to make their avatar.
    pub avatar: Option<(String, PathBuf)>,
}

/// Applies `changes`; `saved` is where a request is kept for sudo.
pub fn apply(
    changes: &Changes,
    accounts: &dyn Accounts,
    elevate: &Elevate,
    saved: &Path,
) -> Outcome {
    if let Some((user, png)) = &changes.avatar
        && let Err(error) = accounts.set_icon(user, png)
    {
        return Outcome::Failed(format!("{error:#}"));
    }
    if changes.request.is_empty() {
        return Outcome::Done(Vec::new());
    }
    let mut bytes = Vec::new();
    if let Err(error) = changes.request.write(&mut bytes) {
        return Outcome::Failed(format!("{error:#}"));
    }
    let exe = match std::env::current_exe() {
        Ok(exe) => installed(exe),
        Err(error) => return Outcome::Failed(format!("{error}")),
    };
    let mut command = match elevate {
        Elevate::Pkexec => {
            let mut command = Command::new("pkexec");
            command.arg(&exe).arg("apply-settings");
            command
        }
        Elevate::Direct { root } => {
            let mut command = Command::new(&exe);
            command.arg("apply-settings").arg("--root").arg(root);
            command
        }
        Elevate::Unavailable { root } => {
            return sudo(&exe, Some(root), &bytes, saved);
        }
    };
    let ran = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    let mut child = match ran {
        Ok(child) => child,
        Err(error)
            if error.kind() == std::io::ErrorKind::NotFound && *elevate == Elevate::Pkexec =>
        {
            return sudo(&exe, None, &bytes, saved);
        }
        Err(error) => {
            let program = command.get_program().to_string_lossy().into_owned();
            return Outcome::Failed(format!("cannot start {program}: {error}"));
        }
    };
    // The helper reads all of it before saying anything; pkexec may read
    // none of it, if the password is not given.
    let mut stdin = child.stdin.take().expect("stdin is piped");
    let writing = bytes.clone();
    let writer = std::thread::spawn(move || {
        let _ = stdin.write_all(&writing);
    });
    let output = match child.wait_with_output() {
        Ok(output) => output,
        Err(error) => return Outcome::Failed(format!("{error}")),
    };
    let _ = writer.join();
    let said = |bytes: &[u8]| String::from_utf8_lossy(bytes).trim().to_string();
    match (elevate, output.status.code()) {
        (_, Some(0)) => Outcome::Done(said(&output.stdout).lines().map(str::to_string).collect()),
        // pkexec's word for a dialog dismissed.
        (Elevate::Pkexec, Some(126)) => Outcome::Cancelled,
        // pkexec's word for no authorisation, which is also what it says
        // when no agent is there to ask for one.
        (Elevate::Pkexec, Some(127)) => {
            log::warn!("pkexec: {}", said(&output.stderr));
            sudo(&exe, None, &bytes, saved)
        }
        _ => {
            let error = said(&output.stderr);
            let error = error.strip_prefix("gpui-greeter: ").unwrap_or(&error);
            Outcome::Failed(error.to_string())
        }
    }
}

/// The program's path as installed: after an upgrade, the kernel names the
/// running one, replaced since, "<path> (deleted)", and the path holds the
/// new one, whose helper reads the same requests.
fn installed(exe: PathBuf) -> PathBuf {
    match exe
        .to_str()
        .and_then(|path| path.strip_suffix(" (deleted)"))
    {
        Some(path) => PathBuf::from(path),
        None => exe,
    }
}

/// Saves the request for sudo, and says how to apply it.
fn sudo(exe: &Path, root: Option<&Path>, request: &[u8], saved: &Path) -> Outcome {
    match save(request, saved) {
        Ok(()) => Outcome::Sudo(sudo_command(exe, root, saved)),
        Err(error) => Outcome::Failed(format!("{error:#}")),
    }
}

fn save(request: &[u8], path: &Path) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let _ = std::fs::remove_file(path);
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .with_context(|| format!("cannot save {}", path.display()))?;
    file.write_all(request)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    Ok(())
}

/// The command that applies the request saved at `saved`.
pub fn sudo_command(exe: &Path, root: Option<&Path>, saved: &Path) -> String {
    let mut words = vec![
        "sudo".to_string(),
        quoted(&exe.display().to_string()),
        "apply-settings".into(),
    ];
    if let Some(root) = root {
        words.push("--root".into());
        words.push(quoted(&root.display().to_string()));
    }
    words.push("<".into());
    words.push(quoted(&saved.display().to_string()));
    words.join(" ")
}

/// `text` as one word for a POSIX shell.
fn quoted(text: &str) -> String {
    let plain = !text.is_empty()
        && text
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"/._-+=:,@".contains(&b));
    if plain {
        text.to_string()
    } else {
        format!("'{}'", text.replace('\'', "'\\''"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Theme;
    use crate::settings::avatar::tests::Recorder;
    use crate::settings::setting::Setting;

    #[test]
    fn runs_the_program_installed_now() {
        assert_eq!(
            installed("/usr/bin/gpui-greeter (deleted)".into()),
            PathBuf::from("/usr/bin/gpui-greeter")
        );
        assert_eq!(
            installed("/usr/bin/gpui-greeter".into()),
            PathBuf::from("/usr/bin/gpui-greeter")
        );
    }

    #[test]
    fn quotes_paths_for_the_shell() {
        assert_eq!(quoted("/usr/bin/gpui-greeter"), "/usr/bin/gpui-greeter");
        assert_eq!(quoted("/home/alice/my files"), "'/home/alice/my files'");
        assert_eq!(quoted("it's"), "'it'\\''s'");
        assert_eq!(
            sudo_command(
                Path::new("/usr/bin/gpui-greeter"),
                None,
                Path::new("/home/alice/.cache/gpui-greeter/settings/request")
            ),
            "sudo /usr/bin/gpui-greeter apply-settings < /home/alice/.cache/gpui-greeter/settings/request"
        );
    }

    #[test]
    fn sets_the_avatar_as_the_user_and_saves_the_rest_for_sudo() {
        let dir = std::env::temp_dir().join(format!("gpui-greeter-apply-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let saved = dir.join("settings/request");
        let changes = Changes {
            request: Request {
                settings: vec![Setting::Theme(Theme::Neon)],
                ..Request::default()
            },
            avatar: Some(("alice".into(), dir.join("avatar.png"))),
        };
        let accounts = Recorder::default();
        let outcome = apply(
            &changes,
            &accounts,
            &Elevate::Unavailable { root: dir.clone() },
            &saved,
        );
        let Outcome::Sudo(command) = outcome else {
            panic!("{outcome:?}");
        };
        assert!(command.starts_with("sudo "), "{command}");
        assert!(
            command.ends_with(&format!("< {}", saved.display())),
            "{command}"
        );
        assert_eq!(
            accounts.0.lock().unwrap().as_slice(),
            [("alice".to_string(), dir.join("avatar.png"))]
        );
        let request = Request::read(&mut std::io::Cursor::new(std::fs::read(&saved).unwrap()));
        assert_eq!(request.unwrap().settings, [Setting::Theme(Theme::Neon)]);
        let mode = std::fs::metadata(&saved).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        let _ = std::fs::remove_dir_all(dir);
    }
}

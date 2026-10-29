//! The X sessions to choose from, read from their desktop entries, and the
//! command and environment greetd starts one with.

use std::os::unix::fs::PermissionsExt;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    /// The desktop entry's file name without `.desktop`, e.g. `xmonad`.
    pub id: String,
    pub name: String,
    pub exec: Vec<String>,
    pub desktop_names: Option<String>,
}

/// Every session in `dir` that may be shown and whose program is installed,
/// by name.
pub fn load(dir: &Path) -> Vec<Session> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        log::error!("cannot read the sessions in {}", dir.display());
        return Vec::new();
    };
    let mut sessions: Vec<Session> = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let id = path
                .file_name()?
                .to_str()?
                .strip_suffix(".desktop")?
                .to_string();
            let text = std::fs::read_to_string(&path).ok()?;
            parse(id, &text, installed)
        })
        .collect();
    sessions.sort_by_key(|session| session.name.to_lowercase());
    sessions
}

/// The session this runs in, among `sessions`, by the name its display
/// manager left in the environment.
pub fn current(sessions: Vec<Session>) -> Option<Session> {
    let names: Vec<String> = ["XDG_SESSION_DESKTOP", "DESKTOP_SESSION"]
        .into_iter()
        .filter_map(|variable| std::env::var(variable).ok())
        .collect();
    named(sessions, &names)
}

/// The first of `sessions` one of `names` names, as an id or as the path of
/// its desktop entry, as some display managers write it.
fn named(sessions: Vec<Session>, names: &[String]) -> Option<Session> {
    let ids: Vec<&str> = names
        .iter()
        .map(|name| name.rsplit('/').next().unwrap_or_default())
        .map(|name| name.strip_suffix(".desktop").unwrap_or(name))
        .collect();
    sessions
        .into_iter()
        .find(|session| ids.contains(&session.id.as_str()))
}

impl Session {
    /// The words of the command greetd runs, which it joins with spaces and
    /// hands to `sh -c`: the wrapper as written, then the session's own
    /// arguments, quoted.
    pub fn command(&self, wrapper: &str) -> Vec<String> {
        wrapper
            .split_whitespace()
            .map(str::to_string)
            .chain(self.exec.iter().map(|arg| shell_quote(arg)))
            .collect()
    }

    /// Set before the session opens, so logind records it as an X11 session
    /// of this desktop.
    pub fn environment(&self) -> Vec<String> {
        let mut env = vec![
            "XDG_SESSION_TYPE=x11".to_string(),
            format!("XDG_SESSION_DESKTOP={}", self.id),
            format!("DESKTOP_SESSION={}", self.id),
        ];
        if let Some(names) = &self.desktop_names {
            env.push(format!(
                "XDG_CURRENT_DESKTOP={}",
                names.trim_end_matches(';').replace(';', ":")
            ));
        }
        env
    }
}

fn parse(id: String, text: &str, is_installed: impl Fn(&str) -> bool) -> Option<Session> {
    let mut in_entry = false;
    let (mut name, mut exec, mut try_exec, mut desktop_names) = (None, None, None, None);
    let mut hidden = false;
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        let Some((key, value)) = line.split_once('=').filter(|_| in_entry) else {
            continue;
        };
        let value = unescape(value.trim());
        match key.trim() {
            "Name" => name = Some(value),
            "Exec" => exec = Some(value),
            "TryExec" => try_exec = Some(value),
            "DesktopNames" => desktop_names = Some(value),
            "Hidden" | "NoDisplay" => hidden |= value == "true",
            _ => {}
        }
    }
    if hidden || try_exec.is_some_and(|program| !is_installed(&program)) {
        return None;
    }
    let exec = split_exec(&exec?)?;
    Some(Session {
        name: name.unwrap_or_else(|| id.clone()),
        id,
        exec,
        desktop_names,
    })
}

/// The escapes a desktop entry's string values may hold.
fn unescape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('s') => out.push(' '),
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}

/// Splits an Exec value into arguments as the desktop entry specification
/// quotes them, dropping field codes such as `%f`, which a session has no
/// use for.
fn split_exec(exec: &str) -> Option<Vec<String>> {
    let mut args = Vec::new();
    let mut chars = exec.chars().peekable();
    loop {
        while chars.next_if_eq(&' ').is_some() {}
        if chars.peek().is_none() {
            break;
        }
        let mut arg = String::new();
        if chars.next_if_eq(&'"').is_some() {
            loop {
                match chars.next()? {
                    '"' => break,
                    '\\' => arg.push(chars.next()?),
                    c => arg.push(c),
                }
            }
            args.push(arg);
            continue;
        }
        while let Some(c) = chars.next_if(|c| *c != ' ') {
            match c {
                '%' if chars.next_if_eq(&'%').is_some() => arg.push('%'),
                '%' => {
                    chars.next();
                }
                c => arg.push(c),
            }
        }
        if !arg.is_empty() {
            args.push(arg);
        }
    }
    (!args.is_empty()).then_some(args)
}

fn shell_quote(word: &str) -> String {
    let plain = !word.is_empty()
        && word
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "_./=:@%+,-".contains(c));
    if plain {
        word.to_string()
    } else {
        format!("'{}'", word.replace('\'', r"'\''"))
    }
}

fn installed(program: &str) -> bool {
    let executable = |path: &Path| {
        path.metadata()
            .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
    };
    if program.contains('/') {
        return executable(Path::new(program));
    }
    std::env::var_os("PATH")
        .unwrap_or_else(|| "/usr/local/bin:/usr/bin:/bin".into())
        .to_str()
        .is_some_and(|path| {
            path.split(':')
                .any(|dir| executable(&Path::new(dir).join(program)))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(id: &str, text: &str) -> Option<Session> {
        parse(id.into(), text, |program| program != "/usr/bin/missing")
    }

    #[test]
    fn reads_the_installed_sessions() {
        let xmonad = session(
            "xmonad",
            "[Desktop Entry]\nEncoding=UTF-8\nName=XMonad\nExec=xinitrcsession-helper xmonad\nType=Application\n",
        )
        .unwrap();
        assert_eq!(xmonad.name, "XMonad");
        assert_eq!(xmonad.exec, ["xinitrcsession-helper", "xmonad"]);
        assert_eq!(
            xmonad.command("startx /usr/bin/env"),
            ["startx", "/usr/bin/env", "xinitrcsession-helper", "xmonad"]
        );
        assert_eq!(
            xmonad.environment(),
            [
                "XDG_SESSION_TYPE=x11",
                "XDG_SESSION_DESKTOP=xmonad",
                "DESKTOP_SESSION=xmonad"
            ]
        );
    }

    #[test]
    fn finds_the_session_running_by_its_name() {
        let entry = "[Desktop Entry]\nName=N\nExec=n\n";
        let sessions = vec![
            session("plasma", entry).unwrap(),
            session("xmonad", entry).unwrap(),
        ];
        let names = |names: &[&str]| {
            names
                .iter()
                .map(|name| name.to_string())
                .collect::<Vec<_>>()
        };
        let id = |found: Option<Session>| found.map(|session| session.id);
        assert_eq!(
            id(named(sessions.clone(), &names(&["xmonad"]))),
            Some("xmonad".into())
        );
        assert_eq!(
            id(named(
                sessions.clone(),
                &names(&["", "/usr/share/xsessions/plasma.desktop"])
            )),
            Some("plasma".into())
        );
        assert_eq!(id(named(sessions, &names(&["i3"]))), None);
    }

    #[test]
    fn skips_hidden_and_missing_sessions() {
        assert!(session("a", "[Desktop Entry]\nName=A\nExec=a\nNoDisplay=true\n").is_none());
        assert!(
            session(
                "b",
                "[Desktop Entry]\nName=B\nExec=b\nTryExec=/usr/bin/missing\n"
            )
            .is_none()
        );
        assert!(session("c", "[Desktop Entry]\nName=C\n").is_none());
        assert!(session("d", "[Other]\nExec=d\n").is_none());
    }

    #[test]
    fn quotes_what_the_shell_would_split() {
        let entry = session(
            "kde",
            "[Desktop Entry]\nName=Plasma\nExec=\"/opt/my wm/start\" --flag %U 100%% it's\nDesktopNames=KDE;Plasma;\n",
        )
        .unwrap();
        assert_eq!(entry.exec, ["/opt/my wm/start", "--flag", "100%", "it's"]);
        assert_eq!(
            entry.command("startx /usr/bin/env"),
            [
                "startx",
                "/usr/bin/env",
                "'/opt/my wm/start'",
                "--flag",
                "100%",
                r"'it'\''s'"
            ]
        );
        assert_eq!(entry.environment()[3], "XDG_CURRENT_DESKTOP=KDE:Plasma");
    }
}

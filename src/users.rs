//! The people who can log in: accounts with a regular uid and a login shell.

use std::path::{Path, PathBuf};
use std::time::Duration;

const AVATARS: &str = "/var/lib/AccountsService/icons";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct User {
    pub name: String,
    pub display_name: String,
    pub avatar: Option<PathBuf>,
    /// The login shell, such as `/usr/bin/zsh`.
    pub shell: String,
}

impl User {
    /// What the avatar shows when the account has no picture.
    pub fn initial(&self) -> String {
        self.display_name
            .chars()
            .next()
            .map(String::from)
            .unwrap_or_default()
    }
}

/// Every account from `/etc/passwd` within login.defs' uid range whose shell
/// is listed in `/etc/shells`.
pub fn load() -> Vec<User> {
    let passwd = std::fs::read_to_string("/etc/passwd").unwrap_or_default();
    let shells = std::fs::read_to_string("/etc/shells").unwrap_or_default();
    let defs = std::fs::read_to_string("/etc/login.defs").unwrap_or_default();
    let mut users = parse(&passwd, &login_shells(&shells), uid_range(&defs));
    for user in &mut users {
        user.avatar = avatar(&user.name);
    }
    users
}

/// The account this runs as, from every account database the system
/// reads, not `/etc/passwd` alone.
pub fn current() -> Option<User> {
    // SAFETY: a passwd of null pointers, filled in by getpwuid_r.
    let mut entry: libc::passwd = unsafe { std::mem::zeroed() };
    let mut buffer = vec![0 as libc::c_char; 16 * 1024];
    let mut found = std::ptr::null_mut();
    // SAFETY: the entry and the buffer outlive the call, which fills the
    // entry with pointers into the buffer.
    let status = unsafe {
        libc::getpwuid_r(
            rustix::process::getuid().as_raw(),
            &mut entry,
            buffer.as_mut_ptr(),
            buffer.len(),
            &mut found,
        )
    };
    if status != 0 || found.is_null() {
        return None;
    }
    let field = |text: *const libc::c_char| {
        if text.is_null() {
            return String::new();
        }
        // SAFETY: a NUL-terminated string in the buffer, as above.
        unsafe { std::ffi::CStr::from_ptr(text) }
            .to_string_lossy()
            .into_owned()
    };
    let name = field(entry.pw_name);
    Some(User {
        display_name: display_name(&name, &field(entry.pw_gecos)),
        avatar: avatar(&name),
        shell: field(entry.pw_shell),
        name,
    })
}

fn avatar(name: &str) -> Option<PathBuf> {
    Some(Path::new(AVATARS).join(name)).filter(|path| path.is_file())
}

/// The full name in the comment field, or else the login name.
fn display_name(name: &str, gecos: &str) -> String {
    let full_name = gecos.split(',').next().unwrap_or("").trim();
    if full_name.is_empty() {
        name
    } else {
        full_name
    }
    .to_string()
}

/// The session AccountsService keeps for `user`, as LightDM and GDM leave
/// it: a desktop entry's name such as `xmonad`. For the first login, before
/// the greeter remembers one of its own.
pub fn accounts_session(user: &str) -> Option<String> {
    const ACCOUNTS: &str = "org.freedesktop.Accounts";
    let bus = zbus::blocking::connection::Builder::system()
        .ok()?
        .method_timeout(Duration::from_secs(2))
        .build()
        .ok()?;
    let reply = bus
        .call_method(
            Some(ACCOUNTS),
            "/org/freedesktop/Accounts",
            Some(ACCOUNTS),
            "FindUserByName",
            &(user,),
        )
        .ok()?;
    let path: zbus::zvariant::OwnedObjectPath = reply.body().deserialize().ok()?;
    let account =
        zbus::blocking::Proxy::new(&bus, ACCOUNTS, path, "org.freedesktop.Accounts.User").ok()?;
    let session: String = account.get_property("XSession").ok()?;
    (!session.is_empty()).then_some(session)
}

fn parse(passwd: &str, shells: &[&str], uids: (u32, u32)) -> Vec<User> {
    passwd
        .lines()
        .filter_map(|line| {
            let fields: Vec<&str> = line.split(':').collect();
            let [name, _, uid, _, gecos, _, shell] = fields[..] else {
                return None;
            };
            let uid: u32 = uid.parse().ok()?;
            if !(uids.0..=uids.1).contains(&uid) || !shells.contains(&shell) {
                return None;
            }
            Some(User {
                name: name.to_string(),
                display_name: display_name(name, gecos),
                avatar: None,
                shell: shell.to_string(),
            })
        })
        .collect()
}

fn login_shells(shells: &str) -> Vec<&str> {
    shells
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with('/'))
        .filter(|shell| !shell.ends_with("/nologin") && !shell.ends_with("/false"))
        .collect()
}

fn uid_range(login_defs: &str) -> (u32, u32) {
    let value = |key: &str| {
        login_defs.lines().find_map(|line| {
            let mut words = line.split_whitespace();
            (words.next() == Some(key))
                .then(|| words.next()?.parse().ok())
                .flatten()
        })
    };
    (
        value("UID_MIN").unwrap_or(1000),
        value("UID_MAX").unwrap_or(60000),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const PASSWD: &str = "\
root:x:0:0::/root:/usr/bin/bash
nobody:x:65534:65534:Kernel Overflow User:/:/usr/bin/nologin
greeter:x:964:964:greetd greeter user:/:/bin/bash
alice:x:1000:100::/home/alice:/bin/zsh
ada:x:1001:100:Ada Lovelace,,,:/home/ada:/bin/bash
svc:x:1002:100::/srv:/usr/bin/nologin
";

    #[test]
    fn keeps_people_with_a_login_shell() {
        let shells = login_shells("# comment\n/bin/sh\n/bin/bash\n/bin/zsh\n/usr/bin/nologin\n");
        let users = parse(PASSWD, &shells, (1000, 60000));
        let names: Vec<_> = users
            .iter()
            .map(|u| (u.name.as_str(), u.display_name.as_str()))
            .collect();
        assert_eq!(names, [("alice", "alice"), ("ada", "Ada Lovelace")]);
        assert_eq!(users[1].initial(), "A");
        assert_eq!(users[0].shell, "/bin/zsh");
    }

    #[test]
    fn reads_the_uid_range() {
        assert_eq!(
            uid_range("UID_MIN\t\t\t 500\n#UID_MAX 1\nUID_MAX 2000\n"),
            (500, 2000)
        );
        assert_eq!(uid_range(""), (1000, 60000));
    }
}

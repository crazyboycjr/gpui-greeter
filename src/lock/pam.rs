//! The password of the user who locked the screen, checked by PAM under the
//! service `gpui-greeter-lock`. Run as that user, pam_unix checks it through
//! its setuid helper, so the lock needs no privileges of its own, and PAM's
//! delays and faillock's count apply as at a login.

use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::path::{Path, PathBuf};

use zeroize::{Zeroize, Zeroizing};

pub const SERVICE: &str = "gpui-greeter-lock";
const SERVICE_C: &CStr = c"gpui-greeter-lock";

/// Where PAM reads a service's file: the administrator's, then the
/// distribution's.
const PAM_DIRS: [&str; 2] = ["/etc/pam.d", "/usr/lib/pam.d"];

const SUCCESS: c_int = 0;
const BUF_ERR: c_int = 5;
const PERM_DENIED: c_int = 6;
const AUTH_ERR: c_int = 7;
const CRED_INSUFFICIENT: c_int = 8;
const AUTHINFO_UNAVAIL: c_int = 9;
const USER_UNKNOWN: c_int = 10;
const MAXTRIES: c_int = 11;
const CONV_ERR: c_int = 19;
const REFRESH_CRED: c_int = 0x0010;
const PROMPT_ECHO_OFF: c_int = 1;
const PROMPT_ECHO_ON: c_int = 2;
const ERROR_MSG: c_int = 3;
const TEXT_INFO: c_int = 4;
const MAX_NUM_MSG: c_int = 32;

#[repr(C)]
struct Message {
    style: c_int,
    text: *const c_char,
}

#[repr(C)]
struct Response {
    text: *mut c_char,
    code: c_int,
}

#[repr(C)]
struct Conv {
    converse:
        unsafe extern "C" fn(c_int, *mut *const Message, *mut *mut Response, *mut c_void) -> c_int,
    data: *mut c_void,
}

#[repr(C)]
struct Handle {
    _opaque: [u8; 0],
}

#[link(name = "pam")]
unsafe extern "C" {
    fn pam_start(
        service: *const c_char,
        user: *const c_char,
        conv: *const Conv,
        handle: *mut *mut Handle,
    ) -> c_int;
    #[cfg(any(test, feature = "fake-pam"))]
    fn pam_start_confdir(
        service: *const c_char,
        user: *const c_char,
        conv: *const Conv,
        dir: *const c_char,
        handle: *mut *mut Handle,
    ) -> c_int;
    fn pam_authenticate(handle: *mut Handle, flags: c_int) -> c_int;
    fn pam_setcred(handle: *mut Handle, flags: c_int) -> c_int;
    fn pam_end(handle: *mut Handle, status: c_int) -> c_int;
    fn pam_strerror(handle: *mut Handle, code: c_int) -> *const c_char;
}

/// The service passwords are checked with: the system's, or for tests a
/// fake one read from a folder of their own.
#[derive(Debug, Clone, Default)]
pub struct Service {
    #[cfg(any(test, feature = "fake-pam"))]
    dir: Option<PathBuf>,
}

impl Service {
    /// The fake service in `dir`, never the system's own.
    #[cfg(any(test, feature = "fake-pam"))]
    pub fn fake(dir: &Path) -> Self {
        Self {
            dir: Some(dir.to_path_buf()),
        }
    }

    /// The service's file. Without one PAM falls back to `other`, which on
    /// most systems refuses every password.
    pub fn file(&self) -> Option<PathBuf> {
        #[cfg(any(test, feature = "fake-pam"))]
        if let Some(dir) = &self.dir {
            return Some(dir.join(SERVICE)).filter(|file| file.is_file());
        }
        PAM_DIRS
            .iter()
            .map(|dir| Path::new(dir).join(SERVICE))
            .find(|file| file.is_file())
    }

    unsafe fn start(&self, user: &CStr, conv: &Conv, handle: &mut *mut Handle) -> c_int {
        #[cfg(any(test, feature = "fake-pam"))]
        if let Some(dir) = &self.dir {
            use std::os::unix::ffi::OsStrExt as _;
            let Ok(dir) = CString::new(dir.as_os_str().as_bytes()) else {
                return BUF_ERR;
            };
            // SAFETY: every pointer is valid for the call, and PAM copies
            // the conversation.
            return unsafe {
                pam_start_confdir(
                    SERVICE_C.as_ptr(),
                    user.as_ptr(),
                    conv,
                    dir.as_ptr(),
                    handle,
                )
            };
        }
        // SAFETY: as above.
        unsafe { pam_start(SERVICE_C.as_ptr(), user.as_ptr(), conv, handle) }
    }
}

/// A question PAM asked after the password, such as a one-time code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Question {
    pub prompt: String,
    pub secret: bool,
    /// What PAM said before asking, if anything.
    pub notice: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Accepted,
    /// Refused, with what PAM said about it, if anything.
    Denied {
        notice: Option<String>,
    },
    /// PAM could not check, for the reason given.
    Failed(String),
}

/// Checks `password` for `user`. PAM's first question is answered with the
/// password; a later one goes to `ask`, whose `None` breaks the
/// conversation off.
pub fn authenticate(
    service: &Service,
    user: &str,
    password: Zeroizing<String>,
    ask: &mut dyn FnMut(Question) -> Option<Zeroizing<String>>,
) -> Verdict {
    let Ok(user) = CString::new(user) else {
        return Verdict::Failed(format!("no user {user:?}"));
    };
    let mut talk = Talk {
        password: Some(password),
        ask,
        notices: Vec::new(),
    };
    let conv = Conv {
        converse,
        data: (&raw mut talk).cast(),
    };
    let mut handle = std::ptr::null_mut();
    // SAFETY: `talk` outlives the handle, which ends below.
    let started = unsafe { service.start(&user, &conv, &mut handle) };
    if started != SUCCESS {
        return Verdict::Failed(describe(handle, started));
    }
    // SAFETY: the handle is the one PAM started, ended once below.
    let status = unsafe { pam_authenticate(handle, 0) };
    if status == SUCCESS {
        // Refreshes what the login set up, such as a Kerberos ticket.
        // SAFETY: as above.
        unsafe { pam_setcred(handle, REFRESH_CRED) };
    }
    let reason = describe(handle, status);
    // SAFETY: as above.
    unsafe { pam_end(handle, status) };
    let notice = talk.notice();
    match status {
        SUCCESS => Verdict::Accepted,
        PERM_DENIED | AUTH_ERR | CRED_INSUFFICIENT | AUTHINFO_UNAVAIL | USER_UNKNOWN | MAXTRIES => {
            Verdict::Denied { notice }
        }
        _ => Verdict::Failed(notice.unwrap_or(reason)),
    }
}

fn describe(handle: *mut Handle, code: c_int) -> String {
    // SAFETY: Linux-PAM's pam_strerror takes any handle, even none, and
    // returns a static string.
    let text = unsafe { pam_strerror(handle, code) };
    if text.is_null() {
        return format!("PAM error {code}");
    }
    // SAFETY: a NUL-terminated string, as above.
    unsafe { CStr::from_ptr(text) }
        .to_string_lossy()
        .into_owned()
}

/// What the conversation keeps between PAM's messages.
struct Talk<'a> {
    password: Option<Zeroizing<String>>,
    ask: &'a mut dyn FnMut(Question) -> Option<Zeroizing<String>>,
    notices: Vec<String>,
}

impl Talk<'_> {
    /// What PAM said since the last question, in one line.
    fn notice(&mut self) -> Option<String> {
        let notice = self.notices.join(" ");
        self.notices.clear();
        (!notice.is_empty()).then_some(notice)
    }
}

unsafe extern "C" fn converse(
    count: c_int,
    messages: *mut *const Message,
    responses: *mut *mut Response,
    data: *mut c_void,
) -> c_int {
    // A panic must neither unwind into PAM nor end the lock.
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // SAFETY: PAM calls with the conversation's own data and messages.
        unsafe { reply(count, messages, responses, data) }
    }))
    .unwrap_or(CONV_ERR)
}

unsafe fn reply(
    count: c_int,
    messages: *mut *const Message,
    responses: *mut *mut Response,
    data: *mut c_void,
) -> c_int {
    if !(1..=MAX_NUM_MSG).contains(&count)
        || messages.is_null()
        || responses.is_null()
        || data.is_null()
    {
        return CONV_ERR;
    }
    // SAFETY: `data` is the `Talk` that `authenticate` handed PAM, which
    // calls back on that thread only while authenticate waits for it.
    let talk = unsafe { &mut *data.cast::<Talk>() };
    let count = count as usize;
    // PAM frees the replies and the answers in them with free().
    // SAFETY: calloc of a count checked above.
    let replies = unsafe { libc::calloc(count, size_of::<Response>()) }.cast::<Response>();
    if replies.is_null() {
        return BUF_ERR;
    }
    for index in 0..count {
        // SAFETY: Linux-PAM passes an array of `count` message pointers.
        let message = unsafe { &**messages.add(index) };
        let text = if message.text.is_null() {
            String::new()
        } else {
            // SAFETY: PAM's messages are NUL-terminated.
            unsafe { CStr::from_ptr(message.text) }
                .to_string_lossy()
                .into_owned()
        };
        match message.style {
            PROMPT_ECHO_OFF | PROMPT_ECHO_ON => {
                let answer = match talk.password.take() {
                    Some(password) => Some(password),
                    None => {
                        let notice = talk.notice();
                        (talk.ask)(Question {
                            prompt: text,
                            secret: message.style == PROMPT_ECHO_OFF,
                            notice,
                        })
                    }
                };
                match answer.as_deref().and_then(|answer| copy_for_pam(answer)) {
                    // SAFETY: within the `count` replies allocated above.
                    Some(copy) => unsafe { (*replies.add(index)).text = copy },
                    None => {
                        // SAFETY: the replies allocated above.
                        unsafe { discard(replies, count) };
                        return CONV_ERR;
                    }
                }
            }
            ERROR_MSG | TEXT_INFO => {
                let text = text.trim();
                if !text.is_empty() {
                    talk.notices.push(text.to_string());
                }
            }
            _ => {
                // SAFETY: as above.
                unsafe { discard(replies, count) };
                return CONV_ERR;
            }
        }
    }
    // SAFETY: PAM's out pointer, checked above.
    unsafe { *responses = replies };
    SUCCESS
}

/// `answer` in memory of malloc's, which PAM frees.
fn copy_for_pam(answer: &str) -> Option<*mut c_char> {
    let bytes = answer.as_bytes();
    // SAFETY: room for the bytes and a NUL, filled before it is handed on.
    unsafe {
        let copy = libc::malloc(bytes.len() + 1).cast::<u8>();
        if copy.is_null() {
            return None;
        }
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), copy, bytes.len());
        *copy.add(bytes.len()) = 0;
        Some(copy.cast())
    }
}

/// Wipes and frees the replies of a conversation that broke off.
unsafe fn discard(replies: *mut Response, count: usize) {
    for index in 0..count {
        // SAFETY: the `count` replies calloc made, each answer from
        // copy_for_pam or null.
        unsafe {
            let text = (*replies.add(index)).text;
            if !text.is_null() {
                std::slice::from_raw_parts_mut(text.cast::<u8>(), libc::strlen(text)).zeroize();
                libc::free(text.cast());
            }
        }
    }
    // SAFETY: as above.
    unsafe { libc::free(replies.cast()) };
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt as _;

    /// A fake service as system-auth stacks pam_unix: a script accepts the
    /// password "test" and says why it refuses anything else.
    fn fake_service(name: &str) -> (Service, PathBuf) {
        let dir =
            std::env::temp_dir().join(format!("gpui-greeter-pam-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let check = dir.join("check");
        std::fs::write(
            &check,
            "#!/bin/sh\n[ \"$(tr -d '\\000')\" = test ] && exit 0\necho 'Wrong test password'\nexit 1\n",
        )
        .unwrap();
        std::fs::set_permissions(&check, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::write(
            dir.join(SERVICE),
            format!(
                "auth [success=1 default=ignore] pam_exec.so expose_authtok quiet stdout {}\n\
                 auth requisite pam_deny.so\n\
                 auth required pam_permit.so\n",
                check.display()
            ),
        )
        .unwrap();
        (Service::fake(&dir), dir)
    }

    fn never_asked(question: Question) -> Option<Zeroizing<String>> {
        panic!("asked {question:?}")
    }

    #[test]
    fn accepts_the_password_and_refuses_another_with_pam_s_words() {
        let (service, dir) = fake_service("check");
        assert!(service.file().is_some());
        let check = |password: &str| {
            authenticate(
                &service,
                "alice",
                Zeroizing::new(password.into()),
                &mut never_asked,
            )
        };
        assert_eq!(check("test"), Verdict::Accepted);
        assert_eq!(
            check("wrong"),
            Verdict::Denied {
                notice: Some("Wrong test password".into())
            }
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_missing_service_is_noticed() {
        let dir =
            std::env::temp_dir().join(format!("gpui-greeter-pam-none-{}", std::process::id()));
        assert_eq!(Service::fake(&dir).file(), None);
    }

    /// Calls the conversation as PAM would, with `messages`.
    fn converse_with(
        messages: &[(c_int, &CStr)],
        password: &str,
        ask: &mut dyn FnMut(Question) -> Option<Zeroizing<String>>,
    ) -> (c_int, Vec<Option<String>>) {
        let mut talk = Talk {
            password: Some(Zeroizing::new(password.into())),
            ask,
            notices: Vec::new(),
        };
        let structs: Vec<Message> = messages
            .iter()
            .map(|(style, text)| Message {
                style: *style,
                text: text.as_ptr(),
            })
            .collect();
        let mut pointers: Vec<*const Message> = structs.iter().map(|m| m as *const _).collect();
        let mut replies: *mut Response = std::ptr::null_mut();
        let status = unsafe {
            converse(
                messages.len() as c_int,
                pointers.as_mut_ptr(),
                &mut replies,
                (&raw mut talk).cast(),
            )
        };
        let mut answers = Vec::new();
        if status == SUCCESS {
            for index in 0..messages.len() {
                let text = unsafe { (*replies.add(index)).text };
                answers.push((!text.is_null()).then(|| {
                    unsafe { CStr::from_ptr(text) }
                        .to_string_lossy()
                        .into_owned()
                }));
            }
            unsafe { discard(replies, messages.len()) };
        }
        (status, answers)
    }

    #[test]
    fn answers_the_first_question_with_the_password_and_asks_the_next() {
        let mut asked = Vec::new();
        let (status, answers) = converse_with(
            &[
                (TEXT_INFO, c"Last login: yesterday"),
                (PROMPT_ECHO_OFF, c"Password: "),
                (ERROR_MSG, c"Two factors are needed"),
                (PROMPT_ECHO_ON, c"Code: "),
            ],
            "hunter2",
            &mut |question| {
                asked.push(question);
                Some(Zeroizing::new("123456".into()))
            },
        );
        assert_eq!(status, SUCCESS);
        assert_eq!(
            answers,
            [None, Some("hunter2".into()), None, Some("123456".into())]
        );
        assert_eq!(
            asked,
            [Question {
                prompt: "Code: ".into(),
                secret: false,
                notice: Some("Last login: yesterday Two factors are needed".into()),
            }]
        );
    }

    #[test]
    fn a_question_left_unanswered_breaks_the_conversation_off() {
        let (status, _) = converse_with(
            &[
                (PROMPT_ECHO_OFF, c"Password: "),
                (PROMPT_ECHO_OFF, c"Code: "),
            ],
            "hunter2",
            &mut |_| None,
        );
        assert_eq!(status, CONV_ERR);
        let (status, _) = converse_with(&[(99, c"?")], "hunter2", &mut never_asked);
        assert_eq!(status, CONV_ERR);
    }
}

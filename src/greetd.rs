//! The greeter's side of greetd's IPC: a login conversation over the socket
//! greetd names in `GREETD_SOCK`.
//!
//! Messages are framed by hand rather than through greetd_ipc's codec so
//! that the buffer a password is serialised into is wiped once it is sent.

use std::io::{self, Read, Write};
use std::os::unix::net::UnixStream;

use anyhow::{Context as _, Result, bail};
use greetd_ipc::{AuthMessageType, ErrorType, Request, Response};
use zeroize::{Zeroize, Zeroizing};

/// Where a login conversation stands after greetd's last answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// PAM asks something; a secret question hides the answer as it is typed.
    Question {
        prompt: String,
        secret: bool,
        notice: Option<String>,
    },
    /// The user is authenticated: the session can start.
    Authenticated,
    /// Authentication failed, with what PAM said about it, if anything.
    Denied { notice: Option<String> },
}

pub struct Greetd {
    stream: UnixStream,
}

impl Greetd {
    pub fn connect() -> Result<Self> {
        let path = std::env::var_os("GREETD_SOCK").context("GREETD_SOCK is not set")?;
        let stream = UnixStream::connect(&path)
            .with_context(|| format!("cannot connect to {}", path.to_string_lossy()))?;
        let mut greetd = Self { stream };
        // greetd keeps a conversation that an earlier connection left open,
        // and would refuse to start another.
        greetd.cancel()?;
        Ok(greetd)
    }

    pub fn from_stream(stream: UnixStream) -> Self {
        Self { stream }
    }

    pub fn create_session(&mut self, username: &str) -> Result<Step> {
        let response = self.call(Request::CreateSession {
            username: username.to_string(),
        })?;
        self.follow(response)
    }

    pub fn answer(&mut self, mut answer: Option<Zeroizing<String>>) -> Result<Step> {
        let response = self.post(answer.as_mut().map(|text| std::mem::take(&mut **text)))?;
        self.follow(response)
    }

    pub fn start_session(&mut self, cmd: Vec<String>, env: Vec<String>) -> Result<()> {
        match self.call(Request::StartSession { cmd, env })? {
            Response::Success => Ok(()),
            Response::Error { description, .. } => bail!("{description}"),
            Response::AuthMessage { .. } => bail!("greetd asked a question instead of starting"),
        }
    }

    pub fn cancel(&mut self) -> Result<()> {
        match self.call(Request::CancelSession)? {
            Response::Error { description, .. } => bail!("{description}"),
            _ => Ok(()),
        }
    }

    /// Answers informational messages on the user's behalf until PAM asks
    /// a question, accepts or refuses, keeping the last message to show.
    fn follow(&mut self, mut response: Response) -> Result<Step> {
        let mut notice = None;
        loop {
            match response {
                Response::Success => return Ok(Step::Authenticated),
                Response::Error {
                    error_type: ErrorType::AuthError,
                    ..
                } => {
                    return Ok(Step::Denied { notice });
                }
                Response::Error {
                    error_type: ErrorType::Error,
                    description,
                } => bail!("{description}"),
                Response::AuthMessage {
                    auth_message_type,
                    auth_message,
                } => match auth_message_type {
                    AuthMessageType::Secret | AuthMessageType::Visible => {
                        let secret = matches!(auth_message_type, AuthMessageType::Secret);
                        return Ok(Step::Question {
                            prompt: auth_message,
                            secret,
                            notice,
                        });
                    }
                    AuthMessageType::Info | AuthMessageType::Error => {
                        notice = Some(auth_message);
                        response = self.post(None)?;
                    }
                },
            }
        }
    }

    fn post(&mut self, answer: Option<String>) -> Result<Response> {
        let mut request = Request::PostAuthMessageResponse { response: answer };
        let response = self.call_ref(&request);
        if let Request::PostAuthMessageResponse {
            response: Some(secret),
        } = &mut request
        {
            secret.zeroize();
        }
        response
    }

    fn call(&mut self, request: Request) -> Result<Response> {
        self.call_ref(&request)
    }

    fn call_ref(&mut self, request: &Request) -> Result<Response> {
        wire::write(&mut self.stream, request).context("cannot write to greetd")?;
        wire::read(&mut self.stream).context("cannot read from greetd")
    }
}

/// greetd's framing: each message is its length as a native-endian u32,
/// then that many bytes of JSON.
pub mod wire {
    use super::*;

    /// Large enough that serialising a password never grows the buffer,
    /// which would leave a copy behind in freed memory.
    const BUFFER: usize = 4096;

    pub fn write<T: serde::Serialize>(stream: &mut impl Write, message: &T) -> io::Result<()> {
        let mut body = Zeroizing::new(Vec::with_capacity(BUFFER));
        serde_json::to_writer(&mut *body, message)?;
        let len = u32::try_from(body.len()).map_err(io::Error::other)?;
        stream.write_all(&len.to_ne_bytes())?;
        stream.write_all(&body)?;
        stream.flush()
    }

    pub fn read<T: serde::de::DeserializeOwned>(stream: &mut impl Read) -> io::Result<T> {
        let mut len = [0; 4];
        stream.read_exact(&mut len)?;
        let mut body = Zeroizing::new(vec![0; u32::from_ne_bytes(len) as usize]);
        stream.read_exact(&mut body)?;
        Ok(serde_json::from_slice(&body)?)
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use std::thread::JoinHandle;

    /// Plays greetd's part from a script: each request must match, and is
    /// answered with the response beside it.
    pub fn scripted(script: Vec<(serde_json::Value, Response)>) -> (Greetd, JoinHandle<()>) {
        let (ours, mut theirs) = UnixStream::pair().unwrap();
        let daemon = std::thread::spawn(move || {
            for (expected, response) in script {
                let request: serde_json::Value = wire::read(&mut theirs).unwrap();
                assert_eq!(request, expected);
                wire::write(&mut theirs, &response).unwrap();
            }
        });
        (Greetd::from_stream(ours), daemon)
    }

    fn question(kind: AuthMessageType, text: &str) -> Response {
        Response::AuthMessage {
            auth_message_type: kind,
            auth_message: text.into(),
        }
    }

    #[test]
    fn passes_info_on_and_stops_at_a_question() {
        let (mut greetd, daemon) = scripted(vec![
            (
                serde_json::json!({"type": "create_session", "username": "alice"}),
                question(AuthMessageType::Info, "Last login: yesterday"),
            ),
            (
                serde_json::json!({"type": "post_auth_message_response", "response": null}),
                question(AuthMessageType::Secret, "Password: "),
            ),
        ]);
        assert_eq!(
            greetd.create_session("alice").unwrap(),
            Step::Question {
                prompt: "Password: ".into(),
                secret: true,
                notice: Some("Last login: yesterday".into())
            }
        );
        daemon.join().unwrap();
    }

    #[test]
    fn reports_a_denial_with_pam_s_reason() {
        let (mut greetd, daemon) = scripted(vec![
            (
                serde_json::json!({"type": "post_auth_message_response", "response": "hunter2"}),
                question(AuthMessageType::Error, "The account is locked"),
            ),
            (
                serde_json::json!({"type": "post_auth_message_response", "response": null}),
                Response::Error {
                    error_type: ErrorType::AuthError,
                    description: "AUTH_ERR".into(),
                },
            ),
        ]);
        let answer = Zeroizing::new("hunter2".to_string());
        assert_eq!(
            greetd.answer(Some(answer)).unwrap(),
            Step::Denied {
                notice: Some("The account is locked".into())
            }
        );
        daemon.join().unwrap();
    }
}

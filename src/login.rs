//! One press of Enter: answer PAM with what was typed and, once greetd
//! accepts, have it start the chosen session.

use anyhow::Result;
use zeroize::Zeroizing;

use crate::greetd::{Greetd, Step};

pub struct Attempt {
    pub user: String,
    pub typed: Zeroizing<String>,
    /// Whether this answers a question PAM asked after the first one, in the
    /// conversation still open, rather than starting a new one.
    pub resumes: bool,
    pub cmd: Vec<String>,
    pub env: Vec<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    /// greetd starts the session as soon as the greeter exits.
    Started,
    /// PAM wants another answer, such as a one-time code.
    Asked {
        prompt: String,
        secret: bool,
        notice: Option<String>,
    },
    /// Wrong password, or PAM refused for the reason in the notice.
    Denied { notice: Option<String> },
}

pub fn attempt(greetd: &mut Greetd, attempt: Attempt) -> Result<Outcome> {
    let Attempt {
        user,
        typed,
        resumes,
        cmd,
        env,
    } = attempt;
    let step = if resumes {
        greetd.answer(Some(typed))?
    } else {
        match greetd.create_session(&user)? {
            Step::Question { .. } => greetd.answer(Some(typed))?,
            done => done,
        }
    };
    let outcome = match step {
        Step::Authenticated => match greetd.start_session(cmd, env) {
            Ok(()) => return Ok(Outcome::Started),
            Err(error) => {
                greetd.cancel()?;
                return Err(error);
            }
        },
        Step::Question {
            prompt,
            secret,
            notice,
        } => {
            return Ok(Outcome::Asked {
                prompt,
                secret,
                notice,
            });
        }
        Step::Denied { notice } => Outcome::Denied { notice },
    };
    greetd.cancel()?;
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::greetd::tests::scripted;
    use greetd_ipc::{AuthMessageType, ErrorType, Response};
    use serde_json::json;

    fn password_prompt() -> Response {
        Response::AuthMessage {
            auth_message_type: AuthMessageType::Secret,
            auth_message: "Password: ".into(),
        }
    }

    fn attempt_for(typed: &str) -> Attempt {
        Attempt {
            user: "alice".into(),
            typed: Zeroizing::new(typed.into()),
            resumes: false,
            cmd: vec![
                "startx".into(),
                "/usr/bin/env".into(),
                "xinitrcsession-helper".into(),
            ],
            env: vec!["XDG_SESSION_TYPE=x11".into()],
        }
    }

    #[test]
    fn a_good_password_starts_the_session() {
        let (mut greetd, daemon) = scripted(vec![
            (
                json!({"type": "create_session", "username": "alice"}),
                password_prompt(),
            ),
            (
                json!({"type": "post_auth_message_response", "response": "right"}),
                Response::Success,
            ),
            (
                json!({"type": "start_session", "cmd": ["startx", "/usr/bin/env", "xinitrcsession-helper"], "env": ["XDG_SESSION_TYPE=x11"]}),
                Response::Success,
            ),
        ]);
        assert_eq!(
            attempt(&mut greetd, attempt_for("right")).unwrap(),
            Outcome::Started
        );
        daemon.join().unwrap();
    }

    #[test]
    fn a_wrong_password_cancels_the_conversation() {
        let (mut greetd, daemon) = scripted(vec![
            (
                json!({"type": "create_session", "username": "alice"}),
                password_prompt(),
            ),
            (
                json!({"type": "post_auth_message_response", "response": "wrong"}),
                Response::Error {
                    error_type: ErrorType::AuthError,
                    description: "AUTH_ERR".into(),
                },
            ),
            (json!({"type": "cancel_session"}), Response::Success),
        ]);
        assert_eq!(
            attempt(&mut greetd, attempt_for("wrong")).unwrap(),
            Outcome::Denied { notice: None }
        );
        daemon.join().unwrap();
    }

    #[test]
    fn a_second_question_keeps_the_conversation_open() {
        let (mut greetd, daemon) = scripted(vec![
            (
                json!({"type": "create_session", "username": "alice"}),
                password_prompt(),
            ),
            (
                json!({"type": "post_auth_message_response", "response": "right"}),
                Response::AuthMessage {
                    auth_message_type: AuthMessageType::Visible,
                    auth_message: "Code: ".into(),
                },
            ),
            (
                json!({"type": "post_auth_message_response", "response": "123456"}),
                Response::Success,
            ),
            (
                json!({"type": "start_session", "cmd": ["startx", "/usr/bin/env", "xinitrcsession-helper"], "env": ["XDG_SESSION_TYPE=x11"]}),
                Response::Success,
            ),
        ]);
        assert_eq!(
            attempt(&mut greetd, attempt_for("right")).unwrap(),
            Outcome::Asked {
                prompt: "Code: ".into(),
                secret: false,
                notice: None
            }
        );
        let code = Attempt {
            resumes: true,
            ..attempt_for("123456")
        };
        assert_eq!(attempt(&mut greetd, code).unwrap(), Outcome::Started);
        daemon.join().unwrap();
    }
}

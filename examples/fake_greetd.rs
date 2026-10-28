//! A stand-in for greetd, to try the greeter without logging anyone in.
//!
//!     cargo run --example fake_greetd -- SOCKET [--password PASSWORD]
//!
//! It listens on SOCKET, which the greeter finds in GREETD_SOCK, and plays
//! greetd's part: any user may log in with PASSWORD ("test" by default),
//! after the short pause PAM takes; a wrong one is refused after a longer
//! one, as pam_faildelay does. Starting a session only prints its command
//! and environment, on a line beginning with START.

use std::os::unix::net::{UnixListener, UnixStream};
use std::time::Duration;

use gpui_greeter::greetd::wire;
use greetd_ipc::{AuthMessageType, ErrorType, Request, Response};

#[derive(Default)]
struct Conversation {
    user: Option<String>,
    authenticated: bool,
}

fn main() -> std::io::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let socket = args
        .first()
        .expect("usage: fake_greetd SOCKET [--password PASSWORD]");
    let password = match args.get(1).map(String::as_str) {
        Some("--password") => args.get(2).cloned().expect("--password needs a value"),
        _ => "test".to_string(),
    };
    let _ = std::fs::remove_file(socket);
    let listener = UnixListener::bind(socket)?;
    eprintln!("fake greetd listening on {socket}");
    let mut conversation = Conversation::default();
    for stream in listener.incoming() {
        if let Err(error) = serve(stream?, &password, &mut conversation) {
            eprintln!("client gone: {error}");
        }
    }
    Ok(())
}

fn serve(
    mut stream: UnixStream,
    password: &str,
    conversation: &mut Conversation,
) -> std::io::Result<()> {
    loop {
        let request: Request = wire::read(&mut stream)?;
        let response = answer(request, password, conversation);
        wire::write(&mut stream, &response)?;
    }
}

fn answer(request: Request, password: &str, conversation: &mut Conversation) -> Response {
    let error = |description: &str| Response::Error {
        error_type: ErrorType::Error,
        description: description.into(),
    };
    match request {
        Request::CreateSession { username } => {
            eprintln!("create_session {username}");
            if conversation.user.is_some() {
                return error("a session is already being configured");
            }
            conversation.user = Some(username);
            Response::AuthMessage {
                auth_message_type: AuthMessageType::Secret,
                auth_message: "Password: ".into(),
            }
        }
        Request::PostAuthMessageResponse { response } => {
            let right = response.as_deref() == Some(password);
            eprintln!(
                "post_auth_message_response ({})",
                if right {
                    "right password"
                } else {
                    "wrong password"
                }
            );
            if conversation.user.is_none() {
                return error("no session under configuration");
            }
            if right {
                std::thread::sleep(Duration::from_millis(300));
                conversation.authenticated = true;
                Response::Success
            } else {
                std::thread::sleep(Duration::from_millis(1500));
                Response::Error {
                    error_type: ErrorType::AuthError,
                    description: "pam_authenticate: AUTH_ERR".into(),
                }
            }
        }
        Request::StartSession { cmd, env } => {
            if !conversation.authenticated {
                return error("session is not ready");
            }
            println!(
                "START user={} cmd={cmd:?} env={env:?}",
                conversation.user.as_deref().unwrap_or("")
            );
            *conversation = Conversation::default();
            Response::Success
        }
        Request::CancelSession => {
            eprintln!("cancel_session");
            *conversation = Conversation::default();
            Response::Success
        }
    }
}

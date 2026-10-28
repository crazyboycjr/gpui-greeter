//! A stand-in for greetd, to try the greeter without logging anyone in.
//!
//!     cargo run --example fake_greetd -- SOCKET [--password PASSWORD]
//!
//! It listens on SOCKET, which the greeter finds in GREETD_SOCK, and plays
//! greetd's part: any user may log in with PASSWORD ("test" by default).
//! Starting a session only prints its command and environment, on a line
//! beginning with START.

use std::os::unix::net::UnixListener;

use gpui_greeter::greetd::fake;

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
    fake::serve(listener, &password);
    Ok(())
}

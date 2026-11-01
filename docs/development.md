# Developing gpui-greeter

gpui-greeter is a Rust program on [GPUI](https://www.gpui.rs), the GPU UI
framework of the Zed editor, through longbridge's
[gpui-kit](https://github.com/longbridge/gpui-kit), drawing with Vulkan
through wgpu on X11. It talks to greetd with
[greetd_ipc](https://crates.io/crates/greetd_ipc), to X with
[x11rb](https://crates.io/crates/x11rb), to logind and AccountsService
with [zbus](https://crates.io/crates/zbus), and to PAM directly, and draws
its SVGs with [resvg](https://crates.io/crates/resvg).

## How it fits together

greetd runs the greeter as its `greeter` user through `startx`, in an X
server of its own on VT 1. The greeter opens a window over every monitor
and lays each one out in points, as a Mac lays out its display
([configuration](configuration.md#size) says how), so it holds on any size
and shape. The login form is on the primary monitor (or the one under the
pointer) and follows the pointer to another one. Nothing in it names a
monitor, a port or a mode.

On Enter it asks greetd to log the user in, and once PAM accepts, to start
the chosen X session the way tuigreet does, `startx /usr/bin/env <Exec>`,
so the user's own `~/.xserverrc` and `~/.xinitrc` apply. The greeter then
exits at once, startx stops its X server, and greetd starts the session on
the same VT. Power actions go to logind over D-Bus, and each asks for a
second press to confirm.

While another VT is in front, such as a text console, the greeter takes
its windows off the screen and draws nothing until its own VT is back.
logind's view of the greeter's session says when, or the kernel's active
console where logind cannot.

The password is kept in memory that is wiped once it is sent, and never
logged.

Paper's and Neon's backgrounds are drawn from the mockups' SVG (with
resvg) at each monitor's size, and kept in
`/var/lib/gpui-greeter/.cache/gpui-greeter/scenery`, one picture for each
size, so that later starts read them back instead of drawing them again;
what moves in them is drawn over the picture.

Each window is given frames at its own monitor's refresh rate, and follows
it as monitors move, sleep, wake or change modes, so what moves keeps up
with a 240 Hz monitor as it does with a 60 Hz one.

## Building

You need Rust 1.88 or newer, and libxcb, libxkbcommon, PAM and a Vulkan
loader to link against.

```sh
cargo build --bins --examples
```

## Trying it out

`scripts/try` runs the greeter in an invisible X server (Xvfb, drawn by
lavapipe) against a fake greetd that accepts the password `test` and only
prints the session it would start. Power buttons only log.

```sh
cargo build --bins --examples
scripts/try start frost --monitors 2
scripts/try type test; scripts/try key Return
scripts/try shot frost-login
scripts/try log
scripts/try stop
```

`scripts/try start neon --background circuit` picks a background,
`scripts/try start path/to/theme` runs a theme folder,
`scripts/try vt away` and `scripts/try vt back` play the greeter's VT
switched to another and back, and
`cargo run --example scenery -- paper-night-desk desk.png` draws one alone.

Try the lock screen the same way, in an X server of its own (Xvfb, with
`xrandr --setmonitor` for monitors), never on the display you work on: a
build with `--features fake-pam`, which exists for tests alone and which
packages leave out, takes `gpui-greeter lock --pam-dir DIR`, where DIR
holds a fake `gpui-greeter-lock` service, so that no real password is
involved. `gpui-greeter preview DIR --lock` shows a theme folder as the
lock screen.

To see how smoothly a look moves on a real screen, set
`GPUI_GREETER_FRAMES=1`: after each burst of motion, such as a wake from
rest, every window says on stderr how many frames it drew, how far apart
they came, how long each took to lay out and paint, and which were slow.
Each window also says how often it is given a frame, at what refresh
rate, and from which monitor's mode, when it is shown and whenever that
changes, as when its monitor sleeps or wakes. The lock screen is the easy
place to read it, in a terminal of the session: wake it, press Escape to
send it back to rest, a few times over, then unlock.

```sh
GPUI_GREETER_FRAMES=1 gpui-greeter lock 2> frames.log
```

## Tests

`cargo test` covers the parts without a screen: the greetd conversation,
the lock's PAM conversation (against libpam, with a fake service of the
tests' own), desktop entries, users, the wallpaper copy, Bing's folder
(against a web server of the tests' own), the settings and the Bing timer
(offline, with `systemctl --root`), the theme format and the example
theme, and the backgrounds, whose skyline and circuit board must come out
of the mockup's seeds exactly as the mockup grows them.

## Vendored crates

gpui's X11 and wgpu backends are vendored in `vendor/`, for a window
dragged to a new size, each window's own refresh rate, and pop-ups that
stay composited; [vendor/README.md](../vendor/README.md) says what differs
from the published crates and why.

## Where things are

| Path | What it holds |
| --- | --- |
| `src/main.rs` | the commands and their options |
| `src/ui/` | the windows, the model of a login, and the looks: `theme/paper.rs`, `frost.rs`, `neon.rs`, and `theme/folder/` for theme folders |
| `src/scenery/` | Paper's and Neon's drawn backgrounds |
| `src/themes/` | reading, checking and installing theme folders |
| `src/lock/` | the lock's supervisor, its cover, its PAM conversation and the wait for a blank screen |
| `src/settings/` | the settings window and the root helper its Apply runs |
| `src/greetd.rs`, `src/sessions.rs`, `src/users.rs` | greetd, the X sessions and the accounts |
| `dist/` | the files the package installs around the binary |
| `skills/gpui-greeter-theme/` | the skill for designing themes, the format's reference and the example |
| `scripts/try` | the greeter in an invisible X server |

## Fonts

Inter, Fraunces and JetBrains Mono are built in, subset to Latin and the
symbols the looks use, under the SIL Open Font License (`assets/fonts`).
Chinese text uses the system's Noto Sans CJK SC.

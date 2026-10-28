# gpui-greeter

A greeter for [greetd](https://git.sr.ht/~kennylevinsen/greetd) on X11,
drawn with [GPUI](https://www.gpui.rs) and longbridge's
[gpui-kit](https://github.com/longbridge/gpui-kit). It comes in three looks,
chosen in its settings:

- **Paper**: a large Fraunces clock on warm paper (or on a dark ground, as
  Paper Night), the name and an underlined password field beneath it, the
  session at the bottom left and Sleep, Restart and Shut down at the bottom
  right.
- **Frost**: a wallpaper, sharp, with a big clock. A click or a key blurs
  and dims it behind a round avatar, a frosted password pill, frosted power
  buttons and a session chip.
- **Neon**: a terminal on a black grid with scanlines and a slow scan beam,
  a status line that goes READY, AUTH, then GRANTED or DENIED, and F-key
  buttons.

It speaks English and Chinese (`language = "zh-CN"`); another language is
one more table in `src/i18n.rs`.

## How it fits together

greetd runs the greeter as its `greeter` user through `startx`, in an X
server of its own on VT 1. The greeter opens a window over every monitor
and lays each one out on the mockups' 1600×900 frame, scaled to fit, so
the looks keep their proportions on any monitor; GPUI draws at the
monitor's pixel density. The login form is on the primary monitor (or the
one under the pointer) and follows the pointer to another one. Nothing in
it names a monitor, a port or a mode.

On Enter it asks greetd to log the user in, and once PAM accepts, to start
the chosen X session the way tuigreet does, `startx /usr/bin/env <Exec>`,
so the user's own `~/.xserverrc` and `~/.xinitrc` apply. The greeter then
exits, startx stops its X server, and greetd starts the session on the same
VT. Power actions go to logind over D-Bus, and each asks for a second press
to confirm. The last user and session are remembered; before the first
login, the session is the one AccountsService remembers from LightDM.

The password is kept in memory that is wiped once it is sent, and never
logged. If the greeter's X server cannot start, greetd's command falls back
to agreety, a text login on the same VT whose session is the user's
`startx`. If the greeter itself fails, greetd restarts.

## Install

Build it, then run the install script as root from the repository:

    cargo build --release
    sudo ./install.sh target/release/gpui-greeter ~/Pictures/Wallpapers

This installs greetd if needed, the greeter as
`/usr/local/bin/gpui-greeter`, its settings as
`/etc/gpui-greeter/config.toml` (kept if present), its home
`/var/lib/gpui-greeter` (through `/etc/tmpfiles.d/gpui-greeter.conf`) and
greetd's settings as `/etc/greetd/config.toml` (the original is kept as
`config.toml.orig`), and copies the wallpapers.

To see it on the real screen before switching, `sudo systemctl start
greetd` shows it on VT 1 without logging in there; Ctrl+Alt+F7 returns to
the running session and `sudo systemctl stop greetd` ends the trial.

    sudo ./install.sh switch

makes greetd the display manager from the next boot. LightDM stays
installed, and

    sudo ./install.sh switch-back

makes it the display manager again from the next boot. To switch at once,
log in on a text console (Ctrl+Alt+F2) and run
`sudo systemctl stop lightdm && sudo systemctl start greetd`.

The greeter's log, startx's and the X server's are in
`journalctl -t gpui-greeter`, Xorg's log in
`/var/lib/gpui-greeter/.local/share/xorg/`.

## Wallpapers

The greeter cannot read anyone's home, so Frost's pictures are copied to
`/usr/local/share/gpui-greeter/wallpapers`; copy them again whenever the
folder changes:

    sudo ./install.sh wallpapers ~/Pictures/Wallpapers

It mirrors the folder: new pictures are added, changed ones replaced,
removed ones removed, and a picture saved twice is copied once. Every picture
in the folder is copied, so keep only wallpapers there.

With `pick = "bing"`, Frost shows Bing's picture of the day: the greeter
asks Bing's archive for it in the background, shows it as soon as it
arrives and keeps the last week's in
`/var/lib/gpui-greeter/.cache/gpui-greeter/bing`.

## Settings

`dist/config.toml` lists every setting with its default: the theme, the
language, each theme's accent, Frost's blur, dim and idle time, the
wallpaper folder and pick, and how sessions start.

## Trying it out

`scripts/try` runs the greeter in an invisible X server (Xvfb, drawn by
lavapipe) against a fake greetd that accepts the password `test` and only
prints the session it would start. Power buttons only log.

    cargo build --bins --examples
    scripts/try start frost --monitors 2
    scripts/try type test; scripts/try key Return
    scripts/try shot frost-login
    scripts/try log
    scripts/try stop

`cargo test` covers the parts without a screen: the greetd conversation,
desktop entries, users, the wallpaper copy, Bing's cache and the settings.

## Fonts

Inter, Fraunces and JetBrains Mono are built in, subset to Latin and the
symbols the looks use, under the SIL Open Font License (`assets/fonts`).
Chinese text uses the system's Noto Sans CJK SC.

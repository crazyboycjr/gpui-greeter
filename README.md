# gpui-greeter

A greeter for [greetd](https://git.sr.ht/~kennylevinsen/greetd) on X11,
drawn with [GPUI](https://www.gpui.rs) and longbridge's
[gpui-kit](https://github.com/longbridge/gpui-kit). It comes in three looks,
chosen in its settings:

- **Paper**: a large Fraunces clock on paper, the name and an underlined
  password field beneath it, the session at the bottom left and Sleep,
  Restart and Shut down at the bottom right. The paper is a sheet on a
  wooden desk, among a pencil, a fountain pen, a cup of coffee and a
  succulent; ledger paper; a drafting sheet, whose title block holds the
  session and the power actions on F10, F11 and F12; or plain. Paper Night
  is the same on a walnut desk or a dark ground.
- **Frost**: a wallpaper, sharp, with a big clock. A click or a key blurs
  and dims it behind a round avatar, a frosted password pill, frosted power
  buttons and a session chip.
- **Neon**: a terminal over a city at night, a circuit board with light
  running along its traces, a synthwave sunset over a moving grid floor, or
  a faint grid, all with scanlines and a slow scan beam; a status line that
  goes READY, AUTH, then GRANTED or DENIED, and F-key buttons.

F9 switches between them at the login screen.

It speaks English and Chinese (`language = "zh-CN"`); another language is
one more table in `src/i18n.rs`.

Other looks come as theme folders, which live outside the package and can be
designed with an agent; see [Theme folders](#theme-folders).

## How it fits together

greetd runs the greeter as its `greeter` user through `startx`, in an X
server of its own on VT 1. The greeter opens a window over every monitor
and lays each one out as densely as a native interface: one of the
mockups' pixels is 1.75 pixels of a 3840×2160 monitor, and in proportion
on others, 0.875 on 1920×1080. Each look keeps its parts against the
window's edges and its middle, as its mockup does, so it holds on any
shape, and `scale` in the settings makes every look larger or denser. The
login form is on the primary monitor (or the one under the pointer) and
follows the pointer to another one. Nothing in it names a monitor, a port
or a mode.

On Enter it asks greetd to log the user in, and once PAM accepts, to start
the chosen X session the way tuigreet does, `startx /usr/bin/env <Exec>`,
so the user's own `~/.xserverrc` and `~/.xinitrc` apply. The greeter then
exits at once, startx stops its X server, and greetd starts the session on
the same VT. Power actions go to logind over D-Bus, and each asks for a
second press to confirm. The last user and session are remembered; before
the first login, the session is the one AccountsService remembers from
LightDM.

While another VT is in front, such as a text console, the greeter takes
its windows off the screen and draws nothing until its
own VT is back. logind's view of the greeter's session says when, or the
kernel's active console where logind cannot.

The password is kept in memory that is wiped once it is sent, and never
logged. If the greeter's X server cannot start, greetd's command falls back
to agreety, a text login on the same VT whose session is the user's
`startx`. If the greeter itself fails, greetd restarts.

## Install

On Arch Linux, build the package from this repository and install it:

    cd pkg/arch
    makepkg -si

It installs the greeter as `/usr/bin/gpui-greeter`, its settings as
`/etc/gpui-greeter/config.toml`, its home `/var/lib/gpui-greeter`, and
greetd's settings for it as `/etc/greetd/gpui-greeter.toml`. greetd's own
`/etc/greetd/config.toml` stays as it is: a drop-in for greetd.service
points greetd at the new file.

Run one display manager at a time. Starting greetd while another display
manager's session is running starts a second X server beside that
session's, and the two can fight over the GPU: on NVIDIA the session's X
server lost the GPU to the greeter's and ended, taking the session with
it. To look at a theme folder without any of that, `gpui-greeter preview
DIR` shows it in a window of the running session (see Theme folders).

    sudo gpui-greeter-switch greetd

makes greetd the display manager from the next boot; nothing starts or
stops before then. LightDM stays installed, and

    sudo gpui-greeter-switch lightdm

makes it the display manager again from the next boot. To switch at once,
log out first, since stopping LightDM ends the session it started; then
log in on a text console (Ctrl+Alt+F2) and run
`sudo systemctl stop lightdm && sudo systemctl start greetd`. Going back
is the same with the two swapped, after logging out of any session greetd
started.

The session greetd starts runs on VT 1, where the login screen was, so
Ctrl+Alt+F1 returns to it from a text console (LightDM's ran on VT 7).
Stopping or restarting greetd ends that session.

The greeter's log, startx's and the X server's are in
`journalctl -t gpui-greeter`, Xorg's log in
`/var/lib/gpui-greeter/.local/share/xorg/`.

## Wallpapers

The greeter cannot read anyone's home, so Frost's pictures are copied to
`/usr/local/share/gpui-greeter/wallpapers`; copy them again whenever the
folder changes:

    sudo gpui-greeter sync-wallpapers ~/Pictures/Wallpapers

It mirrors the folder: new pictures are added, changed ones replaced,
removed ones removed, and a picture saved twice is copied once. Every picture
in the folder is copied, so keep only wallpapers there.

With `pick = "bing"`, Frost shows Bing's picture of the day: the greeter
asks Bing's archive for it in the background, shows it as soon as it
arrives and keeps the last week's in
`/var/lib/gpui-greeter/.cache/gpui-greeter/bing`.

## Settings

`dist/config.toml` lists every setting with its default: the theme, the
language, the scale, each theme's accent and background, Frost's blur, dim
and idle time, the wallpaper folder and pick, and how sessions start.

`scale`, from 0.5 to 1.5, multiplies every size of every look, the theme
folders' too: above 1 for a larger login screen, below 1 for a denser one.

Paper's and Neon's backgrounds are drawn from the mockups' SVG (with
resvg) at each monitor's size, and kept in
`/var/lib/gpui-greeter/.cache/gpui-greeter/scenery`, one picture for each
size, so that later starts read them back instead of drawing them again;
what moves in them is drawn over the picture.

## Theme folders

Besides its own looks, the greeter draws theme folders: a `theme.toml` of
layers on a 3840 by 2160 frame, with the pictures, SVGs and fonts they draw,
kept outside the package in `/usr/local/share/gpui-greeter/themes`, one
folder per theme. `theme = "<name>"` selects one. Layers anchor to the
screen's edges or its centre, change with the state of the login, and can
move; text fills in the time, the user, the session, PAM's words and facts
about the machine, read from files: a theme never runs anything.
[The format](skills/gpui-greeter-theme/reference.md) is documented beside an
example theme, [dial](skills/gpui-greeter-theme/dial).

    gpui-greeter check-theme DIR
    gpui-greeter preview DIR --state error --lang zh-CN --shot error.png
    sudo gpui-greeter install-theme DIR --select

`check-theme` names what is wrong by file and line. `preview` shows the
theme against a fake greetd; with `--shot` it needs no display, drawing in
an Xvfb of its own with lavapipe. `install-theme` checks the folder, copies
its files, owned by root and readable by the greeter, and `--select` sets
`theme` in the settings. A theme that cannot be read gives way to Paper,
and `journalctl -t gpui-greeter` says why.

At the login screen, F9 switches to the next look and Shift+F9 back: the
built-in looks, then the installed theme folders by name, or the list in
`[themes] cycle`. Each look also shows a quiet control for it beside its
session. The greeter keeps the choice in its own state, under
`/var/lib/gpui-greeter`, and starts with it next time, until the settings'
`theme` changes.

### Designing a theme with an agent

The package comes with a skill that walks an agent such as Claude Code
through designing a theme with you: drawing out the idea, gathering fonts
and pictures lawfully, previewing every state, and installing it. Add it to
Claude Code with

    mkdir -p ~/.claude/skills
    ln -s /usr/share/gpui-greeter/skills/gpui-greeter-theme ~/.claude/skills/

and ask it for a new login screen.

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

`scripts/try start neon --background circuit` picks a background,
`scripts/try start path/to/theme` runs a theme folder,
`scripts/try vt away` and `scripts/try vt back` play the greeter's VT
switched to another and back, and
`cargo run --example scenery -- paper-night-desk desk.png` draws one alone.

`cargo test` covers the parts without a screen: the greetd conversation,
desktop entries, users, the wallpaper copy, Bing's cache, the settings, the
theme format and the example theme, and the backgrounds, whose skyline and
circuit board must come out of the mockup's seeds exactly as the mockup
grows them.

## Fonts

Inter, Fraunces and JetBrains Mono are built in, subset to Latin and the
symbols the looks use, under the SIL Open Font License (`assets/fonts`).
Chinese text uses the system's Noto Sans CJK SC.

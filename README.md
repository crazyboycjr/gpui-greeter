# gpui-greeter

A greeter for [greetd](https://git.sr.ht/~kennylevinsen/greetd) on X11,
drawn with [GPUI](https://www.gpui.rs) and longbridge's
[gpui-kit](https://github.com/longbridge/gpui-kit). It comes in three looks,
chosen in its settings:

- **Paper**: a large Fraunces clock on paper and beneath it the avatar,
  beside the name and an underlined password field, the session at the
  bottom left and Sleep, Restart and Shut down at the bottom right. The
  paper is a sheet on a wooden desk, among a pencil, a fountain pen, a cup
  of coffee and a succulent; ledger paper; a drafting sheet, whose title
  block holds the session and the power actions on F10, F11 and F12; or
  plain. Paper Night is the same on a walnut desk or a dark ground.
- **Frost**: macOS's lock screen and login window: a wallpaper, sharp,
  with a big clock and the date. A click or a key blurs and dims it behind
  a round avatar, the name and a password field, low on the screen as
  macOS sizes and places them, above Sleep, Restart and Shut Down, with the
  time in the top right corner and the session in the bottom left.
- **Neon**: a terminal over a city at night, a circuit board with light
  running along its traces, a synthwave sunset over a moving grid floor, or
  a faint grid, all with scanlines and a slow scan beam; a status line that
  goes READY, AUTH, then GRANTED or DENIED, and F-key buttons.

None of them says how to log in or how to wake the screen: as on a Mac,
the line under the field stays quiet until there is something to say, a
refused password or a word from PAM.

F9 switches between them at the login screen. Tab and F2 step to the next
session, Shift+Tab to the one before, in every look, as a click on the
session does.

It speaks English and Chinese (`language = "zh-CN"`); another language is
one more table in `src/i18n.rs`.

Other looks come as theme folders, which live outside the package and can be
designed with an agent; see [Theme folders](#theme-folders).

## How it fits together

greetd runs the greeter as its `greeter` user through `startx`, in an X
server of its own on VT 1. The greeter opens a window over every monitor
and lays each one out in points, as a Mac lays out its display: a Mac
draws a 27-inch display at about 109 points to the inch, so a point is a
monitor's pixels per inch over 109, from the size it reports, and never
less than a pixel. On a 27-inch 3840×2160 monitor a point is 1.5 pixels
and the screen 2560×1440 points; on a 27-inch 2560×1440 or a 24-inch
1920×1080 monitor, a point is a pixel. A monitor that reports no size
counts as 2560×1440 points fitted to its pixels. The looks are as large
as macOS's lock screen, point for point, as it is on a 27-inch display:
13-point text, a 160 by 28 point password field, a 72-point avatar and a
clock whose figures stand 122 points tall (the theme skill's
[reference](skills/gpui-greeter-theme/reference.md#macos-metrics) has
the measurements). Each look keeps its parts against the window's edges
and its middle, so it holds on any size and shape, and `scale` in the
settings makes every look larger or denser.
The login form is on the primary monitor (or the one under the pointer)
and follows the pointer to another one. Nothing in it names a monitor, a
port or a mode.

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
`/usr/local/share/gpui-greeter/wallpapers`: one by one under Wallpaper in
`gpui-greeter settings` (see [Settings](#settings)), or a whole folder at
once, again whenever it changes:

    sudo gpui-greeter sync-wallpapers ~/Pictures/Wallpapers

It mirrors the folder: new pictures are added, changed ones replaced,
removed ones removed, and a picture saved twice is copied once. Every picture
in the folder is copied, so keep only wallpapers there.

With `pick = "bing"`, Frost shows Bing's picture of the day. Bing's
pictures are kept at their full size, 3840×2160, in
`/usr/local/share/gpui-greeter/bing4k`, which the package makes for the
greeter's user and lets everyone read: each under its day,
`2026-10-29.jpg`, beside `2026-10-29.json` with its title and copyright.
The login screen asks Bing's archive for the picture of the day in the
background and shows it as soon as it arrives; until the first one has,
Frost shows one of the wallpapers. `gpui-greeter-bing.timer` does the same
once a day as the greeter's user, so the pictures gather on days without a
login screen too; the settings window turns it on when Bing is chosen and
off otherwise, or by hand:

    sudo systemctl enable --now gpui-greeter-bing.timer

The lock screen shows the newest picture there. The settings window's
Wallpaper section shows them all, by day and title, and a click on one
shows it always, written as `pick = "bing4k/2026-10-29.jpg"`. Nothing is
removed from the folder, which grows by about 3.5 MB a day; delete old
pictures there by hand when it is large enough.

## Settings

`gpui-greeter settings`, "gpui-greeter Settings" among the applications,
is a window for the settings used day to day: the look the login screen
and the lock screen start with, each look's background and accent, Frost's
wallpapers and Bing's pictures, your avatar, the size and the language. It
shows the look as the greeter draws it, small, and follows each change at
once.

    gpui-greeter settings --section wallpaper

Nothing changes until Apply. The avatar goes to AccountsService, which
lets everyone set their own. The settings file, the wallpaper folder and
the daily Bing timer are root's, so Apply runs `gpui-greeter
apply-settings` once through pkexec, and polkit asks for the password.
That helper reads the changes from its standard input, pictures included,
and checks them all before it changes anything: only the settings the
window offers, each value as the greeter reads it; new pictures that
decode, under plain names, written into the configured wallpaper folder
alone; removals of pictures that are there; a pick of one of Bing's
pictures that is there; the package's own Bing timer, only turned on or
off; and no folder that other users can write to. It keeps the settings
file's comments. Without polkit, the window gives the `sudo` command that
does the same.

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
`[themes] cycle`. Paper and Neon also show a quiet control for it beside
their session. The greeter keeps the choice in its own state, under
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

## Lock screen

`gpui-greeter lock` locks a running X session with the same looks: Paper,
Paper Night, Frost, Neon and the installed theme folders, over every
monitor. It runs as the user, in their own session, under greetd or any
other display manager.

    gpui-greeter lock

It starts at rest, the look's clock and art without the form, for a screen
to leave behind while away. A key, a click or moving the mouse more than a
nudge brings the form, and that first key types nothing, as on a Mac:
Space, Enter or Escape is as good as any. Escape takes the form away again;
F9 and Shift+F9 switch looks there too, and the lock keeps the
look picked for next time in `~/.local/state/gpui-greeter/lock.toml`,
until the settings' `theme` changes. F10 to F12 and the power buttons act
as at the login screen, a second press confirming. The user is the one who
locked, and the session the one running, which is not chosen there: the
built-in looks leave out their session controls, the session keys do
nothing, and a theme folder hides its session row with `locked` (see
[the format](skills/gpui-greeter-theme/reference.md)). Once the password
is accepted the lock is gone at once, and every window is as it was. Its
drawn backgrounds are kept in the user's own `~/.cache/gpui-greeter`.

To lock with a key, have the window manager run it. In xmonad's keys:

    , ((modm .|. shiftMask, xK_l), spawn "gpui-greeter lock")

In i3's config:

    bindsym $mod+Shift+l exec --no-startup-id gpui-greeter lock

Any other window manager's key bindings, or a key daemon such as sxhkd,
run the same command.

To lock once the screen has gone blank, let the lock wait for it in the
session, say from `~/.xinitrc` before the window manager starts:

    gpui-greeter lock --when-idle 30 &

It locks once the X screen saver has kept the screen blank for 30 seconds
(`xset s 600` blanks it after ten minutes without a key or the mouse;
`xset s blank` blanks rather than draws a pattern), so that whoever wakes
the screen finds the lock at rest with its clock, and at once on
`loginctl lock-session`; `loginctl unlock-session` ends the lock it
started. It costs nothing while it waits, starts no lock while one runs,
and the lock keeps the screen blank: nothing is drawn until a key or the
mouse wakes the screen, and the move that wakes it leaves the lock at
rest.

xss-lock does the same when the screen saver starts, and locks before a
suspend too, waiting until the lock is showing:

    xss-lock --transfer-sleep-lock -- gpui-greeter lock &

A second lock started while one runs waits for that one and ends with it.
Whatever started it, a lock takes its look off the screen while the
screen saver has the screen blank, and puts it back when it lights up.

The password is checked by PAM as the service `gpui-greeter-lock`, which
the package installs as `/etc/pam.d/gpui-greeter-lock`:

    auth       include      system-auth

so as at a login: pam_unix checks it through its setuid helper, PAM waits
a moment after a wrong one, and faillock counts the refusals and says
when the account is locked. Without that file the lock refuses to start,
since no password could end it.

### How it holds the screen

The process the command starts is the supervisor. It covers the screen
with a plain black window, out of the window manager's hands, above every
other window, docks and fullscreen ones included; it raises it again
whenever another window is mapped or raised, and covers the screen again
when the monitors change. It holds the keyboard and the pointer for as
long as the screen is locked: a window manager's key bindings do nothing,
and no other window gets a key or a click. When another program holds
them for more than two seconds, as an open menu may, the lock says so and
ends, rather than seem to lock. The supervisor checks the password with
PAM and draws nothing else.

The looks are drawn by its view, a second process, which it starts and
hands the keys and the pointer to, and which hands it back what was
typed; both wipe the password from memory once it is sent. Should the
view crash, hang or fail to start, the cover and the grabs stay, the view
starts again, and meanwhile the cover takes the password itself: a line
where it goes, a square for each character, Enter to unlock.

What it cannot do: the supervisor is what holds the screen, so if it ends,
by a bug or a signal, the X server lets go of its grabs and its cover and
the session shows, as with any X screen locker; it is kept small for that.
Programs running as the user can end it, as they can any of the user's
programs. And the X server switches to another VT on Ctrl+Alt+F2 whatever
holds the keyboard, so a text console logged in there stays usable.

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

Try the lock screen the same way, in an X server of its own (Xvfb, with
`xrandr --setmonitor` for monitors), never on the display you work on: a
build with `--features fake-pam`, which exists for tests alone and which
packages leave out, takes `gpui-greeter lock --pam-dir DIR`, where DIR
holds a fake `gpui-greeter-lock` service, so that no real password is
involved. `gpui-greeter preview DIR --lock` shows a theme folder as the
lock screen.

`cargo test` covers the parts without a screen: the greetd conversation,
the lock's PAM conversation (against libpam, with a fake service of the
tests' own), desktop entries, users, the wallpaper copy, Bing's folder
(against a web server of the tests' own), the settings and the Bing timer
(offline, with `systemctl --root`), the theme format and the example
theme, and the backgrounds,
whose skyline and circuit board must come out of the mockup's seeds
exactly as the mockup grows them.

## Fonts

Inter, Fraunces and JetBrains Mono are built in, subset to Latin and the
symbols the looks use, under the SIL Open Font License (`assets/fonts`).
Chinese text uses the system's Noto Sans CJK SC.

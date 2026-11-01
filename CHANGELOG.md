# Changelog

<!-- TODO: the date of the release, once it is tagged. -->

## 0.1.0 (unreleased)

The first release. gpui-greeter is a login screen for greetd on X11, a lock
screen for any X session, and a settings window for both, drawn with GPUI.

**The login screen.** greetd runs the greeter as its `greeter` user, in an X
server of its own that `startx` starts on VT 1, with a window over every
monitor and the form on the primary one, following the pointer to another.
Sessions come from the
desktop entries in `/usr/share/xsessions` and start the way tuigreet starts
them, `startx /usr/bin/env <Exec>`, so your own `~/.xserverrc` and
`~/.xinitrc` apply. The session starts the moment PAM accepts the password,
with no welcome to wait through. The last user and session are remembered,
and before the first login the session is the one LightDM left with
AccountsService. Power actions go to logind, each confirmed by a second
press. If the greeter's X server cannot start, agreety offers a text login on
the same VT. The password is wiped from memory once it is sent, and never
logged.

**Three looks.** Paper, a large Fraunces clock on warm paper, on a desk,
ledger paper, a drafting sheet or plain, with Paper Night on a walnut desk or
a dark ground, each in an accent of your choosing. Frost, macOS's lock screen and login
window over a wallpaper that blurs and dims behind the form. Neon, a terminal
over a drawn city at night, a circuit board, a synthwave horizon or a grid,
with scanlines and a scan beam. F9 and Shift+F9 switch looks, Tab, Shift+Tab
and F2 the session, F10 to F12 the power actions. No look says how to log in:
the line under the field stays quiet until there is something to say.

**Sizes and motion.** The looks are laid out in points, as a Mac lays out a
display, from each monitor's own pixels per inch, at macOS's sizes: a 160 by
28 point field, a 72-point avatar, round password dots, and a refused password
shaken in three small swings. `scale` makes every look larger or denser. Each
window draws at its own monitor's refresh rate and follows it as monitors
sleep, wake or change modes, and nothing is drawn while another VT is in
front.

**Theme folders.** Other looks are folders of a `theme.toml`, pictures, SVGs
and fonts, laid out on a 3840 by 2160 frame and anchored to the screen's
edges, with states, transitions, motions, slideshows, xmonad-like panes, and
text that fills in the time, the user, the session, PAM's words and facts
about the machine, read from files. `check-theme` names every problem by file
and line, `preview --shot` draws a theme without a display, and
`install-theme` installs it. A theme that cannot be read gives way to Paper.
The package carries a skill for designing themes with a coding agent, and an
example theme, Dial.

**The lock screen.** `gpui-greeter lock` locks a running X session with the
same looks, at rest until a key, the first key only waking it. A small
supervisor holds the keyboard, the pointer and a cover and checks the
password with PAM (the service `gpui-greeter-lock`), while the looks are drawn
by a second process that can start again without showing the session.
`--when-idle` locks once the screen saver has kept the screen blank a while,
and on `loginctl lock-session`. The lock asks compositors to keep compositing
it, so it never flips into variable refresh.

**The settings window.** `gpui-greeter settings` changes the look, each
look's background and accent, Frost's wallpapers and Bing's picture of the
day, the avatar (through AccountsService), the size and the language, with a
live preview of the look. What root keeps changes through one run of
`gpui-greeter apply-settings` under pkexec, which accepts a closed set of
changes and checks them all first.

**Wallpapers.** `gpui-greeter sync-wallpapers` mirrors a folder of
wallpapers where the greeter can read them. With `pick = "bing"`, Frost shows
Bing's picture of the day, kept at 3840×2160 in a folder that a daily timer
fills.

**Languages.** English and Chinese, in every look, the lock screen and the
settings window.

**Packaging.** A package for Arch Linux in `packaging/arch`, a flake with a
NixOS module, and `gpui-greeter-switch`, which makes greetd or LightDM the
display manager from the next boot.

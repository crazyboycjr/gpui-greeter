<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/images/header-dark.png">
    <img src="docs/images/header-light.png" width="880" alt="gpui-greeter: a login and lock screen for X11, drawn on the GPU with GPUI">
  </picture>
</p>

<p align="center">
  A greeter for greetd on X11, with a lock screen and a settings window, drawn with GPUI.
</p>

<p align="center">
  <a href="https://crazyboycjr.github.io/gpui-greeter/">Homepage</a> ·
  <a href="#install">Install</a> ·
  <a href="#three-looks">Looks</a> ·
  <a href="#theme-folders">Theme folders</a> ·
  <a href="#lock-screen">Lock screen</a> ·
  <a href="#settings">Settings</a> ·
  <a href="CHANGELOG.md">Changes</a>
</p>

<p><img src="docs/images/readme-frost.webp" width="100%" alt="Frost at rest: a mountain lake at sunset with the date and a large clock reading 09:41; a key blurs and dims the picture behind a round avatar, the name alice and a password field, seven dots are typed, the refused password shakes the field and says so, and Escape brings back the clock"></p>

## Why I wrote this

The login screen is the first thing my computer shows me every day, and on
X11 none of them looked like they belonged there. LightDM's GTK greeter looks
dated, the web greeter that followed lightdm-webkit died along with PyQt5, and
SDDM and the rest are not what I would call fancy. The Mac beside it,
meanwhile, greets me with a blurred wallpaper, a big clock, and a field that
shakes when I mistype.

So I wrote my own: a greeter for greetd on X11, drawn with GPUI, the GPU UI
framework of the Zed editor, and held to the polish of macOS's login and lock
screens. Its Frost look is measured against Apple's own pictures of them,
point for point. It grew two more looks of its own, a lock screen that shares
them, a settings window, and theme folders that you, or a coding agent, can
design.

If you run X11 and would like a login screen you are glad to see, read on.

## Three looks

Paper, Frost and Neon come built in, each with backgrounds or a wallpaper of
its own, and F9 switches between them.

### Paper

A large Fraunces clock on warm paper, and beneath it the avatar, the name and
an underlined password field. The paper is a sheet on a wooden desk among a
pencil, a fountain pen, a cup of coffee and a succulent; ledger paper; a
drafting sheet whose title block holds the session and the power actions; or
plain. Paper Night is the same on a walnut desk or a dark ground, and each
takes an accent.

<p><img src="docs/images/paper.webp" width="100%" alt="Paper on the desk: the clock 09:41 in Fraunces and the date on a sheet of paper, alice's avatar beside the name and a password field with seven dots, the session at the bottom left and Sleep, Restart and Shut down at the bottom right; on the wooden desk to the right a cup of coffee, a pencil, a fountain pen, a sticky note and a succulent"></p>

<p><img src="docs/images/paper-backgrounds.webp" width="100%" alt="Four more Paper backgrounds: ledger paper with blue rules and a blue accent, a drafting sheet with its title block, Paper Night on a walnut desk, and a drafting sheet at night"></p>

### Frost

macOS's lock screen and login window. At rest, the wallpaper, sharp, with a
large clock and the date; a key or a click blurs and dims it behind a round
avatar, the name and a password field, set low as macOS sets them, above
Sleep, Restart and Shut Down. The sizes are macOS's, point for point: a
160 by 28 point field, a 72-point avatar, round password dots set as close as
Apple's, and a refused password shakes the field in three small swings.

<p><img src="docs/images/frost-form.webp" width="100%" alt="Frost's form at full size: the avatar in its round frame, the name alice, a password field holding seven round dots, and Sleep, Restart and Shut Down under it, over a blurred sunset"></p>

### Neon

A terminal over a city at night, a circuit board with light running along its
traces, a synthwave sunset over a moving grid floor, or a faint grid, all with
scanlines and a slow scan beam; a status line that goes READY, AUTH, then
GRANTED or DENIED, and F-key buttons.

<p><img src="docs/images/neon.webp" width="100%" alt="Neon over a synthwave sunset: the host name example-host in large type over a terminal panel with the user alice and a typed password, a moving grid floor below and the time at the top right"></p>

<p><img src="docs/images/neon-backgrounds.webp" width="100%" alt="Neon's panel over two more of its backgrounds: a city at night with its windows lit, and a circuit board whose traces carry pulses of light"></p>

### Switching looks

F9 switches to the next look at the login screen and at the lock screen, and
Shift+F9 to the one before: the built-in looks, then the theme folders you
have installed. The greeter starts with the one picked next time. Tab picks
the session, F10 to F12 sleep, restart and shut down after a second press,
and Escape clears the field or goes back to rest.

<p><img src="docs/images/f9.webp" width="100%" alt="F9 pressed five times at the login screen: Paper on its desk, Paper Night, Frost over a mountain lake, Neon over a city at night, then the example theme Dial, and back to Paper"></p>

None of the looks says how to log in or how to wake the screen: as on a Mac,
the line under the field stays quiet until there is something to say, a
refused password or a word from PAM.

## Theme folders

Other looks come as theme folders: a `theme.toml` of layers on a 3840 by 2160
frame, with the pictures, SVGs and fonts they draw, anchored to the screen's
edges and centre so they hold on any monitor. Layers change with the state of
the login, can move, and fill in the time, the user, the session, what PAM
says and facts about the machine, read from files: a theme never runs
anything. They live outside the package, in
`/usr/local/share/gpui-greeter/themes`, and every one is a lock screen too.

<p><img src="docs/images/dial-pair.webp" width="100%" alt="Dial, the example theme: a ring of sixty ticks around a serif clock with one tick lit in amber, at rest; then signing in, with the name and a password field under the ring and the power actions at the bottom right"></p>

```sh
gpui-greeter check-theme DIR
gpui-greeter preview DIR --state error --lang zh-CN --shot error.png
sudo gpui-greeter install-theme DIR --select
```

`check-theme` names what is wrong by file and line, and a theme that fails it
is never drawn: the greeter shows Paper instead. `preview --shot` draws the
theme in an X server of its own with no display needed. [The format](skills/gpui-greeter-theme/reference.md)
is documented beside an example theme, [dial](skills/gpui-greeter-theme/dial),
and [docs/themes.md](docs/themes.md) has the rest.

**Design one with an agent.** The package comes with a skill that walks an
agent such as Claude Code through designing a theme with you: drawing out the
idea, gathering fonts and pictures lawfully, previewing every state, and
installing it. Add it to Claude Code with

```sh
mkdir -p ~/.claude/skills
ln -s /usr/share/gpui-greeter/skills/gpui-greeter-theme ~/.claude/skills/
```

and ask it for a new login screen. Your themes stay on your machine.

## Lock screen

`gpui-greeter lock` locks a running X session with the same looks, over every
monitor, under greetd or any other display manager. It starts at rest, the
look's clock and art without the form; the first key only wakes it, and once
the password is accepted every window is as it was.

<p><img src="docs/images/lock.webp" width="100%" alt="The lock screen at rest in three looks: Paper Night's clock on a walnut desk, Neon's host name over a city at night marked LOCKED, and Dial's ring of ticks"></p>

```sh
gpui-greeter lock                    # from a key of your window manager's
gpui-greeter lock --when-idle 30 &   # in ~/.xinitrc: lock once the screen has blanked
```

A small supervisor holds the keyboard, the pointer and a black cover, and
checks the password with PAM; the looks are drawn by a second process, which
can crash and start again without ever showing the session. The password is
checked as the PAM service `gpui-greeter-lock`, which the package installs.
[docs/lock-screen.md](docs/lock-screen.md) covers key bindings, xss-lock and
how it holds the screen.

## Settings

`gpui-greeter settings` is a window for the settings used day to day: the
look to start with, each look's background and accent, Frost's wallpapers and
Bing's picture of the day, your avatar, the size and the language. It shows
the look as the greeter draws it, small, and follows each change at once.

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/images/settings-dark.webp">
  <img src="docs/images/settings.webp" width="100%" alt="The settings window in warm paper tones: Look, Wallpaper, Avatar and General in a sidebar; a live preview of Frost signing in over the sunset wallpaper; the looks to choose from, and Frost's blur, dim and the seconds before the clock comes back">
</picture>

Nothing changes until Apply. Your avatar goes to AccountsService; what root
keeps goes through one `pkexec gpui-greeter apply-settings`, which accepts
only the settings the window offers and checks them all first.
[docs/settings.md](docs/settings.md) has the details.

## Made for a real desk

- **Native density.** The looks are laid out in points, as a Mac lays out a
  display, from each monitor's own pixels per inch: as large on a 27-inch 4K
  monitor as on a 24-inch 1080p one, and as large as macOS's lock screen.
- **Every monitor.** A window over each one, and the form on the primary one,
  following the pointer to another. Nothing is tied to a monitor, a port or a
  mode.
- **Smooth.** Each window draws at its own monitor's refresh rate, 240 Hz
  included, and follows it as monitors sleep, wake and change modes.
- **Instant.** The session starts the moment PAM accepts the password, with
  no welcome to wait through.
- **English and 中文.** `language = "zh-CN"` for Chinese, in every look, the
  lock screen and the settings window.

<p><img src="docs/images/zh.webp" width="100%" alt="The Chinese login screen: Frost at rest with the date 10月31日 星期六 over the lake at sunset, and Paper signing in with the date and the Chinese session and power labels"></p>

## Install

gpui-greeter runs on X11, as the login screen of
[greetd](https://git.sr.ht/~kennylevinsen/greetd). It needs a Vulkan driver,
an X server with `startx`, PAM and systemd.

On Arch Linux, from the AUR:

```sh
paru -S gpui-greeter
```

Or from this repository, which builds what is on `main`:

```sh
git clone https://github.com/crazyboycjr/gpui-greeter
cd gpui-greeter/packaging/arch
makepkg -si
```

On NixOS, the flake's module sets up greetd with the greeter
([docs/install.md](docs/install.md#nixos)):

<!-- TODO: check against the packaging branch's flake.nix and module.nix. -->

```nix
imports = [ gpui-greeter.nixosModules.default ];
services.gpui-greeter.enable = true;
```

### Switching to greetd

Run one display manager at a time: starting greetd while another display
manager's session runs starts a second X server beside it, and on NVIDIA that
session's X server lost the GPU and ended, taking the session with it.

```sh
sudo gpui-greeter-switch greetd    # greetd from the next boot
sudo gpui-greeter-switch lightdm   # and back
```

Nothing starts or stops until the next boot. To switch at once, log out, log
in on a text console (Ctrl+Alt+F2) and run
`sudo systemctl stop lightdm && sudo systemctl start greetd`. The session
greetd starts runs on VT 1, where the login screen was, so Ctrl+Alt+F1 returns
to it from a text console; stopping or restarting greetd ends it. The logs
are in `journalctl -t gpui-greeter`, and if the greeter's X server cannot
start, agreety offers a text login on the same VT.
[docs/install.md](docs/install.md) has what the package installs and the rest.

## Configuration

The settings are `/etc/gpui-greeter/config.toml`, which the package installs
with every key and its default: the look, the language, the size, each look's
background and accent, Frost's blur, dim and idle time, the wallpaper folder
and pick, and how sessions start.

```sh
sudo gpui-greeter sync-wallpapers ~/Pictures/Wallpapers
```

copies a folder of wallpapers where the greeter can read them, for Frost; with
`pick = "bing"` it shows Bing's picture of the day instead.
[docs/configuration.md](docs/configuration.md) explains each setting.

## Built with Rust and GPUI

gpui-greeter is written in Rust on [GPUI](https://www.gpui.rs), the GPU UI
framework of the Zed editor, through longbridge's
[gpui-kit](https://github.com/longbridge/gpui-kit), and draws with Vulkan. It
speaks to greetd with [greetd_ipc](https://crates.io/crates/greetd_ipc), to X
with [x11rb](https://crates.io/crates/x11rb), to logind and AccountsService
with [zbus](https://crates.io/crates/zbus), and draws its SVGs with
[resvg](https://crates.io/crates/resvg). How it fits together, trying it in an
invisible X server with `scripts/try`, and the tests are in
[docs/development.md](docs/development.md).

## License

<!-- TODO: the licence is the author's to choose. -->

The licence is not chosen yet.

The fonts built in, Inter, Fraunces and JetBrains Mono, and the example
theme's Noto Serif Display are under the SIL Open Font License. The
screenshots show a test account, alice, on a machine called example-host,
with an avatar drawn for them. Frost's wallpapers in them are five
photographs by Sergei Akulich, Robert Magnusson, Peter Winckler, adrian and
Ales Krivec, released into the public domain (CC0) and kept on Wikimedia
Commons; [docs/images/SOURCES.md](docs/images/SOURCES.md) links each.

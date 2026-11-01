# Configuration

The login screen reads `/etc/gpui-greeter/config.toml`, and the lock screen
the same file. Every key may be left out; `dist/config.toml`, installed as
that file, lists each with its default. The settings window,
`gpui-greeter settings`, changes the ones used day to day
([the settings window](settings.md)).

```toml
# "paper", "paper-night", "frost" or "neon", or the name of a theme folder.
theme = "paper"

# "en" or "zh-CN".
language = "en"

# Every look's sizes multiplied, from 0.5 to 1.5.
scale = 1.0

[paper]
accent = "#B0612E"
background = "desk"          # "desk", "ledger", "drafting" or "plain"

[frost]
blur = 30                    # 8 to 48
dim = 0.28                   # 0 to 0.6
idle_seconds = 60            # before the form gives way to the clock

[neon]
accent = "#45F0FF"
scanlines = true
background = "skyline"       # "skyline", "circuit", "horizon" or "grid"

[wallpaper]
dir = "/usr/local/share/gpui-greeter/wallpapers"
pick = "random"              # "random", "daily", "bing", or a file's name
bing_url = "https://www.bing.com/HPImageArchive.aspx?format=js&idx=0&n=1"
bing_dir = "/usr/local/share/gpui-greeter/bing4k"

[sessions]
dir = "/usr/share/xsessions"
wrapper = "startx /usr/bin/env"

[themes]
dir = "/usr/local/share/gpui-greeter/themes"
cycle = []                   # what F9 steps through, in order
```

## The looks

`theme` picks the look the login screen and the lock screen start with:
Paper, Paper Night (`paper-night`), Frost, Neon, or a theme folder by its
name ([theme folders](themes.md)). At the login screen F9 and Shift+F9
switch to the next look and the one before, and the greeter starts with
the one picked there next time, until `theme` changes.

**Paper** draws a large Fraunces clock on paper, with the avatar beside
the name and an underlined password field beneath it, the session at the
bottom left and Sleep, Restart and Shut down at the bottom right. Its
`background` is a sheet on a wooden desk, among a pencil, a fountain pen,
a cup of coffee and a succulent (`desk`); ledger paper with blue rules and
a red margin (`ledger`); a drafting sheet whose title block holds the
session and the power actions on F10, F11 and F12 (`drafting`); or plain
paper (`plain`). Paper Night is the same on a walnut desk or a dark
ground. The `accent` marks the password field's caret and underline, the
session's mark and the controls under the pointer; the settings window
offers `#B0612E`, `#3E6E8E`, `#5B7F4F` and `#8A5BA6`.

**Frost** is macOS's lock screen and login window: the wallpaper, sharp,
with a big clock and the date; a key or a click blurs and dims it behind
the avatar, the name and the password field. `blur` and `dim` set how
much, and `idle_seconds` how long the form waits for a key before the
clock comes back.

**Neon** is a terminal over a drawn background: a city at night
(`skyline`), a circuit board with light running along its traces
(`circuit`), a synthwave sunset over a moving grid floor (`horizon`), or a
faint grid (`grid`), with scanlines and a slow scan beam. Its `accent`
lights the panel; the settings window offers `#45F0FF`, `#9CFF5A`,
`#FFB547` and `#FF4FD8`.

## Size

The looks are laid out in points, as a Mac lays out a display: a point is
a monitor's pixels per inch over 109, from the size the monitor reports,
and never less than a pixel. On a 27-inch 3840×2160 monitor a point is 1.5
pixels and the screen 2560×1440 points; on a 27-inch 2560×1440 or a
24-inch 1920×1080 monitor, a point is a pixel. A monitor that reports no
size counts as 2560×1440 points fitted to its pixels. At a `scale` of 1
the looks are as large as macOS's lock screen, point for point, on a
27-inch display: 13-point text, a 160 by 28 point password field and a
72-point avatar (the theme skill's
[reference](../skills/gpui-greeter-theme/reference.md#macos-metrics) has
the measurements).

`scale`, from 0.5 to 1.5, multiplies every size of every look, the theme
folders' too: above 1 for a larger login screen, below 1 for a denser one.

## Wallpapers

The greeter cannot read anyone's home, so Frost's pictures are copied to
`/usr/local/share/gpui-greeter/wallpapers`: one by one under Wallpaper in
the settings window, or a whole folder at once, again whenever it changes:

```sh
sudo gpui-greeter sync-wallpapers ~/Pictures/Wallpapers
```

It mirrors the folder: new pictures are added, changed ones replaced,
removed ones removed, and a picture saved twice is copied once. Every
picture in the folder is copied, so keep only wallpapers there.

`pick` chooses among them: `random` picks one at each start, `daily` one
a day, and a file's name always that picture.

## Bing's picture of the day

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

```sh
sudo systemctl enable --now gpui-greeter-bing.timer
```

The lock screen shows the newest picture there. The settings window's
Wallpaper section shows them all, by day and title, and a click on one
shows it always, written as `pick = "bing4k/2026-10-29.jpg"`. Nothing is
removed from the folder, which grows by about 3.5 MB a day; delete old
pictures there by hand when it is large enough.

## Sessions

The sessions to choose from are the X sessions in `[sessions] dir`, by
their desktop entries' names, and each starts the way tuigreet starts one,
as `startx /usr/bin/env <Exec>`, so your own `~/.xserverrc` and
`~/.xinitrc` apply. The last user and session are remembered; before the
first login, the session is the one AccountsService remembers from
LightDM. Tab and F2 step to the next session, Shift+Tab to the one before,
in every look, as a click on the session does.

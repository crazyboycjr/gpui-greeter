---
name: gpui-greeter-theme
description: Design, check, preview and install a theme for gpui-greeter, the greetd login screen for X11. A theme is a folder of theme.toml, pictures, SVGs and fonts laid out on a 3840x2160 frame, kept on the user's own machine. Use when someone wants a new look for their gpui-greeter login screen, wants to turn a mockup or an idea into one, or wants to change, preview, install or undo one.
---

# Designing a gpui-greeter theme

A gpui-greeter theme is a folder the user keeps on their own machine:
`theme.toml` describes layers on a 3840 by 2160 frame, and the pictures,
SVGs and fonts it draws sit beside it. Installed, it lives in
`/usr/local/share/gpui-greeter/themes/<name>/`, outside the package, and
`theme = "<name>"` in `/etc/gpui-greeter/config.toml` selects it.

Read `reference.md`, beside this file, before writing theme.toml: it is
the format's only documentation. `dial/`, beside it too, is a complete,
small theme to start from and to imitate.

The greeter never runs anything a theme says, and a theme that fails its
checks is never drawn: the greeter shows its built-in Paper look instead.
So a mistake can't lock anyone out, but check and preview before
installing all the same.

## 1. Draw out the idea

Ask before building. Keep the questions few and concrete, and offer
choices the user can react to:

- The mood and the references: a screenshot, a desktop they like, a game,
  a colour scheme, a wallpaper they want to see.
- What sits where: the clock, the name and the password field, the session
  choice, the power actions, anything else (a fetch block, a logo, a
  status bar). Show a rough sketch in words or ASCII before the details.
- Which states should look different: a rest screen before the first key
  (`rest = true`), typing, checking, a refused password, the welcome.
- Motion: what moves, and when. Less is usually better on a login screen.
- Languages: English, Chinese, or both (`[strings.en]`, `[strings.zh-CN]`).

Agree on the design with the user before writing much theme.toml. If they
bring a mockup drawn for a 1600 by 900 frame, every length becomes 2.4
times as large.

## 2. Gather the files lawfully

- **Fonts:** prefer fonts under the SIL Open Font License or Apache 2.0.
  Bundle the font files and their licence text in the theme. A variable
  font draws at its default instance; to use a setting such as weight 900
  or a "soft" axis, make a static instance with fontTools
  (`fonttools varLib.instancer Font.ttf wght=900 SOFT=100 -o Font-Soft-Black.ttf`).
  A font with a Reserved Font Name must keep its name only if unmodified, so
  subset or instance only fonts without one, or rename the result.
- **Logos and artwork:** an official logo comes from its project's own
  site or press kit, under that project's terms; fan art and wallpapers
  only with their author's licence or the user's own rights. Such files
  stay in the user's own theme folder: they never go into a public
  repository or a package.
- **Record every source** in a `SOURCES.md` in the theme folder: each
  file, where it came from (the URL), its licence or terms, and the date.
- Prefer SVG for marks and drawings. One-colour marks work best with
  `tint = true`, which lets states recolour and turn them; SVGs must embed
  any pictures as `data:` URLs and have text converted to paths.
- Never fetch anything at login: everything the theme draws is a file in
  its folder.

## 3. Design on the 4K frame

Work in the user's own folder, for example
`~/.local/share/gpui-greeter/themes-src/<name>/`, where `<name>` is lower
case, digits, `-` or `_`. Starting from a copy of `dial/` saves typing:

```sh
mkdir -p ~/.local/share/gpui-greeter/themes-src
cp -r /usr/share/gpui-greeter/skills/gpui-greeter-theme/dial ~/.local/share/gpui-greeter/themes-src/<name>
```

Place layers against the screen's edges or centre (`anchor`), not at
absolute positions, so the design holds on 16:10, 21:9 and 1080p monitors.
Group what belongs together in row and column boxes, so text of any length
or language lines up. Give each state its look with `[layer.on.<state>]`
tables, and keep transitions short (150 to 500 ms).

## 4. Check it

```sh
gpui-greeter check-theme ~/.local/share/gpui-greeter/themes-src/<name>
```

It reads the folder as the greeter would and names every problem by file
and line. Fix all errors; read the warnings (fonts that are not bundled,
strings without a translation) and fix those that matter.

## 5. Preview every state, and look

`preview` with `--shot` needs no display and no GPU: it starts an X server
of its own and draws on the CPU. Take each state in each language, at 4K
first:

```sh
dir=~/.local/share/gpui-greeter/themes-src/<name>
out=~/.cache/gpui-greeter-shots/<name>; mkdir -p "$out"
for lang in en zh-CN; do
  for state in rest idle typing checking error welcome; do
    gpui-greeter preview "$dir" --state $state --lang $lang \
      --user alice --host example-host --shot "$out/$state-$lang.png"
  done
done
```

Then look at every screenshot, and compare it with the design: spacing,
alignment, sizes, colours, what each state shows. Check other shapes too
(`--size 2560x1440`, `--size 1920x1200`, `--size 3440x1440`). A state's
transitions run before the screenshot; to catch the middle of one, use
`--at MS`, for example the welcome at `--at 600`. Iterate until it matches.
Drop `--user` and `--host` to see the user's own names.

Without `--shot`, `preview` opens a window on the current display, where the
user can type (the password is `test`), press the theme's keys and watch it
move. Only do that when the user asks to see it live.

## 6. Install it

```sh
sudo gpui-greeter install-theme ~/.local/share/gpui-greeter/themes-src/<name> --select
```

It checks the folder again, copies it to
`/usr/local/share/gpui-greeter/themes/<name>/` owned by root and readable by
the greeter, and `--select` sets `theme = "<name>"` in the settings,
keeping their comments. Without `--select` it says how to pick it. If you
cannot run sudo, give the user that exact command to run themselves.
Install again after every change; the greeter only reads the installed
copy.

## 7. Try it safely, and know how to undo

- The installed theme shows the next time the greeter does: at the next
  logout or boot. Until then, `preview` of the installed folder is the
  closest look: `gpui-greeter check-theme /usr/local/share/gpui-greeter/themes/<name>`
  and the same `preview` commands.
- If greetd is not yet the display manager, `sudo systemctl start greetd`
  shows the greeter on VT 1 without logging in there; Ctrl+Alt+F7 returns
  to the running session and `sudo systemctl stop greetd` ends the trial.
  Never restart greetd while someone is logged in through it: that ends
  their session.
- If the theme cannot be loaded the greeter shows Paper, and
  `journalctl -t gpui-greeter` says why.
- To undo, set `theme = "paper"` (or another theme) in
  `/etc/gpui-greeter/config.toml`, or install an earlier version of the
  folder again. To remove a theme, select another one first, then
  `sudo rm -r /usr/local/share/gpui-greeter/themes/<name>`.

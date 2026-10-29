---
name: gpui-greeter-theme
description: Design, check, preview and install a theme for gpui-greeter, the greetd login screen and lock screen for X11. A theme is a folder of theme.toml, pictures, SVGs and fonts laid out on a 3840x2160 frame, kept on the user's own machine. Use when someone wants a new look for their gpui-greeter login screen or lock screen, wants to turn a mockup or an idea into one, or wants to change, preview, install or undo one.
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
  (`rest = true`), typing, checking, a refused password.
- The lock screen: every installed theme is also the lock screen of a
  running session (`gpui-greeter lock`), which starts at rest and stays
  there while its user is away, so the rest screen is the face people
  see most and deserves the most care. There the session is the one
  running and cannot change, so the session row should go
  (`[layer.on.locked] visible = false`), and words like "log in" become
  "unlock".
- Motion: what moves, and when. Less is usually better on a login screen.
  The session starts the moment the password is accepted: the greeter
  never holds a welcome, so there is no welcome to design or animate, and
  checking lasts only as long as PAM takes.
- Pictures: one, or a slideshow that fades from one to the next, in an
  image layer or over the wallpaper folder (`slideshow` in the reference).
- Languages: English, Chinese, or both (`[strings.en]`, `[strings.zh-CN]`).

Agree on the design with the user before writing much theme.toml. If they
bring a mockup drawn in CSS pixels, every length becomes 1.5 times as
large: a CSS pixel is a point, as a Mac's browser draws it, and a frame
pixel two thirds of one.

## 2. Gather the files lawfully

- **Fonts:** prefer fonts under the SIL Open Font License or Apache 2.0.
  Bundle the font files and their licence text in the theme. A variable
  font draws at its default instance, so any other setting, such as a
  weight of 900 or Fraunces's "soft" axis, needs a static instance made
  with fontTools. Pin every axis the font has to what the mockup's browser
  used, if there is a mockup: it sets `opsz` from the font size and leaves
  an axis the CSS does not name at its default. Give the instance a family
  of its own, its weight class and, for an italic, its italic flag, so that
  no other face is taken for it:

  ```sh
  python3 - <<'EOF'
  from fontTools.ttLib import TTFont
  from fontTools.varLib.instancer import instantiateVariableFont
  font = instantiateVariableFont(TTFont("Fraunces[SOFT,WONK,opsz,wght].ttf"),
                                 {"wght": 900, "opsz": 96, "SOFT": 100, "WONK": 0})
  family, weight, italic = "Fraunces Soft", 900, False
  names = font["name"]
  for id in (1, 2, 4, 6, 16, 17, 21, 22, 25):
      names.removeNames(nameID=id)
  style = "Italic" if italic else "Regular"
  for id, text in ((1, family), (2, style), (4, f"{family} {style}"),
                   (6, family.replace(" ", "") + "-" + style)):
      names.setName(text, id, 3, 1, 0x409)
  font["OS/2"].usWeightClass = weight
  font["OS/2"].fsSelection = (font["OS/2"].fsSelection & ~0b1100001) | (1 if italic else 0b1000000)
  font["head"].macStyle = 2 if italic else 0
  font.save("fonts/FrauncesSoft-Black.ttf")
  EOF
  ```

  A subset must keep an "m": the greeter's text system takes no font
  without one. A font with a Reserved Font Name may keep that name only
  unmodified, so subset or instance only fonts without one, or give the
  result a name that leaves the reserved one out.
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
- A slideshow's pictures go in a folder of their own, such as `slides/`,
  each scaled to the box it fills on a 4K screen, photographs as JPEG.
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

Place layers against the screen's edges or its centre (`anchor`), so the
design holds on 16:10, 21:9 and on smaller monitors as well as on a
27-inch 16:9 one: a 24-inch 1920 by 1080 monitor shows 2880 by 1620 of
the frame's pixels. Group
what belongs together in row and column boxes, so text of any length or
language lines up, and set a row of text on one line with `align =
"baseline"`. Give each state its look with `[layer.on.<state>]` tables, and
keep transitions short (150 to 500 ms).

Size everything as macOS sizes its login and lock screen, point for
point, at 1.5 frame pixels to a point: body text 13 points, 19.5 frame
pixels; a hint 12 (18); the user's name 15 (22.5); the password field 160
by 28 points (240 by 42) with 13-point text; an avatar 72 (108); a clock
whose figures stand 122 points tall (183 frame pixels), as 173 points of
SF Pro do. The reference's "macOS metrics" says where each comes from. A
theme keeps its character at these sizes, a console's text as body text;
larger looks clumsy, as if the screen were small.

A mockup drawn in CSS pixels carries over at 1.5 times its lengths, each
part anchored where the mockup anchors it:

- What sits at a margin from an edge or a corner, such as a logo, a clock
  at the top right, a column of text at the left or a footer, anchors to
  that edge or corner of the screen, at the mockup's distance from it
  times 1.5.
- What the mockup centres, such as a panel or a dial, anchors to the
  centre, moved by its offset from the middle times 1.5. Keep what goes
  together, such as a clock over a form, in one column so it stays whole.
- What spans the screen, such as a bar or a band, takes `width = "100%"`
  or `anchor = "fill"` with an `inset`.

```toml
[[layer]]
kind = "text"
text = "{time}"
anchor = "top-left"
x = 168                 # 112 by 64 in the mockup
y = 96
```

Never put the whole composition in one 3840 by 2160 box in the middle:
on a screen of another shape, or with a `scale`, it moves away from the
edges as one block, while the mockup's parts keep to theirs.

`scale` at the top of theme.toml multiplies every length around its
anchor, so a theme drawn too large or too small is resized without
rewriting its numbers: one whose lengths are 2.4 times a mockup's takes
`scale = 0.625`. The settings' own `scale` applies on top.

## 4. Check it

```sh
gpui-greeter check-theme ~/.local/share/gpui-greeter/themes-src/<name>
```

It reads the folder as the greeter would and names every problem by file
and line. Fix all errors; read the warnings and fix those that matter. They
name text that would not look as asked without anything showing it: a
family that is neither bundled nor built in, a weight or an italic its
family lacks, a font the text system passes over for having no "m", and a
string without a translation.

## 5. Preview every state, and look

`preview` with `--shot` needs no display and no GPU: it starts an X server
of its own and draws on the CPU. Take each state in each language, at 4K
first:

```sh
dir=~/.local/share/gpui-greeter/themes-src/<name>
out=~/.cache/gpui-greeter-shots/<name>; mkdir -p "$out"
for lang in en zh-CN; do
  for state in rest idle typing checking error; do
    gpui-greeter preview "$dir" --state $state --lang $lang \
      --user alice --host example-host --shot "$out/$state-$lang.png"
  done
done
```

Then look at every screenshot, and compare it with the design: spacing,
alignment, sizes, colours, what each state shows. A 4K screenshot shows
the theme as a 27-inch 4K monitor does, a frame pixel to a pixel. Check
other sizes and shapes too (`--size 1920x1080`, `--size 1920x1200`,
`--size 3440x1440`): a preview's screen reports no size, so it is laid out
as 2560 by 1440 points fitted to its pixels and never less than a pixel
to a point, and `--size 1920x1080` shows a 24-inch 1920 by 1080 monitor,
2880 by 1620 of the frame's pixels. A state's transitions run before the
screenshot; to catch the middle of one, use `--at MS`. Iterate until it
matches. Drop `--user` and `--host` to see the user's own names.

Then the same theme as the lock screen, at rest and with its form, and
check that it looks right without the session row:

```sh
for state in rest idle error; do
  gpui-greeter preview "$dir" --lock --state $state \
    --user alice --host example-host --shot "$out/lock-$state.png"
done
```

The looks that answer to input need it in the screenshot too: `--press
f12` (keys written as in `[keys]`) shoots the armed power action, `--press
mod-j --at 200` the fired look of what mod-j runs while it lasts (700 ms),
`--state idle --press a --press b` the field with two characters typed, and
`--hover X,Y` points at whatever lies there in the screenshot, for a
control's hover look.

To lay a screenshot over a mockup, make what changes match it: preview a
copy of the theme whose clock and date say what the mockup's do, such as
`text = "22:41"` in place of `{time}`, with the mockup's names for
`--user` and `--host`, and its `scale` 1.6 times as large (2.4 / 1.5): a
1600 by 900 mockup's frame then fills a 3840 by 2160 screenshot, and the
mockup, enlarged 2.4 times, lies over it part for part.

Previews keep the SVGs they draw in `~/.cache/gpui-greeter/themes/<name>/`,
33 MB for each full-screen SVG and screen size; remove the folder when the
theme is done.

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

A dry run first needs no root: `gpui-greeter install-theme <dir> --to
~/.cache/gpui-greeter-install` checks the folder and copies it into a
folder of your own, listing what it copies and what it leaves behind.
These commands are the installed greeter's, and only a greeter that has
them draws theme folders at all: if `gpui-greeter --help` lists no
`install-theme`, the package needs updating first.

## 7. Try it safely, and know how to undo

- The installed theme shows the next time the greeter does: at the next
  logout or boot. Until then, `preview` of the installed folder is the
  closest look: `gpui-greeter check-theme /usr/local/share/gpui-greeter/themes/<name>`
  and the same `preview` commands.
- At the login screen, F9 and Shift+F9 switch between the built-in looks
  and the installed themes, and the greeter starts with the one picked
  until the settings' `theme` changes. A theme can offer the same with a
  control: `{theme}` names the look, and `action = "next-theme"` switches.
- The lock screen shows an installed theme the moment it is installed:
  `gpui-greeter lock`, then F9 to it, is the real thing in the user's own
  session, with no display manager involved. Only do that when the user
  asks; their password unlocks it.
- Never start or restart greetd while another display manager's session
  is running. Its X server can lose the GPU to the greeter's and end,
  taking the whole session with it; on NVIDIA it did. To see a theme on
  the real login screen, either make greetd the display manager
  (`sudo gpui-greeter-switch greetd`) and reboot, or have the user log
  out and, on a text console, run
  `sudo systemctl stop lightdm && sudo systemctl start greetd`. Never
  restart greetd while someone is logged in through it either: that ends
  their session too. Until greetd is the display manager, `preview` is
  the safe way to look.
- If the theme cannot be loaded the greeter shows Paper, and
  `journalctl -t gpui-greeter` says why.
- To undo, set `theme = "paper"` (or another theme) in
  `/etc/gpui-greeter/config.toml`, or install an earlier version of the
  folder again. To remove a theme, select another one first, then
  `sudo rm -r /usr/local/share/gpui-greeter/themes/<name>`.

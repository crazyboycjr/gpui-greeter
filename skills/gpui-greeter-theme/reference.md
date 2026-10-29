# gpui-greeter theme folders: the format

A theme is a folder: a `theme.toml` and the files it draws, pictures (PNG,
JPEG, WebP), SVGs and fonts (TTF, OTF, TTC). The greeter reads it from
`/usr/local/share/gpui-greeter/themes/<name>/` (the `[themes] dir` of
`/etc/gpui-greeter/config.toml`), and `theme = "<name>"` in that file
selects it. `paper`, `paper-night`, `frost` and `neon` are the built-in
looks' names and cannot name a folder. A name is lower-case letters,
digits, `-` and `_`.

The greeter checks the whole folder before it draws anything. A theme with
an error is not drawn: the greeter logs every problem (`journalctl -t
gpui-greeter`) and shows Paper instead, so a broken theme never keeps
anyone from logging in. `gpui-greeter check-theme DIR` runs the same checks
and names each problem by file and line.

`dial/`, beside this file, is a complete example.

## The frame

Every length is in pixels of a **3840 by 2160 frame**. Each monitor's window
scales the frame by min(width / 3840, height / 2160): on a 1920 by 1080
monitor one frame pixel is half a pixel, on a 2560 by 1440 one two thirds.
On a monitor of another shape the window is wider or taller than the scaled
frame, so place layers against the screen's edges or its centre (see
`anchor`) rather than at fixed distances from the top left, and they keep
their places on 16:10, 21:9 and 4:3 too.

On a 4K monitor a frame pixel is a pixel, so a theme should be as dense as
a native interface there: body text about 24 to 28 pixels, labels 20 to
24, a clock 200 to 320. A mockup drawn on a 1600 by 900 frame converts by
multiplying every length by 1.75, each part anchored to the edge, the
corner or the centre the mockup keeps it to.

`scale` multiplies every length of the theme around its anchor: sizes,
offsets, gaps, paddings, borders, radii, text and its tracking. A layer
anchored to an edge keeps to it, one in the centre stays there, and shares
like `"100%"` stay shares; a wallpaper's `blur` keeps its share of the
picture. The settings' own `scale` (0.5 to 1.5 in
`/etc/gpui-greeter/config.toml`) multiplies it again.

## theme.toml

```toml
format = 1                 # the format's version; this file describes 1
name = "Dial"              # what the theme calls itself
description = "…"
background = "#14110f"     # under every layer
fonts = ["fonts/NotoSerifDisplay-Light.ttf"]
rest = true                # start at rest (see States)
rest_after = 60            # seconds without input before going back to rest; 0 never
scale = 1                  # every length multiplied by this, 0.25 to 4

[text]                     # the text style every layer inherits
font = "Inter"
size = 25
color = "#a8917c"

[strings.en]               # the theme's words: {t.hint}
hint = "Press Enter"
[strings.zh-CN]
hint = "按回车登录"

[keys]                     # keys, over the defaults
"mod-j" = "tile.focus-next"

[[layer]]                  # the layers, bottom first
kind = "text"
text = "{time}"
anchor = "center"
```

Colours are `#rgb`, `#rrggbb`, `#rrggbbaa` or `transparent`. Durations are
milliseconds, at most 60000. Unknown keys are errors, so a typo is caught;
check-theme suggests the nearest name.

### Fonts

`fonts` lists font files in the folder. A layer names a family as the font
file names it, by its typographic family where it has one; check-theme
prints the families it found. Bundle each font's licence beside it.

The greeter has these built in, all upright:

| family | weights |
| --- | --- |
| `Inter` | 200, 300, 400, 500 |
| `JetBrains Mono` | 400, 500, 700, 800 |
| `Fraunces 144pt` | 300 |
| `Fraunces 44pt`, `Fraunces 17pt` | 400 |

It can also use the system's fonts, but a theme that names one looks
different on a machine without it, so check-theme warns about a family
that is neither bundled nor built in.

A face is found by its family, then its weight and whether it is italic:
the family's face nearest the weight asked for is drawn, an italic one if
`italic = true`, and the system's faces of the same family are candidates
too. So a weight or an italic the family lacks draws in its nearest face:
Inter at 600 draws at 500, and Inter in italic draws upright. check-theme
warns of each. Weights are 100 to 900 or `thin`, `extralight`, `light`,
`regular`, `medium`, `semibold`, `bold`, `extrabold`, `black`.

The text system takes no font without an "m" glyph, so a subset with no
"m", digits alone for a clock say, draws nothing of its own: keep an "m" in
every subset. check-theme warns of a font without one.

A variable font draws at its default instance. For any other setting, make
a static instance with fontTools, as the skill's step 2 shows, and pin
every axis the font has: the mockup's browser sets `opsz` from the font
size and leaves each axis the CSS does not name at its default (Fraunces's
`WONK` at 1, for one). Give the instance a family name of its own, so that
no other face of that family, the system's or another instance, is taken
for it, and set its weight class and italic flag to what it is.

What the font lacks, Chinese above all, comes from the families `fallback`
names (see Looks), then from the greeter's own: Noto Sans CJK SC, Noto Sans
SC and DejaVu Sans, from the system. A serif design names a serif CJK font
there: a bundled subset, or the system's Noto Serif CJK SC.

## Layers

`[[layer]]` tables are drawn in order, later ones over earlier ones. A layer
sits on the screen, or in the layer `parent` names, which must come before
it.

| key | what |
| --- | --- |
| `id` | a name, for `parent`, a tiles layer's `focus`, and messages |
| `kind` | `box` (the default), `text`, `field`, `image`, `svg`, `wallpaper` or `tiles` |
| `parent` | the id of the layer it sits in |
| `monitors` | for a layer on the screen: `form` (the default; the monitor with the login form), `all` or `others` |

### Placing

How a layer is placed depends on its parent's `layout`.

In a **stack** (the screen, a pane, or a box with `layout = "stack"`):

| key | what |
| --- | --- |
| `anchor` | `top-left` (the default), `top`, `top-right`, `left`, `center`, `right`, `bottom-left`, `bottom`, `bottom-right`, or `fill` |
| `x`, `y` | from the parent's anchor point to the same point of the layer; right and down are positive |
| `inset` | with `fill`: space left at the top, right, bottom and left |

With `anchor = "bottom-right"`, `x = -130` and `y = -120`, the layer's bottom
right corner sits 130 pixels left of the screen's right edge and 120 above
its bottom. With `center`, the layer's centre sits at the parent's centre
moved by `x` and `y`. `fill` covers the parent less `inset`; `x` and `y`
move the whole of it.

In a **row** or a **column**, layers follow one another; `x` and `y` nudge
a layer from its place without moving the others, and `grow` (a number)
lets it take a share of the room left over.

| key | what |
| --- | --- |
| `width`, `height` | frame pixels, or a share of the parent like `"100%"`; text and rows take their own size without them |

A picture or an SVG with one of `width` and `height` takes the other from
its shape; with neither, its own size in frame pixels. A stack box places
its layers inside its own size, so give it a width and a height, or `anchor
= "fill"`.

### Looks

Every key in this section can also change with a condition, in an `on`
table (see Conditions).

| key | what |
| --- | --- |
| `visible` | `false` hides the layer: it fades over its transition, and once hidden it takes no clicks and no room in a row or a column. A layer at opacity 0 takes no room either |
| `opacity` | 0 to 1, of the layer and what is in it |
| `scale` | the layer and what is in it, grown from its anchor point; an svg with `tint` grows from its centre |
| `rotate` | degrees clockwise about its centre; only an svg with `tint` turns |
| `clip` | shares of its box cut away from the top, right, bottom and left, `[0, 1, 0, 0]` to `[0, 0, 0, 0]` wipes it in from the left |

Text style, which layers inherit from the one they sit in and from `[text]`:

| key | what |
| --- | --- |
| `font`, `size`, `weight`, `italic` | the family, frame pixels, the weight, `true` for italic |
| `fallback` | a family, or a list of them, for what `font` lacks, tried before the greeter's own (see Fonts); a family the system lacks is passed over |
| `color` | text, an svg's `tint`, a field's caret by default, and a border by default |
| `tracking` | letter spacing in frame pixels (CSS letter-spacing) |
| `line_height` | each line's height, a multiple of `size`; the font's own otherwise. The glyphs keep to the middle of the line, so a line height below the font's own lets them spill over it above and below |
| `case` | `upper`, `lower` or `none` |
| `features` | OpenType features, `["tnum", "lnum", "ss01=0"]` |
| `shadow` | `{ x, y, blur, color }` behind the text, or `"none"` |
| `text_align` | `start`, `center` or `end`, within a layer's width |

Boxes:

| key | what |
| --- | --- |
| `layout` | `stack` (the default), `row` or `column`; this one cannot change with a condition |
| `gap` | between the layers in a row or a column |
| `align` | across a row or column: `start`, `center`, `end`, `stretch` (the default) or `baseline`, which puts the first line of text in each layer of a row on one line, whatever its font, size or line height; a layer with no text sits on it by its bottom edge |
| `justify` | along it: `start`, `center`, `end`, `between` or `around` |
| `padding` | one number, `[vertical, horizontal]`, or `[top, right, bottom, left]` |
| `fill` | the box's colour |
| `border` | width, written as `padding` is: `[0, 0, 3.6, 0]` is an underline |
| `border_color`, `border_style` | the colour; `solid` or `dashed` |
| `radius` | one number, or `[top-left, top-right, bottom-right, bottom-left]` |
| `box_shadow` | `{ x, y, blur, spread, color, inset }`, or `"none"` |
| `overflow` | `hidden` cuts off what spills out; this one cannot change |

### Kinds

**text**: `text` is a template (see Bindings); `\n` breaks lines. Text does
not wrap.

**field**: what has been typed, one `mask` character (`"•"` by default)
for each, at most `max` (64) of them; the answer itself when PAM asks a
question whose answer may show. While the password is checked it keeps the
mask of what was sent. When empty it shows `placeholder`, a template, in
`placeholder_color` (the text colour at half strength by default). Text
style keys set its look.

What is typed is drawn a glyph for each character: a field's text, and what
`{typed}` and `{sent}` write, go without ligatures and contextual
alternates, whatever `features` says, so the glyphs shown never change as
more are typed. JetBrains Mono, for one, would join `***` and raise the
middle asterisk.

A field is one line of its font tall (its `line_height`, if set), or as
tall as its caret where that is taller, empty or not, and a `height` larger
than that centres the line in it. Its baseline is its text's, the same
empty or typed, so a row aligned on baselines stays still at the first key.

Its caret: `caret` is `bar` (the default), `block` or `none`,
`caret_color`, `caret_width` (3.6, or 0.6 em for a block) and `blink`, the
period in ms (1060; 0 is steady); the blink restarts with each key, so the
caret shows while typing. It stands before the placeholder and after what
was typed.

**image**: `file`, a PNG, JPEG or WebP. `fit` is `contain` (the default),
`cover` or `fill` (stretched); `position = [x, y]` is the point a cover
crop keeps, as shares from 0 to 1. `radius` rounds its corners.

**svg**: `file`, an SVG. With `tint = true` its shape is drawn in the
layer's `color`, so states can change the colour smoothly, and it can turn
and scale; this suits marks of one colour. Without it the SVG is drawn in
its own colours, once for each monitor size at the size it takes there,
and kept in the greeter's cache; such a layer needs a width or a height in
frame pixels, the other taken from the SVG's shape, or `anchor = "fill"`.
Drawing a large SVG with heavy filters can take seconds the first time; the
window opens without it after 1.5 s and shows it when done. An SVG must
hold everything it draws: references to other files are refused, pictures
go in as `data:` URLs, and text should be converted to paths.

**wallpaper**: the picture the settings pick from the wallpaper folder
(`[wallpaper]` in config.toml; `gpui-greeter sync-wallpapers` fills it).
`fit` and `position` as for an image. `blur` (frame pixels) makes a blurred
form too, and `blurred` (0 to 1, changeable with states) shows it.

**tiles**: panes laid out as xmonad lays out windows. Its children are the
panes. Give it `anchor = "fill"` on the screen, or a width and a height in
frame pixels.

| key | what |
| --- | --- |
| `layouts` | what `tile.next-layout` cycles through: `["tall", "mirror", "full"]` by default |
| `layout`, `focus` | the layout and the pane (by id) it starts with; in an `on` table, what it shows in that state |
| `ratio`, `ratio_step` | the master's share (0.5) and what shrink and expand change it by (0.03) |
| `pane_border`, `focused_color`, `unfocused_color` | border width and colours (xmonad's `#ff0000` and `#dddddd`) |
| `gap` | between panes and around them |

A pane is a box whose content is laid out on `content = [width, height]`
frame pixels and scaled to fit the pane less its `pane_border` and its
`padding` on every side, between `min_scale` (0.3) and `max_scale` (1);
`max_scale_full` is the largest scale in the Full layout. The padding does
not include the border: where a mockup insets content 28 CSS pixels from
the pane's outer edge, 2 of them its border, `padding` is 26 × 1.75 = 45.5.
`content = "fill"` lays the content out on the whole pane instead,
unscaled. `title` is the pane's title, a template, shown by
`{tiles.title}`. Typing focuses the pane holding the field. A theme has one
tiles layer at most.

### Repeats

| key | what |
| --- | --- |
| `for_each = "session"` | the layer is drawn once for each session; in it, `{item}` is the session's name and `{item.number}` its place from 1, `on.current` applies to the chosen one, and `action = "pick"` chooses it |
| `copies = { count, x, y, rotate, delay }` | `count` copies, each moved by `x` and `y`, turned by `rotate` and held back by `delay` ms more than the one before |
| `stagger` | on a box: each layer in it waits this many ms longer than the one before, in its transitions and state motions |

## States

The login is always in one of six states:

| state | when |
| --- | --- |
| `rest` | before the first key, in a theme with `rest = true`; Escape and `rest_after` come back to it |
| `idle` | ready, the field empty |
| `typing` | the field holds something |
| `checking` | PAM is checking the password |
| `error` | a password was refused or the login failed, until the next key |
| `welcome` | the password was accepted: the greeter leaves at once |

At rest, a key or a click leaves rest; a character typed also goes into the
field. A theme without `rest` starts idle, and Escape clears the field.

A theme is also a lock screen: `gpui-greeter lock` shows the installed
themes over a running session, starting at rest, which is what the screen
shows while its user is away; there the theme keys switch looks without
leaving rest. A theme without `rest` shows its form at once there too.
The user is the one who locked and `{session}` the session running, which
cannot be changed; the condition `locked` tells the two screens apart.

The session starts as soon as the password is accepted: `checking` lasts
only as long as PAM takes, and the greeter never holds `welcome`, which is
drawn for a frame at most. Design nothing for it, no animation and no
fade: whatever a theme puts there is cut short before it shows.

### Conditions

`[layer.on.<condition>]` sets any of the layer's looks for a condition. A
table can name several: `[layer.on."typing, checking"]`. They apply in this
order, so a later one wins where two set the same key:

1. a language: `en`, `zh-CN`
2. `locked`: the theme is the lock screen, not the login screen
3. `failed`: a password was refused since the last rest, and the login has not succeeded
4. `current`: a `for_each` layer stands for the chosen session
5. `focused`: the layer is in the focused tiles pane
6. a state: `rest`, `idle`, `typing`, `checking`, `error`, `welcome`
7. `armed`: a power action waits for its second press; for a layer with a power action, or in one, only that action
8. `fired`: the layer's action ran, by key or click, in the last `fired_for` ms (700); `fired_by = ["tile.focus-previous", …]` adds other actions that count, to the layer's own action or, for a layer without one, to that of the layer it sits in
9. `hover`: the pointer is over the layer

Hide the session row on the lock screen with `[layer.on.locked]` and
`visible = false` on a layer whose visibility no state changes, such as the
row itself inside a form that rest hides; change words there the same way,
and the states' own words still win, as dial's status line shows.

`hover`, `armed` and `fired` belong to the nearest layer with an `action`,
the layer itself or one it sits in, so a button's label can change colour
when the pointer is anywhere on the button.

```toml
[[layer]]
kind = "text"
text = "{t.hint}"
transition = 300            # how it changes on entering any state

[layer.on.error]
text = "{t.wrong}"
color = "#e5703b"
transition = { duration = 120, easing = "ease-out" }   # on entering this state
```

### Transitions

When the state changes, a layer changes from its old look to its new one
over its `transition`: a duration in ms, or `{ duration, easing, delay }`.
`transition` on the layer applies to every state; one in a state's `on`
table applies on entering that state. Opacity, visibility, `x`, `y`,
`scale`, `rotate`, `clip`, colours, `blurred`, and widths and heights in
the same unit change smoothly; the rest switches when the transition
starts, after its delay. A change that interrupts another starts from where
the other had come to. Conditions other than states (hover, armed, fired,
current, focused, failed) apply at once.

Easings are `linear`, `ease` (the default), `ease-in`, `ease-out`,
`ease-in-out`, `cubic-bezier(x1, y1, x2, y2)`, and `steps(n)` or
`steps(n, start)`.

### Motions

A motion moves a layer by keyframes, as a CSS animation does. `motion` on
the layer runs from when the greeter starts; in a state's `on` table it
runs while in that state, from when the state began. Either is one table or
an array of them.

| key | what |
| --- | --- |
| `property` | `x`, `y` and `rotate` add to the layer's own; `scale` and `opacity` multiply it |
| `keyframes` | `[[at, value], …]` from at 0 to at 1; a third element, an easing, applies to the step that follows; or `from` and `to` |
| `duration`, `easing` | one run, and the easing between keyframes (`linear` by default) |
| `repeat` | how many runs (1), or `"infinite"` |
| `alternate` | every other run goes backwards |
| `delay` | ms before the first run |
| `sync = "clock"` | runs by the time of day: a run of 60000 ms is at the same place at the same second of every minute |

```toml
# A lit tick that goes round once a minute, a tick at a time.
motion = { property = "rotate", from = 0, to = 360, duration = 60000, easing = "steps(60)", repeat = "infinite", sync = "clock" }

# A shake on a wrong password.
[layer.on.error]
motion = { property = "x", keyframes = [[0, 0], [0.2, -10], [0.4, 17], [0.6, -17], [0.8, 10], [1, 0]], duration = 450 }

# A blink: on for half of each second.
motion = { property = "opacity", keyframes = [[0, 1], [0.5, 1], [0.5, 0], [1, 0]], duration = 1000, repeat = "infinite" }
```

## Bindings

Text, placeholders, pane titles and strings are templates: `{name}` or
`{name:argument}` is filled in as the layer is drawn. `{{` and `}}` write a
brace. An unknown name is an error.

| binding | what |
| --- | --- |
| `{time}`, `{time:FORMAT}` | the time, `%H:%M` by default |
| `{date}`, `{date:FORMAT}` | the date, "Wednesday, October 28" or "10月28日　星期三" by default |
| `{time.en:…}`, `{date.en:…}`, `{date.zh-CN:…}` | the same, with English or Chinese names whatever the greeter speaks |
| `{user}`, `{user.name}`, `{user.initial}` | the login name, the full name (or the login name), its first letter |
| `{host}` | the machine's name |
| `{session}` | the chosen session's name |
| `{theme}` | the theme's `name`, for a control that switches looks |
| `{prompt}` | what PAM asks, without its colon; "Password" before it asks |
| `{message}` | what PAM or greetd last said, such as why a login was refused, if anything |
| `{status}` | the greeter's own sentence for where the login stands |
| `{os}`, `{arch}`, `{kernel}` | "Arch Linux", "x86_64", "6.17.1-arch1-1" |
| `{uptime}`, `{uptime:short}` | "1 day, 6 hours, 35 mins" (or "1 天 6 小时 35 分钟"), "30h 35m" |
| `{cpu}`, `{gpu}`, `{memory}` | "AMD Ryzen 7 5800X (16)", "NVIDIA GeForce RTX 3060", "3.81 GiB / 15.26 GiB" |
| `{packages}`, `{shell}` | "1234 (pacman)", "zsh 5.9" |
| `{display}` | this monitor: "3840×2160 @ 60Hz" |
| `{layout}`, `{vt}`, `{seat}`, `{network}` | the keyboard layout, "tty1", "seat0", "online" or "offline" |
| `{typed}`, `{typed:*}` | what has been typed, one glyph a character (`•` by default) |
| `{sent}`, `{sent:*}` | what was last sent, so: the attempt that failed |
| `{tiles.title}`, `{tiles.layout}` | the focused pane's title, the layout's name ("Tall", "Mirror Tall", "Full") |
| `{item}`, `{item.number}` | in a `for_each` layer |
| `{armed.key}`, `{armed.action}` | the key that runs the armed power action ("F12"), and its name ("poweroff") |
| `{t.KEY}` | one of the theme's strings |

System facts are read from files (`/etc/os-release`, `/proc`, `/sys`, the
PCI database, the package databases), never by running anything, and only
when a template uses them. A theme cannot run commands.

`{t.KEY}` takes the string from `[strings.zh-CN]` when the greeter speaks
Chinese (`language = "zh-CN"`), falling back to `[strings.en]`; strings may
hold bindings. check-theme warns about a string with no translation. The
greeter's own words are also there: `password`, `press_enter`, `checking`,
`wrong_password`, `welcome_back`, `starting`, `session`, `current`,
`sleep`, `restart`, `shut_down`, `click_to_log_in`; on the lock screen
`press_enter` and `click_to_log_in` say to unlock instead.

Time formats: `%Y` year, `%y` its last two digits, `%m` month, `%d` day,
`%e` day padded with a space, `%H` hour, `%I` hour of twelve, `%M`
minutes, `%S` seconds, `%p` AM or PM, `%A` weekday, `%a` its short form,
`%B` month name, `%b` its short form, `%j` day of the year, `%u` weekday
number (Monday is 1), `%%`. `%-d` drops the leading zero. Names follow the
language: `%A` is "Wednesday" or "星期三", `%a` "Wed" or "周三", `%B`
"October" or "十月", `%b` "Oct" or "10月", `%p` "AM" or "上午". A binding
with a language of its own keeps its names in it, as a status bar might in
a Chinese login screen: `{date.en:%a %b %-d}` is "Wed Oct 28" in either
language, and `{date.en}` the long English date. `time` and `date` both
take `.en` and `.zh-CN`.

## Keys and clicks

`action` makes a layer clickable; a key runs an action through `[keys]`.

| action | what |
| --- | --- |
| `submit` | log in with what was typed |
| `next-session`, `previous-session`, `session:N` | choose a session; N from 1 |
| `pick` | choose the session a `for_each` layer stands for |
| `next-user`, `previous-user` | choose who logs in |
| `next-keyboard-layout` | the next of the configured keyboard layouts |
| `next-theme`, `previous-theme` | the next look the greeter offers, or the one before: the built-in looks and the installed theme folders, or the settings' `[themes] cycle` |
| `rest`, `wake`, `clear` | go to rest (or clear the field, without a rest state), leave rest, clear the field |
| `suspend`, `reboot`, `poweroff` | the first press arms the action; a second within four seconds runs it |
| `tile.focus-next`, `tile.focus-previous`, `tile.swap-master`, `tile.next-layout`, `tile.shrink`, `tile.expand`, `tile.full` | as xmonad's mod-j, mod-k, mod-Return, mod-space, mod-h, mod-l and full screen |
| `none` | nothing: unbinds a key |

Every theme starts with these keys, which `[keys]` can change:

| key | action |
| --- | --- |
| Enter | `submit` |
| Tab, Shift+Tab, F2 | `next-session`, `previous-session`, `next-session` |
| Escape | `rest` |
| Ctrl+U | `clear` |
| F3 | `next-keyboard-layout` |
| F9, Shift+F9 | `next-theme`, `previous-theme` |
| F10, F11, F12 | `suspend`, `reboot`, `poweroff` |
| Up, Down | `previous-user`, `next-user` |

Backspace deletes a character and Ctrl+Backspace clears the field; other
keys type. A key is written in lower case with its modifiers: `f10`,
`shift-tab`, `ctrl-u`, `alt-j`, `super-j`, or `mod-j`, where `mod` is Alt
or Super. Named keys: `enter` (or `return`), `escape`, `tab`, `space`,
`backspace`, `delete`, `insert`, `home`, `end`, `pageup`, `pagedown`, `up`,
`down`, `left`, `right`, `menu`, `f1` to `f24`.

## Limits

A file is at most 64 MiB, a picture at most 16384 pixels a side and 64
megapixels, theme.toml at most 1 MiB, with up to 2000 layers nested up to 32
deep. Every file must lie inside the folder: no absolute paths, no `..`,
and no links that lead out. `install-theme` copies only regular files of
the kinds above and text files (`.txt`, `.md`, LICENSE, SOURCES and the
like); hidden files and links stay behind.

What the format does not do: gradients on boxes (draw them in an SVG),
dotted borders (`dashed` is the nearest), shadows or outlines around
pictures (put them in the picture), wrapping text, per-character pictures,
and smooth changes for hover and the other non-state conditions.

## Commands

```sh
gpui-greeter check-theme DIR
gpui-greeter preview DIR [--state STATE] [--lang en|zh-CN] [--size WxH]
                         [--shot FILE] [--at MS] [--user NAME] [--host NAME]
                         [--wallpaper FILE] [--press KEY]... [--hover X,Y]
                         [--lock]
sudo gpui-greeter install-theme DIR [--name NAME] [--select] [--to DIR]
```

`check-theme` reads the folder as the greeter does, and names each error
and warning by file and line.

`preview` shows the theme in a window on `$DISPLAY`, against a fake greetd
that accepts the password `test` and starts nothing. With `--shot FILE` it
needs no display at all: it starts an X server of its own (Xvfb, from
xorg-server-xvfb) and draws with lavapipe (Mesa's Vulkan on the CPU, from
vulkan-swrast), then writes FILE once the screen has settled, or `--at` ms
after the state shows, and exits. `--state` shows that state the way a
login reaches it, with its transitions; `--size` is 3840x2160 for a
screenshot. `--user` and `--host` stand in for the real names, and
`--wallpaper` picks the wallpaper. `--lock` shows the theme as the lock
screen: `locked` holds, and the greeter's words are the lock screen's.

Once the state shows, each `--press` presses a key, written as in `[keys]`
(`f10`, `mod-j`, `shift-tab`, or `a`, which types), and `--hover X,Y` puts
the pointer at that point of the screenshot; `--at` then counts from the
last of them. So `--press f12` shoots the armed look, `--press mod-j --at
200` the fired one while it lasts, and `--hover` a control's hover look.

The SVGs a preview draws in their own colours are kept, as the greeter
keeps them, in `~/.cache/gpui-greeter/themes/<name>/`: raw pixels, so a
full-screen SVG takes 33 MB for each screen size it was drawn at. Remove
the folder to free them; the next preview draws them again.

`install-theme` checks the folder, copies it to the themes folder as NAME
(the folder's name by default), replacing an earlier copy only once the new
one is complete and checked, and with `--select` sets `theme = "NAME"` in
the settings, keeping their comments. With `--to DIR`, a folder of the
user's own, it needs no root, which makes a dry run: it lists what it
copies and what it leaves behind.

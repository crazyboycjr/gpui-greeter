# Installing gpui-greeter

gpui-greeter runs on X11, as the login screen of
[greetd](https://git.sr.ht/~kennylevinsen/greetd), and draws with Vulkan.
Besides greetd it needs an X server and `startx` (xorg-server and
xorg-xinit), PAM, systemd and a Vulkan driver. These are optional:

- AccountsService, for avatars, and for the session LightDM last used
  before the first login here;
- polkit, for the settings window to apply what root keeps;
- xdg-desktop-portal, for the settings window's file dialogs;
- Noto Sans CJK, for Chinese text;
- Xvfb and Mesa's lavapipe (xorg-server-xvfb and vulkan-swrast on Arch),
  for `gpui-greeter preview --shot`, which draws without a display;
- xss-lock, to lock before a suspend.

## Arch Linux

From the AUR:

```sh
paru -S gpui-greeter
```

Or from this repository, which builds what is on `main`:

```sh
git clone https://github.com/crazyboycjr/gpui-greeter
cd gpui-greeter/packaging/arch
makepkg -si
```

The package installs:

| Path | What it is |
| --- | --- |
| `/usr/bin/gpui-greeter` | the greeter, the lock screen, the settings window and the theme tools |
| `/usr/bin/gpui-greeter-switch` | makes greetd or LightDM the display manager from the next boot |
| `/etc/gpui-greeter/config.toml` | the settings, every key with its default ([configuration](configuration.md)) |
| `/etc/greetd/gpui-greeter.toml` | greetd's settings for the greeter |
| `/usr/lib/systemd/system/greetd.service.d/gpui-greeter.conf` | a drop-in that points greetd at that file |
| `/etc/pam.d/gpui-greeter-lock` | the PAM service the lock screen checks passwords with |
| `/usr/lib/systemd/system/gpui-greeter-bing.timer` | fetches Bing's picture of the day, when the settings ask for it |
| `/usr/share/applications/gpui-greeter-settings.desktop` | the settings window among the applications |
| `/usr/share/gpui-greeter/skills/gpui-greeter-theme/` | the skill for designing themes with an agent, and its example |
| `/var/lib/gpui-greeter` | the home of greetd's `greeter` user: the X cookie, Xorg's log, the last login |

greetd's own `/etc/greetd/config.toml` stays as it is: the drop-in has
greetd read `/etc/greetd/gpui-greeter.toml` instead.

## NixOS

<!-- TODO: check this against flake.nix and packaging/nix/module.nix as
the packaging branch lands them. -->

The flake has a NixOS module. Add the flake to your system's inputs and
turn the module on:

```nix
{
  inputs.gpui-greeter.url = "github:crazyboycjr/gpui-greeter";

  outputs = { nixpkgs, gpui-greeter, ... }: {
    nixosConfigurations.example-host = nixpkgs.lib.nixosSystem {
      system = "x86_64-linux";
      modules = [
        gpui-greeter.nixosModules.default
        {
          services.gpui-greeter.enable = true;
          services.gpui-greeter.settings = {
            theme = "frost";
            language = "en";
          };
        }
      ];
    };
  };
}
```

It sets up greetd with the greeter on VT 1, in an X server of its own that
`startx` starts, and NixOS's X sessions to choose from. `settings` becomes
`/etc/gpui-greeter/config.toml`; since NixOS keeps that file, the settings
window's Apply changes only your avatar there, and `install-theme --select`
cannot select a theme, so set `theme` in `settings` instead.
`services.gpui-greeter.lock.enable` (on by default) adds the PAM service
of `gpui-greeter lock`, and `services.gpui-greeter.bing.enable` the daily
timer for Bing's picture of the day.

Elsewhere, `nix run github:crazyboycjr/gpui-greeter -- preview DIR` shows a
theme folder and `-- settings` opens the settings window; the login screen
itself needs greetd, set up as the Arch package or the NixOS module does.

## From source

You need Rust 1.88 or newer, and the libraries the package depends on.

```sh
cargo build --release
```

The binary is `target/release/gpui-greeter`. `dist/` holds the files the
package installs around it, and `packaging/arch/PKGBUILD` says where each
goes.

## Switching to greetd

Run one display manager at a time. Starting greetd while another display
manager's session is running starts a second X server beside that
session's, and the two can fight over the GPU: on NVIDIA the session's X
server lost the GPU to the greeter's and ended, taking the session with
it. To look at a theme folder without any of that, `gpui-greeter preview
DIR` shows it in a window of the running session (see
[theme folders](themes.md)).

```sh
sudo gpui-greeter-switch greetd
```

makes greetd the display manager from the next boot; nothing starts or
stops before then. LightDM stays installed, and

```sh
sudo gpui-greeter-switch lightdm
```

makes it the display manager again from the next boot. The command only
moves `display-manager.service`, the alias that graphical.target starts,
from one display manager's unit to the other's.

To switch at once, log out first, since stopping LightDM ends the session
it started; then log in on a text console (Ctrl+Alt+F2) and run

```sh
sudo systemctl stop lightdm && sudo systemctl start greetd
```

Going back is the same with the two swapped, after logging out of any
session greetd started.

The session greetd starts runs on VT 1, where the login screen was, so
Ctrl+Alt+F1 returns to it from a text console (LightDM's ran on VT 7).
Stopping or restarting greetd ends that session.

## When something goes wrong

The greeter's log, startx's and the X server's are in
`journalctl -t gpui-greeter`, and Xorg's own log in
`/var/lib/gpui-greeter/.local/share/xorg/`.

If the greeter's X server cannot start, greetd's command falls back to
agreety, a text login on the same VT whose session is your `startx`. If
the greeter itself fails, greetd starts it again. A theme folder that
cannot be read gives way to Paper, and the log says why.

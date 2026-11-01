# services.gpui-greeter: greetd shows gpui-greeter on VT 1, in an X server
# of its own that startx starts as the greeter user, as the Arch package's
# dist/greetd.toml sets it up; the sessions it starts are NixOS's X
# sessions. With it come the PAM service of `gpui-greeter lock`, the
# greeter's home and Bing folder (tmpfiles), and the daily Bing timer.
{
  config,
  lib,
  pkgs,
  ...
}:

let
  cfg = config.services.gpui-greeter;
  settingsFormat = pkgs.formats.toml { };
  sessionData = config.services.displayManager.sessionData;

  # greetd's command for the greeter, with what startx runs on the PATH.
  # Should X fail to start, agreety offers a text login on the same VT,
  # whose session is the user's startx.
  greeter = pkgs.writeShellScript "gpui-greeter-session" ''
    export PATH=${
      lib.makeBinPath [
        pkgs.coreutils
        pkgs.gnused
        pkgs.util-linux
        pkgs.xauth
        pkgs.xinit
        config.systemd.package
      ]
    }''${PATH:+:$PATH}
    systemd-cat -t gpui-greeter env HOME=/var/lib/gpui-greeter \
      startx ${lib.getExe cfg.package} \
      || exec ${lib.getExe' config.services.greetd.package "agreety"} --cmd startx
  '';
in
{
  options.services.gpui-greeter = {
    enable = lib.mkEnableOption "gpui-greeter, a greeter for greetd on X11, as the login screen";

    package = lib.mkPackageOption pkgs "gpui-greeter" { };

    settings = lib.mkOption {
      type = settingsFormat.type;
      default = { };
      example = lib.literalExpression ''
        {
          theme = "frost";
          language = "zh-CN";
          wallpaper.pick = "bing";
        }
      '';
      description = ''
        gpui-greeter's settings, written to {file}`/etc/gpui-greeter/config.toml`;
        {file}`share/gpui-greeter/config.toml` in the package lists every key
        with its default. `sessions.dir` and `sessions.wrapper` default to
        NixOS's X sessions, started with `startx` and NixOS's session wrapper.

        NixOS keeps that file, so the Apply of `gpui-greeter settings`
        changes only what is the user's own (the avatar) and
        `install-theme --select` cannot select a theme: set `theme` here.
      '';
    };

    lock.enable = lib.mkOption {
      type = lib.types.bool;
      default = true;
      description = ''
        Whether to add the PAM service `gpui-greeter-lock`, which
        `gpui-greeter lock` checks the password with and refuses to start
        without.
      '';
    };

    bing.enable = lib.mkOption {
      type = lib.types.bool;
      default = (cfg.settings.wallpaper.pick or null) == "bing";
      defaultText = lib.literalExpression ''(config.services.gpui-greeter.settings.wallpaper.pick or null) == "bing"'';
      description = ''
        Whether `gpui-greeter-bing.timer` fetches Bing's picture of the day
        daily, as the greeter's user, into the Bing folder, so that the
        pictures gather on days without a login screen too.
      '';
    };
  };

  config = lib.mkIf cfg.enable {
    services.gpui-greeter.settings.sessions = {
      dir = lib.mkDefault "${sessionData.desktops}/share/xsessions";
      wrapper = lib.mkDefault "${lib.getExe' pkgs.xinit "startx"} ${sessionData.wrapper}";
    };

    environment.etc."gpui-greeter/config.toml".source =
      settingsFormat.generate "gpui-greeter.toml" cfg.settings;

    environment.systemPackages = [ cfg.package ];

    # X itself, and startx: its xserverrc, and no LightDM.
    services.xserver.enable = true;
    services.xserver.displayManager.startx.enable = true;

    services.greetd = {
      enable = true;
      settings.default_session = {
        command = "${greeter}";
        user = "greeter";
      };
    };

    # The greeter's home (the X cookie, Xorg's log, the last login) and
    # Bing's folder, which the greeter's user fills and everyone reads.
    systemd.tmpfiles.packages = [ cfg.package ];

    systemd.packages = [ cfg.package ];
    systemd.timers.gpui-greeter-bing.wantedBy = lib.mkIf cfg.bing.enable [ "timers.target" ];

    security.pam.services.gpui-greeter-lock = lib.mkIf cfg.lock.enable { };
  };
}

# A NixOS machine whose login screen is gpui-greeter: greetd starts it in
# an X server of its own, drawn by lavapipe; alice logs in with her
# password, and greetd starts her X session. Then the lock screen locks it
# and her password unlocks it. Screenshots: greeter.png, session.png,
# lock.png, lock-awake.png.
{ pkgs, module }:

pkgs.testers.runNixOSTest {
  name = "gpui-greeter";

  nodes.machine = {
    imports = [ module ];

    services.gpui-greeter.enable = true;
    services.xserver.windowManager.icewm.enable = true;
    environment.systemPackages = [ pkgs.xdotool ];

    users.users.alice = {
      isNormalUser = true;
      password = "foobar";
    };

    virtualisation.memorySize = 2048;
    virtualisation.cores = 2;
  };

  testScript = ''
    import shlex

    def as_alice(command):
        return "su - alice -c " + shlex.quote("DISPLAY=:0 " + command)

    machine.wait_for_unit("greetd.service")
    machine.wait_until_succeeds("pgrep -u greeter -x gpui-greeter")
    machine.wait_for_file("/tmp/.X11-unix/X0")
    machine.succeed("test -d /var/lib/gpui-greeter")
    machine.succeed("grep -q xsessions /etc/gpui-greeter/config.toml")
    machine.succeed("test -e /etc/pam.d/gpui-greeter-lock")
    machine.succeed("systemctl cat gpui-greeter-bing.service | grep -q 'ExecStart=/nix/store/.*/bin/gpui-greeter fetch-bing'")
    machine.fail("systemctl is-enabled gpui-greeter-bing.timer")
    # Its windows are up once the greeter has drawn a frame; give lavapipe
    # its time.
    machine.sleep(15)
    machine.screenshot("greeter")
    print(machine.succeed("journalctl -b -t gpui-greeter --no-pager | tail -n 40"))

    # Paper's login form shows from the start.
    with subtest("alice logs in, and greetd starts her session"):
        machine.send_chars("foobar\n")
        machine.wait_until_succeeds("pgrep -u alice -x icewm", timeout=120)
        machine.wait_until_fails("pgrep -u greeter -x gpui-greeter", timeout=60)
        machine.sleep(5)
        machine.screenshot("session")

    with subtest("the lock screen locks her session, and her password unlocks it"):
        machine.succeed(as_alice("gpui-greeter lock >/tmp/lock.log 2>&1 &"))
        machine.sleep(15)
        print(machine.succeed("cat /tmp/lock.log"))
        machine.succeed("pgrep -u alice -f 'gpui-greeter lock'")
        machine.screenshot("lock")
        # The lock starts at rest, where the first key (a key, not a
        # modifier alone) only brings the form.
        machine.send_key("spc")
        machine.sleep(2)
        machine.screenshot("lock-awake")
        machine.send_chars("foobar\n")
        machine.wait_until_fails("pgrep -u alice -f 'gpui-greeter lock'", timeout=120)
  '';
}

#!/usr/bin/env bash
# Installs gpui-greeter as greetd's greeter, and switches the display
# manager between greetd and LightDM. Run as root from the repository:
#
#   sudo ./install.sh BINARY WALLPAPERS   install, or update after a rebuild
#   sudo ./install.sh wallpapers FOLDER   copy the wallpapers again
#   sudo ./install.sh switch              greetd instead of LightDM, from the next boot
#   sudo ./install.sh switch-back         LightDM again, from the next boot
#
# LightDM stays installed throughout, so switch-back always works.
set -euo pipefail

REPO=$(cd "$(dirname "$0")" && pwd)
WALLPAPERS=/usr/local/share/gpui-greeter/wallpapers

die() { echo "install.sh: $*" >&2; exit 1; }
need_root() { [[ $(id -u) == 0 ]] || die "run it as root, with sudo"; }

install_greeter() {
    local binary=${1:?the gpui-greeter binary}
    local pictures=${2:?the wallpaper folder}
    [[ -x $binary ]] || die "$binary is not an executable"
    pacman -S --needed greetd
    install -Dm755 "$binary" /usr/local/bin/gpui-greeter
    if [[ -e /etc/gpui-greeter/config.toml ]]; then
        echo "kept /etc/gpui-greeter/config.toml; the defaults are in $REPO/dist/config.toml"
    else
        install -Dm644 "$REPO/dist/config.toml" /etc/gpui-greeter/config.toml
    fi
    install -Dm644 "$REPO/dist/tmpfiles.conf" /etc/tmpfiles.d/gpui-greeter.conf
    systemd-tmpfiles --create /etc/tmpfiles.d/gpui-greeter.conf
    copy_wallpapers "$pictures"
    if [[ -e /etc/greetd/config.toml && ! -e /etc/greetd/config.toml.orig ]]; then
        cp -a /etc/greetd/config.toml /etc/greetd/config.toml.orig
    fi
    install -Dm644 "$REPO/dist/greetd.toml" /etc/greetd/config.toml
    echo "installed; 'sudo $0 switch' makes greetd the display manager from the next boot"
}

copy_wallpapers() {
    /usr/local/bin/gpui-greeter sync-wallpapers "${1:?the wallpaper folder}" --to "$WALLPAPERS"
}

switch_to() {
    local from=$1 to=$2
    systemctl disable "$from.service"
    systemctl enable "$to.service"
    echo "$to starts at the next boot; $from stays installed ('sudo $0 $3' goes back)"
}

case ${1:-} in
    "" | -h | --help) sed -n '2,10p' "$0" ;;
    wallpapers) need_root; copy_wallpapers "${2:-}" ;;
    switch) need_root; switch_to lightdm greetd switch-back ;;
    switch-back) need_root; switch_to greetd lightdm switch ;;
    *) need_root; install_greeter "$@" ;;
esac

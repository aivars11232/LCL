#!/bin/sh
# Build and install lcl-remote, the PC side of LCL for Android, for the
# invoking user. Writes only under the user's own directories, needs no
# elevation, and prints every path it touches. ./uninstall.sh reverses it.
#
#   ~/.local/bin/lcl-remote                      the service and its commands
#   ~/.config/systemd/user/lcl-remote.service    a user service, NOT enabled
#
# Install LCL itself first (packaging/install.sh): the service loads the
# specification packages installed under ~/.local/share/lcl, and the
# workspace's Settings → Android devices finds lcl-remote beside
# lcl-workspace.
#
# Building needs Rust 1.89 or newer and the crates Cargo.lock names, which
# cargo downloads once. Pass --offline to use only what is already downloaded.
#
# Nothing starts by itself. To run the service now and at every login:
#
#   systemctl --user daemon-reload
#   systemctl --user enable --now lcl-remote
#
# or run `lcl-remote serve` in a terminal.
set -eu

here=$(cd "$(dirname "$0")" && pwd)
bin=${XDG_BIN_HOME:-$HOME/.local/bin}
units=${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user
offline=
case "${1:-}" in
    --offline) offline=--offline ;;
    "") ;;
    *) echo "usage: $0 [--offline]" >&2; exit 2 ;;
esac

# A build directory of its own, removed afterwards, so nothing stale from an
# earlier build is installed.
target=$(mktemp -d)
trap 'rm -rf "$target"' EXIT
CARGO_TARGET_DIR=$target cargo build --release --locked $offline --manifest-path "$here/Cargo.toml"

mkdir -p "$bin" "$units"
install -m 0755 "$target/release/lcl-remote" "$bin/lcl-remote"
echo "installed $bin/lcl-remote"

# One ExecStart word, quoted the way systemd reads it: backslash and double
# quote escaped, `%` doubled (specifiers), `$` doubled (variables).
systemd_quote() {
    printf '"%s"' "$(printf '%s' "$1" | sed -e 's/[\\"]/\\&/g' -e 's/%/%%/g' -e 's/\$/$$/g')"
}

exec_start=$(systemd_quote "$bin/lcl-remote")
sed "s|@EXEC@|$(printf '%s' "$exec_start" | sed 's/[|&\\]/\\&/g')|" "$here/lcl-remote.service.in" >"$units/lcl-remote.service"
chmod 0644 "$units/lcl-remote.service"
echo "installed $units/lcl-remote.service (not enabled)"

echo
echo "Pair a phone:  lcl-remote pair    (or Settings → Android devices in LCL Workspace)"
echo "Run the service now and at every login:"
echo "    systemctl --user daemon-reload"
echo "    systemctl --user enable --now lcl-remote"
echo "Phones connect on TCP port 47300 and look for this PC on UDP port 47301;"
echo "a firewall on this PC must let the local network reach them."
case ":$PATH:" in
    *":$bin:"*) ;;
    *) echo "note: $bin is not on your PATH." ;;
esac

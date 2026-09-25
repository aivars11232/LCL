#!/bin/sh
# Remove what install.sh installed, and nothing else.
#
# The PC's identity and its list of paired devices stay, under
# ~/.config/lcl/remote and ~/.local/state/lcl/remote, so reinstalling keeps
# every pairing. `--purge` deletes them too: every paired device then has to
# pair again with a new QR code, and this PC gets a new fingerprint.
set -eu

bin=${XDG_BIN_HOME:-$HOME/.local/bin}
units=${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user
purge=
case "${1:-}" in
    --purge) purge=1 ;;
    "") ;;
    *) echo "usage: $0 [--purge]" >&2; exit 2 ;;
esac

# Stop the service first, if this user's service manager runs it.
if [ -f "$units/lcl-remote.service" ] && command -v systemctl >/dev/null 2>&1; then
    if systemctl --user is-active --quiet lcl-remote.service 2>/dev/null ||
        systemctl --user is-enabled --quiet lcl-remote.service 2>/dev/null; then
        systemctl --user disable --now lcl-remote.service
        echo "stopped and disabled lcl-remote.service"
    fi
fi

for path in "$bin/lcl-remote" "$units/lcl-remote.service"; do
    if [ -e "$path" ]; then
        rm -f "$path"
        echo "removed $path"
    fi
done

if [ -n "$purge" ]; then
    for dir in "${XDG_CONFIG_HOME:-$HOME/.config}/lcl/remote" "${XDG_STATE_HOME:-$HOME/.local/state}/lcl/remote"; do
        if [ -d "$dir" ]; then
            rm -rf "$dir"
            echo "removed $dir (this PC's identity and its paired devices)"
        fi
    done
else
    echo "kept this PC's identity and paired devices; --purge removes them"
fi

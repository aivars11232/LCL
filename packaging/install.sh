#!/bin/sh
# Install LCL for the invoking user.
#
# Writes only under the user's own directories, needs no elevation, and prints
# every path it touches. ./uninstall.sh reverses it exactly.
#
#   ~/.local/bin/lcl                    the command-line tool
#   ~/.local/bin/lcl-workspace          the editor and debugger
#   ~/.local/bin/lcl-workspace-launch   what the desktop entry runs
#   ~/.local/share/lcl/LCL_Core_0.1.0   the specification package they load
#   ~/.local/share/applications         the desktop entry
#   ~/.local/share/mime/packages        the .lcl media type
#
# The specification package is installed beside the binaries rather than looked
# for, because the engine loads one exact approved package and "Ambient current
# directory and implied nearby files do not exist in portable LCL".
set -eu

here=$(cd "$(dirname "$0")" && pwd)
bin=${XDG_BIN_HOME:-$HOME/.local/bin}
data=${XDG_DATA_HOME:-$HOME/.local/share}

mkdir -p "$bin" "$data/lcl" "$data/applications" "$data/mime/packages"

# Replace @KEY@ with an exact value, reading standard input.
#
# `awk` with the value in a variable rather than `sed`, because a substituted
# path is arbitrary text: `&` in a sed replacement means "the whole match", and
# a home directory holding one would install a launcher pointing somewhere
# else. Nothing here interprets the value.
substitute() {
    awk -v key="$1" -v value="$2" '
        {
            out = ""
            rest = $0
            while ((at = index(rest, key)) > 0) {
                out = out substr(rest, 1, at - 1) value
                rest = substr(rest, at + length(key))
            }
            print out rest
        }'
}

# One desktop-entry argument, quoted as the specification requires.
#
# "Reserved characters are space, tab, newline, double quote, single quote,
# backslash ... and must be escaped with a backslash inside a quoted argument."
# Quoting unconditionally keeps an installation under a path with a space in it
# from silently becoming two arguments.
desktop_quote() {
    printf '"%s"' "$(printf '%s' "$1" | sed 's/[\\"`$]/\\&/g')"
}

install -m 0755 "$here/bin/lcl" "$bin/lcl"
install -m 0755 "$here/bin/lcl-workspace" "$bin/lcl-workspace"
echo "installed $bin/lcl"
echo "installed $bin/lcl-workspace"

rm -rf "$data/lcl/LCL_Core_0.1.0"
cp -r "$here/share/LCL_Core_0.1.0" "$data/lcl/LCL_Core_0.1.0"
echo "installed $data/lcl/LCL_Core_0.1.0"

# The launcher the desktop entry runs. It carries the installed binary, the
# installed specification package and the default project directory as absolute
# paths, so a menu launch needs no environment at all.
substitute '@BIN@' "$bin/lcl-workspace" < "$here/share/lcl-workspace-launch.in" \
    | substitute '@SPEC@' "$data/lcl/LCL_Core_0.1.0" \
    | substitute '@DEFAULT_PROJECT@' "$data/lcl/workspace" \
    > "$bin/lcl-workspace-launch"
chmod 0755 "$bin/lcl-workspace-launch"
echo "installed $bin/lcl-workspace-launch"

# The desktop entry names that launcher by absolute path, so it does not depend
# on the user's PATH.
substitute '@LAUNCHER@' "$(desktop_quote "$bin/lcl-workspace-launch")" \
    < "$here/share/lcl.desktop" > "$data/applications/lcl-workspace.desktop"
chmod 0644 "$data/applications/lcl-workspace.desktop"
echo "installed $data/applications/lcl-workspace.desktop"

cp "$here/share/lcl.xml" "$data/mime/packages/lcl.xml"
echo "installed $data/mime/packages/lcl.xml"

if command -v update-mime-database >/dev/null 2>&1; then
    update-mime-database "$data/mime"
    echo "updated   $data/mime"
else
    echo "note: update-mime-database is absent, so the .lcl media type is not registered" >&2
fi
if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "$data/applications" >/dev/null 2>&1 || true
fi

echo
echo "The desktop entry opens the workspace through:"
echo "    $bin/lcl-workspace-launch"
echo "With no document it opens $data/lcl/workspace; uninstall never removes that."
echo
echo "Every command needs the specification package. It is installed at:"
echo "    $data/lcl/LCL_Core_0.1.0"
echo
echo "Point a command at it with --spec, or export LCL_SPEC:"
echo "    export LCL_SPEC=$data/lcl/LCL_Core_0.1.0"
echo
case ":$PATH:" in
    *":$bin:"*) ;;
    *) echo "note: $bin is not on your PATH." ;;
esac

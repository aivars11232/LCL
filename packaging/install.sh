#!/bin/sh
# Install LCL for the invoking user.
#
# Writes only under the user's own directories, needs no elevation, and prints
# every path it touches. ./uninstall.sh reverses it exactly.
#
#   ~/.local/bin/lcl                    the command-line tool
#   ~/.local/bin/lcl-workspace          the editor and debugger
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

install -m 0755 "$here/bin/lcl" "$bin/lcl"
install -m 0755 "$here/bin/lcl-workspace" "$bin/lcl-workspace"
echo "installed $bin/lcl"
echo "installed $bin/lcl-workspace"

rm -rf "$data/lcl/LCL_Core_0.1.0"
cp -r "$here/share/LCL_Core_0.1.0" "$data/lcl/LCL_Core_0.1.0"
echo "installed $data/lcl/LCL_Core_0.1.0"

# The desktop entry names the installed workspace by absolute path, so it does
# not depend on the user's PATH.
sed "s|@BIN@|$bin/lcl-workspace|" "$here/share/lcl.desktop" > "$data/applications/lcl-workspace.desktop"
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

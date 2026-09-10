#!/bin/sh
# Register the `.lcl` media type for the invoking user.
#
# Writes only into the user's own XDG data directory. It installs nothing
# system-wide, requires no elevation, and touches nothing outside the two paths
# it prints. Run ./uninstall.sh to undo it exactly.
#
# This is desktop integration, not language configuration. Recognizing a file
# by its extension changes no document's meaning, and the LCL toolchain itself
# never uses the extension to decide anything: `lcl check` reads whatever path
# it is given.
set -eu

here=$(cd "$(dirname "$0")" && pwd)
data=${XDG_DATA_HOME:-$HOME/.local/share}
packages=$data/mime/packages

if ! command -v update-mime-database >/dev/null 2>&1; then
    echo "update-mime-database is not installed; on Arch Linux: pacman -S shared-mime-info" >&2
    exit 1
fi

mkdir -p "$packages"
cp "$here/lcl.xml" "$packages/lcl.xml"
update-mime-database "$data/mime"

echo "installed $packages/lcl.xml"
echo "updated   $data/mime"
echo
echo "Verify with:  xdg-mime query filetype some-document.lcl"
echo "Expected:     text/x-lcl"

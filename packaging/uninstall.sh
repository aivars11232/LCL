#!/bin/sh
# Remove everything install.sh installed, and nothing else.
set -eu

bin=${XDG_BIN_HOME:-$HOME/.local/bin}
data=${XDG_DATA_HOME:-$HOME/.local/share}

for path in \
    "$bin/lcl" \
    "$bin/lcl-workspace" \
    "$data/applications/lcl-workspace.desktop" \
    "$data/mime/packages/lcl.xml"
do
    if [ -e "$path" ]; then
        rm -f "$path"
        echo "removed $path"
    fi
done

if [ -d "$data/lcl/LCL_Core_0.1.0" ]; then
    rm -rf "$data/lcl/LCL_Core_0.1.0"
    echo "removed $data/lcl/LCL_Core_0.1.0"
fi
# Only if this installation left it empty.
rmdir "$data/lcl" 2>/dev/null && echo "removed $data/lcl" || true

if command -v update-mime-database >/dev/null 2>&1; then
    update-mime-database "$data/mime"
    echo "updated $data/mime"
fi
if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "$data/applications" >/dev/null 2>&1 || true
fi

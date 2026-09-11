#!/bin/sh
# Remove everything install.sh installed, and nothing else.
set -eu

bin=${XDG_BIN_HOME:-$HOME/.local/bin}
data=${XDG_DATA_HOME:-$HOME/.local/share}

for path in \
    "$bin/lcl" \
    "$bin/lcl-workspace" \
    "$bin/lcl-workspace-launch" \
    "$data/applications/lcl-workspace.desktop" \
    "$data/mime/packages/lcl.xml"
do
    if [ -e "$path" ]; then
        rm -f "$path"
        echo "removed $path"
    fi
done

# Exactly the icon files this installation wrote, one at a time. The hicolor
# theme is shared with every other application on the machine, so a recursive
# removal here would take other products' icons with it. Each directory is
# offered to `rmdir`, which declines unless this installation left it empty.
icons=$data/icons/hicolor
for relative in */apps/lcl-workspace.png */mimetypes/text-x-lcl.png; do
    for path in "$icons"/$relative; do
        if [ -f "$path" ]; then
            rm -f "$path"
            echo "removed $path"
        fi
    done
done
if [ -d "$icons" ]; then
    # Deepest first, so an emptied leaf lets its parent go too. Never -r.
    find "$icons" -depth -type d -exec rmdir {} + 2>/dev/null || true
    rmdir "$data/icons" 2>/dev/null || true
fi
if [ -d "$data/lcl/LCL_Core_0.1.0" ]; then
    rm -rf "$data/lcl/LCL_Core_0.1.0"
    echo "removed $data/lcl/LCL_Core_0.1.0"
fi
# The default project directory holds the operator's own documents, so it is
# never removed. `rmdir` takes the parent only when this installation left it
# empty, which it does not when a project is still sitting in it.
if [ -d "$data/lcl/workspace" ]; then
    echo "kept    $data/lcl/workspace (your documents)"
fi
rmdir "$data/lcl" 2>/dev/null && echo "removed $data/lcl" || true

if command -v update-mime-database >/dev/null 2>&1; then
    update-mime-database "$data/mime"
    echo "updated $data/mime"
fi
if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "$data/applications" >/dev/null 2>&1 || true
fi

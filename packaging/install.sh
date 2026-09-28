#!/bin/sh
# Install LCL for the invoking user.
#
# Writes only under the user's own directories, needs no elevation, and prints
# every path it touches. ./uninstall.sh reverses it exactly.
#
#   ~/.local/bin/lcl                    the command-line tool
#   ~/.local/bin/lcl-workspace          the editor and debugger
#   ~/.local/bin/lcl-workspace-launch   what the desktop entry runs
#   ~/.local/bin/lcl-update             finds, verifies and installs updates
#   ~/.local/bin/lcl-remote             replaced only when it is already
#                                       installed (remote/install.sh installs
#                                       it); its user service is never enabled,
#                                       started or changed here
#   ~/.local/share/lcl/LCL_Core_0.1.0   the specification package they load
#   ~/.local/share/lcl/LCL_Core_0.2.0   the localized-language package, when
#                                       the payload carries one
#   ~/.local/share/lcl/LCL_Core_0.3.0   the multi-file project package, when
#                                       the payload carries one
#   ~/.local/share/applications         the desktop entry
#   ~/.local/share/mime/packages        the .lcl media type
#   ~/.local/share/icons/hicolor/...    the application and document icons
#
# The specification package is installed beside the binaries rather than looked
# for, because the engine loads one exact approved package and "Ambient current
# directory and implied nearby files do not exist in portable LCL".
#
# Every file is published atomically — written beside its final name, then
# renamed over it — and a package directory is swapped in by rename, so a
# running program keeps the file it started from and an interrupted install
# never leaves half a file. `install.sh --list` prints every path an install
# writes and writes nothing; the updater copies exactly those aside first, so
# a failed update can be rolled back.
set -eu

here=$(cd "$(dirname "$0")" && pwd)
bin=${XDG_BIN_HOME:-$HOME/.local/bin}
data=${XDG_DATA_HOME:-$HOME/.local/share}

list=false
case "${1:-}" in
    --list) list=true ;;
    "") ;;
    *) echo "usage: $0 [--list]" >&2; exit 2 ;;
esac

if ! $list; then
    mkdir -p "$bin" "$data/lcl" "$data/applications" "$data/mime/packages"
fi

# Publish standard input as file $1 with mode $2: written beside it, renamed
# over it. With --list, only name it.
publish() {
    if $list; then
        echo "$1"
        cat >/dev/null
        return 0
    fi
    new="$(dirname "$1")/.$(basename "$1").new.$$"
    if cat >"$new" && chmod "$2" "$new" && mv -f "$new" "$1"; then
        return 0
    fi
    rm -f "$new"
    return 1
}

# Publish directory $1 as directory $2: copied beside it, swapped in by rename.
publish_dir() {
    if $list; then
        echo "$2"
        return 0
    fi
    rm -rf "$2.new.$$" "$2.old.$$"
    cp -r "$1" "$2.new.$$"
    if [ -e "$2" ]; then
        mv "$2" "$2.old.$$"
    fi
    mv "$2.new.$$" "$2"
    rm -rf "$2.old.$$"
}

installed() {
    $list || echo "installed $1"
}

# Replace @KEY@ with an exact value, reading standard input.
#
# `awk` rather than `sed`, because a substituted path is arbitrary text: `&` in
# a sed replacement means "the whole match", and a home directory holding one
# would install a launcher pointing somewhere else. The value reaches awk
# through the environment, not `-v`, because `-v` processes backslash escapes
# and would turn a `\t` in a path into a tab. ENVIRON is read as it is, so
# nothing here interprets the value.
substitute() {
    LCL_SUBSTITUTE_VALUE=$2 awk -v key="$1" '
        BEGIN { value = ENVIRON["LCL_SUBSTITUTE_VALUE"] }
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
#
# The value is also a string, and string escapes "are applied before the quoting
# rule", so every backslash the quoting wrote is doubled once more. A literal
# percent sign is written `%%`, because `%f` and its kind are field codes.
desktop_quote() {
    printf '"%s"' "$(printf '%s' "$1" | sed -e 's/[\\"`$]/\\&/g' -e 's/\\/\\\\/g' -e 's/%/%%/g')"
}

# One word a shell reads back as exactly this text: the value in single quotes,
# with each single quote in it written as '\''.
shell_quote() {
    printf "'%s'" "$(printf '%s' "$1" | sed "s/'/'\\\\''/g")"
}

publish "$bin/lcl" 0755 < "$here/bin/lcl"
publish "$bin/lcl-workspace" 0755 < "$here/bin/lcl-workspace"
installed "$bin/lcl"
installed "$bin/lcl-workspace"
if [ -f "$here/bin/lcl-update" ]; then
    publish "$bin/lcl-update" 0755 < "$here/bin/lcl-update"
    installed "$bin/lcl-update"
fi
# The remote service is installed by remote/install.sh, not here: a release
# replaces its binary only where it is already installed. Whether its service
# runs is left exactly as it was; the updater restarts it only if it was running.
if [ -f "$here/bin/lcl-remote" ] && [ -e "$bin/lcl-remote" ]; then
    publish "$bin/lcl-remote" 0755 < "$here/bin/lcl-remote"
    installed "$bin/lcl-remote"
fi

publish_dir "$here/share/LCL_Core_0.1.0" "$data/lcl/LCL_Core_0.1.0"
installed "$data/lcl/LCL_Core_0.1.0"

# A 0.2.0 payload also carries the Core 0.2.0 package. The launcher then passes
# it as --localized-spec; with none, the launcher passes nothing extra.
localized=
if [ -d "$here/share/LCL_Core_0.2.0" ]; then
    localized=$data/lcl/LCL_Core_0.2.0
    publish_dir "$here/share/LCL_Core_0.2.0" "$localized"
    installed "$localized"
fi

# A 0.3.0 payload also carries the Core 0.3.0 package, for multi-file projects
# and file roles. The launcher then passes it as --project-spec.
projects=
if [ -d "$here/share/LCL_Core_0.3.0" ]; then
    projects=$data/lcl/LCL_Core_0.3.0
    publish_dir "$here/share/LCL_Core_0.3.0" "$projects"
    installed "$projects"
fi

# The launcher the desktop entry runs. It carries the installed binary, the
# installed specification package and the default project directory as absolute
# paths, so a menu launch needs no environment at all. Each is written as one
# quoted shell word, so a space or a quote in a path stays part of the path.
substitute '@BIN@' "$(shell_quote "$bin/lcl-workspace")" < "$here/share/lcl-workspace-launch.in" \
    | substitute '@SPEC@' "$(shell_quote "$data/lcl/LCL_Core_0.1.0")" \
    | substitute '@LOCALIZED_SPEC@' "$(shell_quote "$localized")" \
    | substitute '@PROJECT_SPEC@' "$(shell_quote "$projects")" \
    | substitute '@DEFAULT_PROJECT@' "$(shell_quote "$data/lcl/workspace")" \
    | publish "$bin/lcl-workspace-launch" 0755
installed "$bin/lcl-workspace-launch"

# The desktop entry names that launcher by absolute path, so it does not depend
# on the user's PATH.
substitute '@LAUNCHER@' "$(desktop_quote "$bin/lcl-workspace-launch")" \
    < "$here/share/lcl.desktop" | publish "$data/applications/lcl-workspace.desktop" 0644
installed "$data/applications/lcl-workspace.desktop"

publish "$data/mime/packages/lcl.xml" 0644 < "$here/share/lcl.xml"
installed "$data/mime/packages/lcl.xml"

# The application icon the desktop entry names, and the document icon for the
# media type registered above. Both are derived from one supplied master, so a
# launcher and the files it opens look like one product.
#
# Each file is copied individually into the shared hicolor theme. The theme
# belongs to every application on the machine, so nothing here creates, empties
# or removes a directory that is not ours to touch, and uninstall removes
# exactly the files this loop wrote.
icons=$data/icons/hicolor
for source in "$here/share/icons/hicolor/"*/*/*.png; do
    [ -f "$source" ] || continue
    relative=${source#"$here/share/icons/hicolor/"}
    $list || mkdir -p "$icons/$(dirname "$relative")"
    publish "$icons/$relative" 0644 < "$source"
done
installed "$icons/*/apps/lcl-workspace.png"
installed "$icons/*/mimetypes/text-x-lcl.png"

# No icon cache is generated. `gtk-update-icon-cache` writes an
# `icon-theme.cache` into the theme root, which is a lookup accelerator rather
# than a requirement: the icon theme specification's search algorithm reads the
# directories, and every desktop falls back to doing so. Generating one would
# put a shared file into the theme that this installation did not own and could
# not safely remove, and the icons resolve without it.

if $list; then
    exit 0
fi

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
if [ -n "$localized" ]; then
    echo "Localized (LCL 0.2.0) documents also need the 0.2.0 package:"
    echo "    export LCL_LOCALIZED_SPEC=$localized"
    echo
fi
if [ -n "$projects" ]; then
    echo "Multi-file projects (LCL 0.3.0) also need the 0.3.0 package:"
    echo "    export LCL_PROJECT_SPEC=$projects"
    echo
fi
case ":$PATH:" in
    *":$bin:"*) ;;
    *) echo "note: $bin is not on your PATH." ;;
esac

#!/bin/sh
# Remove the `.lcl` media type registration this user installed.
set -eu

data=${XDG_DATA_HOME:-$HOME/.local/share}
packages=$data/mime/packages

if [ ! -f "$packages/lcl.xml" ]; then
    echo "nothing to remove: $packages/lcl.xml is not present"
    exit 0
fi

rm "$packages/lcl.xml"
if command -v update-mime-database >/dev/null 2>&1; then
    update-mime-database "$data/mime"
fi

echo "removed $packages/lcl.xml"

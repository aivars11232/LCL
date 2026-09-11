#!/bin/sh
# Build one LCL release tarball, from a clean offline build.
#
# What it produces, under releases/:
#
#   lcl-<version>-linux-x86_64.tar.gz   the payload
#   lcl-<version>-linux-x86_64.sha256   its checksum
#   lcl-<version>-PROVENANCE.txt        what it was built from, and how
#
# What it does not do: install anything, touch the canonical package, or reach
# the network. `--locked --offline` is not a convenience here; a release that
# resolved a dependency at build time would not be the thing that was tested,
# and this workspace has no dependency to resolve.
set -eu

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/.." && pwd)
impl=$root/impl
releases=$root/releases
canonical=$root/canonical/LCL_Core_0.1.0

version=$(sed -n 's/^version = "\(.*\)"$/\1/p' "$impl/Cargo.toml" | head -1)
[ -n "$version" ] || { echo "cannot read the product version from impl/Cargo.toml" >&2; exit 1; }
name=lcl-$version-linux-x86_64
staging=$(mktemp -d)
trap 'rm -rf "$staging"' EXIT

echo "building $name"
( cd "$impl" && cargo build --release --offline --locked -p lcl-cli -p lcl-workspace )

payload=$staging/$name
mkdir -p "$payload/bin" "$payload/share"

install -m 0755 "$impl/target/release/lcl" "$payload/bin/lcl"
install -m 0755 "$impl/target/release/lcl-workspace" "$payload/bin/lcl-workspace"

# The engine refuses to load a package that is not the approved release, so the
# package travels with the binaries rather than being looked for at run time.
cp -r "$canonical" "$payload/share/LCL_Core_0.1.0"

cp "$here/install.sh" "$here/uninstall.sh" "$payload/"
chmod 0755 "$payload/install.sh" "$payload/uninstall.sh"
cp "$here/lcl.desktop" "$payload/share/lcl.desktop"
cp "$root/impl/integration/linux/lcl.xml" "$payload/share/lcl.xml"
cp "$here/README.md" "$payload/README.md"

mkdir -p "$releases"
tar -czf "$releases/$name.tar.gz" -C "$staging" "$name"
( cd "$releases" && sha256sum "$name.tar.gz" > "$name.sha256" )

# Provenance: what this is, what it was built from, and how to rebuild it.
commit=$(cd "$root" && git rev-parse HEAD 2>/dev/null || echo "not a git checkout")
dirty=$(cd "$root" && git status --porcelain 2>/dev/null | wc -l)
identity=$("$payload/bin/lcl" spec --spec "$payload/share/LCL_Core_0.1.0" | sed -n 's/^ *identity *//p')
{
    echo "LCL product release provenance"
    echo
    echo "artifact:         $name.tar.gz"
    echo "sha256:           $(cut -d' ' -f1 < "$releases/$name.sha256")"
    echo "product version:  $version"
    echo "language version: $("$payload/bin/lcl" version | sed -n 's/^language //p')"
    echo "engine protocol:  $("$payload/bin/lcl" version | sed -n 's/^protocol //p')"
    echo "package identity: $identity"
    echo "source commit:    $commit"
    echo "uncommitted files at build time: $dirty"
    echo "toolchain:        $(rustc --version)"
    echo "built on:         $(uname -srm)"
    echo
    echo "Rebuild with:"
    echo "    packaging/build_release.sh"
    echo
    echo "The payload's own checksums, for every file in the tarball:"
    echo
    ( cd "$staging" && find "$name" -type f -print0 | sort -z | xargs -0 sha256sum )
} > "$releases/lcl-$version-PROVENANCE.txt"

echo
echo "wrote $releases/$name.tar.gz"
echo "wrote $releases/$name.sha256"
echo "wrote $releases/lcl-$version-PROVENANCE.txt"

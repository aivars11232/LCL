#!/bin/sh
# Build one LCL release candidate, bound to the exact source it was built from.
#
# What it produces, under an output directory of its own:
#
#   <name>.tar.gz              the payload
#   <name>.sha256              its checksum
#   <name>-source.tar.gz       every source byte the build consumed
#   <name>-source.sha256       its checksum
#   SOURCE_MANIFEST.sha256     one line per source file
#   SOURCE_CHANGES.patch       tracked modifications, when the tree is dirty
#   lcl-<version>-PROVENANCE.txt
#
# What it does not do: install anything, touch the canonical package, reach the
# network, or write into releases/ itself. `--locked --offline` is not a
# convenience; a release that resolved a dependency at build time would not be
# the thing that was tested, and this workspace has no dependency to resolve.
#
# ## Source identity, recorded before anything is built
#
# The previous recipe recorded a commit and a count of uncommitted files, and
# recorded them *after* writing the artifacts. Neither half worked. A count is
# not an identity, so the commit could not reconstruct what was actually built;
# and reading the working tree after generating output measured a tree the run
# had already changed.
#
# So the source set is enumerated, hashed and exported first, and the payload
# is built from that recorded state. Reconstruction needs no access to this
# machine's history: the source archive holds the bytes, the manifest holds
# their checksums, and the provenance names both.
#
# ## An isolated build
#
# `CARGO_TARGET_DIR` points somewhere this script owns, so a candidate cannot
# pick up a stale binary that a developer's earlier `cargo build` left in
# impl/target/release. The build directory is removed afterwards unless
# LCL_KEEP_BUILD is set.
#
# ## Output location
#
#   LCL_RELEASE_OUT=<dir>   where to write. Defaults to
#                           releases/candidates/<name>-<short source id>.
#
# It never defaults to releases/ itself. The archives there are the published
# historical release, and a candidate that overwrote them would destroy the
# evidence it exists to be compared against.
set -eu

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/.." && pwd)
impl=$root/impl
canonical=$root/canonical/LCL_Core_0.1.0

version=$(sed -n 's/^version = "\(.*\)"$/\1/p' "$impl/Cargo.toml" | head -1)
[ -n "$version" ] || { echo "cannot read the product version from impl/Cargo.toml" >&2; exit 1; }
name=lcl-$version-linux-x86_64

# ---------------------------------------------------------------------------
# 1. Source identity, before any output exists
# ---------------------------------------------------------------------------

staging=$(mktemp -d)
trap 'rm -rf "$staging"' EXIT

# Every file the build may read: tracked files plus untracked ones that are not
# ignored. `releases/` is excluded because it holds published archives, which
# are outputs of earlier runs rather than inputs to this one.
( cd "$root" && git ls-files --cached --others --exclude-standard ) \
    | grep -v '^releases/' \
    | LC_ALL=C sort > "$staging/source-files.txt" \
    || { echo "not a git checkout; source identity cannot be recorded" >&2; exit 1; }

manifest=$staging/SOURCE_MANIFEST.sha256
( cd "$root" && xargs -d '\n' -r sha256sum < "$staging/source-files.txt" ) > "$manifest"
source_count=$(wc -l < "$manifest")
# One digest standing for the whole source set: the checksum of the checksums.
source_id=$(sha256sum "$manifest" | cut -d' ' -f1)
short_source_id=$(printf '%.12s' "$source_id")

out=${LCL_RELEASE_OUT:-$root/releases/candidates/$name-$short_source_id}
mkdir -p "$out"

# The exact bytes, not a reference to them. A candidate built from an
# uncommitted tree is reconstructible from this archive alone.
tar -czf "$out/$name-source.tar.gz" -C "$root" -T "$staging/source-files.txt"
( cd "$out" && sha256sum "$name-source.tar.gz" > "$name-source.sha256" )
cp "$manifest" "$out/SOURCE_MANIFEST.sha256"

commit=$(cd "$root" && git rev-parse HEAD 2>/dev/null || echo "not a git checkout")
dirty=$(cd "$root" && git status --porcelain 2>/dev/null | wc -l)
if [ "$dirty" -gt 0 ]; then
    # What this tree is, relative to the commit it names. Untracked files are
    # already whole in the source archive; this is the tracked difference.
    ( cd "$root" && git diff HEAD --binary ) > "$out/SOURCE_CHANGES.patch"
else
    rm -f "$out/SOURCE_CHANGES.patch"
fi

echo "source:  $source_count files, id $short_source_id"
echo "output:  $out"

# ---------------------------------------------------------------------------
# 2. Build, in a directory this script owns
# ---------------------------------------------------------------------------

build=${LCL_BUILD_DIR:-$staging/build}
mkdir -p "$build"
echo "building $name"
( cd "$impl" && CARGO_TARGET_DIR="$build" cargo build --release --offline --locked \
    -p lcl-cli -p lcl-workspace )

payload=$staging/$name
mkdir -p "$payload/bin" "$payload/share"

install -m 0755 "$build/release/lcl" "$payload/bin/lcl"
install -m 0755 "$build/release/lcl-workspace" "$payload/bin/lcl-workspace"

# The engine refuses to load a package that is not the approved release, so the
# package travels with the binaries rather than being looked for at run time.
cp -r "$canonical" "$payload/share/LCL_Core_0.1.0"

cp "$here/install.sh" "$here/uninstall.sh" "$payload/"
chmod 0755 "$payload/install.sh" "$payload/uninstall.sh"
cp "$here/lcl.desktop" "$payload/share/lcl.desktop"
cp "$here/lcl-workspace-launch.in" "$payload/share/lcl-workspace-launch.in"
cp "$root/impl/integration/linux/lcl.xml" "$payload/share/lcl.xml"
cp "$here/README.md" "$payload/README.md"

# The installed application and document icons, and the master they were
# derived from. The master travels so that a payload carries the provenance of
# its own artwork; nothing at run time reads it.
cp -r "$here/icons" "$payload/share/icons"
mkdir -p "$payload/share/brand"
cp "$root/assets/brand/lcl-logo-master.png" "$payload/share/brand/lcl-logo-master.png"
cp "$root/assets/brand/BRAND_ASSETS.sha256" "$payload/share/brand/BRAND_ASSETS.sha256"

tar -czf "$out/$name.tar.gz" -C "$staging" "$name"
( cd "$out" && sha256sum "$name.tar.gz" > "$name.sha256" )

# ---------------------------------------------------------------------------
# 3. Provenance
# ---------------------------------------------------------------------------

identity=$("$payload/bin/lcl" spec --spec "$payload/share/LCL_Core_0.1.0" | sed -n 's/^ *identity *//p')
{
    echo "LCL product release candidate provenance"
    echo
    echo "SOURCE, recorded before this build produced anything"
    echo
    echo "source id:        $source_id"
    echo "  (the SHA-256 of SOURCE_MANIFEST.sha256, which lists every source file)"
    echo "source files:     $source_count"
    echo "source archive:   $name-source.tar.gz"
    echo "source sha256:    $(cut -d' ' -f1 < "$out/$name-source.sha256")"
    echo "git commit:       $commit"
    echo "uncommitted files at the moment the source was recorded: $dirty"
    if [ "$dirty" -gt 0 ]; then
        echo "tracked changes:  SOURCE_CHANGES.patch"
        echo "                  (untracked files are whole in the source archive)"
    fi
    echo
    echo "TOOLCHAIN AND INPUTS"
    echo
    echo "toolchain:        $(rustc --version)"
    echo "cargo:            $(cargo --version)"
    echo "declared minimum: $(sed -n 's/^rust-version = "\(.*\)"$/\1/p' "$impl/Cargo.toml" | head -1)"
    echo "lockfile sha256:  $(sha256sum "$impl/Cargo.lock" | cut -d' ' -f1)"
    echo "build flags:      --release --offline --locked"
    echo "build directory:  isolated, not impl/target"
    echo "built on:         $(uname -srm)"
    echo
    echo "ARTIFACT"
    echo
    echo "artifact:         $name.tar.gz"
    echo "sha256:           $(cut -d' ' -f1 < "$out/$name.sha256")"
    echo "product version:  $version"
    echo "language version: $("$payload/bin/lcl" version | sed -n 's/^language //p')"
    echo "engine protocol:  $("$payload/bin/lcl" version | sed -n 's/^protocol //p')"
    echo "package identity: $identity"
    echo
    echo "Bit-for-bit reproducibility is not claimed. Rust embeds build paths"
    echo "and no attempt is made to normalize them, so two runs from identical"
    echo "source may differ. What is claimed is the other direction: the source"
    echo "these bytes were built from is recorded exactly."
    echo
    echo "To reconstruct the source and rebuild:"
    echo
    echo "    tar -xzf $name-source.tar.gz -C <dir>"
    echo "    sha256sum -c SOURCE_MANIFEST.sha256   # from <dir>"
    echo "    cd <dir> && packaging/build_release.sh"
    echo
    echo "BRAND ASSETS IN THIS PAYLOAD"
    echo
    sed 's/^/    /' "$root/assets/brand/BRAND_ASSETS.sha256"
    echo
    echo "PAYLOAD, every file in the tarball:"
    echo
    ( cd "$staging" && find "$name" -type f -print0 | LC_ALL=C sort -z | xargs -0 sha256sum )
} > "$out/lcl-$version-PROVENANCE.txt"

if [ "${LCL_KEEP_BUILD:-}" = "" ]; then
    rm -rf "$build"
fi

echo
echo "wrote $out/$name.tar.gz"
echo "wrote $out/$name.sha256"
echo "wrote $out/$name-source.tar.gz"
echo "wrote $out/SOURCE_MANIFEST.sha256"
echo "wrote $out/lcl-$version-PROVENANCE.txt"
echo
echo "The published release in releases/ was not touched."

#!/bin/sh
# Build one LCL release candidate from an exact, verified snapshot of its source.
#
#   packaging/build_release.sh
#   packaging/build_release.sh --reconstruct <source archive> <new directory>
#
# What a build produces, under an output directory of its own:
#
#   <name>.tar.gz              the payload
#   <name>.sha256              its checksum
#   <name>-source.tar.gz       every source byte the build consumed, and the inventory
#   <name>-source.sha256       its checksum
#   SOURCE_INVENTORY.tsv       every source file, with its mode, size and digest
#   SOURCE_CHANGES.patch       tracked modifications, when a checkout has them
#   lcl-<version>-PROVENANCE.txt
#
# What it does not do: install anything, touch the canonical package, reach the
# network, or write into releases/ itself. `--locked --offline` is not a
# convenience; a release that resolved a dependency at build time would not be
# the thing that was tested, and this workspace has no dependency to resolve.
#
# ## Only the captured source is built
#
# The previous recipe recorded the source and then compiled and copied from the
# live checkout, so a change made in between was built but never recorded. It
# also ran `git ls-files` in a pipeline that reported only the last command's
# status, so a failed or partial listing was accepted as the whole source.
#
# Now every step of the listing is checked, each file's mode, size and digest go
# into SOURCE_INVENTORY.tsv, the listed bytes are archived, and the archive is
# unpacked into a new snapshot that must match the inventory exactly. The
# compiler, the bundled specification, the scripts, icons and documents all read
# that snapshot. The live tree is not read again.
#
# ## SOURCE_INVENTORY.tsv
#
# One line per file, in byte order of path, the fields separated by one tab:
#
#   file   0644 or 0755   size in bytes   sha256   path from the root
#
# The mode records only whether the file is executable. Only regular files are
# listed. A path is printable ASCII without a backslash, is not absolute, has no
# empty, `.` or `..` component, and appears once. The inventory does not list
# itself, and the source id is its SHA-256.
#
# ## Where the list comes from
#
# In a Git checkout: the tracked files and the untracked ones that are not
# ignored, leaving out releases/, which holds the outputs of earlier runs. Each
# must be a regular file.
#
# With no .git: the tree must be a source export, holding SOURCE_INVENTORY.tsv
# and exactly the files it names, with their modes, sizes and digests; releases/
# is ignored there too. `--reconstruct` makes an export from a source archive.
# Every member must be a regular file with a safe name, named once by the
# archive's own inventory, and that is checked before anything is extracted, so
# an absolute path, `..`, a link or a duplicate never reaches the disk.
#
# ## Directories this script owns, and nothing else
#
# Every run works inside one new directory from `mktemp -d`: the snapshot, the
# build (it is CARGO_TARGET_DIR, so no stale binary from impl/target can be
# picked up), the payload and the finished artifacts. It is removed when the run
# succeeds and kept, with its path printed, when the run fails, as the evidence
# of what happened. Nothing else is ever removed. LCL_BUILD_DIR and
# LCL_KEEP_BUILD, which pointed the build and its removal at a caller's
# directory, are refused.
#
# ## Output location
#
#   LCL_RELEASE_OUT=<dir>   where to write. Defaults to
#                           releases/candidates/<name>-<short source id>.
#   LCL_RELEASE_VERSION=<v> the language release the candidate is for: 0.1.0
#                           or 0.2.0. Defaults to the product version in
#                           impl/Cargo.toml. A 0.2.0 candidate bundles the
#                           Core 0.1.0 package and the Core 0.2.0 package, whose
#                           localized documents the tools judge alongside 0.1.0
#                           ones; a 0.1.0 candidate bundles only Core 0.1.0.
#
# The directory must not exist yet and its parent must; inside the source tree
# it may only be under releases/. It is created only after every artifact has
# been built and checked, and a candidate is never written into or over anything
# that was already there. The default is never releases/ itself: the archives
# there are the published historical release, and a candidate that overwrote
# them would destroy the evidence it exists to be compared against.
set -eu

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/.." && pwd)
root_real=$(cd "$root" && pwd -P)
tab=$(printf '\t')
staging=

refuse() {
    echo "refused: $*" >&2
    exit 1
}

# The run's own directory is removed after success and kept as evidence after a
# failure. Only the exact directory mktemp returned is ever removed.
finish() {
    status=$?
    if [ -n "$staging" ]; then
        if [ "$status" -ne 0 ]; then
            echo "evidence kept in $staging" >&2
        elif [ -d "$staging" ] && [ ! -L "$staging" ]; then
            rm -rf -- "$staging" || {
                echo "could not remove $staging" >&2
                status=1
            }
        fi
    fi
    exit "$status"
}

# Refuse an output directory that exists, whose parent does not, or that would
# put output inside the source tree anywhere but under releases/.
check_output() {
    if [ -z "$1" ] || [ -e "$1" ] || [ -L "$1" ]; then
        refuse "the output directory '$1' is empty or already exists; a candidate is never written over anything"
    fi
    [ -d "$(dirname "$1")" ] || refuse "the directory that would hold $1 does not exist"
    parent=$(cd "$(dirname "$1")" && pwd -P)
    case "$parent/" in
        "$root_real"/releases/*) ;;
        "$root_real"/*) refuse "the output directory $1 is inside the source tree" ;;
    esac
}

# Refuse unless standard input is paths in byte order, each once, each a plain
# relative path of printable ASCII.
check_paths() {
    LC_ALL=C awk '
        function reject(reason) {
            printf "refused: %s: %s\n", reason, $0 > "/dev/stderr"
            failed = 1
            exit
        }
        { path = $0 "" }
        path == "" || path ~ /^\// || path ~ /\/$/ || path ~ /\/\// { reject("unsafe path") }
        ("/" path "/") ~ /\/\.\.?\// || path ~ /[^ -~]/ || path ~ /\\/ { reject("unsafe path") }
        NR > 1 && path == previous { reject("duplicate path") }
        NR > 1 && path < previous { reject("paths out of order") }
        { previous = path }
        END { exit failed }'
}

# Refuse a malformed inventory $1; otherwise write its paths, checked, to $2.
inventory_paths() {
    [ -s "$1" ] || refuse "SOURCE_INVENTORY.tsv is empty"
    [ "$(tail -c 1 "$1" | od -An -tx1 | tr -d ' ')" = 0a ] ||
        refuse "SOURCE_INVENTORY.tsv does not end with a line break"
    LC_ALL=C awk -F "$tab" '
        NF != 5 || $1 != "file" || ($2 != "0644" && $2 != "0755") || $3 !~ /^[0-9]+$/ ||
        $4 !~ /^[0-9a-f]+$/ || length($4) != 64 {
            printf "refused: line %d of SOURCE_INVENTORY.tsv is malformed: %s\n", NR, $0 > "/dev/stderr"
            failed = 1
            exit
        }
        { print $5 }
        END { exit failed }' "$1" > "$2"
    check_paths < "$2"
}

# For the paths listed in $2, relative to directory $1, write one line each to
# $3, in the same order: executable class, size and SHA-256.
file_facts() {
    ( cd "$1" && xargs -d '\n' -r stat -c '%a %s' -- < "$2" ) > "$3.stat" ||
        refuse "cannot read the metadata of every file listed in $2"
    ( cd "$1" && xargs -d '\n' -r sha256sum -- < "$2" ) > "$3.sha" ||
        refuse "cannot read every file listed in $2"
    paste -d ' ' "$3.stat" "$3.sha" | LC_ALL=C awk '{
        mode = (substr($1, length($1) - 2) ~ /[1357]/) ? "0755" : "0644"
        print mode, $2, $3
    }' > "$3"
}

# Refuse unless directory $1 holds exactly the files inventory $2 names, with
# the same executable class, size and digest, plus SOURCE_INVENTORY.tsv and,
# when $3 is ignore-releases, anything under releases/.
verify_tree() {
    inventory_paths "$2" "$staging/verify-paths"
    ( cd "$1" && find . ! -type d ) > "$staging/verify-found.raw" ||
        refuse "cannot list every file under $1"
    LC_ALL=C awk -v ignore="${3:-}" '
        { sub(/^\.\//, "") }
        ignore == "ignore-releases" && /^releases\// { next }
        { print }' "$staging/verify-found.raw" > "$staging/verify-found.unsorted"
    LC_ALL=C sort "$staging/verify-found.unsorted" > "$staging/verify-found"
    { cat "$staging/verify-paths"; echo SOURCE_INVENTORY.tsv; } |
        LC_ALL=C sort > "$staging/verify-expected"
    missing=$(LC_ALL=C comm -23 "$staging/verify-expected" "$staging/verify-found" | head -n 1)
    [ -z "$missing" ] || refuse "$missing is named by SOURCE_INVENTORY.tsv but is not in $1"
    unexpected=$(LC_ALL=C comm -13 "$staging/verify-expected" "$staging/verify-found" | head -n 1)
    [ -z "$unexpected" ] || refuse "$unexpected is in $1 but not in SOURCE_INVENTORY.tsv"
    ( cd "$1" && find . ! -type d ! -type f ) > "$staging/verify-special" ||
        refuse "cannot list every file under $1"
    LC_ALL=C awk -v ignore="${3:-}" '
        ignore == "ignore-releases" && /^\.\/releases\// { next }
        { print "refused: " $0 " is not a regular file" > "/dev/stderr"; failed = 1; exit }
        END { exit failed }' "$staging/verify-special"
    file_facts "$1" "$staging/verify-paths" "$staging/verify-facts"
    LC_ALL=C awk -F "$tab" -v facts="$staging/verify-facts" '
        (getline fact < facts) <= 0 || fact != ($2 " " $3 " " $4) {
            printf "refused: %s does not match SOURCE_INVENTORY.tsv\n", $5 > "/dev/stderr"
            failed = 1
            exit
        }
        END { exit failed }' "$2"
}

# Refuse archive $1 unless every member is a regular file with a safe name, each
# named once, and write the sorted names to $2. Nothing is extracted. Listing in
# the C locale makes tar escape every byte outside printable ASCII, so each
# member is exactly one line and an unusual name is refused, not decoded.
check_members() {
    LC_ALL=C tar -tPzf "$1" > "$2.names" || refuse "cannot list the members of $1"
    LC_ALL=C tar -tvPzf "$1" > "$2.long" || refuse "cannot list the members of $1"
    LC_ALL=C awk -v long="$2.long" '
        (getline line < long) <= 0 || substr(line, 1, 1) != "-" {
            printf "refused: archive member %s is not a regular file\n", $0 > "/dev/stderr"
            failed = 1
            exit
        }
        END { exit failed }' "$2.names"
    LC_ALL=C sort "$2.names" > "$2"
    check_paths < "$2"
}

# Refuse unless the sorted member names in $1 are exactly the paths in $2 and
# SOURCE_INVENTORY.tsv.
same_members() {
    { cat "$2"; echo SOURCE_INVENTORY.tsv; } | LC_ALL=C sort > "$1.expected"
    [ -z "$(LC_ALL=C comm -3 "$1.expected" "$1")" ] ||
        refuse "the archive's members are not exactly the files its inventory names"
}

# Every file under $1/$name: executable class, size, SHA-256 and path, to $2.
payload_manifest() {
    ( cd "$1" && find "$name" -type f ) > "$2.unsorted" ||
        refuse "cannot list the payload under $1"
    LC_ALL=C sort "$2.unsorted" > "$2.list"
    file_facts "$1" "$2.list" "$2.facts"
    LC_ALL=C awk -v list="$2.list" '(getline path < list) > 0 { print $0, path }' \
        "$2.facts" > "$2"
}

[ -z "${LCL_BUILD_DIR+set}" ] ||
    refuse "LCL_BUILD_DIR is no longer supported: every build uses a new directory this run creates itself"
[ -z "${LCL_KEEP_BUILD+set}" ] ||
    refuse "LCL_KEEP_BUILD is no longer supported: a failed run keeps its directory, a successful one removes it"

# ---------------------------------------------------------------------------
# Reconstruction: a source export from a source archive, and nothing else
# ---------------------------------------------------------------------------

if [ "${1:-}" = "--reconstruct" ]; then
    [ "$#" -eq 3 ] ||
        refuse "usage: build_release.sh --reconstruct <source archive> <new directory>"
    archive=$2
    into=$3
    [ -f "$archive" ] || refuse "there is no source archive at $archive"
    if [ -z "$into" ] || [ -e "$into" ] || [ -L "$into" ]; then
        refuse "'$into' is empty or already exists; reconstruction only makes a new directory"
    fi
    [ -d "$(dirname "$into")" ] || refuse "the directory that would hold $into does not exist"
    staging=$(mktemp -d)
    trap finish EXIT
    check_members "$archive" "$staging/members"
    LC_ALL=C tar -xzOf "$archive" SOURCE_INVENTORY.tsv > "$staging/SOURCE_INVENTORY.tsv" ||
        refuse "$archive holds no readable SOURCE_INVENTORY.tsv"
    inventory_paths "$staging/SOURCE_INVENTORY.tsv" "$staging/archive-paths"
    same_members "$staging/members" "$staging/archive-paths"
    mkdir "$into"
    tar -xzf "$archive" -C "$into" --no-same-owner
    verify_tree "$into" "$staging/SOURCE_INVENTORY.tsv"
    echo "reconstructed source $(sha256sum < "$staging/SOURCE_INVENTORY.tsv" | cut -d' ' -f1)"
    echo "into $into"
    exit 0
fi

if [ -n "${LCL_RELEASE_OUT+set}" ]; then
    check_output "$LCL_RELEASE_OUT"
fi
staging=$(mktemp -d)
trap finish EXIT
case "$(cd "$staging" && pwd -P)/" in
    "$root_real"/*) refuse "TMPDIR is inside the source tree, so this run's own directory would be read as source" ;;
esac
inventory=$staging/SOURCE_INVENTORY.tsv

# ---------------------------------------------------------------------------
# 1. The source set and its inventory, before anything is built
# ---------------------------------------------------------------------------

if [ -e "$root/.git" ]; then
    from_git=yes
    # The listing goes to a file and its status is checked on its own. In a
    # pipeline that status is lost, and a partial list would pass for the source.
    status=0
    ( cd "$root" && git ls-files -z --cached --others --exclude-standard ) \
        > "$staging/git-ls-files" || status=$?
    [ "$status" -eq 0 ] ||
        refuse "git ls-files failed with status $status, so the source set cannot be recorded"
    tr '\0' '\n' < "$staging/git-ls-files" > "$staging/git-files"
    LC_ALL=C awk '!/^releases\// && $0 != "SOURCE_INVENTORY.tsv"' \
        "$staging/git-files" > "$staging/source-unsorted"
    LC_ALL=C sort "$staging/source-unsorted" > "$staging/source-files"
    [ -s "$staging/source-files" ] || refuse "no source files were enumerated"
    check_paths < "$staging/source-files"
    # Desktop-environment metadata is not source. It carries machine-local
    # settings — a `.directory` names an icon path on one person's computer —
    # and every tracked file outside releases/ is recorded in
    # SOURCE_INVENTORY.tsv, hashed into the source id and shipped inside the
    # candidate's source archive. The list is exactly these three file names:
    # nothing is judged by its extension, so a compressed source fixture is
    # inventoried like any other source file and the published archives under
    # releases/ are untouched.
    while IFS= read -r path; do
        case "$path" in
        .directory | */.directory | .DS_Store | */.DS_Store | Thumbs.db | */Thumbs.db)
            refuse "$path is desktop metadata rather than source; remove it from the tree before building"
            ;;
        esac
    done < "$staging/source-files"
    # `--cached` still lists a tracked file that has been deleted from the tree.
    while IFS= read -r path; do
        if [ ! -f "$root/$path" ] || [ -L "$root/$path" ]; then
            refuse "$path is listed as source but is not a regular file"
        fi
    done < "$staging/source-files"
    file_facts "$root" "$staging/source-files" "$staging/source-facts"
    LC_ALL=C awk -v OFS="$tab" -v list="$staging/source-files" '
        (getline path < list) > 0 { print "file", $1, $2, $3, path }' \
        "$staging/source-facts" > "$inventory"

    commit=$(cd "$root" && git rev-parse HEAD) || refuse "git rev-parse HEAD failed"
    ( cd "$root" && git status --porcelain ) > "$staging/git-status" ||
        refuse "git status failed, so the uncommitted entries cannot be recorded"
    ( cd "$root" && git diff HEAD --binary ) > "$staging/SOURCE_CHANGES.patch" ||
        refuse "git diff failed, so the tracked changes cannot be recorded"
    origin="git checkout"
else
    from_git=no
    if [ ! -f "$root/SOURCE_INVENTORY.tsv" ] || [ -L "$root/SOURCE_INVENTORY.tsv" ]; then
        refuse "$root has no .git and no SOURCE_INVENTORY.tsv, so its source cannot be identified"
    fi
    cp "$root/SOURCE_INVENTORY.tsv" "$inventory"
    verify_tree "$root" "$inventory" ignore-releases
    commit="not recorded: no git metadata"
    origin="source export without git metadata, verified against its SOURCE_INVENTORY.tsv"
fi

source_id=$(sha256sum < "$inventory" | cut -d' ' -f1)
[ "${#source_id}" -eq 64 ] || refuse "cannot hash SOURCE_INVENTORY.tsv"
short_source_id=$(printf '%.12s' "$source_id")
source_count=$(wc -l < "$inventory")

# ---------------------------------------------------------------------------
# 2. The snapshot every later step reads
# ---------------------------------------------------------------------------

# Exactly the listed files and the inventory: no directory entries, no
# recursion, no owner names. The members are checked before anything is
# unpacked, and the unpacked snapshot must match the inventory, so a file that
# changed between hashing and archiving is refused rather than built.
cut -f5 "$inventory" > "$staging/archive-list"
tar -czf "$staging/source.tar.gz" --no-recursion --verbatim-files-from \
    --owner=0 --group=0 --numeric-owner \
    -C "$root" -T "$staging/archive-list" -C "$staging" SOURCE_INVENTORY.tsv
check_members "$staging/source.tar.gz" "$staging/members"
same_members "$staging/members" "$staging/archive-list"
snapshot=$staging/snapshot
mkdir "$snapshot"
tar -xzf "$staging/source.tar.gz" -C "$snapshot" --no-same-owner
verify_tree "$snapshot" "$inventory"

version=$(sed -n 's/^version = "\(.*\)"$/\1/p' "$snapshot/impl/Cargo.toml" | head -1)
[ -n "$version" ] || refuse "cannot read the product version from impl/Cargo.toml"
release=${LCL_RELEASE_VERSION:-$version}
case "$release" in
    0.1.0) ;;
    0.2.0)
        [ -f "$snapshot/canonical/LCL_Core_0.2.0/VERSION.txt" ] ||
            refuse "a 0.2.0 candidate needs canonical/LCL_Core_0.2.0 in the captured source"
        ;;
    *) refuse "LCL_RELEASE_VERSION must be 0.1.0 or 0.2.0, not $release" ;;
esac
name=lcl-$release-linux-x86_64

out=${LCL_RELEASE_OUT:-$root/releases/candidates/$name-$short_source_id}
check_output "$out"
artifacts=$staging/artifacts
mkdir "$artifacts"

echo "source:  $source_count files, id $short_source_id"
echo "output:  $out"

# ---------------------------------------------------------------------------
# 3. Build from the snapshot, inside this run's own directory
# ---------------------------------------------------------------------------

build=$staging/build
mkdir "$build"
echo "building $name"
( cd "$snapshot/impl" && CARGO_TARGET_DIR="$build" cargo build --release --offline --locked \
    -p lcl-cli -p lcl-workspace )

payload=$staging/payload/$name
mkdir -p "$payload/bin" "$payload/share"

install -m 0755 "$build/release/lcl" "$payload/bin/lcl"
install -m 0755 "$build/release/lcl-workspace" "$payload/bin/lcl-workspace"

# The engine refuses to load a package that is not the approved release, so the
# package travels with the binaries rather than being looked for at run time.
cp -r "$snapshot/canonical/LCL_Core_0.1.0" "$payload/share/LCL_Core_0.1.0"
if [ "$release" = 0.2.0 ]; then
    cp -r "$snapshot/canonical/LCL_Core_0.2.0" "$payload/share/LCL_Core_0.2.0"
fi

cp "$snapshot/packaging/install.sh" "$snapshot/packaging/uninstall.sh" "$payload/"
chmod 0755 "$payload/install.sh" "$payload/uninstall.sh"
cp "$snapshot/packaging/lcl.desktop" "$payload/share/lcl.desktop"
cp "$snapshot/packaging/lcl-workspace-launch.in" "$payload/share/lcl-workspace-launch.in"
cp "$snapshot/impl/integration/linux/lcl.xml" "$payload/share/lcl.xml"
cp "$snapshot/packaging/README.md" "$payload/README.md"

# The installed application and document icons, and the master they were
# derived from. The master travels so that a payload carries the provenance of
# its own artwork; nothing at run time reads it.
cp -r "$snapshot/packaging/icons" "$payload/share/icons"
mkdir -p "$payload/share/brand"
cp "$snapshot/assets/brand/lcl-logo-master.png" "$payload/share/brand/lcl-logo-master.png"
cp "$snapshot/assets/brand/BRAND_ASSETS.sha256" "$payload/share/brand/BRAND_ASSETS.sha256"

# ---------------------------------------------------------------------------
# 4. The archives, the payload one checked by unpacking it again
# ---------------------------------------------------------------------------

tar -czf "$staging/payload.tar.gz" --owner=0 --group=0 --numeric-owner \
    -C "$staging/payload" "$name"
payload_manifest "$staging/payload" "$staging/payload-manifest"
LC_ALL=C tar -tPzf "$staging/payload.tar.gz" > "$staging/payload-names" ||
    refuse "cannot list the payload archive"
LC_ALL=C tar -tvPzf "$staging/payload.tar.gz" > "$staging/payload-long" ||
    refuse "cannot list the payload archive"
LC_ALL=C awk -v prefix="$name/" -v long="$staging/payload-long" '
    (getline line < long) <= 0 || (substr(line, 1, 1) != "-" && substr(line, 1, 1) != "d") ||
    index($0, prefix) != 1 || ("/" $0 "/") ~ /\/\.\.?\// || $0 ~ /\\/ {
        printf "refused: unexpected payload archive member %s\n", $0 > "/dev/stderr"
        failed = 1
        exit
    }
    END { exit failed }' "$staging/payload-names"
mkdir "$staging/payload-check"
tar -xzf "$staging/payload.tar.gz" -C "$staging/payload-check" --no-same-owner
payload_manifest "$staging/payload-check" "$staging/payload-check-manifest"
[ "$(sha256sum < "$staging/payload-manifest")" = "$(sha256sum < "$staging/payload-check-manifest")" ] ||
    refuse "the payload archive does not unpack to the payload that was staged"

cp "$staging/source.tar.gz" "$artifacts/$name-source.tar.gz"
cp "$staging/payload.tar.gz" "$artifacts/$name.tar.gz"
cp "$inventory" "$artifacts/SOURCE_INVENTORY.tsv"
if [ "$from_git" = yes ] && [ -s "$staging/SOURCE_CHANGES.patch" ]; then
    cp "$staging/SOURCE_CHANGES.patch" "$artifacts/SOURCE_CHANGES.patch"
fi
( cd "$artifacts" && sha256sum "$name.tar.gz" > "$name.sha256" )
( cd "$artifacts" && sha256sum "$name-source.tar.gz" > "$name-source.sha256" )

# ---------------------------------------------------------------------------
# 5. Provenance
# ---------------------------------------------------------------------------

identity=$("$payload/bin/lcl" spec --spec "$payload/share/LCL_Core_0.1.0" | sed -n 's/^ *identity *//p')
# `lcl version` names Core 0.1.0 always and Core 0.2.0 only for a named package
# that opens, so it is asked with exactly the packages this payload carries and
# no inherited LCL_LOCALIZED_SPEC. A language the payload does not carry, or one
# it carries that the tool does not report, refuses the candidate.
carried=0.1.0
if [ "$release" = 0.2.0 ]; then
    carried="0.1.0 0.2.0"
    versions=$(LCL_LOCALIZED_SPEC= "$payload/bin/lcl" version \
        --localized-spec "$payload/share/LCL_Core_0.2.0") ||
        refuse "the built lcl could not report its versions"
else
    versions=$(LCL_LOCALIZED_SPEC= "$payload/bin/lcl" version) ||
        refuse "the built lcl could not report its versions"
fi
language=$(printf '%s\n' "$versions" | sed -n 's/^language //p' | tr '\n' ' ' | sed 's/ *$//')
protocol=$(printf '%s\n' "$versions" | sed -n 's/^protocol //p')
if [ -z "$identity" ] || [ -z "$language" ] || [ -z "$protocol" ]; then
    refuse "the built lcl did not report its package identity, language and protocol"
fi
[ "$language" = "$carried" ] ||
    refuse "the built lcl reports language $language, but this $release payload carries $carried"
# The Core 0.2.0 package's identity, as the built tool's own 0.2.0 engine
# reports it for the package's canonical-English fixture.
localized_identity=
if [ "$release" = 0.2.0 ]; then
    localized_identity=$("$payload/bin/lcl" check --machine \
        --spec "$payload/share/LCL_Core_0.1.0" \
        --localized-spec "$payload/share/LCL_Core_0.2.0" \
        "$payload/share/LCL_Core_0.2.0/09_CONFORMANCE/LOCALIZATION_FIXTURES/sources/canonical_en.lcl" |
        sed -n 's/.*"identity_digest": *"\([0-9a-f]*\)".*/\1/p' | head -1)
    [ -n "$localized_identity" ] ||
        refuse "the built lcl did not report the Core 0.2.0 package identity"
fi
rustc_program=${RUSTC:-rustc}
{
    echo "LCL product release candidate provenance"
    echo
    echo "SOURCE, captured before anything was built"
    echo
    echo "source id:        $source_id"
    echo "  (the SHA-256 of SOURCE_INVENTORY.tsv, which gives every source file's"
    echo "  executable class, size and SHA-256; the build read only a snapshot"
    echo "  unpacked from the source archive and verified against it)"
    echo "source files:     $source_count"
    echo "source archive:   $name-source.tar.gz"
    echo "source sha256:    $(cut -d' ' -f1 < "$artifacts/$name-source.sha256")"
    echo "origin:           $origin"
    echo "git commit:       $commit"
    if [ "$from_git" = yes ]; then
        echo "uncommitted entries when the source was recorded: $(wc -l < "$staging/git-status")"
        sed 's/^/    /' "$staging/git-status"
        echo "  (releases/ is never source; untracked source files are whole in the"
        echo "  source archive)"
        if [ -s "$staging/SOURCE_CHANGES.patch" ]; then
            echo "tracked changes:  SOURCE_CHANGES.patch"
        fi
    fi
    echo
    echo "TOOLCHAIN AND INPUTS"
    echo
    echo "cargo:            $(command -v cargo), $(cargo --version)"
    echo "rustc:            $(command -v "$rustc_program"), $("$rustc_program" --version)"
    echo "declared minimum: $(sed -n 's/^rust-version = "\(.*\)"$/\1/p' "$snapshot/impl/Cargo.toml" | head -1)"
    echo "lockfile sha256:  $(sha256sum < "$snapshot/impl/Cargo.lock" | cut -d' ' -f1)"
    echo "build flags:      --release --offline --locked"
    echo "build directory:  this run's own, not impl/target"
    echo "built on:         $(uname -srm)"
    echo
    echo "ARTIFACT"
    echo
    echo "artifact:         $name.tar.gz"
    echo "sha256:           $(cut -d' ' -f1 < "$artifacts/$name.sha256")"
    echo "release version:  $release"
    echo "product version:  $version"
    echo "language version: $language"
    echo "engine protocol:  $protocol"
    echo "package identity: $identity"
    if [ -n "$localized_identity" ]; then
        echo "0.2.0 identity:   $localized_identity"
    fi
    echo
    echo "Bit-for-bit reproducibility is not claimed. Rust embeds build paths"
    echo "and no attempt is made to normalize them, so two runs from identical"
    echo "source may differ. What is claimed is the other direction: the source"
    echo "these bytes were built from is recorded exactly."
    echo
    echo "To reconstruct the source, with no git metadata, and rebuild it:"
    echo
    echo "    sha256sum -c $name-source.sha256"
    echo "    tar -xzOf $name-source.tar.gz packaging/build_release.sh > build_release.sh"
    echo "    sh build_release.sh --reconstruct $name-source.tar.gz <new directory>"
    echo "    cd <new directory> && LCL_RELEASE_OUT=<new output> packaging/build_release.sh"
    echo
    echo "BRAND ASSETS IN THIS PAYLOAD"
    echo
    sed 's/^/    /' "$snapshot/assets/brand/BRAND_ASSETS.sha256"
    echo
    echo "PAYLOAD, every file in the tarball: executable class, size, SHA-256, path"
    echo
    cat "$staging/payload-manifest"
} > "$artifacts/lcl-$release-PROVENANCE.txt"

# ---------------------------------------------------------------------------
# 6. Publish: claim the output directory, copy, and check what arrived
# ---------------------------------------------------------------------------

set -- "$name.tar.gz" "$name.sha256" "$name-source.tar.gz" "$name-source.sha256" \
    SOURCE_INVENTORY.tsv "lcl-$release-PROVENANCE.txt"
if [ -f "$artifacts/SOURCE_CHANGES.patch" ]; then
    set -- "$@" SOURCE_CHANGES.patch
fi

# mkdir fails if anything has appeared at that name since it was checked, so a
# candidate never lands in, or over, something that was already there.
mkdir "$out" || refuse "$out appeared while the candidate was being built; nothing was written there"
for artifact in "$@"; do
    cp "$artifacts/$artifact" "$out/$artifact"
    [ "$(sha256sum < "$artifacts/$artifact")" = "$(sha256sum < "$out/$artifact")" ] ||
        refuse "$out/$artifact does not match what was built"
done

echo
for artifact in "$@"; do
    echo "wrote $out/$artifact"
done
echo
echo "The published release in releases/ was not touched."

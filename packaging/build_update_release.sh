#!/bin/sh
# Build one signed LCL update release — the PC payload, the Android APK, the
# update manifest, its signature, checksums and provenance — into a new
# directory outside the repository. Nothing is published: the owner uploads
# the result to a GitHub Release by hand (packaging/README.md, "Publishing an
# update release").
#
#   packaging/build_update_release.sh
#
# Settings, all from the environment, none from a file in the repository:
#
#   LCL_UPDATE_OUT=<dir>              where to write; must not exist, and must
#                                     be outside the repository
#   LCL_UPDATE_SIGNING_KEY=<file>     the offline update signing key (PEM, ECDSA
#                                     P-256), outside the repository. Its
#                                     contents are never printed.
#   LCL_UPDATE_KEY_ID=<id>            its id in update/trusted_keys.txt
#   LCL_ANDROID_VERSION_CODE=<n>      the APK's versionCode
#   LCL_PREVIOUS_MANIFEST=<file>      the last published update-manifest.json,
#                                     or "none" for the first update release
#   LCL_ANDROID_SIGNER_SHA256=<hex>   the APK signing certificate expected;
#                                     required with LCL_PREVIOUS_MANIFEST=none,
#                                     otherwise the previous release's
#   LCL_RELEASE_NOTES_FILE=<file>     the release notes shown to people
#   LCL_MINIMUM_SUPPORTED_VERSION=<v> the oldest version this release may be
#                                     installed over (default 0.1.0)
#   LCL_RELEASE_STORE_FILE, LCL_RELEASE_STORE_PASSWORD, LCL_RELEASE_KEY_ALIAS,
#   LCL_RELEASE_KEY_PASSWORD          the Android release signing key, as the
#                                     Android build reads them
#   LCL_UPDATE_DRY_RUN=1              a rehearsal with test keys: allows a key
#                                     that update/trusted_keys.txt does not
#                                     list and a working tree with changes, and
#                                     marks every output NOT FOR PUBLICATION
#
# It refuses when an artifact is missing, the product versions disagree, the
# versionCode does not advance, the APK is unsigned or signed by another key,
# the manifest cannot be signed or verified, a digest does not describe the
# artifact written, the source is not the commit the provenance names, or the
# specification packages are not the approved ones (the built lcl refuses
# those, and build_release.sh with it).
set -eu

refuse() {
    echo "refused: $*" >&2
    exit 1
}

root=$(cd "$(dirname "$0")/.." && pwd)
dry=${LCL_UPDATE_DRY_RUN:-}
for name in LCL_UPDATE_OUT LCL_UPDATE_SIGNING_KEY LCL_UPDATE_KEY_ID LCL_ANDROID_VERSION_CODE \
    LCL_PREVIOUS_MANIFEST LCL_RELEASE_NOTES_FILE; do
    eval "value=\${$name:-}"
    [ -n "$value" ] || refuse "$name is not set"
done
for tool in openssl python3 sha256sum git; do
    command -v "$tool" >/dev/null 2>&1 || refuse "$tool is needed"
done
build_tools=$(ls -d "${ANDROID_HOME:?ANDROID_HOME is not set}"/build-tools/* 2>/dev/null | sort -V | tail -1)
[ -x "$build_tools/apksigner" ] && [ -x "$build_tools/aapt2" ] || refuse "apksigner and aapt2 are needed in $ANDROID_HOME/build-tools"

inside() {
    case "$(realpath -m "$1")/" in "$root"/*) return 0 ;; *) return 1 ;; esac
}
out=$LCL_UPDATE_OUT
key=$LCL_UPDATE_SIGNING_KEY
inside "$out" && refuse "LCL_UPDATE_OUT must be outside the repository"
[ -e "$out" ] && refuse "$out exists; name a new directory"
inside "$key" && refuse "the update signing key must live outside the repository"
[ -r "$key" ] || refuse "the update signing key $key cannot be read"
case "$LCL_UPDATE_KEY_ID" in *[!a-z0-9-]*) refuse "LCL_UPDATE_KEY_ID must be a-z, 0-9 and -" ;; esac
case "$LCL_ANDROID_VERSION_CODE" in ''|*[!0-9]*) refuse "LCL_ANDROID_VERSION_CODE must be a number" ;; esac
code=$LCL_ANDROID_VERSION_CODE
minimum=${LCL_MINIMUM_SUPPORTED_VERSION:-0.1.0}

# The source: one commit, and in a real release nothing else.
commit=$(git -C "$root" rev-parse HEAD) || refuse "cannot read the source commit"
changes=$(git -C "$root" status --porcelain --untracked-files=normal)
if [ -n "$changes" ] && [ -z "$dry" ]; then
    refuse "the working tree has changes; a release is built from exactly one commit"
fi

# One product version for everything this release carries.
version=$(sed -n 's/^version = "\(.*\)"$/\1/p' "$root/impl/Cargo.toml" | head -1)
for crate in update remote; do
    [ "$(sed -n 's/^version = "\(.*\)"$/\1/p' "$root/$crate/Cargo.toml" | head -1)" = "$version" ] ||
        refuse "$crate/Cargo.toml does not carry product version $version"
done

# The last release: this one must be newer and signed by the same APK key.
signer_wanted=${LCL_ANDROID_SIGNER_SHA256:-}
if [ "$LCL_PREVIOUS_MANIFEST" = none ]; then
    [ -n "$signer_wanted" ] || refuse "the first update release needs LCL_ANDROID_SIGNER_SHA256"
else
    previous=$(python3 - "$LCL_PREVIOUS_MANIFEST" <<'PY'
import json, sys
m = json.load(open(sys.argv[1]))
print(m["product_version"], m["android"]["version_code"], m["android"]["signer_sha256"])
PY
) || refuse "cannot read $LCL_PREVIOUS_MANIFEST"
    set -- $previous
    python3 - "$1" "$version" <<'PY' || refuse "product version $version is not newer than the previous release $1"
import sys
def key(v):
    core, _, pre = v.partition("-")
    return [int(x) for x in core.split(".")], (pre == "")
a, b = key(sys.argv[1]), key(sys.argv[2])
sys.exit(0 if (b[0], b[1]) > (a[0], a[1]) else 1)
PY
    [ "$code" -gt "$2" ] || refuse "versionCode $code does not advance past the previous release's $2"
    if [ -n "$signer_wanted" ] && [ "$signer_wanted" != "$3" ]; then
        refuse "LCL_ANDROID_SIGNER_SHA256 is not the previous release's APK signer"
    fi
    signer_wanted=$3
fi

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
mkdir -p "$out"

# 1. The PC payload, through the release builder and all of its checks.
LCL_RELEASE_VERSION=0.3.0 LCL_RELEASE_OUT="$work/pc" "$root/packaging/build_release.sh"
pc_built="$work/pc/lcl-0.3.0-linux-x86_64.tar.gz"
[ -f "$pc_built" ] || refuse "the release builder produced no PC payload"
pc_name=lcl-$version-linux-x86_64.tar.gz
cp "$pc_built" "$out/$pc_name"
cp "$work/pc/lcl-0.3.0-PROVENANCE.txt" "$out/lcl-$version-pc-PROVENANCE.txt"

# 2. The APK, signed with the release key the Android build reads.
( cd "$root/android" && ./gradlew --offline -q assembleRelease \
    -PlclVersionName="$version" -PlclVersionCode="$code" )
apk_built="$root/android/app/build/outputs/apk/release/app-release.apk"
[ -f "$apk_built" ] || refuse "the Android build produced no signed APK (is the release signing key set?)"
apk_name=lcl-android-$version-$code.apk
cp "$apk_built" "$out/$apk_name"
"$build_tools/apksigner" verify --print-certs "$out/$apk_name" > "$work/apk-certs" 2>&1 ||
    refuse "the APK is not signed"
# One signing certificate, however this apksigner labels its schemes
# ("Signer #1 ...", "V2 Signer: ...").
signer=$(sed -n 's/^.*certificate SHA-256 digest: \([0-9a-f]\{64\}\)$/\1/p' "$work/apk-certs" | sort -u)
[ -n "$signer" ] && [ "$(printf '%s\n' "$signer" | wc -l)" -eq 1 ] ||
    refuse "the APK must be signed by exactly one certificate"
[ "$signer" = "$signer_wanted" ] ||
    refuse "the APK is signed by $signer, not by the release key $signer_wanted: phones could not update in place"
"$build_tools/aapt2" dump badging "$out/$apk_name" > "$work/badging"
grep -q "^package: name='io.lcl.workspace' versionCode='$code' versionName='$version'" "$work/badging" ||
    refuse "the APK is not io.lcl.workspace $version ($code)"
min_sdk=$(sed -n "s/^minSdkVersion:'\([0-9]*\)'/\1/p" "$work/badging")

# 3. The manifest, naming each artifact by name, size and digest.
size() { wc -c < "$1" | tr -d ' '; }
digest() { sha256sum "$1" | cut -d' ' -f1; }
python3 - "$work/manifest.json" "$version" "$commit" "$minimum" "$LCL_UPDATE_KEY_ID" \
    "$LCL_RELEASE_NOTES_FILE" "$pc_name" "$(size "$out/$pc_name")" "$(digest "$out/$pc_name")" \
    "$apk_name" "$(size "$out/$apk_name")" "$(digest "$out/$apk_name")" "$code" "$min_sdk" "$signer" <<'PY'
import json, sys, time
(out, version, commit, minimum, key_id, notes, pc_name, pc_size, pc_sha,
 apk_name, apk_size, apk_sha, code, min_sdk, signer) = sys.argv[1:]
manifest = {
    "format": 1, "product": "lcl", "channel": "stable",
    "product_version": version, "release_tag": "v" + version,
    "source_commit": commit,
    "published_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
    "release_notes": open(notes, encoding="utf-8").read().strip(),
    "minimum_supported_version": minimum, "signing_key_id": key_id,
    "pc": {"artifact_name": pc_name, "size": int(pc_size), "sha256": pc_sha,
           "architecture": "x86_64-linux", "required_updater_version": 1},
    "android": {"artifact_name": apk_name, "size": int(apk_size), "sha256": apk_sha,
                "application_id": "io.lcl.workspace", "version_name": version,
                "version_code": int(code), "minimum_sdk": int(min_sdk),
                "signer_sha256": signer},
}
open(out, "w", encoding="utf-8").write(json.dumps(manifest, indent=2, ensure_ascii=False) + "\n")
PY
cp "$work/manifest.json" "$out/update-manifest.json"

# 4. The signature, over exactly those bytes, checked three ways: against the
#    key's own public half, against the key list the apps ship, and (in a real
#    release) by the built updater itself.
openssl dgst -sha256 -sign "$key" -out "$out/update-manifest.sig" "$out/update-manifest.json" ||
    refuse "the manifest could not be signed"
openssl pkey -in "$key" -pubout -out "$work/public.pem" 2>/dev/null ||
    refuse "the signing key is not a readable private key"
openssl dgst -sha256 -verify "$work/public.pem" -signature "$out/update-manifest.sig" \
    "$out/update-manifest.json" >/dev/null || refuse "the signature does not verify"
spki=$(openssl pkey -pubin -in "$work/public.pem" -outform DER | od -An -v -tx1 | tr -d ' \n')
listed=$(awk -v id="$LCL_UPDATE_KEY_ID" '$1 == id { print $2 }' "$root/update/trusted_keys.txt")
if [ "$listed" != "$spki" ]; then
    [ -n "$dry" ] || refuse "key $LCL_UPDATE_KEY_ID is not in update/trusted_keys.txt as this key: the released apps would refuse this update"
fi
if [ -z "$dry" ]; then
    tar -xzf "$out/$pc_name" -C "$work" --wildcards '*/bin/lcl-update'
    "$work"/lcl-*-linux-x86_64/bin/lcl-update verify "$out/update-manifest.json" "$out/update-manifest.sig" ||
        refuse "the released updater does not accept this manifest"
fi

# 5. The manifest describes exactly the files written: read it back, measure
#    the files again.
python3 - "$out" <<'PY' || refuse "the manifest does not describe the artifacts written"
import hashlib, json, os, sys
out = sys.argv[1]
m = json.load(open(os.path.join(out, "update-manifest.json")))
for part in ("pc", "android"):
    a = m[part]
    data = open(os.path.join(out, a["artifact_name"]), "rb").read()
    assert len(data) == a["size"], part
    assert hashlib.sha256(data).hexdigest() == a["sha256"], part
PY

# 6. Checksums and provenance.
( cd "$out" && sha256sum "$pc_name" > "lcl-$version-linux-x86_64.sha256" &&
    sha256sum "$apk_name" > "lcl-android-$version-$code.sha256" )
{
    if [ -n "$dry" ]; then
        echo "DRY RUN — NOT FOR PUBLICATION (test keys and/or uncommitted source)"
    fi
    echo "LCL update release $version"
    echo "source commit:     $commit"
    if [ -n "$changes" ]; then
        echo "working tree:      HAD UNCOMMITTED CHANGES"
    else
        echo "working tree:      clean"
    fi
    echo "product version:   $version"
    echo "android version:   $version ($code), minSdk $min_sdk"
    echo "apk signer sha256: $signer"
    echo "update key id:     $LCL_UPDATE_KEY_ID (public key sha256 $(printf '%s' "$spki" | sha256sum | cut -d' ' -f1))"
    echo "minimum supported: $minimum"
    echo
    echo "artifacts:"
    ( cd "$out" && sha256sum "$pc_name" "$apk_name" update-manifest.json update-manifest.sig )
} > "$out/lcl-$version-UPDATE-PROVENANCE.txt"
( cd "$out" && sha256sum -- * > SHA256SUMS )

echo "wrote $out"
echo "Nothing was published. To publish, follow packaging/README.md, \"Publishing an update release\"."

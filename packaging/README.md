# LCL, packaged

Two binaries, a desktop launcher, the specification package they load, a
desktop entry and a media type. Everything installs under your own home
directory.

## Install

```sh
./install.sh
```

It needs no elevation, writes nothing system-wide, and prints every path it
touches. `./uninstall.sh` removes everything it installed. The one thing
uninstall keeps is `~/.local/share/lcl/workspace`, because that directory holds
your own documents rather than ours.

## From the desktop

The installed menu entry runs `~/.local/bin/lcl-workspace-launch`, not the
binary directly. The script exists because one `Exec` line cannot express three
things a menu launch needs.

- **The specification package.** Nothing searches for one, so the installer
  writes its absolute path into the launcher. A menu launch therefore works
  with no `LCL_SPEC` exported.
- **The browser.** A desktop launch has no terminal, so the URL the workspace
  prints would go nowhere. The launcher passes `--open`.
- **The document.** A file association hands over a *file*; the workspace's
  positional argument is a *project directory*. The launcher passes the file to
  `--document`, which resolves its own project: the nearest ancestor holding
  `lcl.project.json`, or the file's own directory.

Opened from the menu with no document, it opens
`~/.local/share/lcl/workspace`, stated rather than inherited from wherever the
process happened to start.

A 0.2.0 installation also passes the Core 0.2.0 package, but no locale
profile: none is installed, and nothing searches for one. A localized document
gets its profiles from its project. Put the `<locale>.json` files in a
directory and name it in `lcl.project.json`, relative to the project root:

```json
{ "format": "lcl.project/1", "spec": "…", "profiles": "profiles" }
```

Without that a document that needs a profile is refused with
`error.localization.profile_unavailable`, never read with canonical spellings.
From a terminal, `lcl-workspace --profile <file>` and `lcl --profile <file>`
add a profile file after the project's directory, by the same rule.

Anything that goes wrong is written to
`~/.local/state/lcl/launch.log` and, where a dialog tool exists, shown in one.
A launch that fails is not a launch that silently does nothing.

Installing registers the `text/x-lcl` media type and an application that
handles it. It does **not** make LCL your default handler for anything; that
stays your choice.

## Icons

The menu entry and `.lcl` documents both use the supplied LCL mark, installed
into `~/.local/share/icons/hicolor` at seven sizes. Both come from one master,
recorded with its checksum in `assets/brand/`.

The theme directory is shared with every other application, so the installer
copies its own files in one at a time and the uninstaller removes exactly those
files. Neither ever removes a directory it did not fill, the shared `icons/`
and `icons/hicolor/` roots are never removed even when empty, and no icon cache is
generated: the icon theme specification resolves an icon by reading the
directories, and writing a cache would put a shared file there that this
installation could not safely take back.

Setting a menu icon does not change the icon of the browser window the
workspace opens in. That window belongs to your browser.

## The one thing to know

Every command needs a specification package, and **nothing searches for one**.
`05_SEMANTICS/02` puts it plainly: "Ambient current directory and implied
nearby files do not exist in portable LCL." So the package travels with the
binaries and you name it, once:

```sh
export LCL_SPEC=~/.local/share/lcl/LCL_Core_0.1.0
# a 0.2.0 candidate also installs the localized-language package:
export LCL_LOCALIZED_SPEC=~/.local/share/lcl/LCL_Core_0.2.0
```

or per command with `--spec`, or per project in `lcl.project.json`.

## Use

```sh
lcl help                      # commands, options and exit codes
lcl version                   # tool, protocol and the languages it can judge
lcl spec                      # the package identity and whether it is authoritative
lcl check   src/main.lcl      # canonical steps 1 to 5
lcl validate src/main.lcl     # steps 1 to 9, before any effect
lcl run     src/main.lcl      # steps 1 to 13, one terminal status
lcl run --allow-write /tmp/out src/main.lcl    # the same, with one capability
lcl check --machine src/main.lcl               # the JSON a tool consumes
lcl-workspace my-project --open                # the editor and debugger
lcl-workspace --document src/main.lcl --open   # open one document's project
```

## What a document is called

`.lcl` is the native ending: a new document named without an ending is
created as `name.lcl`. `.lcl.txt` is an optional compatibility ending for
places that only handle plain text; a name you give with either ending is kept
exactly as you typed it. Both endings are recognised everywhere: the
project tree, opening, saving, checking, running and the `.lcl` file
association. Nothing renames an existing document, and saving writes exactly
the name it opened.

The ending decides nothing about meaning. A `.lcl.txt` document is judged by the
same engine under the same contracts as a `.lcl` one, and ending a name in
`.txt` never relaxes validation. Ordinary `.txt` files are not LCL documents.

A run is granted nothing unless a flag says so. A document that asks for the
world against a tool that was granted nothing gets a refusal, not a surprise.

## Exit codes

| Code | Meaning |
| --- | --- |
| 0 | the requested work completed, and for `run` the terminal status was `status.succeeded` |
| 1 | the document was rejected by a diagnostic |
| 2 | the document ran to completion with a non-success terminal status |
| 3 | the command line was not usable |
| 4 | the environment was not usable: no package, an unreadable document, a project that would not open |

`1` and `2` are deliberately different. A program that failed its own `VERIFY`
ran correctly and did not succeed, which `05_SEMANTICS/10` treats as an
ordinary outcome rather than an error.

## Building this yourself

```sh
packaging/build_release.sh
```

It lists the source, records every file's mode, size and SHA-256 in
`SOURCE_INVENTORY.tsv`, archives exactly those bytes, and builds `--release
--offline --locked` from a snapshot unpacked from that archive and checked
against the inventory, never from the live checkout. The candidate goes to
`releases/candidates/<name>-<source id>`, or to `LCL_RELEASE_OUT`, which must
not exist yet: nothing is ever written into or over an existing directory.
Beside the payload tarball and its checksum it writes the source archive, the
inventory and a provenance record naming the source id, the commit and
uncommitted entries of a checkout, the toolchain, the package identity and the
checksum of every file in the payload.

A build that fails says so, publishes nothing, and keeps its working directory,
whose path it prints, as evidence.

To rebuild from a source archive, with no Git history:

```sh
tar -xzOf lcl-<version>-linux-x86_64-source.tar.gz packaging/build_release.sh > build_release.sh
sh build_release.sh --reconstruct lcl-<version>-linux-x86_64-source.tar.gz <new directory>
cd <new directory> && LCL_RELEASE_OUT=<new output directory> packaging/build_release.sh
```

`--reconstruct` refuses an archive that holds anything but the regular files
its inventory names, before extracting anything. Rebuilt binaries are not
promised to be bit-for-bit identical; the source they were built from is.

`CLEAN_MACHINE_OFFLINE_REBUILD = NO`. Source reconstruction needs no Git
history, but an offline build requires the Rust/C toolchain and the Cargo
registry metadata and crate sources named by `remote/Cargo.lock` and
`update/Cargo.lock` already in the local cache. These external dependencies
are not bundled in the source archive. The engine workspace itself is std-only.
Android additionally needs the JDK, Android SDK/build tools and cached Gradle
distribution/plugins/dependencies. Provision these before going offline;
`--offline --locked` neither downloads missing inputs nor changes their versions.

## Release version

`LCL_RELEASE_VERSION=0.2.0 packaging/build_release.sh` builds a separate
`lcl-0.2.0-linux-x86_64` candidate. It bundles the Core 0.1.0 package and the
Core 0.2.0 package: `install.sh` installs both, the desktop launcher passes the
0.2.0 package as `--localized-spec`, and the provenance records both package
identities. Without the variable the newest Core package in the source is
bundled (0.3.0 now). The variable chooses the Core bundle only: the candidate
and its archive are named by the product version in `impl/Cargo.toml`,
`lcl-<product version>-linux-x86_64`, which the programs report and the release
is called, and the provenance records which Core packages it carries.
`LCL_RELEASE_VERSION=0.3.0` bundles the Core 0.3.0 package too (multi-file
projects and file roles): `install.sh` installs it, and the desktop launcher
passes it as `--project-spec`. No other value is accepted. Building a candidate
is a release task of its own.

`lcl version` names Core 0.2.0 only when a Core 0.2.0 package is named to it,
by `--localized-spec` or `LCL_LOCALIZED_SPEC`, and that package opens. The
provenance's language version is what the built `lcl version` reports with
exactly the packages the payload carries, and a candidate whose tool reports
any other set is refused.

## Updates

A payload also carries `lcl-update`, the updater, and `lcl-remote`. `install.sh`
installs `lcl-update` beside `lcl`, and replaces `lcl-remote` only where
`remote/install.sh` already installed it; it never installs, enables or starts
the remote service. Every file is published atomically, and `install.sh --list`
names every path an installation writes without writing any.

Every binary of a release reports one **product version**, the `version` in
`impl/Cargo.toml`, `update/Cargo.toml` and `remote/Cargo.toml`, which must
agree; `build_release.sh` refuses a candidate where they do not, or whose
`lcl-update` is a test build. The product version is not the LCL Core
language version.

`lcl-update` finds updates in one place: the latest published, non-draft,
non-pre-release GitHub Release of `aivars11232/LCL`, pinned in the build. It
needs no token. It trusts an update only through the update signing keys in
`update/trusted_keys.txt`, compiled into it (and into LCL for Android). It
verifies the signed `update-manifest.json` before reading it, fetches only the
artifacts it names, by name, from that same release, checks their size and
SHA-256, stages and tests the new payload, keeps a copy of everything it will
replace, installs with the payload's own `install.sh`, checks the result, and
puts the copy back on any failure. User data — projects, `~/.config/lcl`,
`~/.local/state/lcl` — is never written. Its state and downloads live under
`~/.local/state/lcl/update` and `~/.cache/lcl/update`, which holds at most one
staged update and, during an installation, one rollback copy.

## Publishing an update release

Nothing in this repository publishes a release. The owner does, by hand, with
material that never enters the repository.

**The update signing key.** One ECDSA P-256 key, created offline and kept
outside the repository and outside every build machine's checkout, for example:

```sh
openssl ecparam -name prime256v1 -genkey -noout -out /secure/offline/lcl-update-1.key
openssl pkey -in /secure/offline/lcl-update-1.key -pubout -outform DER | od -An -v -tx1 | tr -d ' \n'
```

The second command prints the public key's hex, which goes into
`update/trusted_keys.txt` as `lcl-update-1 <hex>` and is committed. Only a
build that lists the key can verify releases signed with it, so the first
update-capable release is installed by hand. The private key is never printed,
committed, bundled or copied into a build.

**One key, kept.** Update System V1 has exactly one production update
signing key, and it is never replaced. Listing a second key beside it would not
make a replacement safe: LCL checks only the latest stable release, so a PC or
a phone that skipped the release introducing a new key could never verify a
release signed with it, and would stop updating. The updater, the Android
build and the release builder all refuse an `update/trusted_keys.txt` that
lists more than one key; with none, updates are simply not configured. Test
keys are added only by test builds (`LCL_UPDATE_TEST_KEYS`, `lclUpdateTestKeys`)
and never count as production keys. Replacing the key is a separate feature
that is not designed yet. Until it exists, keep the private
key and an offline backup of it safe: without it no further update can be
published, and every installation would have to be updated by hand to a build
that trusts another key. A key a manifest names is never trusted unless the
installed build already lists it.

**The Android signing key.** Android installs an update only over an app
signed with the same key. The release build takes that key from
`LCL_RELEASE_STORE_FILE`, `LCL_RELEASE_STORE_PASSWORD`, `LCL_RELEASE_KEY_ALIAS`
and `LCL_RELEASE_KEY_PASSWORD` (or `~/.gradle/gradle.properties`), never from
the repository. Every release must be signed with the same key as the one
before; the release builder refuses an APK whose certificate differs.

**Building.** From a clean checkout of the commit being released, with the
product version raised in the three `Cargo.toml` files:

```sh
LCL_UPDATE_OUT=/tmp/lcl-release-0.4.0 \
LCL_UPDATE_SIGNING_KEY=/secure/offline/lcl-update-1.key \
LCL_UPDATE_KEY_ID=lcl-update-1 \
LCL_PREVIOUS_MANIFEST=/path/to/the/last/update-manifest.json \
LCL_PREVIOUS_MANIFEST_SIGNATURE=/path/to/the/last/update-manifest.sig \
LCL_RELEASE_NOTES_FILE=/path/to/notes.txt \
LCL_RELEASE_STORE_FILE=/secure/lcl-release.jks LCL_RELEASE_STORE_PASSWORD=... \
LCL_RELEASE_KEY_ALIAS=lcl LCL_RELEASE_KEY_PASSWORD=... \
ANDROID_HOME=/path/to/sdk packaging/build_update_release.sh
```

The APK's `versionCode` is not typed anywhere: it follows from the product
version, as the Android build derives it (`MAJOR * 1000000 + MINOR * 1000 +
PATCH`, so 0.9.0 is 9000), which grows with every release. The version must
be exactly `MAJOR.MINOR.PATCH` with `MINOR` and `PATCH` at most 999 (0.5.999
is 5999 and 0.6.0 is 6000, so no two versions share a code), and the code
must fit Android's range (at most 2100000000); the builder refuses anything
else, as the Android build does. `LCL_ANDROID_VERSION_CODE` overrides it only
for tests. The builder still refuses a code that does not advance past the
previous release's.

The previous manifest's detached signature is verified against the keys already
trusted by the release tooling, then parsed by the updater's strict parser,
before its version or Android signer is trusted. An unsigned history file is
refused, including during a dry run.

A signature proves only that some release was once signed, not that it is the
last one. So the builder also asks the pinned official repository
(`aivars11232/LCL`, through `lcl-update published`) for its latest stable
release, ignoring drafts and pre-releases, and verifies that release's own
manifest and signature as an installed LCL would. The manifest supplied must
be that release's manifest byte for byte: the same release tag, product
version and source commit, and the same SHA-256. That source commit must also
be in the history of the commit being released. An older validly signed
manifest, a release with another tag, a release whose manifest does not verify,
no stable release at all, or a repository that cannot be reached is refused.
Download the two files from the latest release itself, for example
`gh release download v0.4.0 -R aivars11232/LCL -p 'update-manifest.*'`.

The first update release names `LCL_PREVIOUS_MANIFEST=none`, leaves
`LCL_PREVIOUS_MANIFEST_SIGNATURE` unset, and supplies the APK
certificate's SHA-256 in `LCL_ANDROID_SIGNER_SHA256`. Bootstrap is only for
that first release: it is refused once the official repository has any stable
release (drafts and pre-releases do not count), and when the repository's
release state cannot be read. The builder writes, to
the new directory only: `update-manifest.json`, `update-manifest.sig`,
`lcl-<version>-linux-x86_64.tar.gz` and `.sha256`,
`lcl-android-<version>-<code>.apk` and `.sha256`, both provenance records and
`SHA256SUMS`. It refuses when a product version disagrees, the versionCode does
not advance, the APK is unsigned or signed by another key, the manifest cannot
be signed or is not verified by the key list the release carries, a digest does
not describe the file written, or the working tree is not exactly the commit
recorded. `LCL_UPDATE_DRY_RUN=1` rehearses the whole build with test keys and
marks every output *not for publication*.

All artifacts are built and verified in a private temporary directory beside
the destination. Only the complete set is published using Linux
`renameat2(RENAME_NOREPLACE)`; an existing destination is never overwritten.
The parent directory must exist. Unsupported no-replace rename is a refusal,
not a copy fallback. Failures retain the temporary directory and print its
location; failures before publication leave the final destination absent.

**Publishing.** Create a GitHub Release whose tag is `v<version>` — neither a
draft nor a pre-release — and attach every file of that directory, for example
`gh release create v0.4.0 /tmp/lcl-release-0.4.0/* --title "LCL 0.4.0" --notes-file notes.txt`.
Then check it as a client would: `lcl-update check` on a PC with the previous
release installed reports *update available* for the new version.

# LCL updater audit repair — 2026-09-28

STATUS: IMPLEMENTATION_READY_FOR_INDEPENDENT_REVIEW

## Source states

TESTED SOURCE STATE: the repair as an uncommitted working tree on top of
`b9f7fa20a151243d9ef7215b016328a27286e90d` (HEAD when the repair started and
when it was tested). Every result under *Fresh verification* was produced from
that tree. The SHA-256 of each of its 1,617 files is in
`/mnt/F/.lcl-updater-repair-20260928/evidence/final-source.json`.

COMMITTED REPAIR STATE: `d1151c160ac7fcc1917652ccc02c80f2622d69d7`
("LCL update (Astra take over) repair"), committed and pushed to `origin/main`
after those tests, as a single commit. On 2026-09-28 the final updater audit
cleanup compared the 1,617 recorded hashes with that commit's tree: all are
byte-identical. Its only other files are `releases/` (56 files, unchanged since
b9f7fa2) and this report. The tests were not rerun on d1151c1 itself. The
evidence applies to it because the bytes are identical, not because of a
rerun.

This correction changes only how the report describes repository state; the
test evidence below is unchanged from when it was recorded. No release
publication, production installation or canonical-package change was
performed.

## Findings and repairs

| Finding | Final behavior and evidence |
| --- | --- |
| Subprocess pipe deadlock | Concurrent bounded stdout/stderr drains; deterministic timeout kills/reaps the direct child. Tests cover large output, output limits, inherited pipes and a 3,000-file archive. |
| Partial update-release publication | All artifacts, signatures, checksums and provenance are staged beside the destination. Only a complete verified directory is published with Linux `renameat2(RENAME_NOREPLACE)`, followed by parent-directory sync. No copying fallback or destination overwrite. Pre-publication failures retain private evidence and leave the final path absent. |
| Previous-manifest authentication | Required detached signature checked using the updater's existing trusted-key verifier and strict parser before version/code/signer values are consumed. Bootstrap remains explicit. Negative tests cover modified history, wrong signature, unknown key, wrong product/channel/signer, malformed history and missing signature. |
| Key rotation claim | Documentation now requires retaining one production V1 update key. Skipped-release-safe rotation is not implemented or claimed; unknown keys remain refused. |
| PC/Android parity | Shared 40-vector contract covers strict UTF-8/JSON, exact keys, integer bounds, SemVer, digests, artifact sizes and SDK bounds. Architecture syntax and updater-version bounds are shared; host architecture/protocol applicability remains an explicit PC-only check. |
| Offline reconstruction | `CLEAN_MACHINE_OFFLINE_REBUILD = NO`. Existing source reconstruction is Git-independent, but Rust/C toolchains and Cargo registry metadata/crate sources for the locked remote/updater dependencies must already exist. Android also requires JDK, SDK/build tools and Gradle dependencies. README and generated provenance now state the external-cache requirement. No vendor tree was added. |
| VS Code correction | Restored exactly the pre-b9f7fa2 settings bytes; removed only the unrelated Claude Ctrl+Enter preference. Proposed as its own commit at test time; it was committed inside d1151c1 with the rest of the repair. |
| XDG handling | Relative/empty XDG paths fall back under absolute HOME in updater, installer, uninstaller and launcher. Existing locations are preserved. The updater has no configuration-path field; it does not read or relocate XDG_CONFIG_HOME. Android is unaffected by XDG. |
| Installer durability | Files and containing directories are synced. Existing package directories use atomic exchange when GNU mv/filesystem support it; deterministic interrupted-exchange and fallback tests pass. The compatibility fallback retains the two-rename interruption gap. The whole installation is not a single atomic transaction. |
| Android download memory | APK bytes stream into app-private cache while hashing; interrupted, oversized or mismatched downloads are removed. Bounded-copy and controller tests pass; real emulator installation preserves pairing. |

## Fresh verification

All checks used the same final implementation bytes; the only later repository
addition is this report. Exact source hashes (1,617 files) and full logs are in
`/mnt/F/.lcl-updater-repair-20260928/evidence/`.

- Focused PC updater: 30 tests passed (20 unit, 8 flow, 2 shared-vector tests).
- Release-builder regressions: 5 test methods / 18 scenarios passed. Real
  signature verification, strict manifest parsing and no-replace publication;
  PC/APK compilation and SDK inspection are controlled test boundaries.
- Existing full local gate: 71/71 commands returned expected statuses; wrapper
  self-test demonstrated that a failing command cannot become a pass.
- Workspace: 1,958 passed, 0 failed, 1 pre-existing ignored, on both Rust
  1.98.1 and minimum Rust 1.75.0. Formatting/Clippy passed.
- Remote: 65 passed. Updater final gate: 30 passed; production and test-feature
  Clippy passed. Process stress: all 30 sequential/concurrent runs passed.
- Canonical validators/checksums/anchors and historical release checks passed
  their expected gates. Core 0.3 independent_review remains pending; it was
  neither changed nor represented as complete.
- Manual: manifest current; 97 examples passed, 42 fragments intentionally
  skipped. Browser smoke: 36 passed, 0 failed.
- Android: 111 JVM tests, 0 failures/errors/skips; lint 0 errors and 1 existing
  OldTargetApi warning; debug and unsigned-release builds succeeded.
- Android E2E was rerun because production parser/download behavior changed:
  24 phase passes and 35 PC checks, exit 0. Includes reboot/reconnection,
  streamed update verification, Android install confirmation and in-place
  0.2.0 installation with pairing and keys preserved. Emulator-only evidence;
  no physical phone was attached or tested. The task emulator was stopped.

The first release regression run exposed a test-input error: 9,000-character
notes are valid under the existing 20,000-character limit. The invalid fixture
was corrected before the full gate; no production check was weakened.

## Security, integrity and limits

`UPDATE_SIGNING_KEY_STATUS = PRODUCTION_UPDATE_KEY_REQUIRED` (zero trusted
production key entries). `ANDROID_RELEASE_SIGNING_STATUS = NOT_CONFIGURED /
CONTINUITY_NOT_ESTABLISHED` (the release build remains unsigned). Test signing
identities do not establish production continuity. No production key was
generated. Changed-file private-key marker check is clear.

All 739 files captured under canonical/ and releases/ remained byte-for-byte
unchanged. No real release was published. Release-builder tests do not claim a
production signed release build. Linux no-replace/exchange support is required
for their respective atomic guarantees. A failure after final rename during
parent fsync can leave a complete directory with durability unconfirmed; a
successful run syncs it. Installer fallback limitations are stated above.

Native mechanisms were checked against the [XDG specification](https://specifications.freedesktop.org/basedir/0.8/),
[Linux rename semantics](https://man7.org/linux/man-pages/man2/rename.2.html)
and [fsync semantics](https://man7.org/linux/man-pages/man2/fsync.2.html).

## Files and cleanup

Changes are confined to updater source/tests and shared vectors, Android update
source/tests, packaging scripts/documentation/tests, the updating manual and
its manifest, the exact VS Code correction, and this report. No Core language
or Code Hygiene feature was implemented. Future `code_hygiene` mapping to
`kind.part.rules` remains compatible.

Task-owned Rust build trees: 25,467,468,718 bytes (23.72 GiB) -> 0 bytes after cleanup. Removed only
this run's three isolated target trees, generated E2E APK and throwaway private
test key. Retained source, configuration, public test key, signed manifest,
checksums, logs, screenshots and evidence. Existing Cargo/Gradle caches and
pre-existing build trees were preserved. Detailed before/after measurements
and removed APK digest are in the evidence directory.

FINAL VERDICT: IMPLEMENTATION_READY_FOR_INDEPENDENT_REVIEW.
Production updates are not operational until real update trust and Android
release-signing continuity are established. At test time nothing had been
committed or synced, and a split was proposed: the VS Code correction
separately from the updater repair, tests, documentation and this report. The
repair was later committed and pushed as the single commit d1151c1 (see
*Source states*).

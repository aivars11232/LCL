# LCL Android remote — post-audit repair report

Date: 2026-09-25. Verdict: **ANDROID_LCL_REMOTE_REPAIR_COMPLETE** for the
findings A1–A9 below. This is not a release-readiness claim: no release
candidate was built, `lcl-remote` is still outside the release payload, and
Core 0.2 remains blocked on its independent review (§6).

Nothing in this report was committed or pushed.

## 1. Starting state

- Repository `/mnt/F/LCL`, branch `main`.
- HEAD `028dac665fd53ed06fd58be38d4cf5d5df894ea8` ("LCL - ANDROID_LCL_REMOTE_MVP_COMPLETE"),
  parent `4f19a0a214494c0e7b45873918bc3c114e249847`.
- Working tree clean at start (`git status --porcelain -uall` empty).
- Baselines at the untouched HEAD: `remote` `cargo test --locked` 30 passed / 0
  failed (13 unit + 17 end-to-end); Android `testDebugUnitTest` 53 / 0.
- Toolchains: rustc/cargo 1.98.1; MSRV toolchain 1.75.0; JDK and Android SDK
  under `/mnt/F/.lcl-android` (outside the repository); emulator AVD `lcl36`
  (Android 16, API 36, x86_64).
- Work files outside the repository: `/mnt/F/.lcl-pretest/ar/` (logs, gate
  copy, one-phase driver, E2E work directory); gate logs
  `/mnt/F/.lcl-pretest/logs/AR-G-*`.

## 2. Findings

Every repair followed: reproduce against the current source, find the root
cause, add a test that fails before the repair where practical, smallest
production change, focused tests, then the next finding.

| # | Finding | Status |
|---|---|---|
| A1 | PC did not enforce effect approval for remote runs | FIXED_VERIFIED |
| A2 | Remote runs not tied to the device that started them | FIXED_VERIFIED |
| A3 | In-app QR scanner paired without confirmation | FIXED_VERIFIED (emulator; camera not exercised) |
| A4 | Unsharing a project did not reach a running service | FIXED_VERIFIED |
| A5 | CAS is process-local; absolute "no last-write-wins" claim | DOCUMENTED_BOUNDARY |
| A6 | Local Android files decoded leniently (U+FFFD) | FIXED_VERIFIED |
| A7 | Pre-authentication resource allowance | FIXED_VERIFIED |
| A8 | Release/evidence predates the Android commit | FIXED_VERIFIED (new evidence; no release claim) |
| A9 | Hardware-backed Keystore assurance | FIXED_VERIFIED (claim not found in committed docs; see A9) |

### A1 — mandatory pause before every effect (HIGH)

- **Reproduction.** New test `a_device_cannot_switch_off_the_pause_before_an_effect`
  (`remote/tests/remote.rs`) sends `run` with grants and `"break_effects":false`.
  At `028dac6` the first run event after the `operation` notice was
  `permission: granted` for `core.read` — no pause. Restoring the old
  conditional after the fix makes the test fail again the same way.
- **Root cause.** `Session::run` in `remote/src/session.rs` added the
  workspace's `break_effects=1` only when the device did not send `false`: the
  PC trusted the client for a security property.
- **Change.** `remote/src/session.rs`: every remote run is started with
  `break_effects=1`; the request field is ignored. Desktop runs are unchanged
  (`lcl-workspace` routes untouched).
- **Regression.** The test asserts: the first event after the dispatch notice
  is a `paused` of kind `effect`; for 1.5 s with no answer no further event
  arrives and neither file is touched; after the device answers, every `effect`
  event has its own `paused` before it (≥ 2 effects) and the run succeeds.

### A2 — run ownership (HIGH/MEDIUM)

- **Reproduction.** New test `only_the_device_that_started_a_run_can_follow_or_answer_it`
  pairs devices A and B. At `028dac6`, B's `follow` of A's run returned `200
  {"following":true}`, and (with that assertion skipped) B's `answer continue`
  returned `200 {"answered":true}` — B approved A's effect.
- **Root cause.** `follow` and `answer` forwarded any run id to the project's
  shared `Routes`; nothing recorded which device started a run. Run ids are
  sequential per `Routes` (`run-1`, `run-2`, …).
- **Change.** `remote/src/session.rs`: when a remote run starts, the PC records
  an owner keyed by (project id, run id) holding the authenticated device id,
  the certificate fingerprint, and a weak reference to the exact `Routes` the
  run lives in (run ids restart in freshly opened `Routes`, so a record only
  answers for the instance it saw). `follow` and `answer` (continue, deny,
  cancel) for an existing run are refused `403` unless both id and fingerprint
  match and the `Routes` is the same; the refusal text is identical whoever
  owns the run. Stale records are pruned when a new run is recorded. The
  desktop workspace's run semantics are unchanged.
- **Regression.** B's `follow`, `answer continue`, `answer deny` and
  `answer cancel` all get `403`; B receives no run event; A's run stays paused
  with nothing written; A then follows and answers to `status.succeeded`. The
  existing reconnect-and-follow test shows the same device still follows after
  reconnecting.

### A3 — scanner pairs only on confirmation (MEDIUM)

- **Reproduction.** New instrumented phase
  `RemoteEndToEndTest.p8_scan_fills_the_form_and_pairs_only_on_confirmation`
  presses the app's own **Scan QR code** button. The camera activity
  (`com.journeyapps.barcodescanner.CaptureActivity`) is answered by a blocking
  `Instrumentation.ActivityMonitor` with `RESULT_OK` and the pairing link as
  `Intents.Scan.RESULT`; the app's launcher, `ScanContract` parsing and
  callback are the real ones. With the original `PairScreen.kt` against a real
  `lcl-remote serve`: `AssertionError: a scanned code made a pairing key
  before Pair expected:<[]> but was:<[lcl-pc-e626…]>`, and the PC listed a
  paired device.
- **Root cause.** The scanner callback in `PairScreen.kt` set the link and
  called `pair(it)` directly.
- **Change.** `android/app/src/main/java/io/lcl/workspace/ui/PairScreen.kt`:
  the callback only clears any problem and sets the (trimmed) link — the same
  path as a pasted link.
- **Regression (green on the emulator).** After the scan: the preview with the
  PC fingerprint is visible, the link field holds the scanned link, and after
  3 s there is no new Keystore key, no PC record in SharedPreferences, no
  connection-state change and no pairing error; pressing **Pair** then pairs
  (Connected, exactly one new key). On the PC, `e2e.sh` checks exactly one new
  unrevoked device and every pairing code consumed. **A physical camera was
  not used.**

### A4 — unsharing reaches a running service (MEDIUM)

- **Reproduction.** New test `a_project_stops_being_shared_at_once_without_a_restart`
  drives the real `lcl-remote projects add/remove` CLI beside a running
  service. At `028dac6` it failed at "a new share was not offered"; a variant
  sharing before start failed at "an unshared project is still offered" after
  `projects remove` had printed success.
- **Root cause.** `Service::bind` loaded `remote.json` once into
  `Shared.config`; sessions listed projects from that copy, and `Projects`
  cached each project's `Routes` indefinitely.
- **Change.** `remote/src/projects.rs`: `Projects::current` and
  `Projects::open` re-read `remote.json` (written atomically by the CLI) under
  the cache lock on every access; opening refuses a project that is no longer
  shared, and `current` closes cached `Routes` of unshared projects. An
  unreadable `remote.json` shares only the default workspace.
  `remote/src/session.rs`: runs devices started in closed `Routes` are
  cancelled; a session notices closures at once (a counter checked before
  forwarding), stops forwarding those runs, tells the device `failed`
  ("this project is no longer shared by this PC, so the run was stopped") then
  `end`, and drops watched documents of unshared projects. Paths remain
  resolved only by the workspace; nothing new is exposed.
- **Regression.** With a document open and a run paused in the shared
  project: after `projects remove` (no restart) the project is gone from
  `projects`; `tree`, `open`, `save`, `check`, `follow`, `answer` and `run` on
  its id are all `404`; the file is unchanged; the device gets `failed` +
  `end` for its run and the paused effect never happens; sharing again offers
  and opens it afresh.

### A5 — CAS scope (MEDIUM) — DOCUMENTED_BOUNDARY

- **Assessment.** No cross-process document lock exists in the repository
  (the only file locks are `lcl-remote`'s registry locks, using
  `File::lock`, Rust ≥ 1.89). `lcl-workspace` is std-only with an MSRV of 1.75,
  so it cannot use `File::lock` without raising the MSRV or adding a
  dependency. More decisively, the desktop workspace's own save carries no
  precondition at all, so a cross-process lock between the two LCL processes
  still could not make a device save and a desktop save compare-and-swap
  against each other, and no lock binds other editors. Path A was therefore
  not taken.
- **What holds, now pinned by tests.** In `lcl-workspace`:
  `a_change_another_program_made_before_the_comparison_is_caught` (a raw
  `std::fs::write` after the save was accepted and prepared → `Changed`,
  nothing replaced, no temporary left) and `two_saves_from_one_revision_cannot_both_land`.
  In `remote`: `two_devices_saving_from_one_revision_cannot_both_land` (two
  paired devices save concurrently from one base → exactly one `200`, one
  `409`; the file holds the winner). A mutation that disables the in-lock
  precondition makes the first of these and the existing stale-save test fail.
- **Boundary, documented.** `write_expecting` in
  `impl/crates/lcl-workspace/src/document.rs` now states: every change made
  before the comparison is seen, whoever made it; writers in the same process
  cannot land between comparison and rename; another process (the desktop
  workspace or any editor) that replaces the file in that instant is not
  excluded and is replaced. The absolute sentence "There is no last-write-wins"
  in `android/README.md` was replaced with this boundary;
  `android/SUPPORTED_OPERATIONS.md` and `remote/README.md` say the same. The
  CAS code itself is unchanged.

### A6 — strict UTF-8 for local files (MEDIUM)

- **Reproduction.** New instrumented class `LocalDocumentScreenTest` opens a
  file containing `caf` + byte `0xE9` (a Windows-1252 é). With the original
  `SettingsAbout.kt` the screen showed editable source `NAME: "caf` + U+FFFD
  `"` (checked in the captured assertion text) with Check/Inspect available.
- **Root cause.** `ByteArrayOutputStream.toString("UTF-8")` replaces malformed
  input silently.
- **Change.** New `android/app/src/main/java/io/lcl/workspace/workspace/LocalText.kt`:
  a `CharsetDecoder` with `REPORT` for malformed and unmappable input, the
  same 4 MiB limit; a refusal names the first bad byte offset.
  `SettingsAbout.kt` (`LocalDocumentScreen`) uses it; on refusal no text,
  no editor and no Check/Inspect buttons exist, so nothing can be sent.
  Test tags `local_problem`, `local_check`, `local_inspect` were added.
- **Regression.** JVM `LocalTextTest` (3): exact round-trip including a
  4-byte character; six malformed forms (Windows-1252 é, `0xFF`, truncated,
  overlong, encoded surrogate, beyond U+10FFFF) are refused at the right
  offset while a lenient decoder demonstrably yields U+FFFD; the 4 MiB limit
  is unchanged. Instrumented `LocalDocumentScreenTest` (2): the bad file is
  refused and neither shown nor offered to the PC; a UTF-8 file is shown
  exactly with Check/Inspect present.

### A7 — pre-authentication hardening (MEDIUM)

- **Reproduction.** New tests. At the pre-fix source,
  `a_first_message_larger_than_a_hello_is_refused_at_once` failed (a paired
  device's 8 KiB-padded hello was welcomed; the reader allowed 16 MiB before
  authentication), and `unauthenticated_peers_cannot_crowd_out_the_rest`
  failed (a fifth idle connection from one address was accepted and held;
  only a global cap of 32 applied). The slot counter was also released only on
  a normal return from the session thread, so a panicking session leaked its
  slot (there is no `panic = "abort"` profile).
- **Change.** `remote/src/frame.rs`: the reader has a per-connection limit;
  `MAX_HELLO_FRAME` = 8 KiB until admission, `MAX_FRAME` = 16 MiB after.
  `remote/src/service.rs`: `Slots`/`Slot` — at most 32 connections, of which
  at most 8 unauthenticated, at most 4 unauthenticated from one address; a
  connection over a limit is dropped at accept; a `Slot` is released on drop
  (any exit, including a panic) and leaves the unauthenticated allowance on
  admission; a thread that cannot be spawned drops its slot.
  `remote/src/session.rs`: the session takes its `Slot`, marks it
  authenticated and raises the frame limit only after `welcome`. The 10 s
  TLS-plus-hello deadline and fail-closed handling are unchanged. No rate
  limiting was added.
- **Regression.** End-to-end: oversized hello closed with no answer; a length
  above 8 KiB with no body closed at once; after admission a 1 MiB frame is
  accepted; silent TCP, silent TLS and a partial frame all closed by the
  deadline; a non-UTF-8 first frame closed at once; 40 refused connections in
  a row followed by a normal reconnect (slots released); four idle peers from
  one address, the fifth closed at once, an authenticated session unaffected,
  and slots free again when the idle peers leave. Unit (`service.rs`, 3):
  overall and per-address caps; authenticated connections leave the
  allowance; slots come back even when a holder panics. Unit (`frame.rs`, 1).

### A8 — new evidence (RELEASE/EVIDENCE GAP)

Earlier release-closure reports are unchanged and describe their own commits.
This report is the evidence for the source state in §10; §4 lists every gate
re-run on it. No candidate was built and `releases/` was not touched, so
there is no release claim.

### A9 — Keystore protection assurance (LOW)

- **Finding check.** No committed document or comment claimed that keys are
  hardware-backed (searched: READMEs, `android/`, `remote/`, users manual,
  reports). The implementation did not inspect or report the level.
- **Change.** `KeystoreIdentities.kt`: `KeyProtection` (StrongBox, TEE,
  secure hardware of unreported kind, software, unknown), mapped from
  `KeyInfo.securityLevel` on API 31+ and `isInsideSecureHardware` before;
  `IdentityStore.protection(alias)` (default `null`). About shows **This
  device's key**. Key generation is unchanged; StrongBox is not required;
  Android Keystore and non-exportability remain mandatory.
  `android/README.md` now says hardware backing depends on the phone.
- **Tests.** JVM `KeyProtectionTest` (3): every level maps to the right kind,
  only hardware kinds are called hardware-backed, pre-31 flag mapping, a store
  that cannot say reports nothing. Emulator: p1 checks the app's report equals
  a direct `KeyInfo` read; p2 checks the About row. **The emulator reports
  `securityLevel=0` — a software key** — shown as "software keystore (not
  hardware-backed)".

### Other audit checks (touched areas)

Verified by existing or new tests, all passing on the final tree:
pairing code one-use, expiry, hash-only storage, consumed under a file lock
(`an_expired_or_reused_code_is_refused`, e2e p1 PC check); wrong PC certificate
refused in the handshake (`the_wrong_pc_is_refused_before_anything_is_said`,
Android pinning tests); failed pairing keeps the old one and re-pairing
replaces the key (`pairing_again_replaces_the_key_only_when_it_succeeds`, p7);
revoking or `unpair` affects only one device, no device can claim another's id
(existing tests); run control per A2; only shared projects visible, unsharing
per A4, path traversal refused, and — new — a symlink out of the project is
neither read nor written through the remote `save` path
(`a_link_out_of_the_project_is_neither_read_nor_written`); `.lcl`/`.lcl.txt`
listed and plain `.txt` not; stale, conflicting and deleted-file saves
(existing tests); typing during a save stays dirty
(`typing_during_a_save_stays_unsaved`); final line feed reported; atomic
publication (workspace tests); same-signature update keeps the pairing,
backup/transfer exclusions unchanged, no code or key in SharedPreferences,
Forget deletes the key (e2e); frame bound, strict UTF-8 frames, unsupported
protocol, unknown operations, malformed first message, revoked devices stop
(existing and new tests). TLS configuration was not changed: TLS 1.3 only,
mutual certificates, pinning both ways, no tickets, no early data.

## 3. Files changed

PC remote service: `remote/src/session.rs`, `remote/src/projects.rs`,
`remote/src/service.rs`, `remote/src/frame.rs`, `remote/tests/remote.rs`,
`remote/README.md`.

Workspace: `impl/crates/lcl-workspace/src/document.rs` (documentation of the
CAS boundary and two tests; no behavior change).

Android: `android/app/src/main/java/io/lcl/workspace/ui/PairScreen.kt`,
`…/ui/SettingsAbout.kt`, `…/remote/KeystoreIdentities.kt`, new
`…/workspace/LocalText.kt`; tests `android/app/src/androidTest/…/RemoteEndToEndTest.kt`,
new `…/androidTest/…/LocalDocumentScreenTest.kt`, new
`…/test/…/LocalTextTest.kt`, new `…/test/…/KeyProtectionTest.kt`;
`android/tools/e2e.sh`; `android/README.md`, `android/SUPPORTED_OPERATIONS.md`.

Report: this file.

Reviewed and left unchanged (nothing in them was inaccurate after the
repairs): `README.md`, `users_manual/02_Installing_and_Running.md`,
`users_manual/17_Tools_Reference.md`. No dependency or build configuration
changed (`Cargo.lock`/`Cargo.toml` of `remote/` and `impl/`, all Gradle files):
`rustls 0.23.45`, `ring 0.17.14`, `qrcode 0.14.1`; Android minSdk 29.

## 4. Tests executed

All on the final tree unless marked as a red/baseline run.

| Command | Exit | Result |
|---|---|---|
| `remote`: `cargo fmt --check` | 0 | clean |
| `remote`: `cargo clippy --locked --all-targets -- -D warnings` | 0 | no warnings |
| `remote`: `cargo test --locked` | 0 | 43 passed, 0 failed (17 unit + 26 end-to-end); repeated 5 more times: 43/0 each |
| gate copy `ar-gate.sh selftest` | 0 | wrapper records and fails on a failing command |
| `ar-gate.sh a`: `cargo fmt --all -- --check` | 0 | clean |
| `ar-gate.sh a`: `cargo clippy --offline --locked --workspace --all-targets -- -D warnings` | 0 | no warnings |
| `ar-gate.sh a`: `cargo test --offline --locked --workspace --all-targets --no-fail-fast` (1.98.1) | 0 | 1,868 passed, 0 failed, 1 ignored (167 blocks) |
| `ar-gate.sh b`: MSRV 1.75.0 `cargo check` and `cargo test` (same flags) | 0, 0 | 1,868 passed, 0 failed, 1 ignored |
| `ar-gate.sh c`: 20 commands (canonical SHA-256 both packages, validators, EBNF, identities, brand, conformance report, readiness gate, protected paths) | all expected | see §6 |
| `ar-gate.sh d`: `real_process`, 12 sequential + 3 × 6 concurrent | all 0 | 30/30 |
| Gate total | — | **50/50 commands at expected status** |
| `users_manual/tools/verify_examples.py` | 0 | 97 examples checked, 30 fragments skipped, 0 failures |
| `android`: `./gradlew testDebugUnitTest` | 0 | 59 passed, 0 failed (53 before + 6 new) |
| `android`: `./gradlew lint` | 0 | 1 warning, pre-existing (`OldTargetApi`); none in changed files |
| `android`: `./gradlew assembleDebug` | 0 | built |
| `android`: `./gradlew assembleRelease` | 0 | built, unsigned (no signing identity in the repository, by design) |
| `android/tools/e2e.sh` (emulator `lcl36`, API 36 x86_64, real `lcl-remote serve`) | 0 | 11/11 instrumented phases `OK (1 test)`, every PC-side check passed, no ANR or crash recorded for LCL |

The 11 E2E phases: `LocalDocumentScreenTest` × 2, p1 pair and work, p2
restart, p2 after reboot, p3 network loss, p4 PC service restart, p5 PC edits
and conflict, p6 revoked, p7 re-pair and forget, p8 scanner.

Red runs (before each repair; logs under `/mnt/F/.lcl-pretest/ar/`): A1, A2,
A4 (two variants), A7 (2 of 4 new tests red; the deadline and slot-release
tests pass before and after, pinning existing behavior) in `remote`; on the
emulator, A3 with the original `PairScreen.kt` restored temporarily from
`HEAD`, and A6 before `SettingsAbout.kt` was changed; A5 by mutation. A9 has
no red run (reporting added).

## 5. Tests not executed

- **Physical Android phone**: NOT_RUN — none available.
- **Physical camera scanning a printed or on-screen QR code**: NOT_RUN — the
  scanner's result path is tested with the camera activity answered by the
  instrumentation; the camera and ZXing's decoding on a phone are not.
  (ZXing decoding of the PC's QR image is checked on the PC by `e2e.sh`.)
- **Phone on a separate LAN, over the Internet, or on mobile data**: NOT_RUN.
- **Emulator instrumentation via `connectedDebugAndroidTest`**: not used; the
  instrumented tests run through `e2e.sh`, which supplies their arguments.
- **Hosted CI**: none exists for this repository.

## 6. Canonical integrity

- `canonical/LCL_Core_0.1.0`: `sha256sum -c` passes; identity
  `00d648b162939d06c44838481a67c39bc12c64bdd6d105035c24150148fe67ed`, 176 files;
  `validate_release.py --scope all` exit 0.
- `canonical/LCL_Core_0.2.0`: `sha256sum -c` passes; identity
  `00daee8de1919c4945ef04ff65edb22164bd8046a493be08a87d5fa3b4c3e604`, 216 files;
  `validate_release.py --scope all` exit 1 as expected: 31 PASS, 0 FAIL,
  2 OUT_OF_SCOPE, 1 BLOCKED — `language_decisions_and_release_state`,
  `pending_decisions: ["independent_review"]` only. The other 0.2.0 validators
  (language contracts, localization, source fixtures) and both EBNF checks
  exit 0.
- Conformance: 2,413 required probes; source_conforming 2,011/2,011 and
  semantics_conforming 402/402 satisfied, 0 failed, 0 missing, 0 invalid;
  claim `semantics_conforming`; readiness gate exit 0.
- Core 0.1 remains green. Core 0.2 remains blocked only on its independent
  review. No language semantics, grammar, registry or conformance mapping was
  changed.

## 7. Protected paths

`git diff --name-only -- canonical/ releases/ assets/brand/` printed nothing,
and `git status --porcelain --untracked-files=all -- canonical releases assets`
printed nothing. The gate's `protected` command (which also re-verifies every
candidate's checksums under `releases/candidates/`) and `brand` command
(`assets/brand/BRAND_ASSETS.sha256`) exited 0.

## 8. Android build

- `assembleDebug`: built. The E2E rebuilds the debug APK (including a
  versionCode 2 update build signed with the same debug key).
- `assembleRelease`: `app/build/outputs/apk/release/app-release-unsigned.apk`,
  24,485,441 bytes, SHA-256
  `8f71de423933827e10eb2d06cc6e79ebe276302dd2c4ebeb8ca8b3a3fb10fbc8`. Unsigned:
  release signing comes only from outside the repository.
- No APK, AAB or signing material is tracked or addable; `android/app/build/`,
  `android/.gradle/` and `android/local.properties` exist locally and are
  ignored by `android/.gitignore`.

## 9. Remaining limitations

- Not tested on a physical phone, with a physical camera, on a separate LAN,
  over the Internet or on mobile data.
- The save precondition cannot exclude another process (the desktop
  workspace, whose saves have no precondition, or any editor) that replaces
  the file in the instant between the comparison and the rename (A5).
- Peers that keep reconnecting can occupy the 8 unauthenticated slots (4 per
  address) and delay new connections for up to 10 s each; authenticated
  sessions are not affected. There is no rate limiting.
- When a project is unshared, if its routes are closed by another session in
  the instant this session is forwarding that run's events, the owning device
  can receive its own run's final cancellation report before the `failed`
  notice. No other device can receive it.
- `lcl-remote projects remove` matches the stored canonical path or project id;
  a differently spelled path (for example a relative one) is reported as "not
  a shared project" and nothing changes (pre-existing, not changed here).
- The tested emulator's device key is software-backed; hardware backing is
  per phone and not required.
- `lcl-remote` is not part of the LCL release payload; no release candidate
  was built for this source state; Core 0.2 awaits its independent review.

## 10. Final Git status

HEAD `028dac665fd53ed06fd58be38d4cf5d5df894ea8`, branch `main`, nothing staged.

```
 M android/README.md
 M android/SUPPORTED_OPERATIONS.md
 M android/app/src/androidTest/java/io/lcl/workspace/RemoteEndToEndTest.kt
 M android/app/src/main/java/io/lcl/workspace/remote/KeystoreIdentities.kt
 M android/app/src/main/java/io/lcl/workspace/ui/PairScreen.kt
 M android/app/src/main/java/io/lcl/workspace/ui/SettingsAbout.kt
 M android/tools/e2e.sh
 M impl/crates/lcl-workspace/src/document.rs
 M remote/README.md
 M remote/src/frame.rs
 M remote/src/projects.rs
 M remote/src/service.rs
 M remote/src/session.rs
 M remote/tests/remote.rs
?? android/app/src/androidTest/java/io/lcl/workspace/LocalDocumentScreenTest.kt
?? android/app/src/main/java/io/lcl/workspace/workspace/LocalText.kt
?? android/app/src/test/java/io/lcl/workspace/KeyProtectionTest.kt
?? android/app/src/test/java/io/lcl/workspace/LocalTextTest.kt
?? reports/implementation/LCL_ANDROID_REMOTE_AUDIT_REPAIR_REPORT.md
```

## 11. Commit and push

No commit was made and nothing was pushed. No branch was created, nothing was
staged, stashed, reset or cleaned.

Afterwards, on 2026-09-25, the owner asked for the work to be committed and
synced. The changes listed in §3 are commit
`66921afc50e8d16804e1f73ba21b76d2410742a3`, on `main` directly after
`028dac6`, and this report is the commit after it. §10 and the first
paragraph of this section describe the tree as it was when the report was
written, before those commits.

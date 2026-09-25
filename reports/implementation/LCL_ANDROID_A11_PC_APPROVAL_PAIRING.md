# LCL for Android — A11 PC-approved pairing and non-URI QR bootstrap

Date: 2026-09-25. Verdict: **ANDROID_A11_PC_APPROVAL_PAIRING_COMPLETE** for
finding A11 below. Every required check reached its expected result (section
17). A1–A10 were not reopened. This is not a release-readiness claim.

Nothing in this report was committed or pushed.

## 1. Starting HEAD

- Repository `/mnt/F/LCL`, branch `main`.
- HEAD `ec7f65e2cfe413bcc9fe473c637bd44e4182b7f5` ("Report: Android pairing-link
  repair (A10)"), the expected synced state. History: MVP `028dac6`, A1–A9
  `66921af` + report `3ea5152`, A10 `e3e197c` + report `ec7f65e`.

## 2. Clean-state verification

Before any edit: `git branch --show-current` → `main`; `git rev-parse HEAD` →
`ec7f65e2…`; `git status --porcelain --untracked-files=all` → empty;
`git fetch origin` then `git rev-parse origin/main` → `ec7f65e2…` (equal). No
stash, no branch was made.

Toolchains: rustc/cargo 1.98.1 (and 1.75.0 for the MSRV gate); JDK 21 and the
Android SDK under `/mnt/F/.lcl-android`; emulator AVD `lcl36` (Android 16,
API 36, x86_64), headless; Node for the desktop acceptance script; Python 3.14.

All work files are outside the repository, in `/mnt/F/.lcl-pretest/a11/`
(logs, the reproduction probe, E2E work directories, screenshots). The
`lcl-remote` build directory is `/mnt/F/.lcl-pretest/remote-target`.

## 3. A11 reproduction (before any production change)

**Source.** At `ec7f65e`, `remote/src/session.rs` `admit()`, lines 359–378,
for a `hello` with intent `pair`:

1. `shared.pairing.consume(code, fingerprint, now)` (`pairing.rs` 167–186)
   checks the code's hash, expiry and whether it was used;
2. it marks the challenge used (`consumed_at`, `consumed_by` = the presenting
   certificate's fingerprint) — the **first** presenter spends it;
3. the very next statement is `shared.registry.add(name, fingerprint, …)`: a
   trusted device record, with no step in between;
4. the PC answers `paired` and `welcome` and serves an authenticated session;
5. the same certificate later connects with intent `connect`;
6. `pairing.json` had only challenges (`version` 1); there was no pending,
   approved or denied state anywhere.

**Red regression test.** `remote/tests/remote.rs`
`a11_r1_a_valid_code_alone_does_not_create_trust` (added first, before any
production edit) requires `pairing_pending`, no trusted device and a refused
`connect`. Against `ec7f65e` it failed
(`/mnt/F/.lcl-pretest/a11/red-a11-r1.txt`, exit 101):

```text
assertion `left == right` failed: a valid code alone was answered with
Object([("type", String("paired")), ("device", …"whoever holds the code"…
  left: "paired"
 right: "pairing_pending"
```

**Whole chain, out of the repository.** A probe crate in
`/mnt/F/.lcl-pretest/a11/repro/` (depends on `remote/` by path; not part of
the repository) started a real service, issued a challenge, and connected as
a client that is not the LCL app, with a fresh certificate and the code
(`repro-pre-a11.txt`):

| Step | Pre-A11 result |
|---|---|
| attacker presents the code | `paired` + `welcome` (device `attacker`) |
| `pairing.json` after | `consumed_at` set, `consumed_by` = attacker's fingerprint |
| `devices.json` after | a live record for the attacker's fingerprint |
| attacker `intent=connect` | `welcome` |
| legitimate phone, same code, second | `pairing_refused` — "already used" |

Root cause: the one-time code was a **bearer enrollment credential** —
possession of an unused code led directly to `Registry::add`.

## 4. Old trust boundary

```text
QR code → code → first certificate to present it → Registry::add → trusted
```

A10 stopped LCL from taking `lclpair://` links from other apps, but an app the
QR code was scanned with could still read the code and race the phone.

## 5. New trust boundary

```text
QR/code alone cannot create a trusted device.
```

```text
code + certificate proof        → pending request (no trust, no session)
person approves on the PC       → approval bound to that challenge + that
                                   certificate fingerprint
same certificate asks again     → device record, code spent, others superseded
```

Only `lcl-remote approve` (CLI) or the workspace's **Approve device** (which
runs that command) moves a request to approved. Nothing on the phone, no
request id, name or address, and no number of retries can.

## 6. Pairing state machine

Per candidate (a request), keyed by challenge + candidate certificate
fingerprint:

```text
            present (new)            approve (PC)           present again
   (none) ───────────────► pending ──────────────► approved ─────────────► finalized
                              │  │                    │
                   deny (PC)  │  │ another approved   │ deny (PC, before finishing)
                              ▼  ▼                    ▼
                          denied  superseded        denied  (the code then pairs nobody)

   expired = pending/approved past the code's expiry (derived, pruned)
```

Rules (all in `remote/src/pairing.rs`, under `pairing.lock`):

- A new request needs a live, unspent, unapproved challenge; it does **not**
  consume the challenge, so a stranger asking first cannot lock the real
  phone out.
- The same challenge + fingerprint is always the same request (same id);
  a denied one stays denied (`pairing_denied`), a superseded one is refused.
- `approve` requires `pending`, not expired, and no other approved request for
  the challenge; it records `challenge.approved = request` and supersedes the
  other pending requests. A second approval for the same code is refused.
- `deny` requires `pending` (or `approved` not yet being finished). Denying a
  pending request leaves the code usable; withdrawing an approval leaves the
  code usable by nobody.
- **Finalization** happens when the approved certificate asks again with the
  code: the challenge must name exactly this request, and the certificate
  fingerprint must equal the approved one.
- A finalized request asked again (the answer was lost) returns the same
  device while that record is still trusted; a revoked device is never
  revived that way.

**Crash/retry ordering and invariant.** Finalization writes, in order:
(1) the reserved device id into the approved candidate (`pairing.json`);
(2) the device record under that id (`Registry::enroll`, idempotent for the
same id + fingerprint); (3) candidate `finalized`, challenge spent, other
candidates superseded (`pairing.json`). Invariant: at every point only the
approved certificate can complete, because the challenge names the approved
request and approving another is refused while one is approved; a crash
after (1) or (2) is finished by that certificate's next ask, even past the
code's expiry, without a second record. Tested by
`pairing::tests::a_crash_between_approval_and_trust_is_finished_only_by_the_approved_certificate`
and `devices::tests::enrolling_again_after_a_crash_makes_no_second_record`.

**Lock ordering.** `pairing.lock` before `devices.lock`, never the reverse
(documented in `pairing.rs`); `approve`/`deny` take only `pairing.lock`,
`revoke`/`rename` only `devices.lock`.

## 7. QR / pairing payload v2

```text
LCLPAIR|v=2&pc=<id>&n=<name>&fp=<pc-cert-sha256>&a=<address>&a=<address>&c=<code>&e=<expiry>
```

No `://`, no URI scheme, plain ASCII; percent encoding as before (`|` in a
name becomes `%7C`); multiple `a=`; manual paste supported. It carries the
format version, PC id and name, PC certificate fingerprint, addresses,
one-time code and expiry — no private key, no device key, no approval, no
trusted-device record. The PC generates only v2 (`lcl-remote pair`, terminal
QR, `--json` field `payload`, the workspace QR via that command). The QR
still initiates only a request.

## 8. Legacy v1 behaviour

- Android refuses `lclpair://pair?v=1…` and `LCLPAIR|v=1…` for new pairing with
  *"This pairing code uses the older pairing flow. Update LCL on the PC and
  show a new QR code."* (`PairingLink.parse`; the same message in Rust's
  `Payload::parse`).
- The PC refuses, whatever client sends it, a pairing `hello` without
  `"pairing_version": 2`: missing, `1`, `"2"` (a string) or `null` →
  `pairing_upgrade_required`; `3` → `unsupported_pairing_version`. Nothing is
  recorded and the code stays usable (A11-R7).
- `intent=connect` is unchanged.

## 9. Pending-request persistence

`~/.local/state/lcl/remote/pairing.json`, now `version` 2 (version 1 is still
read; an unknown version, a malformed candidate or an unknown status fails
closed):

```json
{"version": 2,
 "challenges": [{"id", "hash" (SHA-256 of the code), "created", "expires",
                 "consumed_at", "consumed_by", "approved"}],
 "candidates": [{"request", "challenge", "fingerprint", "name",
                 "verification", "created", "expires",
                 "status": "pending|approved|denied|finalized|superseded",
                 "decided_at", "device"}]}
```

The raw code is never stored. Limits: 8 waiting requests per code, 32 per PC,
32 records per code (decided ones included); over a limit → `pairing_busy`,
fail closed. On every write, challenges a day past expiry are pruned, and the
candidates of expired codes that never became trusted are pruned.

## 10. Verification-code derivation

```text
code_hash    = SHA256(UTF8(base64url_code))            (= the stored challenge hash)
material     = "lcl-pair-v2" 0x00 pc_fp 0x00 hex(code_hash) 0x00 candidate_fp
verification = first 12 lowercase hex of SHA256(material), shown abcd-ef12-3456
```

Test vectors, computed independently in Python
(`/mnt/F/.lcl-pretest/a11/sas-vectors.txt`) and asserted in both Rust
(`pairing::tests::verification_codes_match_the_published_vectors`) and
Kotlin (`PairingVerificationTest`):

| code | PC fingerprint | candidate fingerprint | verification |
|---|---|---|---|
| base64url(32 × 0x07) = `BwcH…Bwc` | `a`×64 | `b`×64 | `edbb-8bd3-ad82` |
| base64url(0x00…0x1f) = `AAEC…Hh8` | SHA256("pc") | SHA256("phone") | `bd05-957e-fdfc` |
| same | SHA256("pc") | SHA256("another phone") | `d81e-cb19-d16e` |

The phone computes its own code from its certificate and fails closed if the
PC reports a different one. Approval is bound to the full fingerprint, never
to these 12 characters.

## 11. PC CLI changes

```text
lcl-remote pending [--json]     request, device name, full fingerprint,
                                verification, asked (age), expiry, status;
                                never the code
lcl-remote approve REQUEST-ID   exactly one request
lcl-remote deny REQUEST-ID      exactly one request
lcl-remote pair [...] [--json]  v2 pairing text; `--json` field "payload"
```

`pair` now says that scanning does not trust the phone and how to approve.
`devices` still means trusted records only; requests are only in `pending`.

## 12. Desktop Workspace changes

`impl/crates/lcl-workspace`: routes `GET /api/remote/pending`,
`POST /api/remote/approve?id=`, `POST /api/remote/deny?id=` (behind the same
loopback address and session token; the id must be lowercase hex ≤ 64 before
any program runs; direct arguments, no shell). **Settings → Android devices**
gains **Pending pairing requests**: device name, verification code
(prominent), full fingerprint, expiry countdown, **Approve…** (opens an inline
confirmation "Approve this Android device? / Verification code / Fingerprint"
with **Approve device** and **Cancel**) and **Deny**. Every value is set as
text (`textContent`); the page still has no markup sink (route test
`the_frontend_never_assigns_markup`). The trusted list stays separate. No
automatic approval, whatever the number of requests.

## 13. Android UX changes

- Scanner and paste only fill the form (unchanged); the field now reads
  `LCLPAIR|v=2&…`.
- **Pair** makes a candidate key and asks the PC. On `pairing_pending` the
  screen shows *Waiting for approval on <PC>*, **Verification code**
  `abcd-ef12-3456`, and *On the PC, approve the pending device only if this
  code matches*, with **Cancel**. Nothing is saved, the PC is not made
  active, and the connection is not reported.
- It asks again every 2 s (`PAIRING_POLL_MS`), keeps waiting through a brief
  network loss, and stops on approval, denial, expiry, Cancel, leaving the
  screen, or process end.
- Approved: the record is saved, the key kept, the PC made active and
  connected. Denied, expired, cancelled or refused: the attempt's key is
  deleted, nothing is recorded; an older pairing with the same PC is replaced
  only on success.
- At start, `lcl-pc-*` Keystore keys no saved PC uses (left by a process that
  died while waiting) are deleted.

## 14. Files changed

Remote (`remote/`): `src/pairing.rs` (state machine, payload v2,
verification, tests), `src/session.rs` (pairing `hello`), `src/devices.rs`
(`enroll`, `clean_name`), `src/main.rs` (`pair`, `pending`, `approve`,
`deny`, help), `tests/remote.rs` (A11-R1…R9, helpers), `README.md`;
comment/message wording in `src/qr.rs`, `src/b64.rs`, `src/config.rs`,
`src/service.rs`.

Android (`android/`): `remote/PairingLink.kt` (v2 parser, legacy refusal,
`PairingVerification`), `remote/Protocol.kt` (`pairing_version`),
`remote/Transport.kt` (`Opened.Pending`), `remote/KeystoreIdentities.kt`
(`aliases`), `connection/ConnectionManager.kt` (approval wait loop, key
sweep), `ui/PairScreen.kt`, `ui/HomeScreen.kt`; tests `ConnectionManagerTest.kt`,
`Fakes.kt`, `ProtocolTest.kt`, `RemoteEndToEndTest.kt`,
`IncomingIntentsTest.kt`; `tools/e2e.sh`; `README.md`,
`SUPPORTED_OPERATIONS.md`.

Workspace (`impl/crates/lcl-workspace/`): `src/routes.rs`, `src/remote.rs`,
`assets/app.js`, `assets/app.css`, `tests/routes.rs`, `tests/editor_save.rs`,
`tests/editor_save.cjs`.

Manual: `users_manual/02_Installing_and_Running.md`,
`users_manual/17_Tools_Reference.md`.

New: this report. 34 modified files, 2,883 insertions, 479 deletions before
the report.

## 15. Red tests

| Test | Before | Evidence |
|---|---|---|
| `a11_r1_a_valid_code_alone_does_not_create_trust` | FAILED (`paired` instead of `pairing_pending`) | `red-a11-r1.txt`, exit 101 |
| probe, pre-A11 | attacker trusted, `connect` welcomed, real phone refused | `repro-pre-a11.txt` |
| probe, post-A11 | old hello → `pairing_upgrade_required`; v2 hello → `pairing_pending`; no `devices.json`; `connect` → `not_paired`; the real phone's later request also `pairing_pending` | `repro-post-a11.txt` |

## 16. Green tests

Remote integration (`remote/tests/remote.rs`, real service, real TLS 1.3):

| ID | Test |
|---|---|
| R1 | `a11_r1_a_valid_code_alone_does_not_create_trust` |
| R2 | `a11_r2_approval_trusts_only_the_exact_certificate` (B with A's code and A's request id; A with another code; A at another PC) |
| R3 | `a11_r3_a_stolen_code_used_first_still_trusts_nobody_but_the_approved_phone` (attacker first, then both concurrently) |
| R4 | `a11_r4_denying_a_stranger_leaves_the_code_to_the_real_phone` |
| R5 | `a11_r5_one_code_never_approves_two_devices` |
| R6 | `a11_r6_pending_and_approved_requests_expire_with_their_code` |
| R7 | `a11_r7_the_older_pairing_flow_is_refused_by_the_pc` |
| R8 | `a11_r8_a_device_paired_before_a11_reconnects_with_no_qr_code_or_approval` (pre-A11 `devices.json` + v1 `pairing.json`) |
| R9 | `a11_r9_pending_requests_are_deduplicated_and_bounded` |
| R10 | `pairing::tests::verification_codes_match_the_published_vectors` |

Plus unit tests in `pairing.rs` (pending only, approval/supersede/spend,
crash finishing, denial, withdrawn approval, expiry, global bounds and
pruning, unreadable state, v1 state file, payload round trip and legacy
refusal) and `devices.rs` (`enroll` idempotence), and
`every_refused_connection_gives_its_slot_back` now also asks as a pending
phone 40 times (slots given back).

Android JVM: `PairingLinkTest` (v2, malformed, legacy),
`PairingVerificationTest` (vectors), `ConnectionManagerTest`
(`pairing_waits_for_the_pc_and_saves_nothing_until_approved` incl. 2 s poll
bound, `a_denied_request_deletes_its_key_and_saves_nothing`,
`a_request_nobody_approves_expires_and_deletes_its_key`,
`cancelling_a_waiting_request_deletes_its_key_and_stops_asking`,
`a_waiting_request_outlasts_a_moment_without_the_pc`,
`a_pc_showing_another_verification_code_is_not_paired`,
`a_key_left_by_an_unfinished_pairing_is_deleted_at_the_next_start`,
`a_pc_paired_before_approval_existed_reconnects_without_a_qr_code`,
`pairing_again_replaces_the_key_only_when_it_succeeds` extended with denied
and cancelled attempts). Three existing tests counted total connection
attempts assuming pairing takes one connection; it now takes two (ask,
finish), so they compare against the count right after pairing — the
assertion (no attempt after Disconnect / revocation / Forget) is unchanged.

Desktop: `approving_or_denying_a_pairing_request_takes_only_a_request_id`,
`android_device_routes_need_the_session_token` (extended to the three new
routes), and the acceptance scenario
"pending pairing requests are approved only after a confirmation, and denied
at once" (both the controlled transport and real HTTP to the real
`lcl-workspace` with a stand-in `lcl-remote`): request rows, verification
code, full fingerprint, a markup device name shown as text with no extra
image, Deny calls `deny badd0000` only, Approve… shows the confirmation and
calls nothing until **Approve device**, Cancel approves nothing, the request
leaves the list, the device appears in the separate trusted list.

## 17. Commands, exit codes, counts

**Execution discipline.** Verification was deliberately run
**sequentially, one heavy process at a time**, because of host-resource
constraints: the PC has 14 GiB of RAM and 8 CPUs, and an earlier attempt that
overlapped the Android emulator with the workspace-wide Rust tests filled RAM
and swap and made VS Code crash. For the final verification, every heavy
command (Cargo, Gradle, the gate sections, the emulator) ran in the
foreground on its own; before each next one, no Cargo, rustc, test binary,
Gradle, Kotlin daemon, emulator or `lcl-remote serve` process of this task was
left (checked with `ps`). The emulator was started only for the final E2E and
shut down right after it. Worker counts were kept conservative: Cargo builds
with `CARGO_BUILD_JOBS=4`; Gradle with `--max-workers=2`, no lingering daemon
(`--no-daemon`, or `-Dorg.gradle.daemon=false` for the E2E script's own build)
and the Kotlin compiler in process. The only concurrency is what a gate
defines itself: section `d`'s three rounds of 6 concurrent `real_process`
runs. No setting changes what a test checks.

Logs are in `/mnt/F/.lcl-pretest/a11/` and, for the gate, in
`/mnt/F/.lcl-pretest/logs/A11-G-*.log`. The gate is
`/mnt/F/.lcl-pretest/a11/a11-gate.sh`, a copy of the established `ar-gate.sh`
whose only changes are its log prefix and results file (`ar-gate.sh` itself
was not modified); its results file records each command's own exit status
against the expected one.

| Command | Exit | Result |
|---|---|---|
| `remote`: `cargo fmt --check` | 0 | |
| `remote`: `cargo clippy --locked --all-targets -- -D warnings` | 0 | |
| `remote`: `cargo test --locked` | 0 | 62 passed, 0 failed, 0 ignored (27 unit + 35 integration; A10 had 43) |
| gate `selftest` | 0 | a failing command fails the gate, as designed |
| gate `a`: impl `cargo fmt --all -- --check` | 0 | |
| gate `a`: `cargo clippy --offline --locked --workspace --all-targets -- -D warnings` | 0 | |
| gate `a`: `cargo test --offline --locked --workspace --all-targets --no-fail-fast` (Rust 1.98.1) | 0 | 167 blocks: 1,869 passed, 0 failed, 1 ignored (A10: 1,868/0/1; +1 is the new route test) |
| gate `b`: Rust 1.75.0 `cargo check --workspace --all-targets` | 0 | |
| gate `b`: Rust 1.75.0 `cargo test --workspace --all-targets --no-fail-fast` | 0 | 167 blocks: **1,869 passed, 0 failed, 1 ignored** — identical to 1.98.1 (336 s) |
| gate `c`: `sha256sum -c` Core 0.1.0 / Core 0.2.0 | 0 / 0 | |
| gate `c`: `validate_release.py --scope all` Core 0.1.0 | 0 | 31 PASS, 0 FAIL, 0 BLOCKED, 2 OUT_OF_SCOPE; `release_ready: true` |
| gate `c`: `validate_release.py --scope all` Core 0.2.0 | 1 (expected 1) | 31 PASS, 0 FAIL, 1 BLOCKED (`language_decisions_and_release_state`: pending decision `independent_review` only), 2 OUT_OF_SCOPE |
| gate `c`: `validate_language_contracts`, `validate_localization`, `validate_source_fixtures` (0.2.0) | 0, 0, 0 | |
| gate `c`: `validate_ebnf.py` 0.1.0 / 0.2.0 | 0 / 0 | |
| gate `c`: identity 0.1.0 / 0.2.0 | 0 / 0 | `00d648b1…67ed` (176 files) / `00daee8d…e604` (216 files) |
| gate `c`: brand `sha256sum -c assets/brand/BRAND_ASSETS.sha256` | 0 | |
| gate `c`: `m8_conformance_report` | 0 | 2,413 required probes: source 2,011/2,011, semantics 402/402 satisfied; 0 failed, missing or invalid; `CLAIM: semantics_conforming` |
| gate `c`: `m8_conformance_gate` (readiness) | 0 | `ACCEPTED: claim semantics_conforming` |
| gate `c`: protected (canonical/releases/assets status + candidate checksums) | 0 | |
| gate `d`: `real_process` 12 sequential + 3 rounds × 6 concurrent | 0 × 30 | 30 runs × 12 tests: 360 passed, 0 failed, 0 ignored |
| gate verdict | — | 52 recorded commands (50 distinct; `msrv-check` also recorded by the two interrupted attempts), every one with its expected status |
| `lcl-workspace --test editor_save --test routes` | 0 | real-HTTP acceptance passed; routes 24 passed, 0 failed |
| `node tests/editor_save.cjs` (controlled transport) | 0 | 57 passed, 0 failed, 0 skipped |
| `python3 users_manual/tools/verify_examples.py --lcl <built lcl> --spec canonical/LCL_Core_0.1.0` | 0 | 97 checked, 30 fragments skipped, 0 failures |
| `android`: `./gradlew --offline testDebugUnitTest` | 0 | 71 passed, 0 failed (A10: 61) |
| `android`: `./gradlew --offline lint` | 0 | 0 errors, 1 warning: the pre-existing `OldTargetApi` (`targetSdk = 36`, `app/build.gradle.kts`, not changed by A11); nothing new |
| `android`: `./gradlew --offline assembleRelease` | 0 | `app/build/outputs/apk/release/app-release-unsigned.apk`, unsigned (no signing identity configured, as before), 24,501,729 bytes, SHA-256 `971913bff766403a0f56e2f08ab8cc2fc157dedcd2ef50925fdb63f4ad748249`; git-ignored, not added |
| `remote`: `cargo build --locked` (final binary) | 0 | `Fresh lcl-remote`: the binary already matched the current source; built 17:48:01, newer than every `remote/` source file (newest 17:45:55); SHA-256 `3ba8e9a3321b2e40747663b09740efd508de2e079ee43c0078edec65b2b83a01` |
| `LCL_REMOTE=<that binary> android/tools/e2e.sh` (final, fresh work directory `e2e-final`) | 0 | **16 of 16 phases passed**, every PC check passed, no LCL ANR or crash recorded |

**The final E2E used the final binary.** Its SHA-256 was recorded before the
run (`e2e-final/lcl-remote.sha256`) and checked again after it (`OK`); the
script runs the service and every `pending` / `approve` / `deny` / `revoke` /
`devices` from `LCL_REMOTE`. Earlier, `e2e-1` also passed 16/16, with a
binary built before four comment/message-only edits.

Final E2E, in order: two local-document phases (A6); four
`IncomingIntentsTest` phases (A10, including the new check that the v2 text
is taken by no activity); **p1** Pair → pending → the PC listed exactly one
request with the phone's own verification code (`92e3-8250-eec3`) and no new
trusted device → `lcl-remote approve` → Connected → edit, save, check,
validate, inspect, run with approvals (A1 pauses) → the code spent and never
stored → the QR code decodes to exactly the pairing text; a same-signed app
update; **p2** restart reconnects without QR, then again after a phone
reboot; **p3** network loss; **p4** PC service restart; **p5** PC edits and a
real save conflict (A5); **p6** revocation; **p7** the spent code refused, a
new code pending → approved → Connected, Forget; **p8** the app's scanner
fills the form only (A3), no key/record/connection before Pair, pending with
the matching code, approved → Connected; **p9** an older `lclpair://` link
refused on the phone, then a request **denied** on the PC → the phone shows
the denial, its attempt key is deleted, no record added, still Connected with
its working pairing; the denied code not spent; no request left waiting.

The stolen-code race (two different certificates with one code, the first
one a stranger's) is covered by the remote integration tests (A11-R3, with
real TLS and concurrent requests), not by the E2E, which has one phone.

## 18. Stolen-code race

A11-R3 (and the E2E): attacker asks first → `pairing_pending`; attacker and
phone ask concurrently → both `pairing_pending`; 0 trusted devices; attacker
`connect` → `not_paired`; `pending --json` lists 2 requests with different
verification codes and never the code; the one matching the phone's own
computed code is approved via `lcl-remote approve`; the phone finishes →
`paired` + `welcome`; attacker asks again → `pairing_refused`; attacker
`connect` → `not_paired`; a new certificate with the spent code →
`pairing_refused`; exactly 1 trusted device, the phone's.

## 19. Deny-then-legitimate

A11-R4: attacker pending → `lcl-remote deny` → attacker asks again →
`pairing_denied`; the phone with the same, still-live code → pending →
approved → paired; 1 trusted device; approving the denied request is refused.
E2E p9: the PC denies the phone's request → the phone shows *The PC denied
this device. Nothing was paired.*, its attempt key is deleted, no PC record is
added, and it stays Connected with its existing pairing; the denied code is
not spent and no request is left waiting.

## 20. Existing-pair migration / reconnect

- PC: A11-R8 starts a service over a `devices.json` and a version-1
  `pairing.json` written as the pre-A11 PC wrote them; the device connects
  with `intent=connect` (with and without naming its id) and gets `welcome`
  and `projects`; no request is created; new pairing works beside the old
  state file.
- Android: `a_pc_paired_before_approval_existed_reconnects_without_a_qr_code`
  loads a `PcRecord` stored as the earlier app stored it, plus its key; the
  app connects with `connect`, no pairing, and the key survives the start-up
  key sweep.
- E2E: after pairing, a same-signed app update (versionCode 2), app restart,
  phone reboot, network loss and a PC service restart all reconnect with no
  QR code and no approval (p2–p5).

## 21. A1–A10 regression status

All A1–A10 tests pass unchanged in intent: remote-run pause-before-every-
effect and ignoring `break_effects` (A1), run ownership (A2), scanner fills
the form only (A3, E2E p8), live unsharing (A4), save CAS (A5 boundary
unchanged), strict UTF-8 (A6, E2E local-document phases), pre-auth limits
8 KiB / 32 / 8 / 4 and slot return (A7, remote tests), key protection
reporting (A9, E2E p1/p2), no external pairing link (A10: `ManifestTest`,
`IncomingIntentsTest` ×4 in the E2E, now also checking that the v2 text is
taken by no activity as a shared text or a VIEW). TLS 1.3, mutual
certificates, PC pinning, device certificate binding, revocation and project
confinement are untouched.

## 22. Canonical / conformance status

Gate section `c`, run alone after the Rust 1.75 tests had ended:

- **Core 0.1:** checksums verified; `validate_release.py --scope all` exit 0,
  31 PASS / 0 FAIL / 0 BLOCKED, release-ready; EBNF valid; identity digest
  `00d648b162939d06c44838481a67c39bc12c64bdd6d105035c24150148fe67ed` — green.
- **Core 0.2:** checksums verified; exit 1 as expected, 31 PASS / 0 FAIL /
  1 BLOCKED. The one blocked check is `language_decisions_and_release_state`,
  whose only pending decision is the pre-existing **`independent_review`**;
  contracts, localization and source fixtures valid; EBNF valid; identity
  digest `00daee8de1919c4945ef04ff65edb22164bd8046a493be08a87d5fa3b4c3e604`.
- **Semantic conformance:** 2,413 required probes, all satisfied;
  `CLAIM: semantics_conforming`.
- **Readiness gate:** `ACCEPTED: claim semantics_conforming`.
- **Brand integrity:** verified. **Protected paths:** unchanged (section 23).

A11 changes no language semantics: the engine crates are untouched, and the
only `impl/` changes are in `lcl-workspace` (two routes, the Settings page and
their tests).

## 23. Protected paths

```text
$ git -C /mnt/F/LCL diff --name-only -- canonical/ releases/ assets/brand/
(no output)
$ git -C /mnt/F/LCL status --porcelain --untracked-files=all -- canonical releases assets/brand
(no output)
```

## 24. Artifacts / secrets

- No APK, AAB, JKS, keystore, private key, certificate material,
  `local.properties`, build directory, `.gradle`, Rust `target`, log or temp
  file is tracked or untracked in the repository; the only untracked file is
  this report, and nothing is staged. The release APK stays in the
  git-ignored `android/app/build/`.
- The added lines contain no key or password material.
- None of the one-time codes either E2E run issued appears anywhere in the
  tree or in this report (checked by searching for each code); the example
  codes in tests are fixed test values (`0x07` × 32, `0x00…0x1f`).
- Build and test output stays in `/mnt/F/.lcl-pretest/a11/` and the build
  directories outside the repository.

## 25. Remaining limitations

- A stolen code can still cause denial of service: up to 8 waiting requests
  per code (32 per PC) can keep the real phone's request from being recorded
  until the person denies them or shows a new QR code. Accepted by the task
  ("temporary pending-request noise or denial-of-service").
- The verification code is 48 bits and relies on the person comparing it;
  someone who approves a request without comparing can still approve a
  stranger. Approval is bound to the full certificate fingerprint.
- Device names in requests are the device's own claim (bounded, control
  characters removed) and are not identity.
- An approval that the phone does not act on before the code expires lapses;
  the person shows a new QR code.
- A phone whose app process dies while waiting leaves its key until the next
  app start (then swept); the PC request expires with its code.
- The desktop's pending list follows changes only while the Settings dialog
  is open (2 s polling), as the device list did before.
- Not tested: a physical phone, a physical camera scanning a QR code, a phone
  on a separate LAN, the Internet or mobile data.

## 26. Physical phone / camera / network

- Physical phone: **NOT_RUN**.
- Physical camera: **NOT_RUN** (the scanner is tested by answering its camera
  activity with the scanned text).
- Separate LAN / Internet / mobile data: **NOT_RUN**. The emulator reaches the
  host at `10.0.2.2`.

## 27. Final `git status`

`git status --short --branch --untracked-files=all`, after every check had
finished and every process of this task had ended:

```text
## main...origin/main
 M android/README.md
 M android/SUPPORTED_OPERATIONS.md
 M android/app/src/androidTest/java/io/lcl/workspace/IncomingIntentsTest.kt
 M android/app/src/androidTest/java/io/lcl/workspace/RemoteEndToEndTest.kt
 M android/app/src/main/java/io/lcl/workspace/connection/ConnectionManager.kt
 M android/app/src/main/java/io/lcl/workspace/remote/KeystoreIdentities.kt
 M android/app/src/main/java/io/lcl/workspace/remote/PairingLink.kt
 M android/app/src/main/java/io/lcl/workspace/remote/Protocol.kt
 M android/app/src/main/java/io/lcl/workspace/remote/Transport.kt
 M android/app/src/main/java/io/lcl/workspace/ui/HomeScreen.kt
 M android/app/src/main/java/io/lcl/workspace/ui/PairScreen.kt
 M android/app/src/test/java/io/lcl/workspace/ConnectionManagerTest.kt
 M android/app/src/test/java/io/lcl/workspace/Fakes.kt
 M android/app/src/test/java/io/lcl/workspace/ProtocolTest.kt
 M android/tools/e2e.sh
 M impl/crates/lcl-workspace/assets/app.css
 M impl/crates/lcl-workspace/assets/app.js
 M impl/crates/lcl-workspace/src/remote.rs
 M impl/crates/lcl-workspace/src/routes.rs
 M impl/crates/lcl-workspace/tests/editor_save.cjs
 M impl/crates/lcl-workspace/tests/editor_save.rs
 M impl/crates/lcl-workspace/tests/routes.rs
 M remote/README.md
 M remote/src/b64.rs
 M remote/src/config.rs
 M remote/src/devices.rs
 M remote/src/main.rs
 M remote/src/pairing.rs
 M remote/src/qr.rs
 M remote/src/service.rs
 M remote/src/session.rs
 M remote/tests/remote.rs
 M users_manual/02_Installing_and_Running.md
 M users_manual/17_Tools_Reference.md
?? reports/implementation/LCL_ANDROID_A11_PC_APPROVAL_PAIRING.md
```

Nothing is staged. HEAD is still `ec7f65e2cfe413bcc9fe473c637bd44e4182b7f5`,
equal to `origin/main`.

## 28. Commit state

- Committed: NO
- Pushed: NO

Afterwards, on 2026-09-25, the owner asked for the work to be committed and
synced. The changes listed in §14 are commit
`b8fbc2922355beebeee87d02600b44b995c87940`, on `main` directly after
`ec7f65e`, and this report is the commit after it. §27 and the two lines
above describe the tree as it was when the report was written, before those
commits.

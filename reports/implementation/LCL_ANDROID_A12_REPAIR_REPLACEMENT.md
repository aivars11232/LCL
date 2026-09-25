# LCL for Android — A12: a replaced pairing left its old PC credential trusted

Date: 2026-09-25. Verdict: **ANDROID_A12_REPLACEMENT_REPAIR_COMPLETE** for
finding A12 below. Every required check reached its expected result
(section 10). A1–A11 were not reopened. This is not a release-readiness claim.

Nothing in this report was committed or pushed.

## 1. Starting HEAD

- Repository `/mnt/F/LCL`, branch `main`.
- `git rev-parse HEAD` → `718a0892efb310130fca850b1b99023d394fba84` ("Report:
  Android PC-approved pairing (A11)"); `git rev-parse origin/main` → the same;
  `git ls-remote origin refs/heads/main` → the same. History: MVP `028dac6`,
  A1–A9 `66921af` + report `3ea5152`, A10 `e3e197c` + report `ec7f65e`, A11
  `b8fbc29` + report `718a089`.
- `git status --porcelain --untracked-files=all` → empty. No stash, no branch.
- Evidence and scratch files are outside the repository, in
  `/mnt/F/.lcl-pretest/a12/`.

## 2. Reproduction (before any production change)

The regression tests were written first, against the unmodified production
source, with test doubles that model the real PC and the real keystore
(section 4). Only test files had changed when they ran
(`git status`: `ConnectionManagerTest.kt`, `Fakes.kt`).

**JVM, red** (`red-android-unit.txt`): 74 tests, 2 failed — exactly the two
A12 tests, for the A12 reason:

```text
a12_r2_pairing_again_while_trusted_retires_the_old_credential_before_deleting_its_key
  after pairing again, the PC must trust exactly the new key
  expected:<[4a791c18…abef]> but was:<[5456c5e7…882d, 4a791c18…abef]>
a12_r3_forget_after_pairing_again_leaves_nothing_of_this_device_trusted
  after Forget, the PC must trust nothing of this device
  expected:<[]> but was:<[5456c5e7…882d]>
```

The other 72 — all 71 existing tests, now with a fresh key per pairing, and
A12-R1 — passed.

**Real stack, red** (`red-e2e.sh`, `red-e2e/out/summary.txt`): the unmodified
app (production source of `718a089`) with the new A12 phases, on the emulator
with the real Android Keystore, against a real `lcl-remote serve` built from
the unmodified source (SHA-256 `3ba8e9a3…`, the A11 final binary):

| Step | PC (`lcl-remote devices --json`) |
|---|---|
| pair (p0), approve | `d80ce0b8…` fingerprint `1b8aac83…` TRUSTED |
| pair the same PC again while trusted (p10), approve; the phone deleted its old key | `d80ce0b8…` `1b8aac83…` **TRUSTED**, `c259852f…` `ab3bfa4c…` TRUSTED |
| Forget (p11) | `d80ce0b8…` **still TRUSTED**, `c259852f…` REVOKED |

**PC side** (`remote/tests/remote.rs`
`a12_a_replaced_key_stays_trusted_until_it_unpairs_itself_and_nothing_else_ends`,
real service, real TLS 1.3, passes on the unmodified PC source): a phone that
pairs again with a new certificate becomes a second record beside the first;
only the old key's own `unpair` ends the old record; that ends nothing else.

## 3. Root cause

`ConnectionManager.pair()` makes a fresh Android Keystore key for every
attempt. On `Opened.Accepted` it saved the new `PcRecord` and immediately ran
`identities.delete(previous.keyAlias)`. Nothing ever told the PC that the old
certificate was replaced. The PC keeps one record per certificate:
`Registry::enroll` revokes an older live record only when its fingerprint
equals the new one, and a new key never has the same fingerprint. So the PC
went on trusting the old record, while the only thing able to end it — the
old private key — had been destroyed on the phone. A later Forget unpaired
only the current session's key.

The PC cannot fix this on its own: a new certificate is indistinguishable
from a second phone, and a client's claim ("I replace device X") is not
proof. Only possession of the old key is.

## 4. Why the existing tests missed it

- `FakeIdentities.create()` returned the same `testIdentity()` for every
  attempt, so a re-pair presented the **same** fingerprint. `FakePc.devices`
  is keyed by fingerprint, so the second pairing overwrote the first entry —
  the same effect as `enroll`'s same-fingerprint revocation — and a stale
  record could not exist.
- `FakePc` did not model `unpair`: it answered every request `200 {}`.
- The emulator E2E re-paired only in p7, after p6 had revoked the old device.

Now `FakeIdentities` makes a distinct real key and certificate per `create`
(made by the JDK's `keytool`, memoized per index; the first is still
`testIdentity()`; another phone's store starts at another index), and
`FakePc` keeps one record per certificate, revokes on `unpair` exactly the
certificate the session proved, and can be told to fail `unpair` or to lose
connections of one certificate.

## 5. Security invariant

After a successful replacement pairing with an already-paired PC:

```text
old Android credential:  PC trust = REVOKED   old local key = deleted
new Android credential:  PC trust = ACTIVE    new local key = retained
```

The PC holds exactly one live credential of this phone for that pairing. A
key is deleted only after the PC was shown to trust it no more. A replacement
is never reported finished before that. A subsequent Forget leaves no
credential of this phone trusted (when the PC is reachable; see 7).

## 6. Implementation

Android only; the PC already had the right primitive. `unpair` revokes the
record of the certificate that authenticated the session and ignores any id
in the message (`session.rs`, unchanged), so no client-supplied claim is
trusted.

- `PcRecord.retiring: List<RetiringKey>` (`data/Stores.kt`): earlier keys of
  this phone for the PC, each with the device id the PC gave it and the last
  reason it is not retired yet. Persisted as `"retiring"`; a record without
  it (every record saved before) reads as empty.
- `pair()`: on approval the new record is saved with the previous key (and
  any earlier unretired keys) listed in `retiring`; the key is no longer
  deleted there. The loop moves to the new session (which also stops the old
  connection), then `retirePrevious()` runs in the manager's own scope — so
  leaving the Pair screen does not stop it — and `pair()` returns the record
  as saved afterwards. After saving, a cancelled `pair()` no longer deletes
  the new key.
- `retire()`: the old key opens its own TLS 1.3 connection (PC pinned as
  always), `hello` intent `connect` **without** a device id, and sends
  `unpair`. Retired means the PC's own answer: `200 {"unpaired": true}`, or the
  old key refused as `revoked` or `not_paired`. Anything else — unreachable,
  another machine, `unavailable`, a `403` (which also covers an unreadable
  registry), a `500`, a lost answer, 15 s passing (`RETIRE_WITHIN_MS`) — is
  not proof. A listed key whose certificate is the current one is never used
  to unpair (it would end the current pairing).
- `retirePrevious()`: under one `Mutex`, per listed key: retired → the entry
  is removed from the saved record, then the key is deleted; not retired →
  the entry stays, with the reason.
- The connection loop: after every connection it made itself, a pending
  retirement is tried again; it saves over the record as saved *now*, so a
  concurrent retirement is never undone.
- `start()`: the start-up sweep keeps listed keys.
- `forget()`: unchanged for the current key; each listed earlier key asks the
  PC once more for itself, then is deleted.
- UI: the Pair screen does not move on when a retirement is pending: it shows
  "Paired with PC again, and connected", the notice, and **Continue**. The PC's
  card on the PCs screen shows the notice while any key is listed; the Forget
  dialog names the device. The notice (`retiringNotice`) says which device the
  PC may still trust, why it is not retired yet, that the app tries again on
  every connection, and how to revoke it on the PC. The PCs screen follows a
  new `records` flow, so it updates when a retirement finishes.

Nothing about TLS, pinning, device authentication, the A11 pairing state
machine or the PC changed.

## 7. Failure ordering and boundary

Order: (1) the PC trusts the new key (approval, A11); (2) the phone saves the
new record with the old key listed; (3) the old key proves itself and the PC
revokes it (or already had); (4) the entry is removed; (5) the old key is
deleted.

| Stops after | State | Recovery |
|---|---|---|
| (1), before (2) | PC trusts both; the phone still uses the old key; the new key is unreferenced and swept at the next start | the new PC record can never authenticate again (its key is gone). This window exists for every pairing since A11 and is not reported as a replacement. |
| (2) | new pairing in use; old key kept and listed | retried on every connection; shown on the Pair screen and the PC's card |
| (3), before (4) | PC revoked the old key; entry still listed | next attempt gets `revoked` → retired |
| (4), before (5) | key unreferenced | swept at the next start (already retired) |

There is no transaction across the two credentials: nothing on the phone can
make the PC's approval of the new key and its revocation of the old one
atomic. The boundary is therefore the state after (2), and it is visible and
tracked — never reported finished. Forget with a listed key asks once more,
then deletes the key as Forget always has; if the PC cannot be reached then,
the PC keeps trusting that record until it is revoked there (the dialog says
so and names the device). If an old key is missing from the keystore, the
entry stays listed with "this device no longer holds that key, so only the PC
can end its trust".

A second, genuinely different phone is never touched: retirement only ever
uses this phone's own old key, and the PC decides by certificate — never by
name, PC id or request id.

## 8. Files changed (A12)

Android: `connection/ConnectionManager.kt`, `data/Stores.kt`,
`ui/PairScreen.kt`, `ui/HomeScreen.kt`, `AndroidManifest.xml` (comment only);
tests `ConnectionManagerTest.kt`, `Fakes.kt`, `ManifestTest.kt` (KDoc only),
`androidTest/RemoteEndToEndTest.kt`, `androidTest/IncomingIntentsTest.kt`
(KDoc only); `tools/e2e.sh`; `README.md`, `SUPPORTED_OPERATIONS.md`.

Remote: `src/main.rs` (help text), `tests/remote.rs`, `README.md`.

Documentation corrections: the A10-era "a code pairs whoever uses it first"
(manifest comment, `ManifestTest`, `IncomingIntentsTest`) now says a code only
lets a device make a pending request and trust needs the PC's approval of that
certificate; `remote/README.md`'s `pairing.json` row no longer lists a "code"
in requests (it stores the challenge id; no code is stored); `lcl-remote deny`
help and README distinguish denying an unapproved request (the code stays
usable) from withdrawing an approval (the code then pairs nobody).

(The same working tree also holds the Core 0.2 closure; see
`LCL_CORE_0_2_INDEPENDENT_REVIEW_CLOSURE.md`. Of its files, only
`android/tools/e2e.sh`'s `core02` constant is in an A12 file.)

## 9. Tests A12-R1…R7

| ID | Test | What it proves |
|---|---|---|
| R1 | `ConnectionManagerTest.a12_r1_every_pairing_attempt_makes_a_key_with_a_certificate_of_its_own` | the test keystore makes distinct real keys (signatures verify only under their own certificate); two pairings give the PC two certificates |
| R2 | `a12_r2_pairing_again_while_trusted_retires_the_old_credential_before_deleting_its_key` | PC trusts exactly the new key; the old key itself unpaired; the old key was deleted only after the PC stopped trusting it; one key left |
| R3 | `a12_r3_forget_after_pairing_again_leaves_nothing_of_this_device_trusted` | after Forget: no key, no record, neither key trusted |
| R4 | `a12_r4_a_retirement_the_pc_cannot_confirm_keeps_the_old_key_and_says_so`; `a12_r4_an_unretired_key_survives_a_restart_and_is_retired_once_the_pc_is_reached`; `a12_r4_forget_retires_an_old_key_left_unretired` | a `500` on `unpair` or a lost connection: success returned *with* the old key listed and why, old key kept, PC still trusts it, the notice names device, reason and remedy; the next connection finishes it; a restart does not sweep the key; Forget retires it |
| R5 | `a12_r5_pairing_again_leaves_another_phone_trusted` | another phone (own store and keys, same name "Pixel") stays trusted and connected |
| R6 | `a12_r6_an_old_key_the_pc_already_revoked_or_forgot_is_retired_without_unpairing` | `revoked` and `not_paired` are retirement; nothing is unpaired |
| R7 | `RemoteEndToEndTest.p10_pair_again_while_trusted_retires_the_old_key`, `p11_forget_after_pairing_again`, `tools/e2e.sh` block 11 | real Keystore and real PC: old record revoked, new one the only trusted one, then zero trusted after Forget |

Also: `a_retiring_key_with_the_current_certificate_never_ends_the_pairing`,
`the_keys_a_record_is_retiring_survive_a_restart_of_the_store`, and the PC
test in section 2. The existing
`pairing_again_replaces_the_key_only_when_it_succeeds` now runs with distinct
keys and retires the first one through the real path.

R7 is placed after p9: at that point the phone's working pairing (from p8) has
never been revoked, so it is exactly the case p7 could not reach. No earlier
phase or check changed.

One existing E2E phase needed a wait, not a weaker check. p7 re-pairs after
p6 revoked the device and asserted exactly one key the moment the banner read
"Connected". With A12 the old key is deleted only after the PC confirms it is
no longer trusted, which lands just after the new session is live, so the first
final E2E (`e2e-final/`) failed there: `the old key was kept expected:<1> but
was:<2>`. The device showed the retirement had in fact completed (its record's
`retiring` was empty; the PC listed the old device revoked, the new one
trusted). p7 now first waits, up to 30 s, until the Pair screen has finished
and one key is left, then makes the same assertion. The second, complete run
(`e2e-final-2/`) passed.

## 10. Commands, exit codes, counts

Run one heavy job at a time (14 GiB RAM; see the A11 report): Cargo with
`CARGO_BUILD_JOBS=4`, Gradle `--offline --no-daemon --max-workers=2` with the
Kotlin compiler in process, the emulator only for the E2E runs and stopped
right after each. The impl gate is `/mnt/F/.lcl-pretest/a12/a12-gate.sh`, a
copy of the A11 gate whose section c is adapted for the Core 0.2 closure (see
the Core 0.2 report); logs `/mnt/F/.lcl-pretest/logs/A12-G-*`.

| Command | Exit | Result |
|---|---|---|
| `android`: `./gradlew testDebugUnitTest` (red, before the fix) | 1 | 74 run, 2 failed: A12-R2, A12-R3 (section 2) |
| `android`: `./gradlew testDebugUnitTest` (final) | 0 | **81 run, 0 failed, 0 skipped** (A11: 71; +10 A12) |
| `android`: `./gradlew lint` (final, run again after the last androidTest edit) | 0 | 0 errors, 1 warning: the pre-existing `OldTargetApi` (`targetSdk = 36`); nothing new |
| `android`: `./gradlew assembleDebug` | 0 | |
| `android`: `./gradlew assembleRelease` | 0 | `app-release-unsigned.apk`, unsigned as before, 24,534,501 bytes, SHA-256 `ef90307720774b2040402692d872e6041f2e274a709e41a4a68623b848026332`; git-ignored, not added |
| `remote`: `cargo fmt --check` | 0 | |
| `remote`: `cargo clippy --locked --all-targets -- -D warnings` | 0 | |
| `remote`: `cargo test --locked` | 0 | **63 passed, 0 failed** (27 unit + 36 integration; A11: 62) |
| gate `selftest` | 0 | a failing command fails the gate, whatever it prints |
| gate `a`: impl `cargo fmt --all -- --check`; `cargo clippy --offline --locked --workspace --all-targets -- -D warnings`; `cargo test --offline --locked --workspace --all-targets --no-fail-fast` | 0, 0, 0 | 167 blocks, **1,870 passed, 0 failed, 1 ignored** (A11: 1,869; +1 is the Core 0.2 forgery test) |
| gate `b`: Rust **1.75.0** `cargo check` and `cargo test --workspace --all-targets` | 0, 0 | **1,870 passed, 0 failed, 1 ignored**, identical to 1.98.1 |
| gate `c` (17 commands, both packages, conformance, readiness, protected) | all as expected | see the Core 0.2 report, section 10 |
| gate `d`: `real_process` 12 sequential + 3 × 6 concurrent | 0 × 30 | 360 passed, 0 failed |
| gate verdict | — | **52 commands, each at its expected status** |
| `remote`: `cargo build --locked` (final binary) | 0 | SHA-256 `fd05437bd3635d638f57b0dcd40d01f11a177c25a9e936864428ceecdebc7a5c`, newer than every `remote/` and `lcl-spec` source |
| `LCL_REMOTE=<that binary> android/tools/e2e.sh` (`e2e-final/`) | 1 | p7 failed on the check timing described above; 13 phases had passed |
| `LCL_REMOTE=<that binary> android/tools/e2e.sh` (`e2e-final-2/`) | 0 | **18 of 18 phases passed**, every PC check passed, no LCL ANR or crash; the binary's SHA-256 checked again after the run: OK |

Final E2E, in order: two local-document phases (A6); four
`IncomingIntentsTest` phases (A10); p1 pair (pending → the PC lists the phone's
own code → approve) and work, the code spent and never stored, the QR code
decoding to exactly the pairing text; a same-signed update; p2 restart, then
again after a phone reboot (About shows the PC engine's Core 0.1
`00d648b1…67ed` and Core 0.2 `061a79c9…8b3f`); p3 network loss; p4 PC service
restart; p5 PC edits and a save conflict; p6 revocation; p7 the spent code
refused, a new one approved, Forget; p8 the app's scanner; p9 an older link
refused and a denied request; **p10** paired again while trusted:

| `lcl-remote devices --json` | before p10 | after p10 | after p11 (Forget) |
|---|---|---|---|
| p1 device `e7c83b71…` | revoked (p6) | revoked | revoked |
| p7 device `ab5de7c7…` | revoked (Forget) | revoked | revoked |
| p8 device `e1a5d6c3…`, fingerprint `cb94b5e1…` | **trusted** | **revoked** | revoked |
| p10 device `66eb5c27…`, fingerprint `c6096aac…` | — | **trusted, the only one** | **revoked** |

The phone logged `REPLACED e1a5d6c3492ec44d 66eb5c273f8f2ed3` and kept one
key; the host checked that the two certificates differ, the old record is
revoked, the new one is the only trusted record, and after **p11** Forget that
no record is trusted; no pairing request is left waiting.

## 11. A1–A11 regression status

All pass unchanged in intent: A1 pause before every effect and `break_effects`
ignored (remote tests; E2E p1 run with approvals); A2 run ownership (remote
`only_the_device_that_started_a_run_can_follow_or_answer_it`); A3 scanner fills
the form only (E2E p8); A4 live unsharing (remote); A5 save compare-and-swap
(remote; E2E p5); A6 strict UTF-8 (`LocalTextTest`; E2E local-document
phases); A7 pre-authentication limits and slot return (remote); A9 key
protection reporting (E2E p1, p2); A10 no external pairing link
(`ManifestTest`, `IncomingIntentsTest` ×4 in the E2E); A11 PC approval (remote
A11-R1…R9 and `pairing.rs` unit tests; JVM approval tests; E2E p1, p7, p8,
p9). TLS 1.3, mutual certificates, PC pinning, device authentication,
revocation and project confinement are untouched; the A11 pairing state
machine is unchanged.

## 12. Physical phone, camera, network

- Physical phone: **NOT_RUN**. Physical camera: **NOT_RUN** (the scanner is
  tested by answering its camera activity). Separate LAN / Internet / mobile
  data: **NOT_RUN**; the emulator reaches the host at `10.0.2.2`.
- The emulator's key is a software keystore key (A9); the two certificates in
  the E2E are nevertheless genuinely different Android Keystore keys.

## 13. Remaining limitations

- The window before step (2) in section 7 (the PC approved the new key, the
  process died before saving it) leaves a new PC record whose key no longer
  exists; it can never authenticate, but it shows as trusted until revoked on
  the PC. It predates A12 and is not reported as a replacement.
- A retirement that cannot be confirmed is retried only when this phone
  connects to that PC (and once more on Forget); if the PC is never reached
  again, only revoking the listed device on the PC ends it. Forget without the
  PC deletes the keys as Forget always has.
- A phone whose old key vanished from the keystore cannot retire that record
  itself; the notice says so and names the device to revoke on the PC.

## 14. Final repository state

See the Core 0.2 report, section 17: one working tree holds both tasks.
HEAD `718a0892efb310130fca850b1b99023d394fba84` = `origin/main`, branch `main`,
30 modified files plus the two new reports, nothing staged.

## 15. Commit state

- Committed: NO
- Pushed: NO

## 16. Ready to commit

YES. The A12 files are listed in section 8.

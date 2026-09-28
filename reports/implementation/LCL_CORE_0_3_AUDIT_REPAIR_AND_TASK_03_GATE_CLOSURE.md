# LCL Core 0.3 — Audit Repair and Final Task-03 Gate Closure

Governing instruction: the owner's task "LCL CORE 0.3 — AUDIT REPAIR + FINAL
TASK-03 GATE CLOSURE", given in the session on 2026-09-28. The message as
received ends inside section 10.A. The final gates below follow sections 0–9
and the gate list of section 0 and Task 03.

Result: **IMPLEMENTATION_READY_FOR_INDEPENDENT_REVIEW**

- All five audit findings are repaired.
- The complete final gate passed: 67/67 commands at their expected status.
- The full emulator E2E passed from the beginning.
- The physical-phone smoke was not run: no physical device was attached.
- Core 0.3.0 `independent_review` stays **pending**, and
  `release_gate_permitted` stays **false**.
- No release candidate was built, and nothing is released.

## 1. Starting state

- Branch `main`. HEAD = `origin/main` = `614f4c1` ("LCL core 0.3 task 3
  part4"), and the working tree was clean. The task expected `35c1a6c`; the
  difference is the owner's own request in the same message ("Commit and
  Sync and then start this"). Two commits were made and pushed first:
  - `93af486`: `android/tools/e2e.sh`, the Task 03 E2E host-check fix;
  - `614f4c1`: the Task 03 report.

  Neither commit touches `canonical/`, `impl/`, `remote/` or the app sources.
- Identities before any change, computed independently
  (`/mnt/F/.lcl-pretest/p3/identity.py`; LCL-PACKAGE-IDENTITY-V1):

  | Package | Identity | Files |
  |---|---|---|
  | Core 0.1.0 | `00d648b162939d06c44838481a67c39bc12c64bdd6d105035c24150148fe67ed` | 176 |
  | Core 0.2.0 | `061a79c92c76ed7bb05968adde24b016cae3b30666b8fb289c6ab968907e8b3f` | 216 |
  | Core 0.3.0 (pre-repair) | `bf66e36902dbef480db75772494d384847220959d76088dbe102b8ebff04aa12` | 291 |

  All three equal the expected values.
- No git write was made after `614f4c1`: no commit, push, pull, reset, stash
  or branch.

## 2. C03-AUDIT-01 — Core 0.3 review provenance

**Problem.** In `00_RELEASE/05_LANGUAGE_CLOSURE.json`, the Task 01 relabel
turned "Core 0.2.0: … examined this package at identity `00daee8d…e604`
(216 files)" into "Core 0.3.0: … examined this package…". FINAL-05 never
examined Core 0.3.0.

**Repair.** The `independent_review` summary now has three parts:

- *Core 0.1.0 basis* — unchanged.
- *Inherited Core 0.2.0 basis* — FINAL-05 (`reports/tasks/FINAL-05_RESULT.md`)
  examined the Core 0.2.0 package, not this one, at `00daee8d…e604` (216
  files), localization included, and found no technical blocker. The owner
  accepted it on 2026-09-25. The accepted Core 0.2.0 identity is `061a79c9…8b3f`
  (216 files). That review did not examine Core 0.3.0.
- *Core 0.3.0 additions* — not yet given a final accepted independent review.
  The decision stays `pending`, and `release_gate_permitted` stays `false`.

Other changes and checks:

- A CHANGELOG entry records the candidate correction, as
  `00_RELEASE/04_CHANGE_CONTROL.txt` requires for candidate corrections.
- No normative text, registry, fixture or catalog changed. Core 0.3.0 was not
  marked complete, release-ready or released.
- Checked and found correct, so left alone: `00_RELEASE/00` §2, `00_RELEASE/01`,
  `README.txt`, `VERSION.txt`, and the CHANGELOG's historical 0.2.0 section.

**Integrity** (section 5): regenerated; new identity
`7c8d46931933fa28c212a84e6405ef4b3ad99cebf07831bb835deac96a701aff` (291 files).

## 3. C03-AUDIT-02 — closed qualified-identifier domains

### Canon

- `04_GRAMMAR/13` (the same sentence in 0.1.0, 0.2.0 and 0.3.0): "DOMAIN
  resolves through the exact source and JSON Pointer in
  qualified_identifier_domains and, when defined_kind is present, also admits
  an ID whose DEFINE.KIND is that exact registered kind."
- `field_signatures#/qualified_identifier_domains` declares, in every version:
  - *static domains* (no `defined_kind`): `definition_kind`, `document_kind`,
    `encoding`, `mode`, and `part_kind` (0.3.0);
  - *alias-capable domains* (with `defined_kind`): `error`, `event`, `format`,
    `status`, and `terminal_non_success_status` (`object_keys_where terminal`,
    excluding `status.succeeded`; "Resolve the alias first, then apply where
    and exclude to its canonical core status").
- The catalog requires enforcement: `ENUM-GROUPS-0754` … `0762` and `0814`,
  "Resolve and enforce the complete closed identifier contract".
- `07_VERSIONING_AND_EXTENSIONS/03`: alias IDs "remain ordinary user IDs and
  must obey reserved-prefix … rules". An alias `BASE` that resolves to the
  wrong domain or DEFINE KIND is `error.reference.kind`; an unresolved `BASE`
  is `error.reference.unresolved`.
- `built_in_groups#/reserved_namespaces`: core, encoding, error, event, format,
  kind, mode, status, unit.

### Rule implemented

The rule is built generically from each package's own domain metadata.

| Value in a `qualified_identifier(DOMAIN)` field | Result | Stage |
|---|---|---|
| a registered member (after `where`/`exclude`) | accepted | — |
| static domain, not a member | `error.field.type` | grammar_or_schema |
| alias-capable domain, first segment a reserved namespace, not a member (e.g. `format.not_registered`, `event.missing` as a FORMAT, `status.running` or `status.succeeded` as a FAILURE.STATUS) | `error.field.type` — it can never name a DEFINE | grammar_or_schema |
| alias-capable domain, user ID resolving to a DEFINE of the domain's kind | accepted (its BASE chain is checked where it was already checked) | — |
| … resolving to no declaration | `error.reference.unresolved` | resolution |
| … resolving to a declaration of another kind (e.g. a `kind.event` DEFINE as a FORMAT, a `DATA`/`INPUT` ID) | `error.reference.kind` | resolution |
| `terminal_non_success_status` alias whose canonical status fails the filter | `error.reference.kind` | resolution |

The last row is derived by analogy, and the reviewer should check it.
Canon states the filter and the alias-`BASE` error rule ("resolves outside its
own domain → `error.reference.kind`"). It names no separate error for a
use-site alias filtered out by `where`/`exclude`. The owner's instruction
maps "wrong domain" to `error.reference.kind`, and that is applied here. At
run time, the existing rule that an illegal requested FAILURE transition is
`error.execution.order` is unchanged.

### Implementation

- `lcl-parser/src/grammar.rs`:
  - `ENFORCED_DOMAINS` is replaced by `IdentifierDomain` (members after
    `where`/`exclude`, `defined_kind`, `filtered`), loaded for every declared
    domain through its exact registry, pointer and selection
    (`array_values`, `object_keys`, `object_keys_where`);
  - `reserved_namespaces` are loaded, with `identifier_domain()` and
    `is_reserved_namespace()`;
  - `closed_domain_members()` keeps its meaning (registered members).
- `lcl-parser/src/schema.rs`: the grammar-stage check covers every domain.
- `lcl-resolver/src/references.rs`: the binder checks `qualified_identifier(DOMAIN)`
  user IDs as it already checked `operation_identifier` values:
  `domain_value`, plus `canonical_alias` for the filter.

### Versions

The same contract text and domain registry exist in Core 0.1.0 and 0.2.0, so
the enforcement applies in every version. Task 01 did the same for
`SPECIFICATION.KIND` on the same grounds. No package byte of 0.1.0 or 0.2.0
changed.

### Tests (red first)

`lcl-protocol/tests/identifier_domains.rs` has 6 tests, each run for 0.1.0,
0.2.0 and 0.3.0.

- **Before the fix:** 4 failed and 2 passed
  (`/mnt/F/.lcl-pretest/c03/audit/RED-identifier_domains.log`). Every
  unregistered value was silently accepted; the two acceptance controls
  passed.
- **After the fix:** 6 passed.
- Coverage:
  - every registered mode, encoding, format and definition kind is accepted;
  - an unknown MODE, ENCODING, DEFINE.KIND or SPECIFICATION.KIND is
    `error.field.type` at its name;
  - a FORMAT alias, an ERROR alias, an EVENT alias and a two-step STATUS
    alias are accepted;
  - reserved-namespace unknowns are rejected;
  - unresolved aliases are rejected;
  - wrong-kind aliases (event-as-format, DATA-as-format, format-as-event) are
    rejected;
  - a status alias of `status.succeeded` in FAILURE.STATUS is rejected.
- Unknown PART KIND: already covered by `document_kinds.rs` (Task 01).

Existing tests and generators that relied on unregistered values were changed
to use legal values. No assertion was removed.

- **Conformance probe generator** (`lcl-conformance/src/source_cases.rs`):
  - minimum forms now fill a domain field with a registered member (it used
    `sample`);
  - the Task 01 "simple form is rejected" expectation now applies to static
    domains only, because a one-segment identifier can name a DEFINE in an
    alias-capable domain. The parser has always read a one-segment identifier
    as a qualified identifier of length one.
  - The production report remains 2413 required probes, all satisfied (§6).
- **`lcl-parser/tests/parse_matrix.rs`:** the minimum form of `DEFINE` used
  `KIND: x`; it now uses a registered kind.
- **`lcl-resolver/tests/references.rs`:** the fallback-cycle fixture used
  `EVENT: event.failure`, which is not a registered event; it now uses
  `event.execution_error`, and the cycle assertion is unchanged.
- **Users-manual examples:** 97 checked, 0 failures. No manual example used an
  unregistered value.

## 4. C03-AUDIT-03 — Android pairing survives the Manual tab

- `connection/PairingController.kt` (new) is app-scoped (`AppContainer.pairing`).
  It owns the one attempt and its state: Idle, Requesting, Pending(code),
  Finishing, Paired, RetirementIncomplete, Failed (failure or cancel).
- `ConnectionManager.pair` gains `onApproved`, called when the PC accepts
  (→ Finishing). Its rules are otherwise unchanged: PC approval, the
  verification code, pinning, the A12 retirement, the A13 refusal, and key
  deletion on failure or cancel.
- `PairScreen` renders the state. Only the person's Cancel, or Back while the
  attempt runs, cancels it. Opening the Manual tab or recreating the activity
  does not. The pasted text and device name are `rememberSaveable`. After
  recreation mid-attempt, the app reopens on the Pair screen.
- JVM tests (3 new):
  - pending survives the screen, and a second start is refused rather than
    doubled; the PC approving while away gives Paired with the same key;
  - explicit cancel deletes the key, trusts nothing and stops asking, and a
    later attempt works;
  - A13 through the controller still fails before any contact.
- The existing A12 and A13 tests are unchanged and pass.
- Emulator: new phase `p13_pairing_continues_while_the_manual_is_open`. The
  phone is pending, the Manual tab is shown, and the PC approves then; the
  pairing completes and Workspace shows Connected. PC-side check: exactly one
  trusted device.

## 5. C03-AUDIT-04 — New File preview bound to creation

- `POST /api/document` with `role` now requires `scaffold_digest`, the SHA-256
  that `GET /api/scaffold` returned for the same name.
  - The server recomputes the text and compares exactly.
  - Changed → 409; missing → 428. Nothing is created in either case.
  - The frontend never sends text back; the PC generates it.
  - A blank file is separate and needs no digest.
- The desktop dialog keeps the digest of the preview on screen and sends it.
- The remote `create` op forwards `scaffold_digest`.
- Android asks the PC for the scaffold of the exact final name (`scaffold`
  op), shows its text in the New dialog (Create is disabled until then), and
  sends its digest.
- Tests:
  - `lcl-workspace/tests/authoring.rs`, new
    `a_role_file_is_created_only_from_the_scaffold_that_was_previewed`:
    canonical preview → created with the previewed bytes; missing → 428;
    renamed file → 409; Master preview → created; Master edited in between →
    409; role default changed in between → 409; blank file needs no digest.
  - remote `core03`: 428 without a digest, created with it.
  - Android JVM: the preview is the PC's text for `house.lcl` and the digest
    is sent; a PC template change after the preview → not created; no digest
    → refused.
  - The browser smoke and E2E p12 both go through preview → Create.

## 6. C03-AUDIT-05 — documentation drift

- `README.md`: Core 0.3.0 row with the post-repair identity — an unreleased
  project-feature candidate, review pending, `release_gate_permitted` false,
  no archive.
- `android/README.md`: "no WebView anywhere" was false. It now says the one
  WebView is the offline Manual: packaged content only, no network or remote
  content, navigation refused, no LCL parsing or running.
- `impl/README.md`: the workspace row names `--project-spec`/Core 0.3.0.
- `remote/README.md`: `--project-spec`/`LCL_PROJECT_SPEC`, and the
  `scaffold_digest` binding.
- `android/SUPPORTED_OPERATIONS.md`: preview-bound creation, and pairing that
  outlives its screen.
- `packaging/lcl-workspace-launch.in`: the header names `@PROJECT_SPEC@`.
- `reports/implementation/LCL_CORE_0_3_TASK_01_GATE_CLOSURE_ADDENDUM.md` (new).
  The Task 01 gate was **incomplete** when Task 01 was committed: 30 of its
  commands were recorded, all as expected, and it stopped after `msrv-check`,
  three minutes before `865bef9`. Complete gates are recorded only for later
  trees. The original report is not rewritten.

## 7. Core 0.3.0 integrity

Regenerated with the Task 01 script (`/mnt/F/.lcl-pretest/c03/t1/integrity.sh
audit01`), after every intended package byte was frozen, in the documented
acyclic order.

Order and results:

- `generate_integrity.py manifest`;
- per-scope `validate_release`: filesystem, text, structured, grammar,
  registry, catalog — all exit 0;
- `validate_language_contracts`, `validate_localization`,
  `validate_source_fixtures`, `validate_projects`, `validate_ebnf` — all
  exit 0;
- the composed `VALIDATION_REPORT.txt`;
- `generate_integrity.py checksum`;
- `validate_release --scope all` — exit 1 by design: 32 PASS, 0 FAIL,
  1 BLOCKED (`language_decisions_and_release_state`, pending only
  `independent_review`, gate not permitted), 2 OUT_OF_SCOPE;
- `sha256sum -c` — 0.

| | Identity | Files |
|---|---|---|
| old | `bf66e36902dbef480db75772494d384847220959d76088dbe102b8ebff04aa12` | 291 |
| new | `7c8d46931933fa28c212a84e6405ef4b3ad99cebf07831bb835deac96a701aff` | 291 |

- Two independent implementations agree on the new identity. The new
  `MANIFEST.json` SHA-256 is `64b9071d…128a`.
- `APPROVED_PACKAGE_0_3_0` was updated only after the bytes were frozen. The
  0.1.0 and 0.2.0 anchors are unchanged.
- Anchor test (`lcl-spec/tests/trust_anchor.rs`, new
  `the_corrected_0_3_0_package_alone_opens_under_its_anchor`):
  - the corrected package opens under the new anchor;
  - the retired identity's anchor refuses it;
  - a self-consistent forgery that restores the old claim (package records
    rewritten, internally verified) is refused by the anchor.
- Historical reports keep the old identity where it was true.

## 8. Final gates

### Setup

- Runner: `/mnt/F/.lcl-pretest/c03/audit/c03ar-run.sh`. It is the Task 03
  gate (`c03t3-gate.sh`) relabelled `C03AR` so that the Task 03 evidence stays
  intact. The phases are selftest, a, c, b, d, e and verdict.
- Logs: `/mnt/F/.lcl-pretest/logs/C03AR-G-*.log`. The run log ends
  `GATE-DONE`.
- Results: `/mnt/F/.lcl-pretest/c03/audit/c03ar-results.tsv`.
- The gate was inspected before running. One step was adjusted: `protected`
  used to require that nothing under `canonical/` differ from HEAD. It now
  requires that Core 0.1.0, Core 0.2.0, `releases/` and `assets/` do not
  differ, and that Core 0.3.0 differs in exactly the five intended files
  (`05_LANGUAGE_CLOSURE.json`, `CHANGELOG.txt`, `MANIFEST.json`,
  `SHA256SUMS.txt`, `VALIDATION_REPORT.txt`).
- The gate ran once, complete and from the beginning, on `HEAD 614f4c1` plus
  this task's working tree. The source fingerprint, excluding `reports/`, was
  the same before and after the run (`db9cdedc…58f8`), so nothing changed
  under it.

**67 commands; every exit status equals the expected one.**

| Step | Result |
|---|---|
| selftest | the runner detects failing commands (phase exit 0) |
| fmt, clippy `--workspace --all-targets -D warnings` | 0, 0 |
| test-workspace | 176 binaries, **1945 passed, 0 failed, 1 ignored** (pre-existing ignore; Task 03 had 1937 — +8: identifier domains 6, scaffold binding 1, anchor 1) |
| sha 0.1.0/0.2.0/0.3.0; validate_release 0.1.0 | 0 |
| validate_release 0.2.0 / release-state | 1 / 0 (blocked only on release metadata, as before) |
| validate_release 0.3.0 / release-state | 1 / 0: 32 PASS, 0 FAIL, 1 BLOCKED (`independent_review` only), 2 OOS |
| language contracts, localization, source fixtures (0.2.0, 0.3.0), `validate_projects` 0.3.0, EBNF ×3 | 0 |
| identity 0.1.0 / 0.2.0 / 0.3.0 | `00d648b1…` 176 / `061a79c9…` 216 / `7c8d4693…` 291 |
| anchors | the compiled anchors equal the recomputed identities |
| brand | 0 |
| conformance-text | 2413 required probes (2011 source + 402 semantics), all satisfied; `CLAIM: semantics_conforming` |
| readiness-gate | 0 |
| protected | 0 (as adjusted above) |
| msrv-check, msrv-tests (Rust 1.75.0) | 0; 176 binaries, **1945 passed, 0 failed, 1 ignored** |
| real_process 12 sequential + 3 × 6 parallel | 0 each (30 runs) |
| remote fmt, clippy `-D warnings`, tests | 0, 0, **64 passed, 0 failed** |
| manual-manifest | current (23 files, `fa189d30…4dd3`) |
| manual-examples | 97 checked, 42 fragments, 0 failures |
| browser-smoke (headless Firefox, WebDriver BiDi) | **27 passed, 0 failed**; it now names the file before its role preview, as preview-bound creation requires |

Expected non-zero statuses are the same as in Tasks 01–03: the selftest's two
deliberate failures, and the BLOCKED `validate_release` of 0.2.0 and 0.3.0,
each asserted by its release-state step.

### Android, on the final tree

Log: `/mnt/F/.lcl-pretest/logs/C03AR-G-android.log`, built with `--rerun-tasks`.

| Check | Result |
|---|---|
| JVM unit tests | **94 passed, 0 failed** (Task 03: 90; +3 pairing lifecycle, +1 scaffold binding) |
| lint | 0 errors, 1 warning (the pre-existing OldTargetApi) |
| assembleDebug | OK, 31,909,646 bytes |
| assembleRelease | OK, unsigned, 24,735,169 bytes |

## 9. Emulator E2E and physical phone

### Emulator E2E

This ran on the emulator (AVD `lcl36`, API 36 x86_64, `ro.kernel.qemu=1`),
not a phone.

- Command: `android/tools/e2e.sh`, run in full from the beginning on the final
  tree, with the `lcl-remote` built from it.
- Evidence: `/mnt/F/.lcl-pretest/c03/audit/e2e/` and `e2e-run.log`.
- Result: **all 21 phase runs PASS** (20 phases; p2 runs twice), **31 PC-side
  checks**, exit 0.

Phases:

- ManualTabTest, the two local-document phases and the four incoming-intent
  phases;
- p1–p11 (pairing, work, restart, network loss, PC restart, conflicts,
  revocation, re-pair and forget, scanner, denied pairing, A12 retirement);
- p12: a Rules file by role, created only from the PC's preview; unsaved text
  survives Workspace → Manual → Workspace; readiness is incomplete; A13 is
  refused with nothing changed on the PC;
- p13 (new, C03-AUDIT-03): pending → Manual tab → the PC approves → paired →
  Connected; exactly one trusted device.

### Physical phone — NOT RUN

No physical device was attached. `adb devices` listed only `emulator-5554`
while the emulator ran, and nothing before or after. The physical-phone smoke
therefore was not run, and no physical-phone, camera, LAN or mobile-data
coverage is claimed.

## 10. Out-of-scope findings (reported, not changed)

- `05_LANGUAGE_CLOSURE.json` `normative_alignment` has the same relabel
  artifact: "Core 0.3.0 adds 02_LEXICAL/13, the localization surface … the
  meaning of LCL Core 0.1.0 is unchanged". That sentence was written for Core
  0.2.0. The task limited the edit to `independent_review`, so this is left
  for the reviewer.
- As §6 says, the Task 01 gate did not complete before `865bef9`.

## 11. Status

| Item | Status |
|---|---|
| C03-AUDIT-01 | repaired; Core 0.3.0 identity `7c8d4693…aff`, anchor updated and tested |
| C03-AUDIT-02 | repaired generically, every version; red-first tests; conformance, manual and full suites green |
| C03-AUDIT-03 | repaired; JVM ×3 and emulator p13 |
| C03-AUDIT-04 | repaired on desktop, remote and Android; binding tests at each layer |
| C03-AUDIT-05 | repaired; Task 01 addendum added, history not rewritten |
| Core 0.3.0 language | `CORE_0_3_CANDIDATE`; `independent_review` **pending**; `release_gate_permitted` **false**; not `BARE_SPECIFICATION_COMPLETE` |
| Core 0.1.0, Core 0.2.0 | byte-identical; identities unchanged |
| Implementation / desktop / remote | final gate 67/67 |
| Android | JVM 94/0, lint clean, debug + release built, emulator E2E 21/21 phase runs + 31 PC checks; physical phone not run (none attached) |
| Manual parity | unchanged mechanism; manifest current; the APK digest check and ManualTabTest pass in the E2E |
| A13 / A14 | still closed; A13 re-checked through the new pairing controller |
| Release candidate | none built |
| Release-ready | **no** |
| Released | **no** |

## 12. Final Git state and commit readiness

- `main` at `614f4c1`, equal to `origin/main` when this task started. No git
  write was made during the task.
- Everything below is uncommitted, for the owner's review and the fresh
  independent audit. No release candidate was built. `releases/` and Core
  0.1.0/0.2.0 are untouched.
  - **Canon (Core 0.3.0 only):** `00_RELEASE/05_LANGUAGE_CLOSURE.json`,
    `CHANGELOG.txt`, `MANIFEST.json`, `SHA256SUMS.txt`, `VALIDATION_REPORT.txt`.
  - **Implementation:** `lcl-parser` (`grammar.rs`, `schema.rs`,
    `tests/parse_matrix.rs`); `lcl-resolver` (`references.rs`,
    `tests/references.rs`); `lcl-spec` (`anchor.rs`, `tests/trust_anchor.rs`);
    `lcl-conformance/src/source_cases.rs`;
    `lcl-protocol/tests/identifier_domains.rs` (new); `lcl-workspace`
    (`routes.rs`, `assets/app.js`, `tests/authoring.rs`).
  - **Remote:** `src/session.rs`, `tests/remote.rs`, `README.md`.
  - **Android:** `connection/PairingController.kt` (new),
    `ConnectionManager.kt`, `LclApplication.kt`, `ui/PairScreen.kt`,
    `ui/Root.kt`, `ui/WorkspaceScreen.kt`, `workspace/WorkspaceController.kt`,
    the JVM tests (`ConnectionManagerTest.kt`, `WorkspaceControllerTest.kt`),
    `RemoteEndToEndTest.kt`, `tools/e2e.sh`, `README.md`,
    `SUPPORTED_OPERATIONS.md`.
  - **Documentation:** `README.md`, `impl/README.md`,
    `packaging/lcl-workspace-launch.in`.
  - **Reports:** this report and
    `LCL_CORE_0_3_TASK_01_GATE_CLOSURE_ADDENDUM.md` (both new).
- Ready to commit: **yes**, when the owner asks. The owner's workflow suggests
  splitting it into a canon commit (the five Core 0.3.0 files), a source and
  tests commit, and a reports commit.

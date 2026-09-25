# LCL Core 0.2.0 — independent-review closure

Date: 2026-09-25. Result: the `independent_review` decision of Core 0.2.0 is
**closed** by the owner's acceptance of an independent technical review that
already existed. Every language decision is closed and the final release gate
is permitted. Core 0.2.0 is **not released**: it stays an unreleased
candidate, and the release mechanics that remain are listed in section 14.

Nothing in this report was committed or pushed.

## 1. Owner decision

Recorded as given in the task, verbatim:

> The owner accepts the independent technical review of LCL Core 0.2.0
> recorded in `reports/tasks/FINAL-05_RESULT.md`.

and the acceptance to record:

> The owner accepts the FINAL-05 independent technical review for the Core
> 0.2 package identity
> `00daee8de1919c4945ef04ff65edb22164bd8046a493be08a87d5fa3b4c3e604`. FINAL-05
> found no technical blocker in the Core 0.2 localization additions. The
> `independent_review` decision is therefore closed.

This is the owner's acceptance of a review completed on 2026-09-21. No new
review was made or claimed here.

## 2. FINAL-05 evidence

`reports/tasks/FINAL-05_RESULT.md` (committed in `878187d`, 2026-09-21):

- title: "FINAL-05 result — independent exhaustive TESTING_READY audit";
- identity line: "Core 0.2 `00daee8de1919c4945ef04ff65edb22164bd8046a493be08a87d5fa3b4c3e604` (216 files)";
- section "B3 / F29 — Core 0.2 independent review — **no technical blocker;
  the decision remains the owner's**": "This audit is the independent
  technical review, and it finds **no technical blocker** in Core 0.2" —
  checksums, the four 0.2 validators, both EBNF grammars, the identity equal
  to the anchor, the manifest hash, and 33 localized verdicts on the installed
  candidate; "What remains is a *decision record*, not a defect … That is the
  owner's to close";
- its every-file ledger `reports/tasks/FINAL-05_FILE_LEDGER.tsv` has a row for
  each of the 216 Core 0.2 files: 213 `reviewed-pass`, 3 `reviewed-finding`,
  0 defects. The three findings are `02_LEXICAL/01` and `02_LEXICAL/13` (the
  closed F10 decision stated normatively, consistent) and
  `00_RELEASE/05_LANGUAGE_CLOSURE.json` (F29: the pending
  `independent_review` — the record closed here). The localization additions
  (`02_LEXICAL/13`, `localization_surface_v0.2.0.json`,
  `locale_profile_schema_v0.2.0.json`, `validate_localization.py`, 36
  localization fixture files) are among the reviewed rows.

## 3. Reviewed pre-acceptance identity

`00daee8de1919c4945ef04ff65edb22164bd8046a493be08a87d5fa3b4c3e604`, 216
files, `MANIFEST.json` SHA-256 `c1ab983de2566a11…6e67`.

## 4. The bytes before this change were the reviewed bytes

Recorded before any canonical file was touched
(`/mnt/F/.lcl-pretest/a12/b-precheck.txt`, HEAD `718a089`):

| Check | Result |
|---|---|
| identity, independent shell computation (byte-order sort, `sha256sum`) | `00daee8d…e604`, 216 files |
| identity, the established `identity.py` | `00daee8d…e604` 216 |
| commits touching `canonical/LCL_Core_0.2.0` since FINAL-05's audited HEAD `9ddaadb` | 0 (last change: `4dead55`, FINAL-03) |
| worktree changes under `canonical/` | 0 |
| `MANIFEST.json` SHA-256 | `c1ab983d…6e67` (the reviewed one) |
| `sha256sum -c --strict` 0.2.0 / 0.1.0 | OK / OK |

The identity digest covers every byte of every file, so equality proves that
no language-definition byte had changed since the review.

## 5. Files changed to record the acceptance

In `canonical/LCL_Core_0.2.0/` — release metadata:

| File | Change |
|---|---|
| `00_RELEASE/05_LANGUAGE_CLOSURE.json` | `independent_review` `pending` → `closed`, its summary now cites FINAL-05, the reviewed identity and the owner's acceptance; `release_gate_permitted` `false` → `true`. Minimal textual edit (three lines), not re-serialized. |
| `00_RELEASE/01_RELEASE_STATUS_AND_BOUNDARY.txt` | status section: every decision closed and how; `LANGUAGE_STATUS: BARE_SPECIFICATION_COMPLETE` added; `PACKAGE_STATUS: UNRELEASED_CANDIDATE` kept; states that the package is not released |
| `00_RELEASE/00_CANONICAL_SOURCE_AND_PROVENANCE.txt` | §4: the review is accepted, not pending |
| `README.txt`, `VERSION.txt` | the language-definition status lines; README's "no independent review yet" bullet |
| `CHANGELOG.txt` | a new top entry; the earlier entries, which say "pending" as of their time, are unchanged |

and the integrity files the package's own tool regenerated: `MANIFEST.json`,
`VALIDATION_REPORT.txt`, `SHA256SUMS.txt`.

Outside the package: `impl/crates/lcl-spec/src/anchor.rs` (trust anchor),
`impl/crates/lcl-spec/tests/trust_anchor.rs` (new 0.2.0 forgery test),
`README.md` (package table row), `impl/README.md` and `impl/Cargo.toml` (one
sentence each that said the review is pending), `android/tools/e2e.sh` (the
Core 0.2 identity the E2E expects the PC's engine to report).

Not changed: `canonical/LCL_Core_0.1.0/`, `releases/`, `assets/`, every
historical report and candidate provenance that names `00daee8d…`.

**`LANGUAGE_STATUS`.** The old status text tied both consequences to the
pending review: "The independent_review decision … is pending …, *so* this
package declares no active language completion status *and* does not permit
the final release gate." With the review closed, the task requires the gate
to be permitted; the package's own criteria (`00_RELEASE/03`) define
`BARE_SPECIFICATION_COMPLETE` as a classification of the language-definition
bytes that "does not by itself mean packaged or release-ready", and Core
0.1.0 declared it while still a candidate once its decisions closed
(LCL-TASK-0006), before its separate release task. The criteria were freshly
established (section 10). The manifest was generated with the generator's
matching status, `bare_specification_complete_candidate` (release-ready
`false`). This is **not** a release status: `PACKAGE_STATUS` stays
`UNRELEASED_CANDIDATE`. If the owner prefers the language status left
undeclared, removing that one line and regenerating is the whole change.

## 6. No language or localization semantics changed

Every file compared with the reviewed bytes at `718a089`
(`b-bytes-vs-reviewed.txt`): 216 files, **207 byte-identical**, 9 changed —
exactly the 6 metadata and 3 integrity files above. Every file under
`01_FOUNDATION` (5), `02_LEXICAL` (13), `03_TYPES_AND_VALUES` (10),
`04_GRAMMAR` (13), `05_SEMANTICS` (12), `06_STANDARD_LIBRARY` (10),
`07_VERSIONING_AND_EXTENSIONS` (6), `08_EXAMPLES` (55), `09_CONFORMANCE` (61:
cases, fixtures, localization fixtures, tools), `10_REGISTRIES` (14),
`11_RESEARCH_BASIS` (4), `INDEX.txt` and `00_RELEASE/02`–`04` is
byte-identical. The regenerated manifest changed exactly those six records
and three header fields (`generated_utc`, `package`, `status`); its component
counts are unchanged.

## 7. New post-acceptance identity

`061a79c92c76ed7bb05968adde24b016cae3b30666b8fb289c6ab968907e8b3f`, 216
files (both computations agree; `b-identity-post.txt`). `MANIFEST.json`
`9f3a2eb989fdc758e3ca54cb70552af0f3e8b7a4bab988c222a1e9b219b76c73`,
`VALIDATION_REPORT.txt` `fd80b5b5bfb8bc16…`, `SHA256SUMS.txt`
`f6af38fd9021878a…`.

The identity changed because it covers every file, the closure record and the
integrity files included; the reviewed identity is `00daee8d…e604`. The
reviewed language is byte for byte the one accepted (section 6).

## 8. Integrity regeneration sequence

In the package's documented acyclic order (`00_RELEASE/03`, `04`), with its
own tool `09_CONFORMANCE/TOOLS/generate_integrity.py`; no hash was written by
hand:

1. Stable release metadata frozen (section 5); checked: only the six
   permitted files differ from HEAD, each strict UTF-8, LF-only, final LF, no
   control characters.
2. Gates 1–8, fresh, on the frozen payload: `validate_release.py --scope`
   filesystem, text, structured, grammar, registry, catalog → 29 PASS, 0
   FAIL, 0 BLOCKED, 2 OUT_OF_SCOPE (identical to the reviewed snapshot);
   `validate_language_contracts.py` 514 checks, 0 violations, 66 witnesses;
   `validate_localization.py` 141 reserved words, 4 profiles, 21 mutations,
   31 sources, 0 failed; `validate_source_fixtures.py` 15/15;
   `validate_ebnf.py` start `DOCUMENT`, 65 productions, 211 terminals.
3. `generate_integrity.py manifest --generated-utc 2026-09-25T17:42:29Z
   --status bare_specification_complete_candidate` → `MANIFEST.json`, 213
   records of 216 files, release-ready `false`, SHA-256 `9f3a2eb9…6c73`.
4. `VALIDATION_REPORT.txt` bound to that manifest hash: snapshot date and UTC,
   the language-definition status, the closure paragraph (9 of 9 decisions,
   how the last closed, gate permitted, not released), and the Gate 9 pointer
   to this report. The listed Gate 1–8 results are the fresh ones of step 2.
5. `generate_integrity.py checksum` → `SHA256SUMS.txt`, 215 records.
6. `sha256sum -c --strict --quiet SHA256SUMS.txt` → 0; `validate_release.py
   --scope all` (Gate 9 and release) → section 10.
7. The identity recomputed twice (section 7); only then was the anchor changed.

## 9. Trust anchor

`impl/crates/lcl-spec/src/anchor.rs`, set only after the bytes and integrity
files were frozen and the identity recomputed twice:

| | Before | After |
|---|---|---|
| `APPROVED_PACKAGE_0_2_0.identity_digest` | `00daee8d…e604` | **`061a79c92c76ed7bb05968adde24b016cae3b30666b8fb289c6ab968907e8b3f`** |
| `package_file_count` | 216 | 216 |
| `label` | "LCL Core 0.2.0 Localization Feature Candidate (2026-09-15)" | "LCL Core 0.2.0 Bare-Specification-Complete Candidate (2026-09-25)" |
| doc comment | "its independent review is pending and no archive exists" | the identity chain (`e86121c7…`, `6e730315…`, the reviewed `00daee8d…e604`), the owner's acceptance, the new manifest hash; "still an unreleased candidate: no release status is recorded and no archive exists" |
| `APPROVED_PACKAGE` (Core 0.1.0) | `00d648b1…67ed`, 176 | **unchanged** |

Evidence:

- `lcl-spec` `tests/package_0_2_0.rs`: the canonical 0.2.0 package opens under
  the new anchor, authoritative, identity equal to it; the default `open`
  still refuses 0.2.0; the 0.2.0 anchor refuses 0.1.0 — 4/4.
- New `tests/trust_anchor.rs::self_consistent_0_2_0_forgeries_are_rejected`:
  on throwaway copies, a reserved word renamed in
  `localization_surface_v0.2.0.json`, and the reviewed identity in the closure
  record changed by one digit, each with its manifest and checksum records
  regenerated so the copy verifies against itself — both are refused with
  `TrustAnchorMismatch`, internally consistent. The Core 0.1.0 forgery tests
  are unchanged and pass.
- Gate `a`/`b`: every engine test that opens Core 0.2 through the anchor
  (lexer, localization, parser, protocol dispatch/locale/localized/v07,
  diagnostics) passes on 1.98.1 and 1.75.0.
- Gate `c` check `anchors`: each compiled anchor equals its package's
  recomputed identity (`176 00d648b1…`, `216 061a79c9…`).
- The Android E2E (p2, before and after a phone reboot): the PC's engine
  reports Core 0.2 `0.2.0 · authoritative` with identity `061a79c9…8b3f`.

## 10. Validator results (final tree)

| Command | Exit | Result |
|---|---:|---|
| Core 0.2 `sha256sum -c --strict --quiet SHA256SUMS.txt` | 0 | 215 records |
| `validate_language_contracts.py --root canonical/LCL_Core_0.2.0` | 0 | 514 checks, 0 violations, 66 witnesses, 0 executed cases |
| `validate_localization.py --root …` | 0 | surface derivation (141 reserved words), profile schema, invalid-UTF-8 rejection pass; 4 profiles, 21 mutations, 31 sources, 0 failed |
| `validate_source_fixtures.py --root …` | 0 | 15/15 |
| `validate_ebnf.py canonical/LCL_Core_0.2.0/04_GRAMMAR/10_COMPLETE_EBNF.ebnf` | 0 | start `DOCUMENT`, 65 productions, 211 terminals |
| `validate_release.py --root canonical/LCL_Core_0.2.0 --scope all` | **1** | **31 PASS, 0 FAIL, 2 OUT_OF_SCOPE, 1 BLOCKED**; integrity: manifest `9f3a2eb9…` (213) PASS, checksums `f6af38fd…` (215) PASS |
| gate check `release-state-0.2.0` | 0 | the only non-PASS check is `language_decisions_and_release_state`; `pending_decisions: []`, `release_gate_permitted: true`, 9 decisions; its reasons are exactly the two below |
| Core 0.1 `sha256sum -c`, `validate_release.py --scope all`, `validate_ebnf.py` | 0, 0, 0 | 31 PASS, 0 FAIL, 0 BLOCKED, 2 OUT_OF_SCOPE, `release_ready: true` |
| brand `sha256sum -c assets/brand/BRAND_ASSETS.sha256` | 0 | 17 files |
| `m8_conformance_report` | 0 | 2,413 probes; source 2,011/2,011, semantics 402/402; `CLAIM: semantics_conforming` |
| `m8_conformance_gate` (readiness) | 0 | `ACCEPTED: claim semantics_conforming` (mapping digest `c592f8d9…`, Core 0.1 identity) |
| gate `protected` | 0 | Core 0.1, `releases/`, `assets/` unchanged; Core 0.2 changes are exactly the nine files of section 5; every historical candidate's checksums verify |

The one BLOCKED check's reasons, verbatim:

```text
Current status metadata still identifies an unreleased candidate.
Manifest release metadata is not in its final bare-language release state.
```

Neither is `independent_review` or any other decision; both are the release
mechanics of section 14. Before this change the same check carried two more
reasons ("Language decisions remain pending." and "The language decision
ledger has not permitted the final release gate.") with
`pending_decisions: ["independent_review"]`.

## 11. `pending_decisions`

`[]` — all nine decision areas `closed`.

## 12. `release_gate_permitted`

`true`.

## 13. Core 0.2 status, stated separately

| | |
|---|---|
| bare-language closure | **complete**: 9 of 9 decisions closed; `LANGUAGE_STATUS: BARE_SPECIFICATION_COMPLETE` |
| release gate | **permitted** (`release_gate_permitted: true`); the gate itself does not pass yet — it is BLOCKED only on the release mechanics |
| package / candidate status | `PACKAGE_STATUS: UNRELEASED_CANDIDATE`; manifest `bare_specification_complete_candidate`; the anchor pins `061a79c9…` |
| release-ready | **NO** (`release_ready: false`) |
| released | **NO** — no release status recorded, no archive |

## 14. Remaining release mechanics

Not blockers of the independent review, and not done here because releasing
Core 0.2 is a separate decision the owner has not made. In the package's own
procedure (`00_RELEASE/03`, `04`; Core 0.1.0's LCL-TASK-0007 is the
precedent):

1. From a committed revision, record the release status: `PACKAGE_STATUS:
   BARE_LANGUAGE_RELEASE` in `00_RELEASE/01` and the matching release
   metadata.
2. Regenerate `MANIFEST.json` with `--status bare_language_release`
   (release-ready `true`), then `VALIDATION_REPORT.txt`, then
   `SHA256SUMS.txt`.
3. Run `validate_release.py --scope all`: `language_decisions_and_release_state`
   must then PASS and `release_ready` be `true`.
4. Bind a new archive (a new name, e.g. `LCL_Core_0.2.0_Bare_Language_<date>.zip`,
   never reusing an old one) to those exact bytes; verify, extract, compare
   byte for byte and revalidate the extracted copy.
5. That changes the identity again: re-pin `APPROVED_PACKAGE_0_2_0`, the
   `README.md` row and `android/tools/e2e.sh`'s `core02`, and rerun the gates.
6. Build a new product candidate with `packaging/build_release.sh` from the
   committed revision. The existing `releases/candidates/lcl-0.2.0-…`
   candidates bundle Core 0.2 `00daee8d…`; they stay valid evidence for their
   own revisions and were not touched.

## 15. Core 0.1

Byte-identical to HEAD (`git diff --quiet HEAD -- canonical/LCL_Core_0.1.0`);
identity `00d648b162939d06c44838481a67c39bc12c64bdd6d105035c24150148fe67ed`,
176 files; checksums verify; `validate_release.py --scope all` exit 0, 31
PASS, 0 FAIL, 0 BLOCKED, 2 OUT_OF_SCOPE, `release_ready: true`; its anchor is
unchanged and its forgery tests pass.

## 16. Gates

The full established gate ran on the final tree (logs
`/mnt/F/.lcl-pretest/logs/A12-G-*`, results `a12-results.tsv`): **52 commands,
each at its expected status**. The gate script
`/mnt/F/.lcl-pretest/a12/a12-gate.sh` is a copy of the A11 gate with three
section-c changes. One is relaxed, only as far as this task requires; two are
added checks:

- `protected` no longer requires all of `canonical/` unchanged, because this
  task changes Core 0.2. It still requires Core 0.1, `releases/` and
  `assets/` unchanged, and now allows Core 0.2 to differ in exactly the nine
  files of section 5 and nowhere else;
- new `release-state-0.2.0` fails if any decision is pending, the gate is not
  permitted, any check FAILs, or any reason other than the two release
  mechanics is left;
- new `anchors` fails unless each compiled anchor equals its package's
  recomputed identity.

`validate-0.2.0` keeps its expected exit 1. Results: fmt, clippy 0; 1,870
passed / 0 failed / 1 ignored on Rust 1.98.1 and on MSRV 1.75.0; `real_process`
30/30 (360 tests); section c as in section 10. Remote 63/0, Android 81/0 and
the 18-phase E2E are in the A12 report.

## 17. Repository state

After every check had finished and every process of this task had ended
(`git status --short --branch --untracked-files=all`):

```text
## main...origin/main
 M README.md
 M android/README.md
 M android/SUPPORTED_OPERATIONS.md
 M android/app/src/androidTest/java/io/lcl/workspace/IncomingIntentsTest.kt
 M android/app/src/androidTest/java/io/lcl/workspace/RemoteEndToEndTest.kt
 M android/app/src/main/AndroidManifest.xml
 M android/app/src/main/java/io/lcl/workspace/connection/ConnectionManager.kt
 M android/app/src/main/java/io/lcl/workspace/data/Stores.kt
 M android/app/src/main/java/io/lcl/workspace/ui/HomeScreen.kt
 M android/app/src/main/java/io/lcl/workspace/ui/PairScreen.kt
 M android/app/src/test/java/io/lcl/workspace/ConnectionManagerTest.kt
 M android/app/src/test/java/io/lcl/workspace/Fakes.kt
 M android/app/src/test/java/io/lcl/workspace/ManifestTest.kt
 M android/tools/e2e.sh
 M canonical/LCL_Core_0.2.0/00_RELEASE/00_CANONICAL_SOURCE_AND_PROVENANCE.txt
 M canonical/LCL_Core_0.2.0/00_RELEASE/01_RELEASE_STATUS_AND_BOUNDARY.txt
 M canonical/LCL_Core_0.2.0/00_RELEASE/05_LANGUAGE_CLOSURE.json
 M canonical/LCL_Core_0.2.0/CHANGELOG.txt
 M canonical/LCL_Core_0.2.0/MANIFEST.json
 M canonical/LCL_Core_0.2.0/README.txt
 M canonical/LCL_Core_0.2.0/SHA256SUMS.txt
 M canonical/LCL_Core_0.2.0/VALIDATION_REPORT.txt
 M canonical/LCL_Core_0.2.0/VERSION.txt
 M impl/Cargo.toml
 M impl/README.md
 M impl/crates/lcl-spec/src/anchor.rs
 M impl/crates/lcl-spec/tests/trust_anchor.rs
 M remote/README.md
 M remote/src/main.rs
 M remote/tests/remote.rs
?? reports/implementation/LCL_ANDROID_A12_REPAIR_REPLACEMENT.md
?? reports/implementation/LCL_CORE_0_2_INDEPENDENT_REVIEW_CLOSURE.md
```

HEAD `718a0892efb310130fca850b1b99023d394fba84`, equal to `origin/main`;
branch `main`; nothing staged; no stash, no new branch. The secret and
artifact scan found no APK, AAB, keystore, key, certificate, pairing code,
`local.properties`, build or target directory, cache, log or temporary file
tracked or untracked; the ignored build directories are the same as at the
start.

- Committed: NO
- Pushed: NO

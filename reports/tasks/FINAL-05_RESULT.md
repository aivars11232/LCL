# FINAL-05 result — independent exhaustive TESTING_READY audit

## Identity
- Pack: `/mnt/F/LCL_Final_PreTesting_Blocker_Closure_Pack_v3_825694f/`, task `TASKS/FINAL-05_INDEPENDENT_TESTING_READY_AUDIT.md`.
- Entry HEAD: `9ddaadbc1662774f02fb355d3dcf609a678dabdc` ("LCL final pretest task4"), the owner's commit of FINAL-04.
- Exit HEAD: unchanged.
- Branch: `main`. Entry status: **clean**, 1,040 tracked files. Exit status: clean plus two untracked audit outputs (this report and its ledger).
- Git writes by agent: **NO** — no add, commit, amend, merge, rebase, reset, tag, push, pull or fetch. Read-only inspection only.
- Repairs: **NONE.** No tracked file was created, modified or deleted.
- Agents or background workers: **NO.** Network or new dependencies: **NO.**
- Scratch: `/mnt/F/.lcl-pretest/f5/` (`f5-gate.sh`, `ledger.py`, `verify_candidate.py`, probes, acceptance root). Logs: `/mnt/F/.lcl-pretest/logs/F5-*`.
- `TMPDIR` was `/tmp/lcl-final-05`; `CARGO_TARGET_DIR` was the isolated `target-current` / `target-msrv`.
- Toolchains: cargo/rustc 1.98.1, MSRV cargo 1.75.0, node v26.9.0, python 3.14.7.
- Identities recomputed from the tree: Core 0.1 `00d648b162939d06c44838481a67c39bc12c64bdd6d105035c24150148fe67ed` (176 files); Core 0.2 `00daee8de1919c4945ef04ff65edb22164bd8046a493be08a87d5fa3b4c3e604` (216 files).

## Verdict

**BLOCKED.**

One contract item fails outright, and it is the one the pack has carried since PRETEST-05:

> **B1 / F27 — the production claim is `source_conforming`, not `semantics_conforming`, and 45 semantic probes are invalid.**

`05_TESTING_READY_CONTRACT.md` requires `semantics_conforming` and `semantic invalid = 0`. Neither holds at this HEAD. Nothing else found in this audit is a technical blocker; three new hygiene findings and one letter-of-the-contract deviation are recorded below for the owner.

## 1. Frozen identity

| Item | Value |
|---|---|
| HEAD | `9ddaadbc1662774f02fb355d3dcf609a678dabdc` |
| branch | `main` |
| worktree | clean (`git status --short --untracked-files=all` empty at entry and at exit) |
| tracked files | 1,040 (`git ls-files`) |
| inventory | `git ls-tree -r -l HEAD`, path/blob/size, in `reports/tasks/FINAL-05_FILE_LEDGER.tsv` |
| delta since the owner's FINAL-03 commit `4dead55` | 7 added files: the 6 candidate files and `reports/tasks/FINAL-04_RESULT.md`. **No tracked source byte changed.** |
| delta since the pack baseline `825694f` | 60 files, 13,107 insertions, 166 deletions — FINAL-01/02/03 repairs, four task reports, the candidate, and two stray additions (F5-N1, F5-N2) |
| fresh candidate | `releases/candidates/lcl-0.2.0-linux-x86_64-68529c1420ba/`, source id `68529c1420ba…`, artifact `bfbec817…`, commit `4dead55`, 0 uncommitted entries |

## 2. Every-file ledger

`reports/tasks/FINAL-05_FILE_LEDGER.tsv` — one row per tracked path: path, Git blob SHA, size, class, review method, review status, finding/reference.

| Metric | Value |
|---|---|
| rows | **1,040** |
| `git ls-files` | **1,040** (equal) |
| `unreviewed` | **0** |
| `reviewed-defect` | **0** |
| `reviewed-finding` | 14 (each carries its reference) |
| `reviewed-pass` | 1,026 |

Classes: canonical 0.1 176, canonical 0.2 216, rust-source 156, rust-test 147, historical-lineage 158, report 58, release-artifact 32, structured-config 23, binary-image 17, lcl-fixture 15, application 11, rust-example 11, script 7, text-asset 5, documentation 5, frontend-source 2, binary-archive 1.

Every row was produced mechanically: worktree bytes are checked against the HEAD blob and the index; canonical files against `SHA256SUMS.txt`; JSON/TOML/XML/TSV are parsed; `.py` compiled, `.sh` run through `bash -n`, `.js` through `node --check`; PNGs are checked for signature and for dimensions matching their directory; archives for CRC; Rust and `.lcl` sources for UTF-8 and the `todo!`/`unsafe`/unbounded-read/markup scan. The generator is `/mnt/F/.lcl-pretest/f5/ledger.py`, adapted from PRETEST-05's rather than rewritten.

The 8,476 lines of new conformance *case data* (`operation_cases/clauses.rs`, `lifecycle_cases.rs`, `result_cases/engine.rs`, `semantic_cases/behaviors.rs`) were reviewed as executed evidence — every case runs in the gate, its accounting is pinned by `production_report.rs`, and its membership by `MAPPING_DIGEST` — not line by line. The load-bearing production diffs since the baseline were read in full.

## 3. The blockers, rechecked independently

### B1 / F27 — semantic conformance — **STILL OPEN (the blocker)**

Measured here, at this HEAD (`F5-G-conformance-text.log`, `F5-G-conformance-json.log`):

| | required | satisfied | failed | missing | invalid |
|---|---:|---:|---:|---:|---:|
| source | 2,011 | 2,011 | 0 | 0 | 0 |
| semantic | 402 | 357 | 0 | 0 | **45** |

`CLAIM: source_conforming`.

The FALLBACK half of F27 **is** closed, and I reproduced it against the shipped candidate rather than a development binary (`F5-fallback-probe.log`). Taking the canonical example `08_AUTHORITY_OVERRIDE_HANDLER_AND_RETRY.lcl` and changing only its `FALLBACK:` field, the installed CLI gives:

| FALLBACK | Result | Exit |
|---|---|---:|
| `core.stop` (as shipped) | passes every stage through static_checking | 0 |
| `core.move` | `error.operation.parameter` — requires `destination`; and required TARGET unsupplied | 1 |
| `core.delete` | `error.operation.parameter` — required TARGET unsupplied | 1 |

`lcl-checker/src/operation.rs::fallback` runs with each HANDLER, reuses the registry contract and `admits_handler_context`, and its doc quotes the canon it implements. This matches `05_TESTING_READY_CONTRACT`'s "FALLBACK operation-identifier sites are statically checked exactly as canon requires".

What fails is the conformance level. All 45 invalid probes are invalid for one reason only — pinned sub-runs with no executed run:

- 45 of 45 "limited by" lines read `invalid [missing sub-runs: …]`; **0** mention a failed, unexpected or duplicated sub-run;
- the 45 rows name **79** distinct missing sub-runs, matching FINAL-02's record;
- families: `operation_errors` 34, `operation_binding` 3, `result_schemas` 3, `error_contract` 2, `operation_effects` 2, `failure_lifecycle` 1.

**No pin hides a failure**, verified by reading the oracle, not by trusting it:
- `production_report.rs`: `KNOWN_FAILED` is `[]` and `UNPOPULATED` is `[]`; the test asserts `failed_ids()` is empty, that no record is unrequired, excluded or duplicated, that no probe is missing at source level, and that an `Invalid` probe is accepted **only** when its account has missing sub-runs and no failed, unexpected or duplicated one;
- `MAPPING_DIGEST` `9b32a28b…a8ad` equals `sha256sum impl/crates/lcl-conformance/src/obligations_v0.1.0_r2.json`, so the inventory cannot be narrowed silently;
- the single `#[ignore]` in the workspace is `manifest_input_bounds.rs::child_parses_nested_manifest`, the child half its parent invokes by name — it hides nothing.

FINAL-02 recorded the root cause as `BLOCKED_NEW_ROOT_CAUSE`: the 79 sub-runs need six engine capabilities (scope resolution G1, graph execution G2, host-observed targets G4, a fallible store G5, a determinism trigger G6, key-operation profiles G7) plus five canon-ambiguous rows. I confirm that boundary and did not repair it — FINAL-05 may not.

### B2 / F28 — current-source candidate — **CLOSED**

Independently verified (`F5-candidate-provenance.log`, 10 of 10 checks, and the `protected` gate):

- the source id recomputes as the SHA-256 of `SOURCE_INVENTORY.tsv`;
- the source archive and the payload digests equal the provenance's, and both `.sha256` files verify;
- provenance records **0** uncommitted entries and there is no `SOURCE_CHANGES.patch`;
- the 1,007 inventory paths are exactly `git ls-tree 4dead55` minus `releases/`;
- all 1,007 recorded digests equal the current worktree files, byte for byte;
- the provenance carries both identities, including the current Core 0.2 `00daee8d…`;
- the 26 historical `releases/` files are digest-for-digest what FINAL-04 recorded at its exit; the new candidate lives in its own directory.

One deviation from the letter of the contract is recorded as **F5-N3** below.

### B3 / F29 — Core 0.2 independent review — **no technical blocker; the decision remains the owner's**

This audit is the independent technical review, and it finds **no technical blocker** in Core 0.2:

- `sha256sum -c --strict --quiet SHA256SUMS.txt` verifies (215 records);
- `validate_language_contracts.py`, `validate_localization.py`, `validate_source_fixtures.py` all exit 0; `validate_ebnf.py` accepts both packages' `10_COMPLETE_EBNF.ebnf` (start `DOCUMENT`, 211 terminals);
- the package identity recomputes to `00daee8d…` over 216 files and equals `lcl-spec::anchor::APPROVED_PACKAGE_0_2_0.identity_digest`;
- `MANIFEST.json` hashes to `c1ab983de2566a11356c2b8d1e7d2b667c1cfce4661d1b5d27cfed4aad336e67`, exactly the value the anchor's documentation names;
- the installed 0.2.0 package drives 33 localized verdicts on the unpacked candidate across en/lv-LV/nl-NL/ru-RU/zh-CN, including fail-closed behaviour without the package.

What remains is a *decision record*, not a defect: `validate_release.py --scope all` exits 1 with 31 PASS, 0 FAIL, 2 OUT_OF_SCOPE, 1 BLOCKED — `language_decisions_and_release_state`, `pending_decisions: ["independent_review"]`, `release_gate_permitted: false`, `UNRELEASED_CANDIDATE`. That is the owner's to close, and it gates *release*, not testing. Core 0.1, by contrast, validates with `release_ready: true`.

### B4 / F10 — invalid-UTF-8 attribution — **CLOSED, unambiguous**

- Canon now states exactly one rule, Reading A, in the two files that own the question: `02_LEXICAL/01` ("UTF-8 decoding is atomic over the complete source unit … Bytes that are readable before the first invalid byte establish no version") and `02_LEXICAL/13` (the localized consequence). `grep` finds no third statement and no contradicting text.
- The implementation agrees, and the **installed candidate** agrees (`F5-EXTRA.log`): a `0xFF` at five positions in `canonical_en.lcl` and `explicit_lv.lcl` — after a complete `VERSION "0.2.0"`, late in the document, inside the `VERSION` keyword, after a complete localized `VERSIJA`, inside `@locale` — each yields Core **0.1.0**, `error.encoding.invalid`, at original bytes 26, 202, 14, 40 and 5. Three of those carry a readable 0.2.0 VERSION before the bad byte, so Reading B would have swung them; it does not.
- Strict UTF-8 stays original-byte correct: every offset above is the offset in the original bytes, not in a repaired or truncated text.

### B5 — `real_process` test-gate race — **CLOSED, not hidden**

`F5-realproc.log`, against the current-toolchain test binary:

| Mode | Runs | Failures |
|---|---:|---:|
| default parallel | 20 | 0 |
| `--test-threads=1` | 3 | 0 |
| 8 concurrent copies × 5 rounds | 40 | 0 |
| `--exact many_flooding_children_in_sequence_leave_nothing_behind` | 5 | 0 |

68 of 68 green, `grep -c "FAILED\|panicked"` = 0. The test body is unchanged where it matters: it still runs 20 flooding children and still asserts `zombie_children() == 0`; only the *process* it counts in is now its own, via the `LCL_PROCESS_FIXTURE` re-exec the same file already used for `inherited_pipe_case`. The product half is real: `process/linux.rs::vanished` accepts ENOENT **or** ESRCH during the `/proc` group scan, with a unit test that ESRCH is "gone" and EACCES is not. Nothing was weakened, skipped or `#[ignore]`d.

### Regression sample of previously closed high-risk areas

From the current-toolchain workspace run (162 targets, all green):

| Area | Target | Result |
|---|---|---|
| object constraints / demand | `object_constraints.rs`, `declaration_demand.rs`, `demand_obligations.rs`, `declared_constraints.rs` | 9 / 6 / 7 / green, 0 failed |
| VERIFY/FAILURE diagnostics | `verify_and_test.rs`, `success_and_failure.rs`, `failure_handling.rs`, `completion_matrix.rs` | 24 / 21 / 27 / 6, 0 failed |
| PATH / GLOB / WORKSPACE | `path_patterns.rs`, `patterns.rs`, `real_filesystem.rs`, `routes.rs` | 5 / green / green / green, 0 failed |
| profile bounds | `profiles.rs` + the candidate's own boundary cases (1,048,576 bytes accepted, +1 refused as `error.localization.profile_invalid`, 257 profile files refused as a host limit with exit 4) | 13 + 4, 0 failed |
| workspace containment | `workspace_containment.rs`, `capability_boundary.rs` | 5 / green, 0 failed |
| cache / CLI / package boundaries | `manifest_and_lock.rs`, `persistence.rs`, `commands_and_exit_codes.rs`, `machine_output.rs`, `package_0_2_0.rs`, `trust_anchor.rs`, `source_identity.rs` | all green, 0 failed |

## 4. Gates

All run by me in this session, isolated (`TMPDIR=/tmp/lcl-final-05`, out-of-tree `CARGO_TARGET_DIR`), through `/mnt/F/.lcl-pretest/f5/f5-gate.sh`, adapted from PRETEST-05's gate.

| Command | Exit | Result | Log |
|---|---:|---|---|
| `cargo fmt --all -- --check` | 0 | clean | `F5-G-fmt.log` |
| `cargo clippy --offline --locked --workspace --all-targets -- -D warnings` | 0 | clean | `F5-G-clippy.log` |
| `cargo test --offline --locked --workspace --all-targets --no-fail-fast` | 0 | 162 blocks, **1,695 passed, 0 failed, 1 ignored** (238s) | `F5-G-test-workspace.log` |
| `cargo test --workspace --doc` | 0 | 17 blocks, no doctests defined | `F5-G-doctests.log` |
| MSRV 1.75.0 `cargo check --workspace --all-targets` | 0 | clean | `F5-G-msrv-check.log` |
| MSRV 1.75.0 `cargo test --workspace --all-targets --no-fail-fast` | 0 | 162 blocks, **1,695 passed, 0 failed, 1 ignored** (674s) | `F5-G-msrv-tests.log` |
| `real_process` stress + repeats | 0 | **68 of 68**, 0 failures | `F5-realproc.log` |
| Core 0.1 `sha256sum -c --strict --quiet` | 0 | verifies | `F5-G-sha-0.1.0.log` |
| Core 0.1 `validate_release.py --scope all` | 0 | `release_ready: true`, `scope_ready: true`, 0 FAIL, 0 BLOCKED | `F5-G-validate-0.1.0.log` |
| Core 0.1 identity | 0 | `00d648b1…fe67ed`, 176 files | `F5-G-identity-0.1.0.log` |
| Core 0.2 `sha256sum -c --strict --quiet` | 0 | verifies | `F5-G-sha-0.2.0.log` |
| Core 0.2 `validate_release.py --scope all` | 1 | 31 PASS, 0 FAIL, 2 OUT_OF_SCOPE, **1 BLOCKED** (`independent_review`) | `F5-G-validate-0.2.0.log` |
| `validate_language_contracts.py` | 0 | PASS | `F5-G-validate_language_contracts.log` |
| `validate_localization.py` | 0 | PASS | `F5-G-validate_localization.log` |
| `validate_source_fixtures.py` | 0 | PASS | `F5-G-validate_source_fixtures.log` |
| `validate_ebnf.py` (0.1 and 0.2) | 0 / 0 | start `DOCUMENT`, 211 terminals each | `F5-G-ebnf-0.1.0.log`, `F5-G-ebnf-0.2.0.log` |
| Core 0.2 identity | 0 | `00daee8d…c3e604`, 216 files | `F5-G-identity-0.2.0.log` |
| `sha256sum -c assets/brand/BRAND_ASSETS.sha256` | 0 | verifies | `F5-G-brand.log` |
| `m8_conformance_report` (text and `--json`) | 0 | `source_conforming`; semantic 357/0/0/45 | `F5-G-conformance-*.log` |
| `lcl spec` / `lcl check --machine` identity | 0 | identities match | `F5-G-identity.log` |
| protected areas + every candidate checksum | 0 | no change under `canonical`, `releases`, `assets`; every `.sha256` verifies | `F5-G-protected.log` |
| candidate provenance (independent recompute) | 0 | 10 PASS, 0 FAIL | `F5-candidate-provenance.log` |
| FALLBACK probes through the installed CLI | 0/1/1 | as canon requires | `F5-fallback-probe.log` |
| **candidate smoke, unpacked, in a fresh disposable HOME/XDG** | | | |
| E1 install + installed CLI over every VALID/INVALID example | 0 | **57 PASS, 0 FAIL** | `F5-E1.log` |
| E2 launcher, loopback HTTP session/API, token and Host refusal | 0 | **35 PASS, 0 FAIL** | `F5-E2.log` |
| localized 0.2.0 across en/lv-LV/nl-NL/ru-RU/zh-CN, lock and drift | 0 | **33 PASS, 0 FAIL** | `F5-LOC.log` |
| F10 attribution, nested relative path, profile limits | 0 | **14 PASS, 0 FAIL** | `F5-EXTRA.log` |
| E5 uninstall, 29 unrelated files preserved, reinstall | 0 | **11 PASS, 0 FAIL** | `F5-E5.log` |
| | | **150 verdicts, 0 failures** | |
| checkout-local / shared temp contamination | — | `impl/target/test-tmp` 0, `/tmp/lcl-apps` 0 entries written | gate stdout |

The candidate smoke is a full independent re-run in a new acceptance root (`/mnt/F/.lcl-pretest/f5/accept1`), not a re-reading of FINAL-04's logs: same 150 verdicts, same result.

## 5. Contract checklist (`05_TESTING_READY_CONTRACT.md`)

| Item | Verdict | Evidence |
|---|---|---|
| claim = `semantics_conforming` | **FAIL** | `source_conforming` |
| source required/satisfied complete | PASS | 2,011 / 2,011 |
| semantic failed = 0 | PASS | 0 |
| semantic missing = 0 | PASS | 0 |
| semantic invalid = 0 | **FAIL** | 45, all awaiting 79 pinned sub-runs |
| no known-failure pin hides a failure | PASS | `KNOWN_FAILED` and `UNPOPULATED` both empty; mapping digest recomputed; the one `#[ignore]` is a parent-driven child |
| FALLBACK sites statically checked as canon requires | PASS | `F5-fallback-probe.log` |
| Core 0.1 canonical tree unchanged | PASS | 0 files changed since `825694f`; worktree clean |
| Core 0.1 checksums/validator pass | PASS | `sha256sum -c` 0, `validate_release` `release_ready: true` |
| Core 0.1 identity `00d648b1…` | PASS | recomputed, 176 files |
| F10 has one explicit owner-selected answer | PASS | decision A in `02_LEXICAL/01` and `/13` |
| implementation and canonical candidate agree | PASS | anchor `00daee8d…` = computed identity; MANIFEST digest = documented; behaviour = Reading A |
| Core 0.2 checksums/validators pass | PASS (with one decision record) | 4 validators + EBNF green; `validate_release` BLOCKED only on `independent_review` |
| Core 0.2 trust anchor matches | PASS | `lcl-spec::anchor::APPROVED_PACKAGE_0_2_0` |
| strict UTF-8 original-byte correct | PASS | offsets 26, 202, 14, 40, 5 through the installed CLI |
| no remaining localization/candidate blocker | PASS | none found technically |
| current toolchain fmt/clippy/tests green | PASS | 0 / 0 / 1,695 passed |
| MSRV checks/tests green | PASS | 1.75.0, 1,695 passed |
| no flaky required gate | PASS | 68 of 68 `real_process` runs |
| hardening/security/applications/packaging green | PASS | `applications.rs` 9, `security_matrix.rs` 6, `packaging_smoke.rs` 5, `installed_launcher.rs` green |
| no new checkout-local or shared temp contamination | PASS | 0 / 0 entries |
| candidate built from final clean HEAD | **DEVIATION** | built from `4dead55` = `HEAD~1`; see F5-N3 |
| zero uncommitted source entries | PASS | provenance `0`, no `SOURCE_CHANGES.patch` |
| exact source inventory/checksums | PASS | 1,007 / 1,007 paths and digests |
| current Core 0.2 identity | PASS | `00daee8d…` in the provenance and the payload |
| historical candidates untouched | PASS | 26 digests identical |
| actual unpacked candidate smoke passes | PASS | 150 verdicts, 0 failures |
| one row per final tracked file | PASS | 1,040 = 1,040 |
| `unreviewed` = 0 | PASS | 0 |
| all blocker root causes rechecked | PASS | section 3 |
| no technical blocker remains | **FAIL** | B1/F27 |

## 6. New findings from this audit

| Id | Finding | Severity | Evidence | Owning next step |
|---|---|---|---|---|
| **F5-N1** | `archive-EWXwoU/gk_3.1.75_linux_amd64.zip` — a 9.4 MB third-party binary archive (one member, `gk`, 24 MB, the GitKraken CLI) is tracked in the product repository. It was added in `5ca774e`, is referenced by nothing in the tree, and `packaging/build_release.sh` records every tracked file outside `releases/`, so it is inside the candidate's `SOURCE_INVENTORY.tsv` and its 12 MB source archive. | Provenance/packaging hygiene. Not a language or product defect; no gate reads it. | `git log --diff-filter=A`, `git grep` finds no reference, `grep -c archive-EWXwoU` in the inventory = 1 | Owner: remove it from tracking (a Git write, forbidden to this task) and rebuild the candidate, or record it deliberately. |
| **F5-N2** | `.directory` — a KDE folder-settings file, tracked since `5ca774e`, whose `Icon=` line carries a developer-local absolute path (`/home/aivars/Pictures/…`). It ships in the candidate source archive for the same reason. | Minor hygiene / local-path leak. | the file's 84 bytes; inventory row | Owner: untrack and rebuild, or accept. |
| **F5-N3** | The candidate's provenance names `4dead55`, which is now `HEAD~1`. The delta to HEAD is exactly the candidate's own six files plus `reports/tasks/FINAL-04_RESULT.md`. A candidate cannot contain its own artifacts, so the *build inputs* are identical to HEAD's: all 1,007 inventory digests equal the current worktree, and the only path in HEAD's source set missing from the inventory is that one report. | Letter-of-contract deviation, not a technical defect. | `F5-candidate-provenance.log` | Owner: accept as-is, or rebuild after this report is committed if "final clean HEAD" is to be literal. |
| **F5-O3** | `impl/crates/lcl-spec/src/lib.rs` package reads remain unbounded. Unchanged non-blocking observation from PRETEST-05: the path is operator-selected (`--spec`) and identity-verified after the read. | Observation. | ledger row | None before testing. |

O1 and O2 from `01_BASELINE_AND_BLOCKERS.md` are confirmed repaired: `CompletionError::PatternMismatch` is mirrored in registry order (`lcl-completion/src/diagnostic.rs`), and the `load` doc comment is back on `Contracts::load`.

## 7. Root-cause assignment for the BLOCKED verdict

| Blocker | Root cause | Why it is outside FINAL-05 | Deterministic evidence |
|---|---|---|---|
| B1 / F27 | 79 pinned semantic sub-runs have no constructible input in this build; they need six engine capabilities (G1 scope resolution, G2 graph execution for `core.execute`/`core.test`, G4 host-observed targets, G5 a fallible MEMORY/STATE store, G6 a determinism trigger, G7 key-operation profiles) and five rows whose canon does not determine a trigger. | FINAL-05 is strictly read-only, and FINAL-02 already recorded this as a `BLOCKED_NEW_ROOT_CAUSE` architectural boundary, not a defect in a row. | `F5-G-conformance-text.log`: 45 × `invalid [missing sub-runs: …]`, 79 sub-runs, 0 failed/unexpected/duplicated |
| B3 / F29 (non-technical) | The Core 0.2 language-decision ledger still lists `independent_review` as pending, so `release_gate_permitted` is false and the package stays `UNRELEASED_CANDIDATE`. | An owner decision, explicitly listed in `07_OWNER_ACTIONS.md`. This audit clears it technically. | `F5-G-validate-0.2.0.log` |

## 8. Residual / out-of-scope
- **F30** — real KDE desktop-menu and file-association acceptance still needs a graphical session; none exists here. The desktop entry, media type, icons and launcher were verified installed, valid and cleanly removed (E1/E2/E5).
- **F31** — branch protection and required status checks: owner action, untouched.
- The E4 headless-browser harness was not re-run; FINAL-05's list asks for workspace launch/API, which E2 and the localized harness cover.
- The candidate remains an `UNRELEASED_CANDIDATE`. Nothing here promotes it.
- No HawkScan run: this task changed no code, the pack forbids network access, and no API key is configured.

## 9. AI quota / reuse
- Reused: PRETEST-05's `ledger.py` (adapted, not rewritten), PRETEST-05's `p5-gate.sh` (adapted), PRETEST-03's `identity.py`, the FEATURE-04 acceptance harnesses E1/E2/E5 and FINAL-04's localized and extra harnesses, all unmodified; FINAL-01/02/03/04 reports for claims, each re-verified before being repeated here.
- Each broad gate ran exactly once. The 8,476 lines of conformance case data were reviewed as executed evidence with a pinned digest rather than read line by line; that boundary is stated in section 2.
- New code written: one gate script, one 10-check provenance verifier, three one-line FALLBACK probe documents, and a patch of ~40 lines to the reused ledger generator. Nothing was written into the repository beyond this report and its ledger.

## 10. Files changed
**No tracked file was created, modified or deleted.** Two untracked audit outputs for the owner to commit:

| Path | What |
|---|---|
| `reports/tasks/FINAL-05_RESULT.md` | this report |
| `reports/tasks/FINAL-05_FILE_LEDGER.tsv` | the 1,040-row every-file ledger |

## Proposed commit message
```
LCL FINAL-05: independent TESTING_READY audit - BLOCKED

Read-only audit of 9ddaadb. Every tracked path has a ledger row: 1,040
rows for 1,040 files, 0 unreviewed, 0 defects, 14 findings.

Gates re-run independently and green: fmt, clippy -D warnings, the full
workspace (1,695 passed, 0 failed) on 1.98.1 and on MSRV 1.75.0, 68 of 68
real_process stress and repeat runs, both packages' checksums, the four
0.2 validators and both EBNF grammars, brand assets, protected areas, and
150 candidate smoke verdicts with 0 failures against the unpacked
candidate in a fresh disposable HOME/XDG.

Blockers rechecked: F27's FALLBACK half is closed and reproduced through
the installed CLI; F28's candidate provenance recomputes in full, 1,007
of 1,007 inventory digests equal the worktree; F10 is unambiguous, with
decision A stated in canon and the installed candidate attributing five
invalid-UTF-8 positions to Core 0.1.0 at the original byte; B5 is fixed,
not hidden - the zombie assertion is unchanged and only its process is
isolated. Core 0.1 is untouched at 00d648b1..., Core 0.2 recomputes to
00daee8d... and equals its trust anchor.

Verdict is BLOCKED on B1/F27 alone: the claim is source_conforming, not
semantics_conforming, and 45 semantic probes remain invalid - every one
of them only because 79 pinned sub-runs have no constructible input, with
0 failed, 0 missing and no pin hiding a failure. Core 0.2's independent
review is technically clear; its pending_decisions entry is the owner's.

Three new findings are recorded: a tracked 9.4 MB GitKraken CLI archive
and a .directory file with a local path, both of which the release
process carries into the candidate source archive, and the candidate's
provenance naming HEAD~1, whose only source-set delta is the FINAL-04
report itself.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
```

## Next task
The pack's sequence is complete; its end condition is **not** met. `TESTING_READY` cannot be issued at this HEAD.

To reach it, the owner needs a new task that closes B1/F27 — the six engine capabilities and the five canon-ambiguous rows FINAL-02 enumerated — after which FINAL-05 must be re-run in full. F30 and F31 remain owner actions, and public release stays an owner decision after hands-on testing.

# LCL-REPAIR-06 — Result

## Identity

- Task: LCL-REPAIR-06, hardening, conformance reconciliation and final verification (B-13, B-14, final reconciliation of Tasks 1–5)
- Repository: `/mnt/F/LCL`
- Branch: `main`
- Entry HEAD: `ab24ace7daed5e64ecf18495f6cd6c31ab25fd9f` ("LCL repair task 5"). The owner committed REPAIR-05 (its 13 files and result report) and instructed on 2026-09-16 to start REPAIR-06, which resolves REPAIR-05's BLOCKED (partial) state for sequencing. S4 remains blocked as REPAIR-05 records.
- Exit HEAD: unchanged
- Entry worktree: clean
- Exit worktree: 10 modified tracked files (listed below), plus this report. Nothing under `canonical/`, `releases/` or `assets/` changed.
- Pack manifest: all 21 `MANIFEST_SHA256.md` hashes verified at the start.
- Git writes performed: **NO**
- Publication performed: **NO**
- Background/sub/parallel agents used: **NO**
- Evidence logs: `/mnt/F/.lcl-repair-6t/logs/R6-*`
- Scratch: `/mnt/F/.lcl-repair-6t/tmp/r6/` (markers, the mutation backup) and `/mnt/F/.lcl-repair-6t/tmp/r6-gate.sh` (the Part D gate)

## Result

**Status:** PASS WITH OUT-OF-SCOPE FINDINGS

- B-13 and B-14 are FIXED_VERIFIED.
- Every broad gate either passed or has an explained, expected non-zero result (see Evidence).
- Protected identities are unchanged, and no candidate was built or overwritten.
- The production conformance claim is exactly `source_conforming`.

> This does not establish full semantic conformance, and it does not release Core 0.2.0. Its independent review is still pending.

## Scoped findings

| Finding | Reproduction | Repair | Verification | Disposition |
|---|---|---|---|---|
| B-13: test scratch in the checkout target | `R6-A-RED.log`: run with `CARGO_TARGET_DIR` set outside the checkout, the lcl-spec (`trust_anchor`, `canonical_release`), lcl-project, lcl-cli and hardening `protocol_and_tooling` targets still wrote **295** entries under `impl/target/test-tmp` (`R6-A-RED-writes.txt`) | The five helpers that joined `CARGO_MANIFEST_DIR/../../target/test-tmp` now use Cargo's `env!("CARGO_TARGET_TMPDIR")` (Rust ≥ 1.54; MSRV 1.75). Same helper names, same call sites | `R6-A-GREEN.log`: the same 15 targets, 130 passed, 0 failed; **0** checkout writes, all scratch under `<target>/tmp`. Final gate: 0 checkout writes; `target-current/tmp` and `target-msrv/tmp` each received their own 103 entries, so separate target roots no longer share scratch | FIXED_VERIFIED |
| B-14: durable 0.2 packaging coverage | Phase 0 inventory. Already durable from REPAIR-01/03: launcher `--localized-spec` substitution (`install_paths_reach_the_workspace_exactly`), workspace profile provisioning (`a_localized_document_uses_the_profiles_its_project_declares`), localized MIME magic (`a_localized_document_is_recognised_by_its_first_bytes`). **Not covered:** payload holding both packages, installer installing both, installed-CLI localized check, 0.1 result with 0.2 present, 0.2 uninstall boundary, reinstall | (1) New `installed_launcher.rs` test `a_0_2_0_installation_checks_both_languages_and_uninstalls_only_its_own`, reusing `Home`, `stage_payload`, `copy_tree` and `run_installer`. (2) `release_build.rs` `a_0_2_0_candidate_records_both_languages` now also unpacks the built archive and asserts `share/LCL_Core_0.1.0` and `share/LCL_Core_0.2.0` | `R6-B-targeted.log`: both pass. Mutation (`R6-B-mutation.log`): with the 0.2.0 removal deleted from `packaging/uninstall.sh`, the new test fails with "the 0.2.0 package survived the uninstall"; the script was restored from a byte copy and `git diff` on it is empty | FIXED_VERIFIED |
| Final reconciliation, Tasks 1–5 | Full gates below on the combined state | none needed | See the task table | see below |
| Active documentation | Searched the 17 tracked Markdown files outside `canonical/`, `reports/tasks/` and `releases/` for the Task 1–6 changed facts | 3 edits (Files changed) | Mapping digest and count recomputed from `obligations_v0.1.0_r2.json` (319 rows, 3,724 sub-runs; 3,717 at `7ed84ba`) and `sha256sum` (`32b3e634…`) | FIXED_VERIFIED |

### What the new installed test asserts

Against a real installation of a staged payload carrying Core 0.2.0, with a cleared environment:

1. The installed `lcl version --localized-spec <installed 0.2.0>` exits 0 and names languages 0.1.0 and 0.2.0. The option refuses any package that does not open as the approved Core 0.2.0 package.
2. The installed `lcl check --machine`, given the installed packages and `--profile lv-LV.json`, judges `explicit_lv.lcl` as `formal_version` 0.2.0 with `lv-LV` and no localization error.
3. `valid_minimum.lcl` gives a byte-identical machine report and exit code with and without `--localized-spec`, and that report is 0.1.0.
4. Uninstall removes both packages and the tool. It keeps the operator's `~/.local/share/lcl/workspace/profiles/lv-LV.json` and the project files outside the data directory.
5. Reinstall restores the 0.2.0 package, and the launcher names it again. The localized check report is byte-identical to the one before uninstall.

## Tasks 1–6

| Task | Commit | Status | Findings |
|---|---|---|---|
| REPAIR-01 | `7d100b7` | PASS WITH OOS | B-01, B-02, B-03, B-04 FIXED_VERIFIED |
| REPAIR-02 | `94dc07d` | PASS WITH OOS | B-05, B-06, B-07, B-08 FIXED_VERIFIED; B-09 FIXED_VERIFIED by external erratum |
| REPAIR-03 | `40c5106` | PASS WITH OOS | B-10, B-11 FIXED_VERIFIED |
| REPAIR-04 | `9b3739e` | PASS WITH OOS | B-12 FIXED_VERIFIED |
| REPAIR-05 | `ab24ace` | BLOCKED (partial) | B-15: 8 of 9 pinned sub-runs FIXED_VERIFIED; `division/declared-bound` BLOCKED_WITH_OWNER_DECISION |
| REPAIR-06 | uncommitted | PASS WITH OOS | B-13, B-14 FIXED_VERIFIED |
| — | — | — | B-16 boundary: remaining semantic evidence reported below, not repaired. B-17, B-18, B-19: owner actions |

The full gates below re-establish every earlier task's regression tests on the combined state. No earlier finding regressed.

## Files changed

| File | Why | Minimal change |
|---|---|---|
| `impl/crates/lcl-cli/tests/common/mod.rs` | B-13 | `scratch` root → `CARGO_TARGET_TMPDIR`; doc comment |
| `impl/crates/lcl-project/tests/common/mod.rs` | B-13 | same; module and function doc comments |
| `impl/crates/lcl-spec/tests/trust_anchor.rs` | B-13 | same; module doc comment |
| `impl/crates/lcl-spec/tests/canonical_release.rs` | B-13 | same |
| `impl/crates/lcl-hardening/tests/protocol_and_tooling.rs` | B-13 | same, keeping the `hardening/` subdirectory |
| `impl/crates/lcl-hardening/tests/installed_launcher.rs` | B-14 | one new test (above) |
| `impl/crates/lcl-hardening/tests/release_build.rs` | B-14 | payload assertions in the existing 0.2.0 candidate test |
| `impl/README.md` | Part C | scratch location sentence; a dated line under the 2026-09-14 correction pointing to this report |
| `reports/implementation/LCL_CONFORMANCE_OBLIGATIONS.md` | Part C (REPAIR-05 OOS-5) | mapping digest `386c1499…` → `32b3e634…`; 3,717 → 3,724 sub-runs |
| `reports/tasks/LCL-REPAIR-06_RESULT.md` | Result | this report |

`reports/implementation/LCL_RESIDUAL_REPAIR_REPORT.md` still cites the old digest and failure table. It is a dated execution record (G-15), so it was not edited.

## Evidence

Every command ran from `/mnt/F/LCL` with `TMPDIR=/mnt/F/.lcl-repair-6t/tmp`. The current toolchain used `CARGO_TARGET_DIR=/mnt/F/.lcl-closure-4t-4c1cd4c659b7/target-current`. The Rust 1.75.0 gates used `target-msrv`.

Toolchains:
- rustc/cargo 1.98.1 at `/usr/bin`;
- rustc/cargo 1.75.0 from `/mnt/F/.lcl-residual-repair-01-lxd8dwu8/toolchains/1.75.0/bin`.

Gate summary: `R6-D-gate-summary.log`.

| # | Gate | Command | Exit | Result |
|---|---|---|---:|---|
| 1 | Format | `cargo fmt --all -- --check` | 0 | clean |
| 2 | Clippy | `cargo clippy --offline --locked --workspace --all-targets -- -D warnings` | 0 | no warnings |
| 3 | Workspace tests, 1.98.1 | `cargo test --offline --locked --workspace --no-fail-fast` | 0 | 162 result blocks (including doctests): **1,593 passed, 0 failed, 1 ignored** |
| 4a | MSRV check, 1.75.0 | `cargo check --offline --locked --workspace --all-targets` | 0 | clean |
| 4b | MSRV tests, 1.75.0 | `cargo test --offline --locked --workspace --all-targets --no-fail-fast` | 0 | 156 result blocks (no doctest blocks under `--all-targets`): **1,593 passed, 0 failed, 1 ignored** |
| 5 | Core 0.1.0 integrity | `sha256sum -c --strict --quiet SHA256SUMS.txt`; `validate_release.py --scope all` | 0 / 0 | OK |
| 5 | Core 0.1.0 identity | built `lcl spec --spec canonical/LCL_Core_0.1.0` (`R6-D-identity.log`) | 0 | identity `00d648b162939d06c44838481a67c39bc12c64bdd6d105035c24150148fe67ed`, authoritative, equal to the pack |
| 6 | Core 0.2.0 integrity | `sha256sum -c --strict --quiet SHA256SUMS.txt` | 0 | OK |
| 6 | Core 0.2.0 release validator | `validate_release.py --root canonical/LCL_Core_0.2.0 --scope all` | 1 | PASS 31, FAIL 0, OUT_OF_SCOPE 2, **BLOCKED 1**: `language_decisions_and_release_state`, `pending_decisions: ["independent_review"]`, `release_gate_permitted: false`. Expected; the same exit as Task 4 `T4-G4-validate-all-0.2.0` |
| 6 | Core 0.2.0 identity | built `lcl check --machine --spec … --localized-spec … canonical_en.lcl`, absolute paths (`R6-D-identity-abs.log`) | 0 | `formal_version` 0.2.0, `identity_digest` `e86121c734cb51065b791329738b9ed6b77c53e03e0eebbb2ad1076bea973ef0`, equal to the pack. The trust-anchor tests in gate 3/4b also pass |
| 7 | Localization validator | `validate_localization.py --root canonical/LCL_Core_0.2.0` | 0 | pass |
| 8 | 0.1 compatibility | inside gates 3/4b: `lcl-spec` anchor and canonical-release tests, the 0.1 dispatch rows, `a_core_0_1_0_document_stays_core_with_both_packages`, and the new installed 0.1-beside-0.2 assertion | 0 | pass |
| 9 | Packaging/hardening | inside gates 3/4b: every `lcl-hardening` target, including `installed_launcher` and `release_build` | 0 | pass |
| 10 | Conformance | `cargo run -p lcl-conformance --example m8_conformance_report [-- --json]` (`R6-D-conformance-{text,json}.log`) | 0 / 0 | see Final conformance result |
| 11 | Protected | `t4_verify_protected.py`; `git diff 7ed84ba -- canonical releases assets`; untracked files there; every candidate's product and source `.sha256` (`R6-D-protected.log`) | 0 | 192/192 + 7/7 matched; no diff and no untracked file since the pack baseline; all 6 candidate checksum files OK |
| — | Brand | `sha256sum -c assets/brand/BRAND_ASSETS.sha256` | 0 | OK |
| 12 | Task-pack regressions | inside gates 3/4b: the REPAIR-01..05 tests, `R6-A-GREEN`, `R6-B-targeted` | 0 | pass |

The harness's first identity command exited 4 (`R6-D-identity.log`) because it passed a relative document path with a directory part. The CLI resolved that path wrongly, which is REPAIR-02 OOS-1, already recorded and not new. The rerun with absolute paths is the evidence. The candidate `SOURCE_MANIFEST.sha256` was skipped on purpose: it lists source-tree paths, so it cannot pass in place, as recorded in earlier tasks.

### Scratch and worktree contamination (G)

- Writes to the checkout's `impl/target/test-tmp` during the whole gate: **0**.
- `git status` after the gate: only the files listed above.
- `impl/target/test-tmp` still holds entries written before this repair. That directory is Git-ignored and was left in place.
- The hardening application tests still wrote `/tmp/lcl-apps/*` (OOS-1).

## Final conformance result

**CLAIM: `source_conforming`**. Mapping digest `32b3e634dd135784163b90dd94736fae4522b68945dbbfaede05e454296fbf8e`, package `00d648b1…67ed`.

| Level | Required | Satisfied | Failed | Missing | Invalid |
|---|---:|---:|---:|---:|---:|
| source_conforming | 2,011 | 2,011 | 0 | 0 | 0 |
| semantics_conforming | 402 | **268** | **1** | 2 | 131 |
| semantics_conforming at Task 4 (`T4-G7`) | 402 | 261 | 8 | 2 | 131 |

- Executed cases: 2,411, of which 2,410 passed and 1 failed.
- Witnesses established: 66 of 66.
- Failed case ID: `semantic/operator_invalid//`, sub-run `division/declared-bound`. This is S4, blocked by the owner in REPAIR-05.
- Claim limits: 135. At Task 4 there were 149.

The 2 missing and 131 invalid semantic probes come from missing sub-run evidence. Examples are `semantic/result_schemas/*` and `semantic/operation_errors/core.write`. They are the B-16 boundary and were not worked on here.

## Final 0.2 state

- `canonical/LCL_Core_0.2.0` is byte-identical to the pack baseline.
- The validator reports `independent_review` pending and `release_gate_permitted: false`. `VALIDATION_REPORT.txt` says the same.
- Nothing here changed that state.
- Core 0.2.0 is an **unreleased candidate**.

## Part E — candidate

**No candidate was built.**
- The durable hardening tests stage the payload layout that `build_release.sh` produces from the repaired scripts, then install and exercise it. That covers acceptance F.
- A build from this uncommitted worktree would not have exact-source provenance for a commit.
- Existing candidates were verified unchanged (gate 11).

## Protected-material check

- Core 0.1.0: unchanged. SHA256SUMS OK, validator OK, identity `00d648b1…67ed`.
- Core 0.2.0: unchanged. SHA256SUMS OK, localization validator OK, identity `e86121c7…73ef0`. The release scope is BLOCKED only on the independent review.
- Existing candidates: all 3 unchanged; checksums OK, no diff since `7ed84ba`.
- Historical evidence: not edited. That includes the earlier reports and the Task 4 logs.

## Out-of-scope findings

1. **Fixed `/tmp/lcl-apps`.**
   - `apps/*/src/main.lcl` declare `WORKSPACE PATH: PATH("/tmp/lcl-apps/<app>")` in their own source.
   - `lcl-hardening/tests/applications.rs` and `lcl-protocol/tests/v07_matrix.rs` use those paths.
   - The path ignores `TMPDIR`, and two test processes that drive the same application at once would share a directory. The in-process mutexes do not cover separate processes. Cargo runs test binaries one at a time, so this does not affect gates 3 or 4b.
   - It is not a checkout-target path. Changing it means changing the example applications and their V07 variants, so it was left for an owner decision.
2. **Relative document path with a directory part** (REPAIR-02 OOS-1) came up again in the identity harness. Not changed.
3. **Still open from REPAIR-05:** absolute PATH `MATCHES` GLOB, object-schema constraint validation (S4), a DATA VALUE with `/` being UNKNOWN at runtime, `Checked::deferred()` having no consumer, and the VERIFY `ASSERT` fault observation. No new evidence here.

## Remaining blockers

- None for REPAIR-06.
- REPAIR-05 S4 (`division/declared-bound`) is still BLOCKED_WITH_OWNER_DECISION and is the only failed semantic probe.

## Owner actions

- O-01: independent review of Core 0.2.0. Still pending (B-17).
- O-02: manual desktop-menu launch (B-18).
- O-03: review and commit these 10 files and this report.
- O-04: publication. None was authorized or performed.
- O-05: branch protection and signing (B-19).
- Decide whether to schedule S4 and REPAIR-05 OOS 2–4 as a dedicated semantic task (B-16).
- Decide on OOS-1 (`/tmp/lcl-apps` in the example applications).
- Optional: after committing, build a fresh 0.2.0 candidate into an external directory for exact-source provenance of the repaired packaging.

## Quota/efficiency notes

- Reused:
  - the Task 4 compile caches (`target-current`, `target-msrv`), the private 1.75.0 toolchain, the Task 4 gate commands and `t4_verify_protected.py`;
  - `r5-gate.sh`'s structure;
  - the existing hardening helpers (`Home`, `stage_payload`, `run_installer`, `copy_tree`, `shell`) and canonical fixtures.
- No new helper layer or dependency.
- Broad gates ran exactly once, after Parts A and B were green. Parts A and B used only their targeted targets.
- Earlier task results were reconciled from their reports and the full gates, not re-audited.

## Handoff

- This is the last task in the pack; no further task is unlocked.
- The pack as a whole ends with REPAIR-05 S4 blocked by the owner and B-16, B-17, B-18 and B-19 left as owner actions.

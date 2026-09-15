# LCL-REPAIR-02 — Result

## Identity

- Task: LCL-REPAIR-02 — Versioning, Release Metadata, and Repository Hygiene (LCL Six-Task Repair Pack 1.0; pack manifest verified, 21 of 21 files)
- Repository: /mnt/F/LCL
- Branch: main
- Entry HEAD: 7d100b7e4573cb9bad78649aca15eaf1ad32d044 (`LCL repair task1`). It is the pack baseline `7ed84ba` plus the owner's commit of LCL-REPAIR-01, which holds exactly that task's 6 changed files and its result.
- Exit HEAD: 7d100b7e4573cb9bad78649aca15eaf1ad32d044
- Entry worktree: clean
- Exit worktree: the 22 modified tracked files listed below, plus 2 new files: `reports/LCL_Core_0.1.0_ERRATUM_2026-09-15.md` and this report. No other tracked or untracked change.
- Git writes performed: **NO**
- Background/sub/parallel agents used: **NO**

## Result

**Status:** PASS WITH OUT-OF-SCOPE FINDINGS

## The meaning of `lcl version`

`lcl version` prints the tool version, the engine protocol and one `language` line for each LCL Core version that a command's engines judge documents under. It uses the rule `engines()` in `impl/crates/lcl-cli/src/main.rs` already applies:

- Core 0.1.0 is always listed.
- Core 0.2.0 is listed only when a Core 0.2.0 package is named to the invocation, by `--localized-spec` or `LCL_LOCALIZED_SPEC`, and that package opens against the 0.2.0 trust anchor.
- A named package that does not open is refused with exit 4 before anything is printed, as every command refuses it.
- `version` parses options as `spec` and `syntax` do, so a stray argument is a usage error (exit 3) instead of being ignored.
- No project manifest is read. `version` acts on no document or project, so its output does not depend on the working directory.

Grounds:
- Neither canonical package has a rule for an implementation's version output; both were searched.
- The existing contract, in the `impl/Cargo.toml` product-version comment, keeps the product version separate from the language version it states.
- `engines()` already defines when Core 0.2.0 takes part. A 0.1-only installation names no Core 0.2.0 package, so it no longer claims one.

Release provenance uses the same meaning. `packaging/build_release.sh` asks the built tool with exactly the packages the payload carries and with no inherited `LCL_LOCALIZED_SPEC`. The answer must equal what the payload carries, `0.1.0` or `0.1.0 0.2.0`; any other set refuses the candidate.

## Scoped findings

| Finding | Reproduction | Repair | Verification | Disposition |
|---|---|---|---|---|
| B-05: `lcl version` advertises 0.2.0 unconditionally | Real binary (`R2-B05-RED.log`): with nothing named it printed `language 0.2.0`, so the `build_release.sh` scrape gave `0.1.0 0.2.0`. Naming the 0.1.0 package as the localized spec still claimed 0.2.0 with exit 0, and a trailing `--localized-spec /nonexistent` was ignored. CLI tests RED: `commands_and_exit_codes.rs:41` and `localized_cli.rs:60` (exit 0, expected 4). Release tests RED against the unrepaired script: an inherited `LCL_LOCALIZED_SPEC` recorded `0.1.0 0.2.0`; a tool claiming 0.2.0 was accepted; with a tool following the meaning above, a 0.2.0 candidate recorded only `0.1.0` | `Command::Version(Common)` through `only_options`. The version arm opens a named Core 0.2.0 package, then prints; `open_localized` is extracted from `engines()` with the same error text, and both use it. `build_release.sh` asks with the payload's packages and refuses a mismatch | lcl-cli 63 passed, 0 failed. `release_build` 10 passed. Command level (`R2-B05-GREEN-cmd.log`): nothing named gives 0.1.0 only; the 0.2.0 package gives both; the 0.1.0 package or a missing path gives exit 4; `version extra` gives exit 3. The real binary under the script's exact command forms (`R2-B05-PROV-realforms.log`) | FIXED_VERIFIED |
| B-06: implementation documentation drift | `impl/Cargo.toml` described a Core 0.1.0-only consumer and said `lcl version` prints "on three lines". The `impl/README.md` crate table held 15 of 18 members (no `lcl-capabilities`, `lcl-stdlib` or `lcl-localization`) and never mentioned localization. The lexer, parser, resolver, protocol and project crates implement Core 0.2.0 behaviour, but their descriptions named Core 0.1.0 only. The `version` help said "print the tool and protocol versions" | Workspace header; an LCL-FEATURE-04 scope paragraph; the product-version comment. README title, intro, 3 new crate rows, and Core 0.2.0 clauses on the protocol, project and CLI rows. 5 crate descriptions, the help text, and the `packaging/README.md` version line plus a version-and-provenance note | `R2-B06-consistency.log` and `R2-B06-consistency-2.log`: 0 hits for each repaired phrase (the one intro match is the repaired sentence itself); 18 of 18 members in the table; no 0.1-only description left in a crate whose source mentions localization. `cargo metadata` parses 18 packages, and `Cargo.lock` is unchanged | FIXED_VERIFIED |
| B-07: stale task and release state | `LCL-FEATURE-04_RESULT.md` says "This record covers phases A, B and C" and "Current state: uncommitted phase B and C changes", and its final closure still asks whether to commit. `LCL-CLOSE-03_RESULT.md` and `LCL_RESIDUAL_REPAIR_REPORT.md` end with "LCL-FEATURE-04 stays locked". All were superseded by `a826716`, the owner's 2026-09-15 "finished" statement and `7ed84ba` | One dated reconciliation addendum appended to each record. Each names the superseding commits and keeps open what is open: no desktop-menu launch result is recorded, and the 0.2.0 independent review is pending | `R2-B07-verify.log`: each committed file is an unchanged byte prefix of the new one; lines added 14, 9 and 12, none deleted; no control characters | FIXED_VERIFIED |
| B-08: machine-specific application manifests | `R2-B08-RED.log`: a copy of `apps/small-invoice-total` and its package, moved to scratch, loaded `/mnt/F/LCL/canonical/LCL_Core_0.1.0`. Tests RED: `every_application_names_its_package_relative_to_its_root` (line 187) and `a_moved_checkout_loads_its_own_package` (line 237) | The 4 manifests name `"spec": "../../canonical/LCL_Core_0.1.0"`, which `lcl-project` resolves against the project root (`Project::resolve`). No discovery was added | `applications` 8 passed: the 6 existing application tests now load their package through the relative path, and the moved copy loads its own package | FIXED_VERIFIED |
| B-09: 0.1 archive-name contradiction | `VERSION.txt:11` names `LCL_Core_0.1.0_Final.zip`. `README.txt:43–44`, `00_RELEASE/01_RELEASE_STATUS_AND_BOUNDARY.txt:18–21` and `VALIDATION_REPORT.txt:111–118` name `LCL_Core_0.1.0_Bare_Language_2026-09-05.zip` as the current archive. The 2026-09-13 erratum covers counts only. The `VERSION.txt` inside each archive carries the stale line | New external, non-normative erratum `reports/LCL_Core_0.1.0_ERRATUM_2026-09-15.md`. No canonical byte changed | `R2-B09-verify.log`: all 10 cited SHA-256 values match; the quoted lines match; the `archive-verification.json` facts are confirmed. `lcl spec` reports identity `00d648b1…67ed` as authoritative, and 175 checksums verify | FIXED_VERIFIED (external reconciliation) |

## Files changed

| File | Why | Minimal change |
|---|---|---|
| impl/crates/lcl-cli/src/main.rs | B-05 | Version arm opens a named Core 0.2.0 package before printing; `open_localized` helper shared with `engines()` |
| impl/crates/lcl-cli/src/args.rs | B-05, B-06 | `Version(Common)` via `only_options`; help text |
| impl/crates/lcl-cli/tests/commands_and_exit_codes.rs | B-05 regression | No 0.2.0 claim with nothing named; `version extra` is a usage error |
| impl/crates/lcl-cli/tests/localized_cli.rs | B-05 regression | New `version_names_core_0_2_0_only_when_its_package_is_named` |
| packaging/build_release.sh | B-05 | Provenance asks with the payload's packages and no inherited `LCL_LOCALIZED_SPEC`, and refuses any other language set |
| impl/crates/lcl-hardening/tests/release_build.rs | B-05 regression | The stub `lcl` follows the version meaning, `STUB_LCL_CLAIMS_0_2_0` imitates an over-claiming tool, and the stub answers `check`. 3 new tests |
| apps/{small-invoice-total,medium-release-notes,large-release-pipeline,large-release-pipeline-decomposed}/lcl.project.json | B-08 | `spec` is `../../canonical/LCL_Core_0.1.0` |
| impl/crates/lcl-hardening/tests/applications.rs | B-08 regression | 2 new tests |
| impl/Cargo.toml | B-06 | Header, LCL-FEATURE-04 scope paragraph, product-version comment |
| impl/crates/{lcl-lexer,lcl-parser,lcl-resolver,lcl-protocol,lcl-project}/Cargo.toml | B-06 | `description` states the crate's Core 0.2.0 role |
| impl/README.md | B-06 | Title, intro, 3 crate rows, 3 row clauses |
| packaging/README.md | B-05, B-06 | `lcl version` line; version and provenance note |
| reports/tasks/LCL-FEATURE-04_RESULT.md, reports/tasks/LCL-CLOSE-03_RESULT.md, reports/implementation/LCL_RESIDUAL_REPAIR_REPORT.md | B-07 | One appended dated addendum each; nothing above it changed |
| reports/LCL_Core_0.1.0_ERRATUM_2026-09-15.md (new) | B-09 | External archive-name erratum |

## Evidence

Toolchain: cargo 1.98.1 and rustc 1.98.1 (Arch Linux rust 1:1.98.1-1, `/usr/bin`).
Environment: `CARGO_TARGET_DIR=/mnt/F/.lcl-closure-4t-4c1cd4c659b7/target-current` (the reused Task 4 cache) and `TMPDIR=/mnt/F/.lcl-repair-6t/tmp`.
Logs: `/mnt/F/.lcl-repair-6t/logs/R2-*`.

| Gate | Command/Test | Exit | Result |
|---|---|---:|---|
| R2-warm-build | `cargo build --offline --locked -p lcl-cli -p lcl-workspace --bins` | 0 | up to date |
| R2-B05-RED | real `lcl version` probes | 0 | defect reproduced (see B-05) |
| R2-B05-CLI-RED | `cargo test -p lcl-cli --test commands_and_exit_codes --test localized_cli -- version` | 101 | EXPECTED-FAIL at `commands_and_exit_codes.rs:41`; cargo stopped before the second target |
| R2-B05-CLI-RED-localized | `cargo test -p lcl-cli --test localized_cli -- version` | 101 | EXPECTED-FAIL at `localized_cli.rs:60` (left 0, right 4) |
| R2-B05-CLI-GREEN | `cargo test -p lcl-cli --no-fail-fast` | 0 | 63 passed, 0 failed (7 suites) |
| R2-B05-clippy-cli | `cargo clippy -p lcl-cli --all-targets -- -D warnings` | 0 | pass |
| R2-B05-fmt | `cargo fmt --all --check` | 0 | pass. The first attempt did not run: cargo found no manifest. It was rerun with `--manifest-path` |
| R2-B05-PROV-RED | `cargo test -p lcl-hardening --test release_build --no-fail-fast` | 101 | EXPECTED-FAIL: the 3 new tests failed at lines 607, 625 and 661; the 7 existing tests passed, so the stub change alone is neutral |
| R2-B05-PROV-fmt | `cargo fmt --all --check` | 1 | one formatting hunk in a new test; applied. Recheck (`-fmt2`) exit 0 |
| R2-B05-PROV-GREEN | `cargo test -p lcl-hardening --test release_build --no-fail-fast` | 0 | 10 passed |
| R2-B05-clippy-hardening | `cargo clippy -p lcl-hardening --all-targets -- -D warnings` | 0 | pass |
| sh -n | `packaging/build_release.sh` | 0 | pass |
| R2-B05-PROV-realforms | real binary under the script's command forms | 0 | an inherited 0.2.0 value is cleared, giving 0.1.0 only; `--localized-spec` gives both |
| R2-B08-RED | real `lcl check --machine --project <moved copy>` | 0 | loaded the `/mnt/F/LCL` package (reproduced) |
| R2-B08-RED.test | `cargo test -p lcl-hardening --test applications --no-fail-fast` | 101 | EXPECTED-FAIL: 2 new tests failed; 6 passed |
| R2-B08-GREEN | the same | 0 | 8 passed |
| R2-B08-clippy-hardening | `cargo clippy -p lcl-hardening --all-targets -- -D warnings` | 0 | pass |
| R2-B06-metadata, R2-B06-consistency, R2-B06-consistency-2 | `cargo metadata --no-deps`; phrase, table and description searches | 0 | see B-06 |
| R2-B09-verify | cited hashes, quoted lines, archive facts | 0 | 10 of 10 hashes match |
| R2-B07-verify | committed bytes are a prefix; `git diff --numstat` | 0 | 3 of 3 prefixes unchanged; 0 lines deleted |
| R2-INT-fmt | `cargo fmt --all --check` | 0 | pass |
| R2-INT-lcl-cli | `cargo test -p lcl-cli --no-fail-fast` | 0 | 63 passed, 0 failed (7 suites) |
| R2-INT-hardening | `cargo test -p lcl-hardening --test release_build --test applications --no-fail-fast` | 0 | 18 passed, 0 failed |
| R2-INT-clippy | `cargo clippy -p lcl-cli -p lcl-hardening --all-targets -- -D warnings` | 0 | pass |
| R2-INT-protected, R2-INT-scope | checksums, identities, Git state, files written since the first gate | 0 | see below |

Not run, by design:
- `installed_launcher.rs` and `packaging_smoke.rs`: `install.sh`, `uninstall.sh` and the launcher are unchanged, and `packaging_smoke.rs` installs the committed candidate tarball.
- A real release build. The stub-driven tests run the real `build_release.sh` end to end, and the real binary was checked under the script's exact command forms. A fresh candidate belongs to a later release step.
- The lcl-project, lcl-protocol and lcl-workspace suites: their code is unchanged. Only crate descriptions changed, and the app manifests are exercised by `applications`.
- The Rust 1.75 minimum-version gate and the broad workspace gates belong to REPAIR-06.

## Protected-material check

- Core 0.1.0: no tracked change and no untracked file. `SHA256SUMS.txt` verifies all 175 entries, and `lcl spec` reports identity `00d648b162939d06c44838481a67c39bc12c64bdd6d105035c24150148fe67ed` as authoritative.
- Core 0.2.0: no tracked change and no untracked file. `SHA256SUMS.txt` verifies all 215 entries, and the tool reports identity `e86121c734cb51065b791329738b9ed6b77c53e03e0eebbb2ad1076bea973ef0`. `independent_review` is still `pending`, and the release state is unchanged (UNRELEASED_CANDIDATE).
- Existing candidates: `releases/` has no status entry against HEAD, and nothing was rebuilt. Their recorded provenance is not false: both 0.1.0 candidates record `language version: 0.1.0`, and the 0.2.0 candidate records `0.1.0 0.2.0` and carries both packages.
- Historical evidence: `reports/` changed only by the 3 prefix-verified appended addenda and 2 new files; `assets/` is unchanged.
- Unexpected writes: none to the tracked tree. The gates wrote into the Git-ignored `impl/target/test-tmp` and into `/tmp/lcl-apps` (OOS-2). Scratch kept as evidence: `/mnt/F/.lcl-repair-6t/tmp/r2-moved.W0aTx2` (the B-08 reproduction copy) and `r2-metadata.json`.

## Acceptance criteria

| Criterion | Met by |
|---|---|
| Version reporting has one explicit, test-backed meaning | The meaning above; the 2 CLI tests |
| Release provenance cannot claim unavailable language support | The refusal in `build_release.sh`; `a_tool_claiming_a_language_its_payload_lacks_is_refused`; `an_inherited_localized_spec_adds_no_language_to_a_0_1_0_candidate` |
| Active docs match current crate, version and localization behaviour | B-06 |
| Checked-in app manifests are portable | B-08; `a_moved_checkout_loads_its_own_package` |
| Historical state is preserved, not falsified | B-07 addenda; prefix verification |
| 0.1 archive naming is externally reconciled without protected-byte edits | B-09 erratum |
| No canonical or existing candidate bytes changed | Protected-material check |

## Out-of-scope findings

- **OOS-1: a relative document path with a directory part is not found** (`R2-OOS-relative-document.log`).
  - From a project root with `LCL_SPEC` set, `lcl check src/main.lcl` reads `…/src/src/main.lcl` and exits 4.
  - From the repository root, `lcl check --spec <absolute> apps/small-invoice-total/src/main.lcl` reads `…/src/apps/small-invoice-total/src/main.lcl`.
  - A bare file name works, and so does `lcl check --project . src/main.lcl`.
  - `packaging/README.md` documents the failing form (`lcl check src/main.lcl`, and likewise `validate` and `run`).
  - It predates this task: this task's `main.rs` diff touches only the version arm and the extracted helper.
  - Affects project and document resolution in `impl/crates/lcl-cli/src/main.rs`, and the README examples. Fixing it is a CLI path-semantics decision, not documentation hygiene.
- **OOS-2: test scratch leaves the configured `TMPDIR`.**
  - The lcl-cli suite writes under `impl/target/test-tmp` (73 of its 103 subdirectories were touched by this task's runs), through `impl/crates/lcl-cli/tests/common/mod.rs:61`. `lcl-project`'s test helper and `lcl-hardening/tests/protocol_and_tooling.rs` use the same convention. This is known finding B-13, assigned to REPAIR-06.
  - Also observed: `lcl-hardening/tests/applications.rs` writes a fixed `/tmp/lcl-apps` whatever `TMPDIR` says. It belongs with B-13.
- **OOS-3: the existing candidates carry the unrepaired `lcl version`.** The `bin/lcl` packaged in `lcl-0.2.0-linux-x86_64-a0006c38fb79` (tarball checksum OK), extracted to scratch and run with an empty environment, lists 0.2.0 with no package named (`R2-final-check.log`). That payload carries both packages, so its provenance is correct. The candidates are protected and were not rebuilt.

## Remaining blockers

- None.

## Owner actions

- O-03: review and commit decisions for the 22 modified files, the new erratum and this report.
- O-01 and O-02 are unchanged: the Core 0.2.0 independent review is pending, and no desktop-menu launch result is recorded for either candidate.
- Decide OOS-1: the relative document path semantics, or the README examples.

## Quota/efficiency notes

- Reused:
  - the `engines()` rule, `localized_spec_root`, `only_options`, and `Engine::open_localized` behind one extracted helper;
  - the `release_build.rs` stub harness (`Case`, `origin`, `build`);
  - the `applications.rs` helpers;
  - `lcl-project`'s existing root-relative resolution;
  - the Task 4 compile cache.
- RED and GREEN runs were targeted. The integration gate ran once. No real release build was made, and broad gates are deferred to REPAIR-06.
- Documentation was repaired by searching for the known stale claims. History got short appended addenda, not rewrites.

## Handoff

The task passed its acceptance contract. LCL-REPAIR-03 is unlocked.

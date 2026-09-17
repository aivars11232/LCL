# PRETEST-03 — Result

## Identity

- Task: PRETEST-03, Core 0.2 localization and localization hardening (F09–F13)
- Pack: `/mnt/F/LCL_PreTesting_Closure_Pack_v2_c50618a/`. All 20 `machine/SHA256SUMS.txt` entries verified.
- Repository: `/mnt/F/LCL`, branch `main`
- Entry HEAD: `6620886ea5bb96850489a8dccee9e30cf02ec864` ("LCL pretest task2"), the owner's commit of PRETEST-02
- Entry worktree: clean
- Exit HEAD: unchanged
- Exit worktree: 14 modified tracked files and this report. `canonical/LCL_Core_0.1.0/` and `releases/` are unchanged.
- Git writes: **NO**. Publication: **NO**. Background, sub or parallel agents: **NO**. Network or new dependencies: **NO**.
- Evidence logs: `/mnt/F/.lcl-pretest/logs/P3-*`
- Scratch:
  - `/mnt/F/.lcl-pretest/p3/`: `env.sh`, `p3-gate.sh`, `f11_probe.py`, and `identity.py`, which reproduces `lcl_spec::compute_identity_digest`. It was cross-checked against the HEAD package and reproduced `e86121c7…`.
  - `TMPDIR=/tmp/lcl-pretest-03`
  - `CARGO_TARGET_DIR` is the existing isolated `/mnt/F/.lcl-closure-4t-4c1cd4c659b7/target-current`

## Result

**Status:** PASS WITH ONE OWNER DECISION (F10)

- F09, F11, F12 and F13 are FIXED_VERIFIED with red-first regressions.
- F10 is STOPPED as a normative ambiguity. See the section below. No lossy decoding is used anywhere, and the current attribution is pinned by a test so that any change must be deliberate.
- The Core 0.2 candidate changed in one tool. It was regenerated through its own process, and it is internally consistent under a new candidate identity.
- The independent review is still pending, and `release_gate_permitted` is still false.

## Core 0.2 identity

| | Value |
|---|---|
| Old identity (audit baseline, LCL-FEATURE-04) | `e86121c734cb51065b791329738b9ed6b77c53e03e0eebbb2ad1076bea973ef0` |
| New identity | `6e7303157f5ba378b4e8c0b89852a7a81b00b6dd26b85cac36a8977532690ceb` |
| Package file count | 216, unchanged. The manifest has 213 records and the checksums have 215. |
| Old `MANIFEST.json` SHA-256 | `9196a29580a9c720421d606720b01c3449bbc131539abc15a88aa07809820172` |
| New `MANIFEST.json` SHA-256 | `2e029a0ce19bc6a5a453659abd351054fdd6a646d339dbdf3fd511e0ee968357` |
| Manifest status | `localization_feature_candidate`, `release_ready: false`, unchanged |
| Compiled anchor | `lcl_spec::anchor::APPROVED_PACKAGE_0_2_0.identity_digest` now holds the new identity. Its doc comment records the old identity. |

Exact candidate files changed (SHA-256 prefix, old → new):

| File | Old | New | Why |
|---|---|---|---|
| `09_CONFORMANCE/TOOLS/validate_localization.py` | `cef525de0a824919` | `92da47ba9df052f5` | F11 repair and its self-check |
| `CHANGELOG.txt` | `c731b161c0fad720` | `90e7a351b1402e20` | PRETEST-03 entry |
| `MANIFEST.json` | `9196a29580a9c720` | `2e029a0ce19bc6a5` | regenerated (`generate_integrity.py manifest --status localization_feature_candidate`) |
| `VALIDATION_REPORT.txt` | `3f96cef8900e3de1` | `f3f5d5347d6fa40e` | Rebound to the new manifest hash, with a new snapshot date and UTC. The localization line names the new check, and the gate-9 pointer names this report. |
| `SHA256SUMS.txt` | `4fa78962bca8a7d4` | `042b1130ede70092` | regenerated (`generate_integrity.py checksum`) |

No normative text, registry, fixture or `00_RELEASE/05_LANGUAGE_CLOSURE.json` byte changed.

**Why the candidate remains unreleased:** the independent review of the localization additions is still pending. `validate_release.py --scope all` therefore stays BLOCKED on `language_decisions_and_release_state` (`pending_decisions: ["independent_review"]`), and `release_gate_permitted` stays false. Tests passing against the implementation are not an independent review, so this task has no authority to publish the package or open the release gate. Historical reports and `releases/candidates/` still cite the old identity, and they were not rewritten.

## Root causes and repairs

### F09 — a selected profile established 0.2 authority

- **Root cause:** `Engines::engine_for` (`lcl-protocol/src/engine.rs`) sent an accepted localized reading with no readable `VERSION` to the 0.2.0 engine whenever a profile had been selected (`None if outcome.profile.is_some() => localized`). This conflicts with `02_LEXICAL/13` ("These rules apply only to a document whose LCL block declares VERSION "0.2.0"") and `07_VERSIONING_AND_EXTENSIONS/01` ("localized spelling requires VERSION "0.2.0"").
- **Red:** `P3-A-dispatch-probe.log`. A localized document whose `VERSIJA` string is unclosed went to 0.2.0 with `error.literal.unclosed`. A localized document with no `LCL` block went to 0.2.0 with `error.block.context`.
- **Repair:** an accepted localized reading goes to the 0.2.0 engine only when it declares exactly `0.2.0`. Owner decision D9 for *rejected* localizations is unchanged.
- **Green:** `P3-A-dispatch-green.log`. Both documents now stay with Core 0.1.0 (`error.keyword.unknown`).

### F10 — invalid UTF-8 version attribution (STOPPED: normative ambiguity)

The evidence:

- `01_FOUNDATION/03` (0.2.0) orders step 1 "Decode UTF-8, apply the localization stage … validate line/indentation rules" before step 4 "Resolve exact LCL version". A unit rejected at step 1 never reaches the step that resolves its version.
- `02_LEXICAL/01` forbids invalid UTF-8. `02_LEXICAL/13` and `07/01` attach 0.2.0 rules to a document that *declares* `VERSION "0.2.0"`. Neither says which package owns a unit that is rejected before its declaration is resolved.
- The existing "readable before the first defect" boundary is an implementation reading recorded in LCL-REPAIR-03 ("it adds no language rule … the owner may still prefer another boundary"). It already reads `VERSION` ahead of *other* step-1 defects, such as a tab after `VERSION`. Applying it to bytes would mean decoding a valid prefix of a unit that is not UTF-8. Canon neither requires nor forbids that. Reading 1 treats decoding as atomic for the unit: no characters, so no declaration, so Core 0.1.0. Reading 2 decodes the valid prefix, so a header before the bad byte declares 0.2.0. Both readings are consistent with the text, so canon does not uniquely determine the attribution.

What is guaranteed now, under either reading:

- There is no lossy decoding. The Rust lexer reports `error.encoding.invalid` at `valid_up_to()` of the original bytes and yields no tokens. Localization selects nothing for non-UTF-8 bytes. The Core 0.2 reference tool now does the same (F11).
- Current attribution is pinned in the dispatch matrix. Invalid UTF-8 before *and* after `VERSION` goes to Core 0.1.0 with `error.encoding.invalid`. The D9 directive rule still applies when the raw bytes begin with an `@locale` line.

**Owner decision needed:** keep Reading 1 (the current behaviour, no code change) or adopt Reading 2 (only these two matrix rows would change).

### F11 — reference localization validator used replacement decoding

- **Root cause:** `evaluate_source` in `TOOLS/validate_localization.py` decoded with `errors="replace"`. Invalid UTF-8 was therefore accepted as localized source, and replacement characters, 3 bytes each, could shift the offsets `byte_offsets` derived.
- **Red:** `P3-F11-red.log`. `f11_probe.py` fed bytes invalid before `VERSIJA`, after it, and after a directive. All three returned success with no error.
- **Repair:** strict decoding. A `UnicodeDecodeError` returns `error.encoding.invalid` at `error.start`, the first offending byte of the original data. This matches `validate_source_fixtures.py` and the Rust lexer. A durable self-check, `invalid_utf8_rejected_on_original_bytes`, covers the same three cases with expected offsets 0, 26 and 14. `validate_release.py` consumes only `passed` and `counts`, so the added check leaves every count unchanged.
- **Green:** `P3-F11-green.log` and `P3-F11-validate_localization.log`, followed by regeneration.

### F12 — profile reads unbounded, profile count unbounded

- **Root cause:** `Engine::open_localized`, the one path both the CLI and the workspace use to build a localized engine, read every profile with `std::fs::read`, so the whole file was allocated before `validate_profile` checked its size. There was no ceiling on the number of files. A bounded read already existed inside `DirectoryResolver::resolve`.
- **Red:** `P3-C-engine-red.log`, run under `ulimit -v 4000000`. A `lv-LV.json` symlinked to `/dev/zero` failed with `out of memory`, and 257 profile files were accepted.
- **Repair:** the existing bounded read became `lcl_localization::read_profile_file`, which reads at most `MAX_PROFILE_BYTES + 1`. `DirectoryResolver::resolve` and `Engine::open_localized` now both use it. There is no second implementation. `MAX_PROFILE_FILES = 256` is documented as a host/product limit, not a language rule. Exceeding it refuses engine construction (`EngineError::Contracts`, which is the CLI's environment exit) and produces no language diagnostic.
- **Green:** `P3-C-engine-green.log`. The endless file becomes `error.localization.profile_invalid` at the localization stage, and 257 files are refused with "host limit".

### F13 — `--profile` silently ignored without a localized package

- **Root cause:** when no localized package was named, the CLI's `engines()` returned before it read `common.profiles`, and `version` opened the localized package with `&[]`. The workspace's `open_with_profiles` also dropped `profiles` when no localized package applied.
- **Red:** `P3-D-cli-red.log` (`check --profile` exited 1 with Core 0.1.0 diagnostics) and `P3-D-ws-red.log` (the workspace opened).
- **Repair:** the CLI's `refuse_inactive_profiles` returns a usage error (exit 3) that names `--profile` and the three ways to name a localized package. It is used by `engines()` and by `version`, and `version` now opens the profiles it is given. The workspace refuses with the same guidance (`WorkspaceError::Spec`).
- **Green:** `P3-D-cli-green.log` and `P3-D-ws-green.log`.

## Tests added or changed

| File | Test | Covers |
|---|---|---|
| `lcl-protocol/tests/dispatch.rs` | 7 new rows in `the_dispatch_matrix_attributes_each_document_to_one_package` | F09: unreadable or missing localized `VERSION`, localized `0.1.0`, malformed localized and canonical `VERSION`. F10: invalid UTF-8 before and after `VERSION`. Existing rows already cover canonical 0.1, canonical 0.2, localized 0.2 and no profiles. `without_a_localized_engine_everything_is_core` covers the case with no localized package. |
| `lcl-protocol/tests/localized_engine.rs` | `a_profile_file_is_bounded_before_it_is_allocated`, `the_profile_file_count_has_a_host_ceiling` | F12 |
| `lcl-cli/tests/localized_cli.rs` | `a_profile_without_a_localized_package_is_a_usage_error` | F13 (`check`, `run`, `version`, and an unreadable profile with `version`) |
| `lcl-workspace/tests/localized.rs` | `a_profile_file_without_a_localized_package_is_refused` | F13 |
| `canonical/LCL_Core_0.2.0/09_CONFORMANCE/TOOLS/validate_localization.py` | self-check `invalid_utf8_rejected_on_original_bytes` | F11 |

No existing expectation was weakened or reclassified.

## Files changed

- `canonical/LCL_Core_0.2.0/09_CONFORMANCE/TOOLS/validate_localization.py`, `CHANGELOG.txt`, `MANIFEST.json`, `SHA256SUMS.txt`, `VALIDATION_REPORT.txt`
- `impl/crates/lcl-spec/src/anchor.rs`: the 0.2.0 anchor digest and its doc comment
- `impl/crates/lcl-protocol/src/engine.rs`: `engine_for` (F09); `open_localized` bounded read and count ceiling (F12)
- `impl/crates/lcl-localization/src/lib.rs`: `MAX_PROFILE_FILES`, `read_profile_file`, and `DirectoryResolver::resolve` reusing it
- `impl/crates/lcl-cli/src/main.rs`: `refuse_inactive_profiles`, and `version` opens the given profiles
- `impl/crates/lcl-workspace/src/project.rs`: refuses profiles without a localized package
- Tests: `lcl-protocol/tests/{dispatch,localized_engine}.rs`, `lcl-cli/tests/localized_cli.rs`, `lcl-workspace/tests/localized.rs`
- `reports/tasks/PRETEST-03_RESULT.md` (new)

## Commands and exit results

Final gate: `/mnt/F/.lcl-pretest/p3/p3-gate.sh`, summary in `P3-E-gate-summary.log`.

| Step | Command | Exit | Result |
|---|---|---|---|
| fmt | `cargo fmt --all -- --check` | 0 | |
| check | `cargo check --offline --locked --workspace --all-targets` | 0 | |
| clippy | `cargo clippy -p lcl-localization -p lcl-protocol -p lcl-spec -p lcl-cli -p lcl-workspace --all-targets -D warnings` | 0 | |
| localization + spec | `cargo test -p lcl-localization -p lcl-spec` | 0 | 8 targets, 46 passed, 0 failed (includes `package_0_2_0` anchor tests) |
| lexer/parser localized | `cargo test -p lcl-lexer --test localized_words -p lcl-parser --test localized_ast` | 0 | 2 targets, 6 passed, 0 failed |
| products | `cargo test -p lcl-protocol -p lcl-cli -p lcl-workspace` | 0 | 30 targets, 261 passed, 0 failed |
| other anchor user | `cargo test -p lcl-diagnostics --test registered_model` | 0 | 10 passed |
| MSRV | Rust 1.75.0 `cargo check --workspace --all-targets` | 0 | |
| Core 0.1 checksums | `sha256sum -c --strict --quiet` | 0 | |
| Core 0.2 checksums | `sha256sum -c --strict --quiet` | 0 | 215 entries |
| Core 0.2 localization tool | `validate_localization.py --root …` | 0 | 3 checks pass, 4 profiles, 21 mutations, 31 sources, 0 failed |
| Core 0.2 release tool | `validate_release.py --scope all` | 1 | PASS 31, OUT_OF_SCOPE 2, BLOCKED 1 (`independent_review` pending). Identical to the baseline `P3-base-validate_release.log` and to REPAIR-06. |
| Core 0.2 identity | `identity.py` | 0 | `6e730315…90ceb`, 216 files |

Earlier red and green runs are listed in each finding above.

## Protected material

- `canonical/LCL_Core_0.1.0/`: 0 changes, and its checksums verify.
- `releases/`: 0 changes.
- Core 0.2 independent review: pending. `release_gate_permitted`: false. `release_ready`: false.

## Out-of-scope findings, recorded rather than fixed

1. **CLI option scope (PRETEST-04):** `spec`, `syntax`, `package list` and `package vendor` accept every common option, including `--profile` and `--input`, and ignore the ones they do not use. This is a general CLI option-truthfulness issue, not localization activation.
2. **Manifest `profiles` without `localized_spec` (PRETEST-04):** a project manifest that declares a profile directory but no localized package leaves the directory unused without saying so. This is project configuration rather than an explicit `--profile`.
3. **`DirectoryResolver` (library only):** it now has the bounded read, but `available_locales` has no count ceiling. No product uses it.
4. **`Project::profile_files`** lists the directory's paths before the count ceiling applies. Only paths are held; no profile bytes are read before the ceiling refuses.

## Residual risks

- F10 is open until the owner chooses Reading 1 or Reading 2.
- The anchor `label` still reads "Localization Feature Candidate (2026-09-15)". Only its digest and doc comment changed.
- The value `MAX_PROFILE_FILES = 256` is a product choice and can be revised without any language impact.

## Proposed commit message

```
LCL pretest task3

PRETEST-03: Core 0.2 localization hardening (F09-F13)

- F09: a selected locale profile no longer establishes 0.2.0 authority;
  only a declared VERSION "0.2.0" does (Engines::engine_for).
- F10: invalid UTF-8 attribution recorded as a normative ambiguity for
  owner decision; no lossy decoding; current attribution pinned.
- F11: Core 0.2 validate_localization.py decodes strictly and reports
  error.encoding.invalid at original byte offsets; integrity regenerated;
  candidate identity e86121c7... -> 6e730315...; anchor updated;
  independent review still pending, release gate false.
- F12: bounded profile reads shared via read_profile_file; host ceiling
  MAX_PROFILE_FILES on profile files per engine.
- F13: --profile / workspace profile files without a localized package
  are refused instead of silently ignored.
```

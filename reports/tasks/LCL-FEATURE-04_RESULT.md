# LCL-FEATURE-04 — Automatic multilingual source localization — task result

## Identity and authorization

- Package: LCL-CLOSURE-4T v1.1, task **LCL-FEATURE-04**. This record covers phases A, B and C.
- Report date: 2026-09-15, Europe/Riga (system local).
- Repository: `/mnt/F/LCL`, branch `main`.
  - Entry state: HEAD `a826716` ("LCL task3 finish"), clean.
  - Current state: uncommitted phase B and C changes, listed in the phase C continuation and the handoff.
- This task made no Git write.
- Baseline: on 2026-09-15 the owner declared LCL-CLOSE-03 finished. That statement is taken as acceptance of `releases/candidates/lcl-0.1.0-linux-x86_64-b4506c8aa3d4/` as the frozen LCL 0.1 baseline. `reports/tasks/LCL-CLOSE-03_RESULT.md` was not changed, and it still records the desktop-menu check as the owner's.
- Approvals, given on 2026-09-15 as "All is approved" in reply to the Phase A plan:
  - **D1** the new version is 0.2.0, as `canonical/LCL_Core_0.2.0/`, with 0.1.0 untouched;
  - **D2** one scripted copy and relabel creates the package, proven by a reversal check, and later normative edits go one file at a time;
  - **D3** a new trust anchor for the finished 0.2.0 package;
  - **D4** exact code-point matching with only the standard library and no normalization dependency;
  - **D5** a new internal crate, `lcl-localization`;
  - **D6** `lcl-lock/2`, written only for localized units;
  - **D7** no locale-transform feature and no CLI locale override;
  - **D8** the new candidate bundles both packages, and `build_release.sh` gains a version parameter.
- Tools: recorded per gate. Phase C used rustc and cargo 1.98.1 at `/usr/bin`, and rustc and cargo 1.75.0 from `/mnt/F/.lcl-residual-repair-01-lxd8dwu8/toolchains/1.75.0/bin` for T4-C-msrv-check.
- Evidence: `/mnt/F/.lcl-closure-4t-4c1cd4c659b7/logs/T4-*`. Each gate has a `.log`, an `.exit` and a `.record.json` (argv, cwd, environment, timestamps, real exit, log digest, and the worktree identity before and after). The scratch scripts named below are in the same scratch root.
- Next: owner decisions only. See the closure of the phase G continuation.

## Summary status

Task: **BLOCKED on owner decisions only**. Phases A to G are executed and every executable gate passes. What remains are the owner's independent review of the 0.2.0 additions and the manual desktop-menu launch of the 0.2.0 candidate. Phases C to G are recorded in the continuations at the end of this report.

- **Implementation correctness:** Phase C implemented the localization stage, the lexer word map and canonical parser words. Phase D integrated them into the resolver, protocol engine, project lock, CLI and workspace. Every test named in both continuations passes. Phase E ran the V07 cross-language matrix, found one parser defect and repaired it. Phase F proved dynamic profiles through the engine. Phase G passed the full workspace gates on Rust 1.98 and 1.75.0 and the 0.1 compatibility gates. It built the separate candidate `lcl-0.2.0-linux-x86_64-a0006c38fb79` and accepted it installed.
- **Conformance established:** none for 0.2.0. The 0.1.0 claim (`source_conforming`) is unchanged.
- **Canonical identity:**
  - 0.1.0 is unchanged: 192 protected files match, and identity `00d648b1…67ed` is not affected.
  - The 0.2.0 package identity under the implementation's `LCL-PACKAGE-IDENTITY-V1` algorithm is `e86121c734cb51065b791329738b9ed6b77c53e03e0eebbb2ad1076bea973ef0` (216 files), minted in phase C0 (T4-C0-mint-anchor) and pinned as `APPROVED_PACKAGE_0_2_0`.
- **0.2.0 package state:** `UNRELEASED_CANDIDATE`, with manifest status `localization_feature_candidate`.
  - `independent_review` is **pending** in `05_LANGUAGE_CLOSURE.json`, because no independent review of the localization additions has taken place, and the rules forbid review agents.
  - The final release gate is therefore BLOCKED by design, and no archive exists.
- **Publication:** NOT PERFORMED.

## Finding dispositions

| ID | Current applicability | Baseline evidence class | Actual reproduction / control | Disposition | Gate | Remaining condition |
|---|---|---|---|---|---|---|
| FEATURE-ML10N | Owner feature request | explicit_owner_requirement | Phase B contract and fixtures verified statically | OPEN | T4-B9b-* | Phases C–G: implementation, integration, the V07 matrix, the new candidate, and installed acceptance |

## Phase A — decisions from the read-only reconciliation

- **Version 0.2.0.** `00_RELEASE/04_CHANGE_CONTROL.txt` makes a backward-compatible addition a minor release, and `07/01` and `07/05` agree. Localized spelling is accepted only in documents declaring `VERSION "0.2.0"`, so every valid 0.1.0 program keeps its meaning.
- **Lexical inventory, from the registries:**
  - 141 reserved words in 19 categories;
  - 21 adopted symbols and 23 excluded lexemes;
  - 11 functions, 19 operators and 11 constructors;
  - 8 diagnostic stages after this task, 7 before.
- **Implementation seams found:**
  - `lcl-lexer` `scan.rs` word recognition and its non-ASCII rejection;
  - two parser spelling slices, at `block.rs:41` and `expr.rs:204`;
  - four lexer callers: protocol `engine.rs`, conformance `runner.rs`, parser `lib.rs` and workspace `intelligence.rs`;
  - `lcl-spec`, which pins 0.1.0 through `APPROVED_PACKAGE`, `PINNED_FORMAL_VERSION` and `REGISTRY_FILES`;
  - `lock.rs` (`lcl-lock/1`) and the content-addressed `cache.rs`.
- **The editor has no JavaScript lexer.** It paints the tokens the server returns.

## Changes, in actual order (all under `canonical/LCL_Core_0.2.0/`)

| Order | File | Change | Verification | Result |
|---|---|---|---|---|
| B1 | whole tree | Copied 173 files from 0.1.0, excluding the 3 integrity files. Renamed 14 files and relabelled 1,229 mentions from 0.1.0 to 0.2.0. Kept 24 mentions: archive names, frozen GLOB/REGEX profile identities, and history. | Reversal proof on every file; the 0.1.0 tree digest `f484f2c0…` is the same before and after | T4-B1-create-package 0 |
| B1 | TOOLS/validate_release.py | Re-pinned 17 hash constants that the relabel necessarily changed. See the oracle corrections below. | Validator; T4-B1-control-reversed | T4-B1-validate-repinned-2: 28 PASS; FAIL only in integrity/release |
| B2 E1 | 10_REGISTRIES/statuses_and_errors_v0.2.0.json | Added the `localization` stage first, with a `pre_effect` default. Added 9 `error.localization.*` identifiers, none handler-recoverable, `status.invalid`. Widened `error.keyword.unknown` and `error.source.non_ascii_outside_string`, and extended the compatibility note. | Strict JSON; 86 sorted errors | T4-B2-E1-errors 0 |
| B2 E2 | built_in_groups_and_results_v0.2.0.json | Added the 9 identifiers to the errors enum, and `localization` to `lifecycle_stages` | The group equals the registry; the stages equal `stage_order` | T4-B2-E2-groups 0 |
| B2 E3 | CASES/core_conformance_cases_v0.2.0.json | Appended ERROR-CONTRACT-0800 to -0808. Case count 808, `error_contract` 86. DIAGNOSTIC-POLICY-0796 now says "86 errors" and "eight stages". | Positional IDs; subjects equal the registry | T4-B2-E3-catalog 0 |
| B2 E4 | 06_STANDARD_LIBRARY/06 | Error count 77 changed to 86 | diff | verified |
| B2 E5 | 09_CONFORMANCE/01 | Counts 808/86, with the policy entries named by ID | diff | verified |
| B2 E6 | TOOLS/validate_release.py | Counts, stage lists, prose tokens and 3 re-derived pins | Controls below | T4-B2-E6-validate: 28 PASS |
| B2 E7 | 10_REGISTRIES/localization_surface_v0.2.0.json (new) | Generated from the registries: 141 `localized_lexeme` entries, symbols and structure invariant, 11 qualified domains display-only, 13 metadata vocabularies invariant, 8 user-data classes | Generator cross-checks; validator | T4-B2-E7-surface 0 |
| B2 E1b | statuses_and_errors_v0.2.0.json | Made `mixed` and `keyword.unknown` independent of other installed profiles | Strict JSON; validator | T4-B2-E1b-meanings 0 |
| B2 E8 | 10_REGISTRIES/locale_profile_schema_v0.2.0.json (new, ASCII) | Profile format, SHA-256 content identity, BCP 47 subset, explicit repertoires (latin 119, cyrillic 48, cjk_unified 20,992), spelling rules, 17 confusable pairs, directive, candidate words, selection, provider contract, source map, diagnostics | Every diagnostic is registered; validator | T4-B2-E8-profile-schema 0 |
| B3 | 02_LEXICAL/13_LOCALIZED_SOURCE_AND_LOCALE_DIRECTIVE.txt (new) | Normative prose summary of the two registries. A garbled sentence was corrected before verification. | ASCII, width 80, header; validator | T4-B3-lexical13-validate |
| B2 E8b | locale_profile_schema_v0.2.0.json | Candidate words: any maximal run, and runs containing ASCII lowercase keep the 0.1.0 rules | Strict ASCII JSON; validator | T4-B2-E8b-* |
| B4 | LOCALIZATION_FIXTURES/profiles/{lv-LV,nl-NL,ru-RU,zh-CN}.json (new) | 60 spellings each, exactly the reserved words of the three phase E programs; `provider_class` fixture; `complete: false` | `validate_profile` | T4-B4-profiles-validate 0 |
| B5 | TOOLS/validate_localization.py (new) | Surface re-derivation, schema checks, profile validation, 21 mutations, and pre-grammar selection over the fixtures | Checks and mutations | T4-B5-tool-checks 0 |
| B6 | LOCALIZATION_FIXTURES/sources/*.lcl (31) and expected_results.json (new) | Auto and explicit selection in 4 locales, canonical, ambiguous, mixed, failed, unavailable, directive errors, tag error, confusable, unknown, multi-byte offset, strings in other languages, pins | Full tool | T4-B6-validate-localization 0 |
| B7 | 02_LEXICAL/01 | Outside strings, only the directive and repertoire letters of candidate words become legal | diff; validator | verified |
| B8 | 06_STANDARD_LIBRARY/07; 01_FOUNDATION/03 (step numbering kept); 02_LEXICAL/02; 01_FOUNDATION/02; 05_SEMANTICS/09; 09_CONFORMANCE/01 | Stage lists, processing step 1, word boundaries, principle 2, pre-effect checks, and the localization tool paragraph | Each: diff and validator | verified, one file per gate |
| B8 | 00_RELEASE/01, 05, 00, 03, 04; 07/01; README; VERSION; CHANGELOG; INDEX | Candidate status, pending review, provenance, completeness criterion, change control, versioning note, the 0.2.0 changelog entry (history byte-identical), and 40 INDEX entries | Each: diff and validator | verified |
| B8 | TOOLS/generate_integrity.py | `localization_feature_candidate` status and 4 localization component counts | Dry `component_counts` run; validator | T4-B8-genint-validate |
| B8 | TOOLS/validate_release.py | Text-scope `localization_fixtures` check | PASS on the package; negative control FAIL | T4-B8-hook-validate, T4-B8-hook-negative 0 |
| B9 | MANIFEST.json, VALIDATION_REPORT.txt, SHA256SUMS.txt | Integrity metadata. First generation defective, then regenerated. See below. | Final gates | T4-B9b-* |

### Test-oracle corrections

1. **Relabel pins (17).**
   - `static_example_contracts`, `operation_contracts`, `set_and_sort_semantics` and `requirements_index_integrity` pin SHA-256 hashes of exact 0.1.0 bytes, which contain version strings.
   - **Control:** in a scratch copy with the relabel reversed, the pin failures disappeared (T4-B1-control-reversed). Each replaced constant was first recomputed with the validator's own functions over the protected 0.1.0 package and had to equal the pinned value.
     - T4-B1-repin: 15 replaced, 0 conflicts. The 3 constants left alone are unaffected, and their checks pass.
     - T4-B1-repin-2: the Task-0005 and Task-0006 subset hashes.
2. **Localization additions (3).**
   - The diagnostic-selection fingerprint, the failure-lifecycle fingerprint and the Task-0006 catalog hash changed with E1 and E3.
   - **Control:** reverting only the intended change in memory reproduced each old pin exactly, and each new value equals the validator's own reported actual value (T4-B2-E6-validator).

### Defect found and repaired within phase B

- **Symptom:** the first generation (T4-B9-*) passed every gate. Inspection then showed `MANIFEST.json` counts of 213/213/212 and a report stating 215/214.
- **Cause:** `generate_integrity.py` counts the files present when it runs. In 0.1.0 the old integrity files always existed during regeneration; this new tree had none.
- **Impact:** `lcl-spec` checks `package_file_count` and `checksum_record_count`, so it would have rejected the package. `validate_release.py` does not check those fields.
- **Repair:**
  1. The defective files were preserved in `logs/T4-B9-first-generation/`.
  2. The manifest was regenerated with all three integrity files present, giving 216/213/215.
  3. The report generator now refuses unless those counts describe the final package.
  4. The report and checksums were regenerated (T4-B9b-*).

## Verification records (final phase B state)

| Gate | Result |
|---|---|
| T4-B9b-validate-all | exit 1 as expected: 31 PASS, 0 FAIL, 2 OUT_OF_SCOPE, 1 BLOCKED. The BLOCKED check is `language_decisions_and_release_state`: `independent_review` pending, release gate not permitted, unreleased candidate, candidate manifest status. |
| T4-B9b-sha256sum | exit 0; `sha256sum -c --strict` over 215 lines |
| T4-B9b-localization | exit 0: surface and schema PASS, 4 profiles valid, 21 mutations as expected, 31 sources matched, 0 failed |
| T4-B9b-protected-after | exit 0: 192 of 192 protected files and 7 of 7 Task 3 candidate files match |
| Package | 216 files, all mode 644, no bytecode caches. `MANIFEST.json` sha256 `9196a295…0172`, bound by `VALIDATION_REPORT.txt`. |

The language-contract checker ran inside the validator: 514 checks, 0 violations, 66 descriptive witnesses, 0 executed LCL cases.

## Contract and preservation review

- **Canonical 0.1.0:** byte-identical (T4-B0-protected, T4-B9-protected-before, T4-B9b-protected-after).
- **Historical archives and candidates:** unchanged. The Task 3 candidate was added to Task 4's protected set (`t4-protected-extra.json`) because `starting-state.json` predates it.
- **Brand assets:** unchanged, as part of the 192 files.
- **Scope:** only the new `canonical/LCL_Core_0.2.0/` and this report were written.
- **Git and remote:** no action.
- **Owned scratch:** `t4-control-reversed/` and `t4-hook-negative/` were removed by exact verified path after their evidence was logged. No processes or servers were started.

## Closure and handoff

- **Remaining:** phases C–G are NOT_EXECUTED. The independent review of the 0.2.0 localization additions is the owner's decision, and it keeps the 0.2.0 release gate BLOCKED.
- **Next exact action, phase C0:** version-aware `lcl-spec` loading for 0.2.0 and the D3 trust anchor.
  - Read the `lcl-spec` `lib.rs` constants and loader, `lexicon.rs` `Lexicon::load`, and `obligations.rs:52`.
  - Mint the 0.2.0 identity with `cargo run --offline --locked -p lcl-spec --example mint_anchor -- /mnt/F/LCL/canonical/LCL_Core_0.2.0`.
  - Add the anchor and version-parameterized registry and catalog names, keeping 0.1.0's defaults byte-for-byte.
  - The lexicon loads only from an authoritative package, so this must precede C1 (`lcl-localization`), C2 (the lexer hook) and C3 (parser canonical words).
- **Reusable gates:** T4-B9b-*, which bind package manifest `9196a295…`. Any later package edit invalidates them, along with the manifest, report and checksums.
- **Handoff:** `/mnt/F/.lcl-closure-4t-4c1cd4c659b7/LCL_FEATURE_04_HANDOFF.txt`.

## Phase C continuation — 2026-09-15

Phase C is **complete**. It added the localized lexical front-end: the version-aware specification loader and 0.2.0 anchor, the localization stage, the lexer word map, version-aware diagnostic stages, and canonical parser words. Every change is uncommitted, and no Git write was made. The protected check after phase B was not repeated, because phase C touched no protected path and no canonical byte.

### Changes, in actual order

| Order | File | Change | Verification | Result |
|---|---|---|---|---|
| C0 | — | Minted the 0.2.0 identity with the implementation's `mint_anchor` | `e86121c734cb51065b791329738b9ed6b77c53e03e0eebbb2ad1076bea973ef0`, 216 files | T4-C0-mint-anchor 0 |
| C0.1 | impl/crates/lcl-spec/src/anchor.rs | `APPROVED_PACKAGE_0_2_0` (decision D3). It is not the default; `SpecPackage::open` stays pinned to 0.1.0. | `cargo check -p lcl-spec` | T4-C0.1-check-spec 0 |
| C0.2 | impl/crates/lcl-spec/tests/package_0_2_0.rs (new) | 4 tests for 0.2.0 under its anchor, the default pin, and anchor refusal | EXPECTED_REPRODUCTION: 1 passed, 3 failed on `ambiguous_replacements_v0.1.0.json` NotFound | T4-C0.2-red 101 |
| C0.3 | impl/crates/lcl-spec/src/lib.rs | `REGISTRY_FILES_0_2_0` (14 entries), `CATALOG_FILES_0_2_0`, and `package_files(version)`. Every other version keeps the 0.1.0 names. | `lcl-spec`: 18 unit + 8 + 4 + 8 passed; clippy | T4-C0.3-test-spec 0, T4-C0.3-clippy-spec 0 |
| C1.1 | impl/crates/lcl-localization/{Cargo.toml,src/lib.rs} (new) | `Contract::load`, `LocaleTag`, `validate_profile`, directive, words outside strings, and `localize`: explicit, then pinned, then auto, then canonical. Provider-neutral `LocaleDetector` and `LocaleProfileResolver`, with coverage detector and memory, directory and unavailable resolvers. | 3 unit tests; clippy | T4-C1.2-test-localization 0, T4-C1.2-clippy-localization 0 |
| C1.2 | impl/Cargo.toml, impl/Cargo.lock | Workspace member `lcl-localization`. `Cargo.lock` gained exactly one package entry, from one offline unlocked check. | `diff` of `Cargo.lock` | T4-C1.2-check-localization 0 |
| C1.3 | impl/crates/lcl-localization/tests/fixtures.rs (new) | Mirrors `validate_localization.py`: 4 profiles with identities equal to the Python-computed values, 31 sources, 21 mutations, and identical canonical word sequences for English, lv, nl, ru and zh | 4 passed. Clippy `type_complexity` in the new test was fixed with a type alias, then re-run. | T4-C1.3-fixtures-2 0, T4-C1.3-clippy-2 0 |
| C2.0 | impl/crates/lcl-lexer/src/scan.rs | The only `Token` struct literal replaced by `Token::new`; behaviour-neutral | Lexer 121 passed | T4-C2.0-test-lexer 0 |
| C2.1 | impl/crates/lcl-lexer/src/token.rs | `Token::canonical` field and `Token::word(source)` | Workspace check; lexer 121 passed | T4-C2.1-check-workspace 0, T4-C2.1-test-lexer 0 |
| C2.2 | impl/crates/lcl-lexer/src/lib.rs | `WordMap`: directive length, repertoire letters, spelling to canonical word | Check | T4-C2.2-check-lexer 0 |
| C2.3 | impl/crates/lcl-lexer/src/scan.rs | `lex_with`: skips the directive line, makes repertoire letters word scalars only under a map, maps candidate words to canonical `ReservedWord`s, reports other candidates as `error.keyword.unknown`, and keeps the 0.1.0 non-ASCII rule for non-candidate runs. Four keyword identity reads use `Token::word`. With no map, lexing is unchanged. | Lexer 121 passed | T4-C2.3-test-lexer 0 |
| C2.4 | impl/crates/lcl-lexer/src/lib.rs | `Lexer::lex_localized` | Check | T4-C2.4-check-lexer 0 |
| C2.5 | impl/crates/lcl-lexer/tests/localized_words.rs (new) | lv/nl/ru/zh documents lex to canonical token kinds and words with spans on the author's spellings; unknown words at offsets 220 and 226; no-map rejection; non-candidate run | **Unexpected failure**, 0 of 4: `Lexicon::load` on 0.2.0 failed with `StageOrderMismatch` | T4-C2.5-localized-words 101 |
| C2.6 | impl/crates/lcl-diagnostics/src/lib.rs | Diagnosis: the 7-stage `Stage::ORDER` was hard-coded. Added `Stage::Localization`. `ORDER` became the 8-stage 0.2.0 order, `ORDER_0_1_0` holds the original 7, `order_for(version)` chooses between them, and each registry's stage order and error stages are checked against its version's order. | Workspace check; diagnostics tests | T4-C2.6-check-workspace 0 |
| C2.6 | impl/crates/lcl-diagnostics/tests/registered_model.rs | Oracle correction below, plus a new test for the 0.2.0 registry: 8 stages with `localization` first, 86 errors, 9 localization-stage errors | Diagnostics 2 unit + 10 passed; localized words **4 of 4**; clippy for diagnostics and lexer | T4-C2.6-test-diagnostics-2 0, T4-C2.6-localized-words 0, T4-C2.6-clippy 0 |
| C3.1 | impl/crates/lcl-parser/src/block.rs | `word()`, the IF/FOR dispatch, ELSE and `expect_word` use `Token::word` | Parser 99 passed | T4-C3.1-test-parser 0 |
| C3.2 | impl/crates/lcl-parser/src/expr.rs | `word()`, the NOT prefix and type-expression words use `Token::word` | Parser 99 passed | T4-C3.2-test-parser 0 |
| C3.3 | impl/crates/lcl-parser/tests/localized_ast.rs (new) | The 8 localized fixtures parse to the canonical English AST once spans are removed. A duplicated TYPE keeps its canonical diagnostic identifier at the author's own bytes and spelling. | 2 passed; clippy | T4-C3.3-localized-ast 0, T4-C3.3-clippy-parser 0 |
| C close | workspace | Formatting check; actual Rust 1.75.0 `cargo check --locked --all-targets` of lcl-spec, lcl-diagnostics, lcl-lexer, lcl-parser and lcl-localization | cargo and rustc 1.75.0 from the private toolchain | T4-C-fmt 0, T4-C-msrv-check 0 |

### Test-oracle corrections in phase C

1. **`lcl-diagnostics/src/lib.rs` unit test.**
   - **Change:** `Stage::ORDER.len() == 7` became 8.
   - **Contract:** `statuses_and_errors_v0.2.0.json#/diagnostic_selection/stage_order` declares `localization` first.
   - **Control:** the added assertions `ORDER[1..] == ORDER_0_1_0` and `order_for("0.1.0") == ORDER_0_1_0`, and every 0.1.0 consumer still compiling and passing.
2. **`lcl-diagnostics/tests/registered_model.rs:35`.**
   - **Change:** a 0.1.0 registry's `stage_order()` is now compared with `Stage::ORDER_0_1_0` instead of the widened `Stage::ORDER`.
   - **Original assumption:** one stage order per build, which no longer holds.
   - **Control:** the same test's unchanged assertion of the exact seven registry stage names.

### Closure (supersedes the phase B closure above)

- **Remaining:** phases D to G are NOT_EXECUTED. Independent review of the 0.2.0 localization additions stays pending; that decision is the owner's.
- **Next exact action, phase D1:** engine integration.
  - Read `lcl-protocol/src/engine.rs` (`Engine::open`, `resolve`, `early_diagnostics`, `lex` at ~672), `lcl-resolver`'s use of lexer and parser, and `lcl-conformance/src/runner.rs:412`.
  - Design the dispatch: the 0.1 pipeline first and unchanged; the localized 0.2.0 pipeline used only when a document declares `VERSION "0.2.0"`. It runs `localize` with a `WordMap` built from `Contract` letter ranges and the selected profile, then `lex_localized`, then the same parser and later stages.
  - Localization diagnostics must enter the diagnostics model at `Stage::Localization`.
  - A `WordMap` builder is still needed. Either `lcl-localization` gains a dependency on `lcl-lexer`, which is one more lockfile edge to record, or the protocol builds the map. Decide at D1 entry.
- **Gates that must re-run after the next edit:** the consuming crates' tests. The final workspace, MSRV and package gates run in phase G.
- **Handoff:** `/mnt/F/.lcl-closure-4t-4c1cd4c659b7/LCL_FEATURE_04_HANDOFF.txt`.

## Phase D continuation — 2026-09-15

Phase D is **complete**. One engine path now judges localized documents in the resolver, protocol, project lock, CLI and workspace. Every change is uncommitted, and no Git write was made. After D4.9 the protected check matched 192 of 192 files, and the 7 Task 3 candidate files were unchanged.

### Owner decision D9 (2026-09-15)

When localization fails before `VERSION` is readable, the 0.2.0 localization diagnostic is the result if the file has an `@locale` line, or if the failure is anything other than `detection_failed` without a directive. Otherwise the Core 0.1.0 result stands. It is implemented as `lcl_localization::localization_decides` and used only by `Engines::engine_for`.

### Changes, in actual order

| Order | File | Change | Verification | Result |
|---|---|---|---|---|
| D1.1 | impl/crates/lcl-localization/src/lib.rs | `ErrorMetadata`, `Contract::error_metadata`, `letter_ranges` | Localization tests; clippy | T4-D1.1-test-localization 0, T4-D1.1-clippy-localization 0 |
| D1.1b | impl/crates/lcl-localization/src/lib.rs | `has_locale_directive`, `localization_decides` (D9). The first patch attempt was **refused**: rustfmt had reformatted the anchors, the command did not stop, and T4-D1.1b-check-workspace, -test-localization and -clippy-localization ran on an **unchanged** file. Those three exits are no-ops, not verification. The retry used exact anchors, anchor-count checks and fail-stop. | Workspace check; tests; clippy | T4-D1.1b-check-workspace-2 0, T4-D1.1b-test-localization-2 0, T4-D1.1b-clippy-localization-2 0 |
| D1.2 | impl/crates/lcl-resolver/{Cargo.toml,src/lib.rs}; impl/Cargo.lock | `LocalizationSetup`, `Resolver::with_localization`, public `stage` (localize, then `WordMap` lex, then parse), `declared_lcl_version`, `ResolvedUnit::localization`. Lock edge lcl-resolver → lcl-localization. | Offline check, then locked check; tests; clippy; workspace check | T4-D1.2a-check-resolver 0, T4-D1.2a-check-resolver-locked 0, T4-D1.2b-test-resolver 0, T4-D1.2b-clippy-resolver 0, T4-D1.2b-check-workspace 0 |
| D1.3 | impl/crates/lcl-protocol/{Cargo.toml,src/engine.rs}; impl/Cargo.lock | `Engine::with_localization`, `with_locale_pins`, `localization_contract`, `stage`, private `resolver()`; `Stage::Localization` diagnostics reach `Reached::Lexical` | Protocol tests. Clippy first failed on an unused `Lexer` import, which was removed and re-run. | T4-D1.3a-check-protocol 0, T4-D1.3a-check-protocol-locked 0, T4-D1.3-patch 0, T4-D1.3-test-protocol 0, T4-D1.3-clippy-protocol **101**, T4-D1.3-test-protocol-2 0, T4-D1.3-clippy-protocol-2 0, T4-D1.3-check-workspace-2 0 |
| D1.4 | impl/crates/lcl-protocol/tests/localized_engine.rs (new) | 8 localized fixtures accept exactly as canonical English; a mixed word stops at the localization stage at bytes 169..173 (line 11, column 1); ambiguity at byte 9; an unmapped word is lexical at byte 220; a 0.1.0 engine refuses localization | 4 passed; clippy | T4-D1.4-localized-engine 0, T4-D1.4-clippy-protocol 0 |
| D1.5 | impl/crates/lcl-protocol/src/{engine.rs,lib.rs} | `Engines { core, localized }`, `engine_for` (0.1-first dispatch, then D9), exported | Protocol tests; clippy; checks | T4-D1.5a-test-protocol 0, T4-D1.5a-clippy-protocol 0, T4-D1.5a-check-workspace 0, T4-D1.5b-check-protocol 0 |
| D1.6 | impl/crates/lcl-protocol/tests/dispatch.rs (new) | A 0.1.0 document's front result equals the direct Core result; seven 0.2.0 documents go to 0.2.0; seven D9 failures go to 0.2.0; `detection_failed` without a directive stays 0.1.0; `Engines::new` refuses swapped engines | 4 passed; clippy | T4-D1.6-dispatch 0, T4-D1.6-clippy-protocol 0 |
| D2a | impl/crates/lcl-project/src/lock.rs | `LockedLocale`, `Lock::with_locales`, `lcl-lock/2` only when a unit is localized (D6), line `locale <tag> <method> sha256:<hex>  <unit>`, `Drift::Locale`; 4 unit tests | Project tests; clippy; check | T4-D2a-test-project 0, T4-D2a-clippy-project 0, T4-D2a-check-workspace 0 |
| D2b | impl/crates/lcl-protocol/src/{record.rs,engine.rs} | `LocaleRecord` (method, locale, profile identity, detector identity, candidates, `lcl_version`) on `SourceRecord`, as JSON `locale` only when present; the engine fills it from each unit's own localization; `early_diagnostics` removed in favour of one staging (script `t4_d2b4_engine_locale_patch.py`) | Protocol tests; clippy | T4-D2b1…D2b4 gates all 0 |
| D2b5 | impl/crates/lcl-protocol/tests/locale_records.rs (new) | Auto and explicit records carry the Python-matching profile identity and detector identity; canonical, mixed and ambiguous units are recorded honestly; 0.1.0 reports have no `locale` key | 2 passed; clippy | T4-D2b5-locale-records 0, T4-D2b5-clippy-protocol 0 |
| D2c | impl/crates/lcl-protocol | Protocol integration tidy-up and full protocol test run | Protocol tests; clippy; check | T4-D2c-test-protocol 0, T4-D2c-clippy-protocol 0, T4-D2c-check-workspace 0 |
| D3.0 | impl/crates/lcl-cli/Cargo.toml; impl/Cargo.lock | Dependency on lcl-localization | Offline check, then locked check | T4-D3.0-check-cli 0, T4-D3.0-check-cli-locked 0 |
| D3.1–D3.2 | impl/crates/lcl-project/src/{manifest.rs,lib.rs} | Manifest keys `localized_spec` and `profiles`; `localized_spec_path()`, `profiles_path()` | Project tests; clippy | T4-D3.1-test-project 0, T4-D3.1-clippy-project 0, T4-D3.1-check-workspace 0, T4-D3.2-test-project 0, T4-D3.2-clippy-project 0 |
| D3.3 | impl/crates/lcl-cli/src/args.rs | `--localized-spec` (a duplicate is refused) and repeatable `--profile` | CLI 59 unchanged; clippy | T4-D3.3-test-cli 0, T4-D3.3-clippy-cli 0, T4-D3.3-check-workspace 0 |
| D3.4 | impl/crates/lcl-cli/src/main.rs | `localized_spec_root` (`--localized-spec`, then `LCL_LOCALIZED_SPEC`, then manifest) and `engines()`; `check`, `validate`, `inspect`, `run` and `package lock/verify` judge each document with `engine_for`; under `--locked`, the lock's locale pins apply; locks record locale pins. `spec` and `syntax` stay on Core 0.1.0. | CLI 59 unchanged. Clippy first failed on a needless borrow of the now-borrowed engine, fixed and re-run. | T4-D3.4-test-cli 0, T4-D3.4-clippy-cli **101**, T4-D3.4-clippy-cli-2 0 |
| D3.5 | impl/crates/lcl-cli/tests/localized_cli.rs (new) | `check` of `auto_lv` under 0.2.0 records lv-LV auto, and is rejected without the 0.2.0 package; a 0.1.0 document's machine output is byte-identical with and without the 0.2.0 package; `package lock` writes `lcl-lock/2` with `locale lv-LV auto sha256:`, `package verify` and `check --locked` pass, and a changed profile byte makes `check --locked` fail | 3 passed; clippy; workspace check | T4-D3.5-localized-cli 0, T4-D3.5-clippy-cli 0, T4-D3.5-check-workspace 0 |
| D4.1 | impl/crates/lcl-protocol/src/engine.rs | `Engine::open_localized(root, profile_files)`: the 0.2.0 anchor, `<locale>.json` profile files, coverage detector. The one constructor tools use. | Clippy | T4-D4.1-clippy-protocol 0 |
| D4.2 | impl/crates/lcl-project/src/lib.rs | `Project::profile_files()`: `*.json` directly in the profile directory, sorted | Clippy | T4-D4.2-clippy-project 0 |
| D4.3 | impl/crates/lcl-cli/src/main.rs | `engines()` reuses D4.1 and D4.2, removing its own profile loading. A misnamed `--profile` file is now an environment failure, not a usage failure. | CLI 62 passed. Clippy first failed on an unused `Arc` import, removed and re-run. | T4-D4.3-test-cli 0, T4-D4.3-clippy-cli **101**, T4-D4.3-clippy-cli-2 0 |
| D4.4 | impl/crates/lcl-workspace/src/project.rs | `Workspace` holds `Engines`; `open_with(root, spec, localized)` (explicit, else manifest `localized_spec`; profiles from the manifest's directory); `locate_localized_spec` (explicit, then `LCL_LOCALIZED_SPEC`); `engine()` stays Core 0.1.0; `engines()`, `engine_for`, `localized_spec_root` | Check | T4-D4.4-check-workspace 0 |
| D4.5–D4.6 | impl/crates/lcl-workspace/src/{intelligence.rs,routes.rs} | Token spans come from `engine.stage(unit)`, classified by `Token::word`, so localized words paint by canonical class on original bytes; `/api/tokens` uses the document id the frontend already sends; `check`, `inspect`, `validate` and runs use `engine_for`; the session reply gains `localized_spec` when one is in use | Check | T4-D4.6-check-workspace 0 |
| D4.7 | impl/crates/lcl-workspace/src/main.rs | `--localized-spec`; with `--create` the project is created and then opened with it | Workspace 107 existing tests passed; clippy | T4-D4.7-test-workspace 0, T4-D4.7-clippy-workspace 0 |
| D4.8 | impl/crates/lcl-workspace/tests/localized.rs (new) | A project with `localized_spec` and `profiles` judges `main.lcl` under 0.2.0 (lv-LV auto, accepted), while a 0.1.0 example stays 0.1.0; token spans fall on char boundaries, `SPECIFIKĀCIJA` and `DATI` paint `block`, `VESELS` paints `type`, and the class sequence equals canonical English's; an edit saves the exact bytes, reopens identical, lists no other document, and still checks as lv-LV | 3 passed; clippy | T4-D4.8-localized-workspace 0, T4-D4.8-clippy-workspace 0 |
| D4.9 | impl/crates/lcl-cli/src/main.rs | `version` also prints `language 0.2.0` | CLI 62 passed; workspace check; protected check 192/192, 7/7 | T4-D4.9-test-cli 0, T4-D4.9-check-workspace 0 |

### Test-oracle corrections in phase D

None. Every existing test passed unchanged. The three clippy failures were defects in new code, and each was fixed before its re-run.

### Not done in phase D, by scope

- **Autocomplete in localized spellings:** the task says tools *may* offer it. Completion is unchanged and inserts canonical words.
- **Locale transform:** none (D7), and ordinary save never translates.
- **Workspace frontend:** unchanged. It paints the server's spans. The real HTTP and browser flow for localized files is phase G item 7.
- **Debugger loci:** these come from AST spans, which are original bytes since C3. Phase E asserts navigation and diagnostic spans.
- **Locale profiles:** every profile used so far is **fixture-based**, the four package fixtures loaded from files or memory. None has been cached, resolved dynamically or found unavailable (phase F).

### Closure (supersedes the phase C closure above)

- **Remaining:**
  - phases E, F and G are NOT_EXECUTED;
  - independent review of the 0.2.0 additions stays pending (the owner's decision).
- **Next exact action, phase E entry:**
  1. Decide where the V07 matrix profiles live. The package's fixture profiles have 60 spellings each, and adding spellings for the medium and execution programs inside `canonical/LCL_Core_0.2.0` would change the package identity. Test-side fixtures under a conformance or protocol test directory would not.
  2. Build equivalent en, lv-LV, nl-NL, ru-RU and zh-CN versions of a small program (the package fixtures), a medium one (`apps/medium-release-notes`) and an execution-bearing one (`apps/small-invoice-total`).
  3. Assert the V07 items through `Engines`: canonical token IDs, AST structural digest, declarations and references, outcomes, preflight, execution graph, host requests, effects, outputs, evidence, terminal status, and original-byte spans.
- **Handoff:** `/mnt/F/.lcl-closure-4t-4c1cd4c659b7/LCL_FEATURE_04_HANDOFF.txt`.

## Phase E continuation — 2026-09-15

Phase E is **complete**. The V07 cross-language matrix passes for canonical English, lv-LV, nl-NL, ru-RU and zh-CN. The matrix found one real defect in the phase C parser work, which is repaired below. Every change is uncommitted, no Git write was made, and no canonical byte changed.

### Matrix design

- **Programs.**
  - Small: the package's localized minimum, `canonical_en.lcl` and `auto_{lv,nl,ru,zh}.lcl`.
  - Execution-bearing: `apps/small-invoice-total`.
  - Medium: `apps/medium-release-notes`, with one import and one granted write effect.
- **Profiles.** Every uppercase word outside strings in both applications is already one of the fixture profiles' 60 preferred spellings (checked by script before generation). The matrix therefore uses the package fixture profiles unchanged, and the 0.2.0 package identity is untouched. Profile state: **fixture-based** only.
- **Localized application sources.**
  - Rendered by the scratch script `t4_e1_generate_v07.py`: `LCL VERSION` becomes `0.2.0`, and every reserved word outside a string takes the profile's preferred spelling.
  - Strings, identifiers, numbers and punctuation are byte-identical. The script fails closed on any unmapped word.
  - The medium root carries an explicit `@locale`; every other file is auto-detected.
  - 15 files under `impl/crates/lcl-protocol/tests/fixtures/v07/`, with hashes in `logs/T4-E1-v07-fixtures.sha256`. This was a script run, not a gate.

### Changes, in actual order

| Order | File | Change | Verification | Result |
|---|---|---|---|---|
| E1 | impl/crates/lcl-protocol/tests/fixtures/v07/** (new, 15 files) | Generated sources: invoice main in 5 variants; release_notes main and rules in 5 variants | Generator refusals: none; 96, 167 and 26 words replaced per file | script run |
| E2 | impl/crates/lcl-protocol/tests/v07_matrix.rs (new) | Seven tests (listed under Coverage) | **Unexpected failure**: 2 passed, 5 failed | T4-E2-v07-matrix **101** |
| E3 | impl/crates/lcl-parser/src/expr.rs | **Defect repair.** Four reserved-word reads still used the raw source slice instead of `Token::word`: the reserved-word primary (literal words and callable names, so localized `REF`, `SUM`, `TRUE` and `PATH` became `error.grammar.invalid` "unexpected_word"), reserved-word property names, spaced word operators and word comparisons. All four now read the canonical word. Identifier and symbol reads are unchanged. | Parser 101 passed, unchanged; clippy | T4-E3-test-parser 0, T4-E3-clippy-parser 0 |
| E2b | impl/crates/lcl-protocol/tests/v07_matrix.rs | Harness correction: the protocol projects navigation locations as `id_span`, `id_position` and `binding_span`. Every `*_span` and `*_position` key is now a location: dropped from the decision comparison, and checked by span mapping. | 6 passed, 1 failed on a `null` `binding_span` | T4-E2-v07-matrix-2 **101** |
| E2c | impl/crates/lcl-protocol/tests/v07_matrix.rs | Harness correction: a `null` span locates nothing and is skipped | **7 of 7**; clippy; full lcl-protocol suite 76 passed | T4-E2-v07-matrix-3 0, T4-E2-clippy-protocol 0, T4-E3-test-protocol 0 |
| E4 | impl/crates/lcl-protocol/tests/v07_matrix.rs | Source-safety test: BOM, CR, trailing space, U+202E and a no-break space in indentation, at the same structural place in each invoice variant | 1 passed; clippy | T4-E4-source-safety 0, T4-E4-clippy-protocol 0 |

### Harness corrections in phase E

These corrections were made to a new test before it first passed. No existing oracle changed.

1. **E2b.**
   - **Change:** location keys widened from `span` and `position` to every `*_span` and `*_position` key.
   - **Basis:** `record.rs` `NavigationRecord`, whose declarations carry `id_span` and `id_position`.
   - **Control:** the widened keys are not ignored. `spans()` collects them, and each must map exactly to the variant's own bytes. `mapped > 0` is asserted per program.
2. **E2c.**
   - **Change:** a `null` span value is skipped.
   - **Basis:** `binding_span` is optional in the projection.
   - **Control:** non-null spans still fail on a missing `start` or `end`.

### Coverage

| V07 item | Where | Result |
|---|---|---|
| Selected locale, method and profile content identity | `every_variant_selects_its_locale_and_maps_each_word_through_its_profile`: auto for small, invoice and rules; explicit for the medium root; `canonical` with no identity for English; identity equals the SHA-256 of the profile bytes | pass |
| Exact localized-to-canonical word map | Same test: every reserved-word token's own spelling maps through the profile's `spellings` to its canonical word | pass |
| Same canonical token sequence; same AST structural digest | `every_variant_normalizes_to_the_same_canonical_tokens_and_ast`: terminal plus canonical word or exact text; SHA-256 of the span-free AST for every unit | pass |
| Same check, validate and inspect decisions, declarations, references, plan and diagnostics | `every_variant_reaches_the_same_static_decisions_on_its_own_bytes`: the report JSON without location keys is equal | pass |
| Spans on original localized bytes | Same test: every span in every report maps exactly through the aligned-token offset map (boundaries and identical token interiors) | pass |
| Same execution, host requests, effects, outputs, evidence and terminal status | `every_variant_runs_exactly_as_the_core_0_1_0_application`: each variant's run report without location keys equals the **unmodified Core 0.1.0 application** run through the 0.1.0 engine; `status.succeeded`; `NOTES.txt` written under the grant | pass |
| Equivalent invalid cases | `equivalent_invalid_variants_report_the_same_diagnostics_on_their_own_bytes`: an unresolved reference gives the same report, and the primary span maps exactly | pass |
| Selection cases 1, 2, 4–8, plus confusables, directive errors, unavailable resolver and multibyte offsets | `package_fixtures_select_and_fail_closed_through_the_engine`: all 31 package fixtures through `Engine::check` with their available profiles, pins and resolver state; the registered identifier, the offset as the primary span start, the stage, the method and the locale match `expected_results.json` | pass |
| Selection case 3: explicit differs from detection | `an_explicit_locale_wins_over_what_detection_chooses`: `LCL` and `ID` are spelled identically in lv-LV and nl-NL; `@locale lv-LV` and `@locale nl-NL` each select explicitly, and detection alone selects neither | pass |
| Source safety: BOM, CR, trailing space, bidi control, NBSP | `source_safety_rejections_are_the_same_on_each_variants_own_bytes`: the same identifier and stage as English, with the primary span starting exactly at the injected bytes | pass |
| Profile mapping collision, confusable and invalid profile data | Library level, before grammar: 21 profile mutations in `lcl-localization/tests/fixtures.rs` (phase C). The engine path with a malicious resolver is phase F. | pass (phase C) |

**Claim limit.** This proves the mechanism, the four fixture profiles and their 60 mappings each. It is not a claim about translation quality or coverage of any natural language.

### Closure (supersedes the phase D closure above)

- **Remaining:**
  - Phases F and G are NOT_EXECUTED.
  - Independent review of the 0.2.0 additions stays pending, as the owner's decision.
  - The E3 parser change affects every consumer, and phase G's full workspace run covers them.
- **Next exact action, phase F.** A deterministic, provider-neutral dynamic resolver in a test. It must show:
  - no static profile exists for a test locale;
  - the resolver supplies one through `LocaleProfileResolver`, and it validates and is content-pinned;
  - localized source checks and runs;
  - after the resolver's output changes, the pinned evaluation reproduces or reports `profile_drift`, while unpinned use records the new identity;
  - an unavailable resolver with no cache fails closed;
  - malicious resolver output is rejected before grammar through the engine.
- **Handoff:** `/mnt/F/.lcl-closure-4t-4c1cd4c659b7/LCL_FEATURE_04_HANDOFF.txt`.

## Phase F continuation — 2026-09-15

Phase F is **complete**. Four tests prove the design without a world dictionary through `Engine`, with a deterministic, provider-neutral test resolver. No product code changed in phase F, and no Git write was made.

### Changes, in actual order

| Order | File | Change | Verification | Result |
|---|---|---|---|---|
| F1 | impl/crates/lcl-protocol/tests/v07_matrix.rs | Phase F section: `DynamicResolver` implements `LocaleProfileResolver` for `lt-LT`, and its answer changes between calls. It can return the first revision, a revised revision, offline, not JSON, another locale's profile, or a spelling collision. The profile is built at run time from the lv-LV spellings under `locale: lt-LT` and `provider_class: dynamic_resolver`; revisions differ only in provenance bytes. Four tests follow the table. | 4 passed; whole file 12 passed; clippy | T4-F1-dynamic-profiles 0, T4-F1-v07-all 0, T4-F1-clippy-protocol 0 |

| Phase F requirement | Test | Result |
|---|---|---|
| No built-in static profile exists for the test locale | `a_dynamically_resolved_profile_is_validated_selected_and_runs`: no file in `canonical/LCL_Core_0.2.0` contains the bytes `lt-LT` | pass |
| The resolver supplies a profile, which validates and is content-identified; the source checks and runs | Same test: auto-selects `lt-LT` with identity `sha256` of the supplied bytes; the Latvian-spelled invoice runs to `status.succeeded`, and its run report without location keys equals canonical English's | pass |
| The same pinned profile reproduces after the resolver's output changes | `a_pinned_profile_reproduces_and_a_changed_answer_is_visible`: pinned selection is recorded as `pinned`. After the provider's answer changes, a content cache holding the pinned bytes, in the provider's place, reproduces the recorded report exactly (`Report` equality). | pass |
| Changed provider data is visible, never silently substituted | Same test: unpinned use records the new identity. The pinned engine consulting the changed provider fails with `error.localization.profile_drift` at `Stage::Localization`: reached lexical, no AST. | pass |
| An unavailable resolver with no cache fails closed | `an_unavailable_provider_with_no_cache_fails_closed`: auto and explicit both give `error.localization.profile_unavailable` with no AST | pass |
| Malicious or invalid output is rejected before canonical grammar | `malicious_provider_output_is_rejected_before_grammar`: not JSON, another locale's profile, and a spelling that is another canonical word each give `error.localization.profile_invalid` at `Stage::Localization` with no AST | pass |

### Scope statement

- **Cache.** The product ships no network provider and no content cache of its own; C14 keeps provider transports outside the feature. Reproduction under a pin needs the pinned bytes supplied again: by a project's `profiles` directory (the CLI, phase D3.5), or by any resolver serving cached bytes (this phase). Any other bytes fail closed with `profile_drift`.
- **Profiles.** The profiles in this phase were **dynamically resolved** by the test resolver (`lt-LT`) or **cached** (the same bytes through a memory cache). No live commercial or model API was used.

### Closure (supersedes the phase E closure above)

- **Remaining:**
  - Phase G is NOT_EXECUTED.
  - Independent review of the 0.2.0 additions stays pending, as the owner's decision.
- **Next exact action, phase G.**
  1. Full workspace gates on Rust 1.98: `cargo fmt --check`, `clippy --workspace --all-targets -D warnings`, and `test --workspace`.
  2. Actual Rust 1.75.0 locked workspace check and tests.
  3. The 0.1 compatibility, application and conformance gates against the preserved 0.1 package.
  4. The 0.2.0 validators and checksums.
  5. `build_release.sh` version parameter (D8), then a new non-overwriting `lcl-0.2.0` candidate bundling both packages.
  6. Installed-candidate tests in an isolated HOME and XDG for en, zh-CN, lv-LV, ru-RU and nl-NL.
  7. Workspace HTTP and browser flows with localized files.
  8. Protected check: 0.1.0 and Task 3 candidate bytes unchanged.
- **Handoff:** `/mnt/F/.lcl-closure-4t-4c1cd4c659b7/LCL_FEATURE_04_HANDOFF.txt`.

## Phase G continuation — 2026-09-15

Phase G is **executed**. Every executable gate passes. The Core 0.1.0 package and the Task 3 candidate are byte-identical, and no Git write was made. Two acceptance failures were harness oracle errors, recorded with their evidence below.

### Gates, in actual order

| Order | What | Result | Gate |
|---|---|---|---|
| G1 | Rust 1.98.1: `cargo fmt --all --check`, `clippy --workspace --all-targets -D warnings`, `test --workspace` | fmt and clippy clean; **1578 passed, 0 failed, 1 ignored** | T4-G1-fmt 0, T4-G1-clippy-workspace 0, T4-G1-test-workspace 0 |
| G2 | **D8 packaging change** (5 files, diff in `logs/T4-G2-packaging.diff`, before-copies in `logs/T4-G2-packaging.before/`). `packaging/build_release.sh`: `LCL_RELEASE_VERSION` (0.1.0 or 0.2.0; default the product version; other values refused); a 0.2.0 candidate needs and bundles `canonical/LCL_Core_0.2.0`; the language field is one line; provenance records the release version and the 0.2.0 identity reported by the built tool. `install.sh`: installs `LCL_Core_0.2.0` when the payload has it and substitutes `@LOCALIZED_SPEC@`. `uninstall.sh`: removes it. `lcl-workspace-launch.in`: passes `--localized-spec` only when one was installed. `README.md`: documented. A 0.1.0 payload behaves as before. | `sh -n` clean for all four scripts; lcl-hardening **71 passed**, unchanged, including release_build, installed_launcher and packaging_smoke | T4-G2-test-hardening 0 |
| G3 | Core 0.1.0 compatibility: `validate_release.py --scope all`, `sha256sum -c SHA256SUMS.txt`, brand checksums, all with Task 3's exact argv | 31 PASS, 2 OUT_OF_SCOPE, 0 FAIL (as T3-C); 175 OK; 17 OK | T4-G3-canonical-validator-0.1.0 0, T4-G3-canonical-checksums-0.1.0 0, T4-G3-brand-checksums 0 |
| G4 | Core 0.2.0: `validate_release.py --scope all`, `validate_localization.py`, strict `sha256sum` | **EXPECTED exit 1**: 31 PASS, 2 OUT_OF_SCOPE, 1 BLOCKED (the release gate, `independent_review` pending); JSON identical to T4-B9b-validate-all apart from generated lines. Localization and checksums pass. | T4-G4-validate-all-0.2.0 1 (expected), T4-G4-localization-0.2.0 0, T4-G4-sha256sum-0.2.0 0 |
| G5 | Protected check before the Rust 1.75.0 gates and the build | 192 of 192; 7 of 7 Task 3 candidate files | T4-G5-protected 0 |
| G6 | Actual Rust 1.75.0 from the private toolchain, locked: `check --workspace --all-targets`, `test --workspace --all-targets` | Check clean; **161 suites, 1578 passed, 0 failed, 1 ignored** | T4-G6-msrv-check 0, T4-G6-msrv-tests 0 |
| G7 | 0.1 conformance report, `m8_conformance_report --json`, compared leaf by leaf with T3-C-m8-json-bound | Exactly one differing leaf: `/implementation/source_snapshot`, "unrecorded" in this unbound run. `source_conforming` and semantics 402 = 261 + 8 + 2 + 131 are unchanged. | T4-G7-m8-json 0 |
| G8 | `LCL_RELEASE_VERSION=0.2.0 sh packaging/build_release.sh` | New candidate: see the next table | T4-G8-build-candidate 0 |
| G8b | Candidate checksums and source inventory | Payload and source archives OK. The inventory holds 980 files: `canonical/LCL_Core_0.2.0` 216, `canonical/LCL_Core_0.1.0` 176, V07 fixtures 15, `releases/` 0. The inventory digests equal the checkout for the four packaging scripts and `lcl-parser/src/expr.rs`. | T4-G8-candidate-checksums 0 |
| G9 | Installed-candidate acceptance in disposable homes (see the acceptance tables) | E1 57, E2 35, E4 24, localized 32, E5 11 PASS; 0 FAIL | T4-G9-e1-install-cli-3 0, T4-G9-e2-launch-http 0, T4-G9-e4-browser 0, T4-G9-localized 0, T4-G9-e5-uninstall-reinstall 0 |
| G10 | Protected check after the build and acceptance | 192 of 192; 7 of 7 | T4-G10-protected 0 |

G1 ran before the G2 packaging change. That change touched no Rust source, so only lcl-hardening, which reads the packaging scripts, was run again (G2). G6 ran on the final code.

### Candidate

| Field | Value |
|---|---|
| Directory | `releases/candidates/lcl-0.2.0-linux-x86_64-a0006c38fb79/` (new; the Task 3 candidate is untouched) |
| Artifacts | `lcl-0.2.0-linux-x86_64.tar.gz`, `.sha256`, `-source.tar.gz`, `-source.sha256`, `SOURCE_INVENTORY.tsv`, `lcl-0.2.0-PROVENANCE.txt`, `SOURCE_CHANGES.patch` |
| Payload sha256 | `35624d8b9e536f4b19ceafbb3c419dd475a39953ca509db7da19e6b353514960` |
| Source | 980 files, source id `a0006c38fb79…` (SHA-256 of `SOURCE_INVENTORY.tsv`); uncommitted state recorded in provenance and `SOURCE_CHANGES.patch` |
| Release / product / language | 0.2.0 / 0.1.0 (the `impl/Cargo.toml` version is unchanged, per decision D8's version parameter) / `0.1.0 0.2.0` |
| Package identities | Core 0.1.0 `00d648b162939d06c44838481a67c39bc12c64bdd6d105035c24150148fe67ed`; Core 0.2.0 `e86121c734cb51065b791329738b9ed6b77c53e03e0eebbb2ad1076bea973ef0`, reported by the built tool and equal to `APPROVED_PACKAGE_0_2_0` |
| Status | **BUILT_NOT_FULLY_VERIFIED**. The manual desktop-menu launch is the owner's, as for Task 3's GATE-01, and the 0.2.0 release gate awaits independent review. |

### Installed acceptance

The scratch harnesses run under `env -i` in a disposable HOME and XDG, with stubbed opener and dialog tools, and no session bus.
- **Task 3 harnesses:** E1, E2, E4 and E5 are copies of the Task 3 harnesses with only the candidate and provenance names changed. E5 also requires `LCL_Core_0.2.0` to be present, to be removed by uninstall, and to be restored by reinstall.
- **New harness:** `t4_g9_localized_installed.py`.

The acceptance root is `accept-t4c`. The earlier roots `accept-t4` and `accept-t4b` are kept as failure evidence.

| Harness | Result |
|---|---|
| E1 install and CLI | 57 PASS. The archive matches its checksum. Every payload member equals the provenance record. Installed binaries, package, icons, desktop entry and media type are as recorded. `version` reports language 0.1.0; every VALID example is accepted and every INVALID one gives its registered error and status. |
| E2 launcher and HTTP | 35 PASS. Menu launch, `.lcl`, `.lcl.txt` in a path with a space, and a missing document (status 4, dialog, log). Session root and bundled spec, listing, brand assets, token and Host refusal, CLI grants. |
| E4 browser | 24 PASS. Headless Firefox over WebDriver BiDi on the installed launcher: New, typing, Save and Ctrl+S bytes, reload, Check diagnostic, Run status. |
| Localized (new) | 32 PASS. The installed 0.2.0 package equals the payload's (216 files); the launcher carries it; `version` reports both languages. CLI `run` of the V07 invoice in en, lv-LV, nl-NL, ru-RU and zh-CN: `status.succeeded` under 0.2.0 with locale records (`canonical`, then `auto` per locale), all with outputs `{output.subtotal: 5525, output.total: 6630}`. Without the 0.2.0 package it fails closed (exit 1). The unmodified 0.1.0 application gives identical machine output with and without the 0.2.0 package. The lock is `lcl-lock/2` with the lv-LV pin; verify and `check --locked` pass, and changed profile bytes give exit 4 with drift. The launcher-started workspace session names both packages. All five documents are accepted under 0.2.0 with their locales. Tokens paint `SPECIFIKĀCIJA` 27..41, `SPECIFICATIE` 26..38, `СПЕЦИФИКАЦИЯ` 32..56 and `规范` 26..32 as `block`. `error.reference.unresolved` covers the author's bytes 540..553. Save writes the localized bytes exactly, reopen returns them, the saved document is still lv-LV, and no other file appears. |
| E5 uninstall and reinstall | 11 PASS. Exactly the installed files and both packages go; operator and unrelated files stay byte for byte; reinstall restores everything. |

### Acceptance harness corrections (the original failures are kept)

1. **T4-G9-e1-install-cli: exit 1, 55 PASS, 1 FAIL.**
   - **Failure:** "an ordinary .txt file is not accepted as an LCL document: exit 0".
   - **Reproduction:** every verdict line is identical to T3-E1-install-cli once names and hashes are normalized. It reproduces LCL-CLOSE-03 oracle correction 1, which Task 3 recorded without editing its script.
   - **Contract:** `impl/crates/lcl-cli/tests/lcl_integration.rs::the_extension_changes_no_verdict`, where the same bytes named `.lcl`, `.txt`, with no extension or `.LCL` produce the same record apart from the document's identity.
   - **Correction:** `.txt` must give the same exit code and record as `.lcl`.
   - **Control:** an INVALID example named `.txt` is still rejected (exit 1).
2. **T4-G9-e1-install-cli-2: exit 1, 56 PASS, 1 FAIL.**
   - **Cause:** my correction normalized only the full path. The human output also names the document ("minimal.lcl passed…" against "plain.txt passed…"), and the diff showed that was the only difference.
   - **Correction:** normalize the document's name as well, as the contract test does.
   - **Result:** T4-G9-e1-install-cli-3 passes 57 of 57.

### Locale profile state (task completion report requirement)

| Class | Profiles | Where |
|---|---|---|
| Fixture | lv-LV, nl-NL, ru-RU, zh-CN (the package's 4 profiles, 60 mappings each, not complete) | Phases C to E, the CLI and workspace tests, and the installed candidate |
| Dynamically resolved | lt-LT, built by the deterministic test resolver | Phase F |
| Cached | The pinned lt-LT bytes, served from a memory cache after the provider's answer changed | Phase F |
| Unavailable | The offline test resolver, `UnavailableResolver`, and locales with no profile | Phases E and F: fail closed with `profile_unavailable` |

No live commercial or model provider was used or is required. Passing these locales proves the mechanism and those mappings, not translation quality or coverage of any natural language.

### Completion criteria (task section 11)

| Criterion | State |
|---|---|
| New language version explicit; Core 0.1.0 untouched | Met: `canonical/LCL_Core_0.2.0`, protected check 192/192 |
| One provider-neutral localization stage feeding the canonical engine | Met: phases C and D |
| `@locale` always wins over detection | Met: package fixtures and `an_explicit_locale_wins_over_what_detection_chooses` |
| Detection fails closed on ambiguity or unavailability | Met: phases E and F |
| Dynamic profiles validated and content-pinned, no static dictionary | Met: phase F |
| Localized bytes preserved; original-byte source maps drive diagnostics and navigation | Met: phases D, E and G9 |
| en, zh, lv, ru and nl equivalents give the same tokens, AST and behaviour | Met: V07 matrix, and G9 on the installed candidate |
| Negative collision, Unicode, drift and mixed-language cases | Met: phases C, E and F |
| CLI and workspace save and reopen preserve localized source | Met: D3.5, D4.8 and G9 |
| Exact new-version integrated, MSRV and conformance gates pass | Implementation gates met (G1, G6). The 0.2.0 **release** gate is BLOCKED by design until independent review. |
| Exact-source new-version candidate built and tested without altering 0.1 | Met, except the owner's manual desktop-menu launch |
| Report separates mechanism proof from translation claims | Met: the locale profile state table |

### Closure (supersedes the phase F closure above)

- **Task status:** BLOCKED on owner decisions only. Every executable gate passed. No Git write and no publication was made.
- **Owner decisions:**
  1. Independent review of the Core 0.2.0 localization additions (`05_LANGUAGE_CLOSURE.json` `independent_review`), which gates the 0.2.0 release validator.
  2. The manual desktop-menu launch of the installed `lcl-0.2.0` candidate, which has no graphical session here. Use the Task 3 procedure; with the 0.2.0 payload, a localized document should open and check.
  3. Whether and how to commit the uncommitted Task 4 files and the candidate.
- **Checkout against the candidate source:** after the build, only this report changed in the source tree (verified by the post-record inventory recheck in the handoff). `releases/` is never source.
- **Handoff:** `/mnt/F/.lcl-closure-4t-4c1cd4c659b7/LCL_FEATURE_04_HANDOFF.txt`.

## Reconciliation addendum — 2026-09-15, LCL-REPAIR-02

Added under the LCL Six-Task Repair Pack, finding B-07. Nothing above this heading was changed; this section records what superseded the state written above.

- **Commit.** The owner committed the Task 4 changes, this report and the candidate as `7ed84ba` ("LCL task 4 final"). LCL-REPAIR-01 recorded a clean worktree at that commit, so no Task 4 change remained uncommitted.
- **Identity lines.** "This record covers phases A, B and C" and "Current state: uncommitted phase B and C changes" were written during phase C. The record covers phases A to G.
- **Phase closures.** Each "NOT_EXECUTED" and "Next exact action" in the phase B to F closures was superseded by the next continuation. The phase G closure is the final Task 4 state.
- **The owner decisions of that closure:**
  1. Independent review of the Core 0.2.0 additions: still pending (repair pack owner action O-01).
  2. Manual desktop-menu launch of the 0.2.0 candidate: no result is recorded, and it remains owner action O-02.
  3. Commit: resolved by `7ed84ba`.
- **Candidate.** `releases/candidates/lcl-0.2.0-linux-x86_64-a0006c38fb79/` is unchanged. It predates LCL-REPAIR-01 (committed as `7d100b7`) and LCL-REPAIR-02, so it carries neither their packaging scripts nor the repaired `lcl version`.
- **Later work** follows the LCL Six-Task Repair Pack: see `reports/tasks/LCL-REPAIR-01_RESULT.md` and `reports/tasks/LCL-REPAIR-02_RESULT.md`.

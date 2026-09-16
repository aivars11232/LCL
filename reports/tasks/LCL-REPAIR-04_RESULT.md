# LCL-REPAIR-04 — Result

## Identity

- Task: LCL-REPAIR-04, localization source-byte correctness (B-12)
- Repository: `/mnt/F/LCL`
- Branch: `main`
- Entry HEAD: `40c510642c0c92f7ce6f5db97c8ad1a7044cb0cb` ("LCL reapair task3"). The owner committed REPAIR-03: that commit holds exactly the 8 files and the result report that REPAIR-03 lists.
- Exit HEAD: unchanged
- Entry worktree: clean, level with `origin/main`
- Exit worktree: 2 modified tracked files (listed below), plus this report. Nothing under `canonical/` or `releases/` changed.
- Pack manifest: all 21 `MANIFEST_SHA256.md` hashes verified, at the start and again when the session resumed on 2026-09-16.
- Git writes performed: **NO**
- Background/sub/parallel agents used: **NO**
- Evidence logs: `/mnt/F/.lcl-repair-6t/logs/R4-*`
- Scratch: `/mnt/F/.lcl-repair-6t/tmp` (`r4/`, `r4-cli-cases.py`, `r4-gate.sh`)

## Result

**Status:** PASS WITH OUT-OF-SCOPE FINDINGS

## Scoped findings

| Finding | Reproduction | Repair | Verification | Disposition |
|---|---|---|---|---|
| B-12: lossy UTF-8 before byte offsets | Five invalid-UTF-8 variants of the canonical localization fixtures, one per required case, checked on HEAD through the `lcl` CLI (`R4-B12-RED-cli.log`) and through the 0.2.0 engine by the new test (`R4-B12-RED.log`, exit 101). `localize` decoded with `String::from_utf8_lossy` and took offsets and selections from the replacement text. Three cases reported localization diagnostics on the wrong bytes and hid the encoding error. Four selected a locale from bytes that do not decode. | `localize` decodes with `std::str::from_utf8`, the lexer's own check. Source that does not decode is not localized: no diagnostic, no selection, `directive_len` 0. The lexer's `error.encoding.invalid` on the original offending bytes is then the unit's only result. | `invalid_utf8_is_never_localized` GREEN (`R4-B12-GREEN.log`). CLI rerun (`R4-B12-GREEN-cli.log`). Integration gate below, first run all green. | FIXED_VERIFIED |

### Required cases (bytes inserted into canonical fixtures)

| Case | Construction | Before (HEAD) | After |
|---|---|---|---|
| 1. before a localized candidate word | `mixed_canonical_word_explicit.lcl`, `FF` at byte 169, before `DATA` | `error.localization.mixed` at (172,176), the bytes `TA:` and LINE FEED; locale lv-LV recorded | `error.encoding.invalid` at (169,170) = `FF`; no locale |
| 2. inside a locale directive | `explicit_lv.lcl`, `E2 80` at byte 13, the end of `@locale lv-LV` | `error.localization.locale_invalid` at (8,16), which takes in the directive's LINE FEED | `error.encoding.invalid` at (13,15) = `E2 80` |
| 3. before a multi-byte word | `confusable_mixed_script_word.lcl`, `D0` at byte 189, before `ДАННЫE` | `error.localization.confusable` at (192,203), starting at the word's second letter; locale ru-RU recorded | `error.encoding.invalid` at (189,190) = `D0`; no locale |
| 4. after a valid localized spelling | `explicit_ru.lcl`, `FF` at byte 267, after `ЗНАЧЕНИЕ` | `error.encoding.invalid` at (267,268), but locale ru-RU selected from undecodable bytes | same diagnostic; no locale |
| 5. canonical source | `canonical_en.lcl`, `80` at byte 96, inside the `NAME` string | 0.2.0 engine: `error.encoding.invalid` at (96,97), but method `canonical` recorded | same diagnostic; no locale |

- Every row now reports exactly one diagnostic whose span covers exactly the inserted bytes. The outcome is rejected, with default status `status.invalid`.
- Line and column:
  - Every row reports position 1:1, with the position offset equal to the span start. This was true before the repair and still is.
  - That is the documented presentation for an undecodable unit. `Lexed::source` is "Empty when the source was not valid UTF-8" (`lcl-lexer/src/lib.rs`), and the record calls line and column presentation only (`lcl-protocol/src/record.rs`).
  - The byte span is the normative location. The test pins both.
- Valid localized Unicode is not affected. For valid UTF-8, the lossy decode already returned the identical text, so its offsets were exact before and are unchanged. The fixture suite, including `multibyte_offset_unknown_word` at 220 and `confusable_mixed_script_word` at 189, and the protocol spans are green.

### Authority applied

- `01_FOUNDATION/03`, step 1: "Decode UTF-8, apply the localization stage of 02_LEXICAL/13 …, and validate line/indentation rules." Decoding comes first.
- `02_LEXICAL/01`: invalid UTF-8 is forbidden, and an implementation "must not silently repair, normalize, re-indent, re-quote, or otherwise rewrite source before validation."
- `02_LEXICAL/13`:
  - matching compares exact Unicode scalar sequences;
  - source bytes are never rewritten;
  - every location is a byte offset into the original localized source.
- `statuses_and_errors_v0.2.0.json`:
  - `error.encoding.invalid` is "Source is not valid UTF-8.", at the lexical stage, with `status.invalid`;
  - the `location_rule` uses the first offending UTF-8 byte offset.
- Reading:
  - Bytes that do not decode carry no scalars, so the localization stage has nothing it could judge without first repairing them.
  - The registry has no localization identifier for encoding, and none may be invented.
  - The lexer already makes the encoding error the only result (`invalid_utf8_is_the_only_result`).
  - Therefore a localization problem beside invalid bytes (cases 1–3) no longer hides the encoding error.
- This adds no language rule. The owner may prefer another reading, for example localizing the decodable prefix. That would change only cases 1–3 and the test's expectations for them.

## Files changed

| File | Why | Minimal change |
|---|---|---|
| `impl/crates/lcl-localization/src/lib.rs` | Owns B-12: the only `from_utf8_lossy` on source text | `localize` decodes with `std::str::from_utf8`. Undecodable source returns `Localization::failed(Vec::new())`, which carries no diagnostic and no selection. A 4-line comment; `directive(text)`. 9 lines in total. |
| `impl/crates/lcl-protocol/tests/localized_engine.rs` | Direct regression, at the layer that reports spans, positions and locale records | One test, `invalid_utf8_is_never_localized`, covering the five cases. It compares every case's diagnostics (id, stage, covered bytes, span, position) and locale presence against the expected rows in one assertion. |

## Evidence

| Gate | Command/Test | Exit | Result |
|---|---|---:|---|
| B-12 RED, CLI | `r4-cli-cases.py red` with the HEAD `lcl` binary (build was a no-op) (`R4-B12-RED-cli.log`) | 0 | the "Before" column above |
| B-12 RED | `cargo test -p lcl-protocol --test localized_engine` (`R4-B12-RED.log`) | 101 | 4 passed, 1 failed: the new test, with all five rows as predicted |
| B-12 GREEN | same (`R4-B12-GREEN.log`) | 0 | 5 passed |
| B-12 GREEN, CLI | `r4-cli-cases.py green` after rebuilding `lcl` (`R4-B12-GREEN-cli.log`) | 0 | the "After" column above |
| Localization fixture suite | `cargo test -p lcl-localization --no-fail-fast` (`R4-INT-localization.log`) | 0 | 8 passed, 3 targets |
| Protocol: localized engine, dispatch matrix, v07 matrix, locale records, 0.1 engine stages | `cargo test -p lcl-protocol --no-fail-fast` (`R4-INT-protocol.log`) | 0 | 83 passed, 10 targets (REPAIR-03 had 82) |
| Lexer: `invalid_utf8_is_the_only_result`, robustness, localized words | `cargo test -p lcl-lexer --no-fail-fast` (`R4-INT-lexer.log`) | 0 | 125 passed, 9 targets |
| Resolver: owns `stage`; 0.1 invalid-byte robustness | `cargo test -p lcl-resolver --no-fail-fast` (`R4-INT-resolver.log`) | 0 | 103 passed, 11 targets |
| Parser localized AST | `cargo test -p lcl-parser --test localized_ast` (`R4-INT-parser-localized-ast.log`) | 0 | 2 passed |
| Workspace projection of a multi-byte localized source | `cargo test -p lcl-workspace --test localized` (`R4-INT-workspace-localized.log`) | 0 | 4 passed, including `token_spans_paint_localized_words_by_their_canonical_class` |
| Lint | `cargo clippy -p lcl-localization -p lcl-protocol --all-targets -- -D warnings` (`R4-INT-clippy.log`) | 0 | clean |
| Formatting | `cargo fmt --all -- --check` (`R4-INT-fmt.log`) | 0 | no diff |
| Scope | `git diff`, `git status --short --untracked-files=all`, `grep from_utf8_lossy impl/crates/*/src` (`R4-INT-scope.log`) | 0 | only the 2 files. The remaining lossy uses are host process and HTTP output (`lcl-capabilities`) and the hardening corpus generator; none reads LCL source text. |

- Environment:
  - every command ran from `/mnt/F/LCL` with `--manifest-path impl/Cargo.toml`;
  - `CARGO_TARGET_DIR=/mnt/F/.lcl-closure-4t-4c1cd4c659b7/target-current`;
  - `TMPDIR=/mnt/F/.lcl-repair-6t/tmp`;
  - rustc 1.98.1.
- Counts are summed from the `test result:` lines (`R4-INT-summary.log`). The gate ran once.

## Acceptance criteria

- Localization never derives authoritative offsets from lossy replacement text. `from_utf8_lossy` is gone from `localize`, and no source-text path uses it.
- All localization offsets refer to original source bytes. Localization now runs only on source that decodes, and there its text is the original bytes, so `byte_offsets` returns original offsets.
- Invalid UTF-8 fails closed. All five cases are rejected with `error.encoding.invalid` (`status.invalid`), on the exact offending bytes. No locale is selected, so nothing can be pinned from them.
- Valid localized Unicode offsets remain exact. Behavior on valid UTF-8 is unchanged by construction, and the fixture, protocol, v07-matrix and workspace suites are green.
- No source normalization or rewrite was introduced. The repair removes the only one.

## Protected-material check

- Core 0.1.0: `SHA256SUMS.txt` verifies (175 entries, exit 0). `lcl spec` identity is `00d648b162939d06c44838481a67c39bc12c64bdd6d105035c24150148fe67ed`, the pinned value. No git change.
- Core 0.2.0: `SHA256SUMS.txt` verifies (215 entries, exit 0). No git change. `lcl spec` refuses 0.2.0 as `--spec` by design (exit 4, "this build is pinned to 0.1.0"). Its identity `e86121c734cb51065b791329738b9ed6b77c53e03e0eebbb2ad1076bea973ef0` is:
  - the `APPROVED_PACKAGE_0_2_0` anchor;
  - the anchor the passing localized-engine and fixture tests open the package against;
  - the digest the RED CLI record prints (`R4-INT-protected.log`).
- Existing candidates: no tracked or untracked change under `releases/`, and nothing in this task reads or writes them.
- Historical evidence: untouched. No prior result file was edited.
- Checkout writes: none by the gates. The ignored `impl/target/flycheck0` and `impl/target/debug/deps` were written at 17:27 by the editor's rust-analyzer check. The gate ran at 17:35 against the external target directory, and `impl/target/test-tmp` is unchanged since REPAIR-03.

## Out-of-scope findings

- **OOS-1: engine attribution of undecodable documents (REPAIR-03 surface).**
  - How dispatch decides: `Engines::engine_for` reads `VERSION` from the token prefix (`lexed_lcl_version`), or uses a selected profile. A unit that does not decode has no tokens, and localization now selects nothing, so every such document is judged by Core 0.1.0.
  - Before this repair: cases 1–4 went to 0.2.0, but only because localization had chosen a profile from repaired text. Case 5 (canonical English) already went to 0.1.0.
  - What differs: only the report's `spec` block. The diagnostic is the same either way, since `error.encoding.invalid` is identical in both registries and the span does not change. Evidence: `R4-B12-RED-cli.log` against `R4-B12-GREEN-cli.log`.
  - The open question: the REPAIR-03 report's wording ("declares `0.2.0` only when its whole header line precedes the first lexical defect") could be read as sending all five cases to 0.2.0, because each header precedes the invalid bytes. Honoring that would mean reading a header from the decodable prefix, a dispatch rule this task does not own.
  - Status: not pinned in the dispatch matrix, pending the owner's decision.
- **OOS-2: the canonical reference tool decodes lossily.**
  - `canonical/LCL_Core_0.2.0/09_CONFORMANCE/TOOLS/validate_localization.py` (line 411) decodes with `errors="replace"` and derives offsets from the replacement text: the same pattern as B-12.
  - It is protected Core 0.2.0 material and was not edited.
  - None of the localization fixture files contains invalid UTF-8 (checked), so the tool and the implementation agree on every fixture.
  - This is a canon decision, suited to the pending independent review (O-01).
- **Note: line and column of an undecodable unit.** Every position is 1:1, as documented, because the lexer keeps no text for such a unit. Deriving them from the decodable prefix would change lexer and protocol presentation for Core 0.1.0 as well. Not a B-12 defect, and not changed here.

## Remaining blockers

- None.

## Owner actions

- O-03: review and commit decisions for the 2 modified files and this report.
- Confirm or override the reading above: undecodable source is not localized, and the encoding error is the result even beside a localization problem.
- Decide OOS-1 (attribution of undecodable documents) and OOS-2 (the reference tool's lossy decode, for the 0.2.0 review).
- O-01 and O-02 are unchanged: the Core 0.2.0 independent review is pending, and no desktop-menu launch result is recorded.

## Quota/efficiency notes

- Reused:
  - `std::str::from_utf8`, the lexer's check, so there is no second decoder;
  - `Localization::failed`;
  - the `localized_engine` and `fixtures` helpers;
  - the canonical fixture sources and profiles, with cases made by inserting bytes, so no new fixture files;
  - the Task 4 compile cache and the cached `lcl` binary for the RED CLI run;
  - REPAIR-03's evidence for CLI usage.
- One 9-line source change and one test; the integration gate ran once.
- Deferred to REPAIR-06:
  - the workspace-wide suite and the Rust 1.75 gate (the change uses `let … else`, which the crate already uses);
  - the CLI and hardening suites;
  - the production conformance report.

## Handoff

The task passed its acceptance contract. LCL-REPAIR-05 is unlocked. The owner decisions above do not block it.

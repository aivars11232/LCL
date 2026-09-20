# FINAL-03 result — F10 owner decision and Core 0.2 attribution closure

## Identity
- Pack: `/mnt/F/LCL_Final_PreTesting_Blocker_Closure_Pack_v3_825694f/`, task `TASKS/FINAL-03_F10_CORE_0.2_ATTRIBUTION.md`.
- Entry HEAD: `42ec954a238d53371be75b0a62e9ae7fa41788c1` ("LCL final pretest task 2.1"), the owner's commit of FINAL-02.
- Exit HEAD: unchanged.
- Entry status: clean (1,032 tracked files).
- Exit status: 9 modified tracked files plus this report.
- Git writes by agent: **NO**
- Agents or background workers: **NO**
- Network or new dependencies: **NO**
- Scratch: `/mnt/F/.lcl-pretest/f3/`, holding `f3-gate.sh` (adapted from `f1-gate.sh`).
- Logs: `/mnt/F/.lcl-pretest/f3/F3-*` and `/mnt/F/.lcl-pretest/logs/F3-G-*`.
- `TMPDIR` was `/tmp/lcl-final-03`; `CARGO_TARGET_DIR` was the isolated `target-current`, MSRV used `target-msrv`.

## Owner decision

`F10_DECISION=A` — **atomic decode**. Supplied by the owner on 2026-09-20, in this session, after being shown both readings and their cost. Reading B (valid-prefix attribution) was rejected and is **not** implemented. The coding agent did not select the rule.

## Status
**PASS.** B4/F10 is closed: canon and the implementation now carry exactly one attribution rule, and every Core 0.2 integrity, localization and dispatch gate passes.

The candidate stays `UNRELEASED_CANDIDATE` with `release_gate_permitted: false`. B3/F29, the independent review, is an owner action and is untouched; the readiness record for it is below.

## Findings

### B4 / F10 — invalid-UTF-8 version attribution was undetermined
- **Reproduction (canon).** `grep -rn "invalid UTF-8"` over `canonical/LCL_Core_0.2.0` returned exactly one line, `02_LEXICAL/01`'s "…and invalid UTF-8 are forbidden." No text said whether decoding is atomic, whether a readable prefix can establish `VERSION`, or which package owns the rejected unit. Log: `F3-repro-canon.log`.
- **Reproduction (implementation).** `lcl-lexer/src/scan.rs` returns on the first `from_utf8` error with `error.encoding.invalid` at the first offending byte, an empty text and **no tokens**; `Engines::engine_for` therefore finds no declared `VERSION` through `lexed_lcl_version` and keeps the Core 0.1.0 reading. `dispatch.rs` pinned that behaviour with the comment "Canon does not settle which package owns a unit rejected before its VERSION is resolved; this pins the current attribution pending an owner decision."
- **Root cause.** A normative gap, not a defect: the implementation already behaved as Reading A, and no canonical sentence required it, so a conforming implementation could have chosen Reading B.
- **Repair.** State Reading A normatively in the two sections that own the question, and replace the "pending an owner decision" pin with a regression that fails under Reading B.
- **Verification.** `invalid_utf8_is_attributed_to_core_0_1_0` (8 byte positions incl. `@locale`), the unchanged dispatch matrix, the three 0.2 validators, `sha256sum -c`, and the full gate below.

## Normative text changed

Exactly two normative files, both in `canonical/LCL_Core_0.2.0`. No registry, fixture, tool, example, EBNF or `00_RELEASE/05_LANGUAGE_CLOSURE.json` byte changed.

**`02_LEXICAL/01_CHARACTER_ENCODING_AND_SOURCE_TEXT.txt`** — one bullet added before the no-silent-repair bullet:

> - UTF-8 decoding is atomic over the complete source unit. A unit that is not
>   valid UTF-8 is not decoded in whole or in part: no character, token, line,
>   locale directive or declaration, including the LCL block VERSION declaration,
>   exists for any purpose, and no prefix is decoded to recover one. The unit is
>   rejected with error.encoding.invalid at the first offending byte of the
>   original source. Because no VERSION is resolved from it, the unit is not a
>   document declaring VERSION "0.2.0" and keeps the default LCL Core 0.1.0
>   reading. Bytes that are readable before the first invalid byte establish no
>   version and bring no unit under 0.2.0 rules.

**`02_LEXICAL/13_LOCALIZED_SOURCE_AND_LOCALE_DIRECTIVE.txt`** — one sentence appended to the closing VERSION section:

> A source unit that is not valid UTF-8 declares no VERSION: 02_LEXICAL/01 makes
> decoding atomic, so these rules never apply to it, a locale directive in its
> readable bytes selects nothing, and its error.encoding.invalid belongs to the
> LCL Core 0.1.0 reading at the first offending original byte.

The second is a consequence of the first plus the pre-existing "These rules apply only to a document whose LCL block declares VERSION "0.2.0""; it states no independent rule.

## Implementation behaviour

No product code changed. The decision was chosen to match what the implementation already does, so the code is evidence, not a target:

- `impl/crates/lcl-lexer/src/scan.rs:91` — the encoding gate returns `error.encoding.invalid` spanning `valid_up_to()..error_len()` on the **original** bytes, with `String::new()` as text and zero tokens.
- `impl/crates/lcl-protocol/src/engine.rs:898` — `lexed_lcl_version` needs a complete `LCL`/`VERSION` token line before the first defect; with no tokens it yields `None`, so `engine_for` keeps the Core 0.1.0 engine.
- `impl/crates/lcl-localization/src/lib.rs:1147` — `localize` returns a failure with no diagnostics for non-UTF-8 bytes, so a locale directive inside an invalid unit selects nothing.
- `canonical/LCL_Core_0.2.0/09_CONFORMANCE/TOOLS/validate_localization.py` — already decodes fixture source strictly (PRETEST-03).

## Files changed
| File | Why | Reused mechanism |
|---|---|---|
| `canonical/LCL_Core_0.2.0/02_LEXICAL/01_CHARACTER_ENCODING_AND_SOURCE_TEXT.txt` | States decision A | Existing bullet form of the section |
| `canonical/LCL_Core_0.2.0/02_LEXICAL/13_LOCALIZED_SOURCE_AND_LOCALE_DIRECTIVE.txt` | Consequence for localization applicability | Existing VERSION section |
| `canonical/LCL_Core_0.2.0/CHANGELOG.txt` | Decision record for the candidate | Existing changelog entry form |
| `canonical/LCL_Core_0.2.0/MANIFEST.json` | Integrity regeneration | `TOOLS/generate_integrity.py manifest` |
| `canonical/LCL_Core_0.2.0/VALIDATION_REPORT.txt` | Rebound to the new manifest hash, new snapshot date/UTC, gate-9 pointer | Same three fields PRETEST-03 rebound |
| `canonical/LCL_Core_0.2.0/SHA256SUMS.txt` | Integrity regeneration | `TOOLS/generate_integrity.py checksum` |
| `impl/crates/lcl-spec/src/anchor.rs` | New 0.2 trust anchor digest; doc records the identity chain | Existing `TrustAnchor` constant |
| `impl/crates/lcl-protocol/tests/dispatch.rs` | F10 regression + retired the "pending an owner decision" comment | Existing fixtures, `Engines`, matrix helpers |
| `README.md` | Candidate identity table | Existing table row |

No new file, crate, dependency, helper or fixture was added. `canonical/LCL_Core_0.1.0`, `releases/` and `assets/` are byte-identical.

## Core 0.2 identity
| | |
|---|---|
| Old identity (PRETEST-03) | `6e7303157f5ba378b4e8c0b89852a7a81b00b6dd26b85cac36a8977532690ceb` |
| New identity (FINAL-03) | `00daee8de1919c4945ef04ff65edb22164bd8046a493be08a87d5fa3b4c3e604` |
| Package file count | 216, unchanged |
| Old `MANIFEST.json` SHA-256 | `2e029a0ce19bc6a5a453659abd351054fdd6a646d339dbdf3fd511e0ee968357` |
| New `MANIFEST.json` SHA-256 | `c1ab983de2566a11356c2b8d1e7d2b667c1cfce4661d1b5d27cfed4aad336e67` |
| Compiled anchor | `lcl_spec::anchor::APPROVED_PACKAGE_0_2_0.identity_digest` holds the new identity; its doc comment records both earlier identities |

Changed candidate bytes (SHA-256 prefix, old → new):

| File | Old | New |
|---|---|---|
| `02_LEXICAL/01_CHARACTER_ENCODING_AND_SOURCE_TEXT.txt` | `a7c46bd61cbf8fda` | `d0e033806f66c225` |
| `02_LEXICAL/13_LOCALIZED_SOURCE_AND_LOCALE_DIRECTIVE.txt` | `e8235bc7d85eac51` | `5fa95a08bf012fc7` |
| `CHANGELOG.txt` | `90e7a351b1402e20` | `dca6e59115e5bc8a` |
| `MANIFEST.json` | `2e029a0ce19bc6a5` | `c1ab983de2566a11` |
| `VALIDATION_REPORT.txt` | `f3f5d5347d6fa40e` | `68f702f05aebabb0` |
| `SHA256SUMS.txt` | `042b1130ede70092` | `9dc92f1c09f39f06` |

Historical identities, candidates under `releases/` and prior reports were not rewritten. The release gate was not marked true.

## Regression

`impl/crates/lcl-protocol/tests/dispatch.rs::invalid_utf8_is_attributed_to_core_0_1_0` replaces one ASCII byte with `0xFF` at a known original offset and asserts `("0.1.0", "error.encoding.invalid", that offset)` for seven positions:

| Case | Marker position |
|---|---|
| before VERSION | inside `LCL:` |
| inside VERSION | inside the `VERSION` keyword |
| inside the VERSION literal | inside `"0.2.0"` |
| immediately after a complete VERSION | first byte of the next line |
| late in the document | in the `VALUE:` line's indent |
| inside the locale directive | inside `@locale` |
| inside a localized VERSION | inside `VERSIJA`, after `@locale lv-LV` |
| after a complete localized VERSION | first byte after the `VERSIJA` line |

Two rows discriminate Reading A from Reading B rather than merely restating current output: "immediately after a complete VERSION" and "late in the document" both carry a complete readable `VERSION "0.2.0"` line before the first invalid byte, so Reading B would attribute them to `0.2.0`; the test asserts `0.1.0`. "after a complete localized VERSION" is the same shape under a locale directive, where Reading B is itself unclear because the declaration is only readable through the localization stage. The remaining rows (marker inside `LCL`, inside the `VERSION` keyword, inside its literal, inside `@locale`, inside `VERSIJA`) have no complete declaration before the defect and are `0.1.0` under either reading; they pin position-independence and the fact that a locale directive in the readable bytes selects nothing. Valid `0.1.0` and `0.2.0` documents (canonical and localized) stay in the dispatch matrix and are unchanged. Lexical offset behaviour for invalid UTF-8 remains covered by `lcl-lexer/tests/lexical_rules.rs`; it was not duplicated.

## Gates
| Command | Exit | Result | Log |
|---|---:|---|---|
| `cargo fmt --all -- --check` | 0 | clean | `F3-G-fmt.log` |
| `cargo clippy --offline --locked --workspace --all-targets -- -D warnings` | 0 | clean | `F3-G-clippy.log` |
| `cargo test --offline --locked --workspace --no-fail-fast` | 0 | 168 blocks, **1,695 passed, 0 failed, 1 ignored** | `F3-G-test-workspace.log` |
| MSRV 1.75.0 `cargo check --offline --locked --workspace --all-targets` | 0 | clean | `F3-G-msrv-check.log` |
| `cargo run -p lcl-conformance --example m8_conformance_report` | 0 | `CLAIM: source_conforming` | `F3-G-conformance-text.log` |
| `sha256sum -c --strict --quiet SHA256SUMS.txt` (0.2) | 0 | 215 files verify | `F3-G-sums.log` |
| `validate_language_contracts.py` | 0 | 514 checks, 0 violations | `F3-G-validate_language_contracts.log` |
| `validate_localization.py` | 0 | PASS | `F3-G-validate_localization.log` |
| `validate_source_fixtures.py` | 0 | PASS | `F3-G-validate_source_fixtures.log` |
| `validate_release.py --scope all` | 1 | 31 PASS, 0 FAIL, 2 OUT_OF_SCOPE, **1 BLOCKED** — `language_decisions_and_release_state`, `pending_decisions: ["independent_review"]`, unchanged from PRETEST-03 | `F3-G-validate_release.log` |
| protected areas (`canonical/LCL_Core_0.1.0`, `releases`, `assets`) | 0 | 0 entries | gate stdout |
| `impl/target/test-tmp` / `/tmp/lcl-apps` writes | — | 0 / 0 | gate stdout |

The three 0.2 validators produced byte-identical output before and after the normative edit (`diff F3-before-* F3-after-*`), so the added text changed no validated contract.

## Identities
- Core 0.1: `00d648b162939d06c44838481a67c39bc12c64bdd6d105035c24150148fe67ed`, 176 files — unchanged, immutable.
- Core 0.2: `00daee8de1919c4945ef04ff65edb22164bd8046a493be08a87d5fa3b4c3e604`, 216 files.

## Conformance
Untouched by this task; FINAL-02's state at entry HEAD is carried forward.
- claim: `source_conforming`
- source: 2011 required, 2011 satisfied
- semantic required: 402
- satisfied: 357
- failed: 0
- missing: 0
- invalid: 45

## Independent-review readiness record (for F29)

A reviewer of the Core 0.2 candidate needs five facts from this task:

1. **Selected rule.** F10 = Reading A, atomic decode, owner-supplied 2026-09-20. One rule, no alternative left in canon.
2. **Normative text changed.** `02_LEXICAL/01` (one bullet, the rule) and `02_LEXICAL/13` (one sentence, its localization consequence). Quoted verbatim above. Nothing else normative changed; the language accepted by the 0.2 candidate is unchanged for every valid document.
3. **Implementation behaviour.** Already conformant; no product code changed. Evidence: `scan.rs:91` (atomic decode, original-byte offset, zero tokens), `engine.rs:898` (no tokens → no declared VERSION → Core 0.1.0 reading), `lib.rs:1147` (no locale selection from invalid bytes).
4. **Package identity.** `6e730315…690ceb` → `00daee8d…c3e604`; 216 files both times; `MANIFEST.json` `2e029a0c…` → `c1ab983d…`; trust anchor updated in the same change.
5. **Validators and regressions.** All four 0.2 validators and `sha256sum -c` pass; `validate_release.py` remains BLOCKED only on this very review; the new discriminating regression is `invalid_utf8_is_attributed_to_core_0_1_0`.

Open for the reviewer, unchanged by this task: the localization additions themselves (`02_LEXICAL/13`, `localization_surface_v0.2.0.json`, `locale_profile_schema_v0.2.0.json`, the nine localization errors, the fixtures). Tests passing is not an independent review.

## Residual / out-of-scope
- **B3 / F29** — independent review of the Core 0.2 localization additions is still pending; it is an owner action and the single remaining `validate_release.py` BLOCKED.
- **B1 / F27** — 45 invalid semantic probes remain from FINAL-02. Not reopened here.
- The `label` field of `APPROVED_PACKAGE_0_2_0` still reads "(2026-09-15)", the candidate's creation date, matching `VERSION.txt`'s "Created: 2026-09-15". Regeneration dates live in the doc comment and the changelog, as in PRETEST-03.
- `VERSION.txt` was not touched: its integrity-metadata sentence already describes the regeneration order that was followed.

## AI quota / reuse
- Evidence reused: PRETEST-03's `identity.py`, `env.sh` and regeneration procedure; FINAL-01's `f1-gate.sh` was copied and re-pointed rather than rewritten; F01–F09 and F11–F26 were not re-audited.
- Broad gates: one integration gate run, after the targeted `--test dispatch` run proved the repair.
- Minimum-code notes: zero product-code changes, one new test function, two normative additions totalling 13 lines, and the mechanical integrity/anchor updates those bytes force.

## Proposed commit message
```
LCL FINAL-03: close F10 with owner decision A (atomic decode)

Canon now states one attribution rule for a source unit that is not
valid UTF-8: decoding is atomic, no VERSION is resolved, the unit keeps
its Core 0.1.0 reading, and error.encoding.invalid stays at the first
offending original byte. 02_LEXICAL/01 carries the rule and 02_LEXICAL/13
its localization consequence. The implementation already behaved this
way, so no product code changed; dispatch.rs now pins the decision with a
regression over seven byte positions including @locale, replacing the
"pending an owner decision" comment.

Core 0.2 integrity regenerated through generate_integrity.py; candidate
identity 6e730315...690ceb -> 00daee8d...c3e604 (216 files) and the
compiled trust anchor follows. Core 0.1 is untouched, historical
candidates and identities are preserved, and the release gate stays
false with independent_review still pending.

Gates: fmt 0, clippy -D warnings 0, workspace tests 0 (1,695 passed,
0 failed, 1 ignored), MSRV 1.75 check 0, 0.2 sums and three validators 0,
validate_release BLOCKED only on independent_review.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
```

## Next task
FINAL-04 is **unlocked** once the owner reviews and commits this change. FINAL-04 must build the candidate from that clean committed tree.

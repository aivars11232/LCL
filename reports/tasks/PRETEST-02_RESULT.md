# PRETEST-02 — Result

## Identity

- Task: PRETEST-02, PATH / WORKSPACE / capability-boundary correctness (F06–F08)
- Pack: `/mnt/F/LCL_PreTesting_Closure_Pack_v2_c50618a/`. All 20 `machine/SHA256SUMS.txt` entries verified. All five copy-paste prompts equal their task files after the shared rules header.
- Repository: `/mnt/F/LCL`, branch `main`
- Entry HEAD: `2de89ecff31afa6f6ff17c9b7153c79a6b557791` ("LCL pretest task1"), the owner's commit of PRETEST-01. Its 27 files match `PRETEST-01_RESULT.md`.
- Entry worktree: clean
- Exit HEAD: unchanged
- Exit worktree: 17 modified tracked files, 2 new test files and this report. Nothing under `canonical/`, `releases/` or `assets/` changed.
- Git writes: **NO**. Publication: **NO**. Background, sub or parallel agents: **NO**. Network or new dependencies: **NO**.
- Evidence logs: `/mnt/F/.lcl-pretest/logs/P2-*`
- Scratch:
  - `/mnt/F/.lcl-pretest/p2/`: `env.sh`, `p2-gate.sh`, and `fs.rs.keep` from the mutation check
  - `TMPDIR=/tmp/lcl-pretest-02`
  - `CARGO_TARGET_DIR` is the existing isolated `/mnt/F/.lcl-closure-4t-4c1cd4c659b7/target-current`

## Result

**Status:** PASS WITH OUT-OF-SCOPE FINDINGS

- F06, F07 and F08 are FIXED_VERIFIED with red-first regressions.
- One pattern rule and one containment authority serve every caller, so there are no parallel patches.
- The targeted gates passed.
- One existing conformance oracle encoded F06 itself. It was corrected against canon (see "Conformance oracle correction").

> The production claim stays `source_conforming`. PRETEST-04 owns conformance closure.

## Phase A — trace and reproductions (before any edit)

| Finding | Trace | Reproduction at entry |
|---|---|---|
| F06 | `Evaluator::matches` took any value with `Value::text()` as the subject. An absolute `PATH` is `Constructed{PATH}`, so its absolute text reached `Glob::matches`. That returned `Ok(false)` for any empty segment, and a malformed STRING did the same. `"a/../b"` was matched segment by segment. | `PATH("/case/src/a.py") MATCHES GLOB("**")` gave FALSE; `"/src/a.py" MATCHES GLOB("**")` gave FALSE (`P2-A-red-runtime.log`) |
| F07 | `stdlib::control::matches` had a second copy of the rule that admitted only `Value::Text`. It also split REGEX flags on `'\t'`, but the shared representation joins them with `REGEX_FLAG_SEPARATOR` (`'\0'`). | a `core.compare` WorkspacePath subject gave `error.operator.operand`; a malformed STRING gave FALSE (`P2-A-red-stdlib.log`); `"ABC"` against `REGEX("[a-z]+", "i")` gave FALSE (`P2-A-red-stdlib-flags.log`) |
| F08 | `PATH(REF(ws), rel)` keeps only `workspace`, `relative` and `resolved`. The host adapter handed `resolved` to `FileSystem`, whose methods take a bare `&Path`. `RealFileSystem::admit` checks containment on the resolved path against host grant scopes only. Preflight (`lcl-semantics::scope::contains`) and the runtime constructor are lexical and cannot see links. | Under a grant of `/`, a read through `ws/src/linked -> outside` succeeded, and so did a create at `ws/src/linked/new/escaped.txt` (`P2-A-red-containment.log`) |

## Root causes and repairs

### F06 and F07 — one subject rule, one pattern engine

**Root cause.** The operator and `core.compare` each had their own MATCHES. Neither implemented `types_v0.1.0.json#/pattern_profiles/GLOB/input`: "A STRING operand denotes either the empty relative path or nonempty slash-separated segments, with no empty, . or .. segment and no leading or trailing slash. A PATH operand requires an explicit WORKSPACE root retained by that value … An input that cannot supply this form uses error.operator.operand; no root is inferred."

**Repair.** `lcl_runtime::pattern::matches(input, pattern) -> Result<Value, MatchFault>` is the only implementation:
- UNKNOWN yields UNKNOWN.
- REGEX takes a STRING only, and splits flags on `REGEX_FLAG_SEPARATOR`.
- GLOB takes a subject from `glob_subject`:
  - STRING: its exact text, if it has the relative-segment form, otherwise `Operand`;
  - `WorkspacePath`: its normalized retained relative segments (`""` and `.` dropped, `..` popped, leaving the root is `Operand`);
  - anything else, including an absolute `PATH`: `Operand`.

The callers map faults to their own identifiers:
- `Evaluator::matches` gives `operator.operand`, `pattern.resource_limit` and `literal.invalid`.
- `core.compare` gives `operator.operand` and `pattern.resource_limit`. It maps an invalid pattern to `operator.operand` because `operations_v0.1.0.json#/contracts/core.compare/errors` does not register `error.literal.invalid`.

The duplicate `split_regex` in both crates is removed.

### F08 — WORKSPACE authority at the real filesystem

**Root cause.** The language root was dropped before the only layer that can see links, so a broad host grant was the only remaining gate.

**Repair.**
- `lcl-semantics`: `Value::WorkspacePath` gains `root`, the declared WORKSPACE root. It is not part of identity (`strict_equal` still compares `workspace` and `relative`), and `Display` is unchanged, so no serialized form changes.
- `lcl-capabilities::fs`:
  - `Location { path, within }` is a primitive; `From<&P: AsRef<Path>>` gives an unconfined location. The `FileSystem` methods take `Location`.
  - `RealFileSystem::admit` resolves the path once through the existing `resolve`, which follows links, dangling final links and nearest existing ancestors. Given `within`, it requires `grant::contains(resolve(root), resolved)` **before** the host grant, and returns `FsError::Escape` otherwise. The grant decision and the grant re-check then run on the same resolution the operation opens, so the WORKSPACE decision has no separate time-of-check window. The `O_NOFOLLOW` guarantee is unchanged.
- `lcl-stdlib`:
  - `host.rs` uses `Addressed` / `addressed()` to carry `root` for WORKSPACE-form targets and destinations into every filesystem call: read, inspect, create, write, append, modify, delete, rename, move, copy, download and upload/publish.
  - `fs_failure` maps `Escape` to `CapabilityOutcome::Refused { error.value.out_of_range }`. That is the registered constructor escape error, never `error.permission.denied`.
  - The `MemoryFileSystem` fixture applies the same order lexically.
- `lcl-conformance`: `SharedFs` forwards `Location`.
- The absolute `PATH` form stays confined by grants alone. Invariant 3: no workspace identity is inferred from its text.

## Conformance oracle correction

The first gate run (`P2-E-gate1.summary`, `P2-E-tests-owned-run1.log`) failed `semantic/operator_valid/MATCHES` sub-run `match/glob-absolute-path-false`, which expected `(PATH("/case/a.txt") MATCHES GLOB("*.txt")) == FALSE`.

- LCL-CLOSE-02, LCL-CLOSE-03 and REPAIR-05 had already recorded this oracle as an open question against canon. F06 now requires the canonical answer.
- The sub-run is now `match/glob-absolute-path-operand`: `Special::Rejects("error.operator.operand")` in the same MATCHES row, with the canonical citation. The mapping label was renamed and nothing else changed.
- `MAPPING_DIGEST` changed from `32b3e634…` to `27e3271fc2db2a86291c58e9fe55f54d24a130b71efa6b9e9f28d2b9acb6e385`. `reports/implementation/LCL_CONFORMANCE_OBLIGATIONS.md` cites the new digest; historical task reports are unchanged.
- This is a correction to canon, not a relaxation. No sub-run was dropped, reclassified as invalid or out of scope, or turned from a failure into an acceptance.

## Tests added or changed

| File | Tests | Red first |
|---|---|---|
| `lcl-runtime/tests/path_patterns.rs` (new) | 5: WorkspacePath subject (with `./` and `//`); absolute PATH rejected; malformed STRING rejected (leading or trailing slash, empty, `.`, `..`); STRING matching unchanged; UNKNOWN | 2/2 defect tests red (`P2-A-red-runtime.log`); 3 controls passed |
| `lcl-stdlib/tests/control_operations.rs` | +4: WorkspacePath through `core.compare`; absolute PATH and malformed STRING rejected; REGEX flags; MISSING projection | 3 red (`P2-A-red-stdlib*.log`); the MISSING control passed |
| `lcl-stdlib/tests/workspace_containment.rs` (new, unix) | 5 end-to-end through `Runtime` + `HostAdapter` + `RealFileSystem`: symlink read escape under grant `/`; create under a symlinked ancestor; legitimate nested path and in-workspace link; root spelled through a link; host grant still applies | 2/2 escapes red (`P2-A-red-containment.log`); 3 controls passed |
| `lcl-capabilities/tests/real_filesystem.rs` | 14 existing calls moved to `Location`; +6: `..` escape (read and create); symlink escape under grant `/` (read and delete; the absolute form stays grant-confined); new destination under a symlinked ancestor (write and copy); dangling link out of the WORKSPACE; legitimate nested, linked and linked-root paths; the WORKSPACE does not replace the grant | these need the new API. Red evidence is a mutation check: with the WORKSPACE check disabled, 4 escape tests failed (`P2-C-red-mutation-capabilities.log`). The file was restored byte-for-byte and the tests rerun green. |
| `lcl-conformance/src/semantic_cases.rs` | oracle correction above | red in gate run 1 |

## Files changed

| File | Change |
|---|---|
| `lcl-runtime/src/pattern.rs` | `MatchFault`, `matches`, `glob_subject` |
| `lcl-runtime/src/eval.rs` | `Evaluator::matches` delegates; `split_regex` removed; WorkspacePath carries `root` |
| `lcl-runtime/src/lib.rs` | exports `MatchFault` |
| `lcl-stdlib/src/control.rs` | `core.compare` MATCHES delegates; second implementation and `split_regex` removed |
| `lcl-semantics/src/value.rs`, `eval.rs` | `WorkspacePath.root` |
| `lcl-capabilities/src/fs.rs`, `lib.rs` | `Location`, `FsError::Escape`, WORKSPACE check in `admit` |
| `lcl-stdlib/src/host.rs` | `Addressed` through every filesystem call; `Escape` mapped to `value.out_of_range` |
| `lcl-stdlib/src/fixtures/fs.rs` | `Location`, lexical WORKSPACE check |
| `lcl-conformance/src/operation_cases.rs` | `SharedFs` forwards `Location` |
| `lcl-conformance/src/semantic_cases.rs`, `obligations_v0.1.0_r2.json`, `obligations.rs` | oracle correction; `MAPPING_DIGEST` |
| `reports/implementation/LCL_CONFORMANCE_OBLIGATIONS.md` | mapping digest |
| tests | as listed above |

## Commands and exit results

The final gate is `/mnt/F/.lcl-pretest/p2/p2-gate.sh`, with summary `P2-E-gate.summary`. Toolchains: cargo 1.98.1 and 1.75.0.

| Gate | Command | Exit | Result |
|---|---|---:|---|
| fmt | `cargo fmt --all -- --check` | 0 | clean |
| workspace compile | `cargo check --offline --locked --workspace --all-targets` | 0 | every crate and target compiles |
| clippy | `cargo clippy --offline --locked -p lcl-semantics -p lcl-runtime -p lcl-capabilities -p lcl-stdlib -p lcl-completion -p lcl-conformance --all-targets -- -D warnings` | 0 | clean |
| owned tests | `cargo test --offline --locked` for the same 6 crates, `--no-fail-fast` | 0 | 68 targets, **756 passed, 0 failed** |
| downstream tests | `cargo test --offline --locked -p lcl-protocol -p lcl-workspace -p lcl-cli -p lcl-hardening --no-fail-fast` | 0 | 42 targets, **339 passed, 0 failed** |
| MSRV | Rust 1.75.0 `cargo check --offline --locked --workspace --all-targets` | 0 | compiles |
| conformance | `lcl-conformance --no-fail-fast` (`P2-E-lcl-conformance.log`), and within owned tests | 0 | 71 passed; failed semantic set `[]`; claim `source_conforming` |
| Core 0.1.0 | `sha256sum -c --strict --quiet SHA256SUMS.txt` | 0 | untouched |
| Core 0.2.0 | same | 0 | untouched |

The first gate run failed exactly the two conformance tests that report the oracle above (754 passed, 2 failed); everything else passed.

`lcl-protocol` was run as a downstream suite. Value serialization was not touched: `Display` is unchanged, and no protocol code matches `WorkspacePath` fields.

## Protected material

- Core 0.1.0 checksums pass, and `git status` shows no `canonical/` entry. **Core 0.1 untouched.**
- Core 0.2.0 checksums pass; it is unchanged.
- `releases/` and `assets/` are unchanged, and no candidate was built.

## Out-of-scope findings, recorded rather than fixed

1. **Contained `..` in a WORKSPACE relative STRING is judged differently by preflight and runtime.** `lcl-semantics` `constructor_value` accepts a lexically contained `PATH(REF(ws), "a/../b")`. The runtime constructor refuses any `..` segment with `error.value.out_of_range`. Canon checks containment "on the resolved target", so the runtime refusal is stricter than canon for a contained form. This was found by code reading and not executed. The pattern subject rule handles either spelling. **Suggested owner: PRETEST-04 conformance, or an owner decision.**
2. **Process targets are not WORKSPACE-confined.** For a WorkspacePath, `core.execute`/`core.start` pass `resolved` to `RealProcess`, which checks the program grant only. That is not a filesystem read or effect, so it is outside F08's wording. **Suggested owner: PRETEST-04, or PRETEST-05 to judge.**
3. **Behavior change backed by canon:** a GLOB over a malformed STRING (leading or trailing slash, empty, `.` or `..` segment) is now `error.operator.operand` where it was FALSE, or even TRUE for `"a/../b"`. No existing test or conformance case depended on the old answer.

## Residual risks

- The threat model stated in `fs.rs` is unchanged: a *directory* along the path replaced by another process between resolution and the syscall is not defended. That now applies to the WORKSPACE decision exactly as it applies to grants.
- A WORKSPACE root that does not exist yet is judged by its nearest existing ancestor, through the same `resolve`.
- `MemoryFileSystem` checks containment lexically because it has no links. End-to-end link behavior is proven only with `RealFileSystem`.
- The `FileSystem` trait signature changed, from `&Path` to `Location<'_>`. All three in-tree implementors were updated. An out-of-tree implementor would need the same one-line change.

## Proposed commit message

```
PRETEST-02: shared MATCHES subjects and WORKSPACE containment at the filesystem

F06/F07: one lcl_runtime::pattern::matches for the MATCHES operator and
core.compare; GLOB subjects are a relative-form STRING or a WorkspacePath's
normalized relative segments, anything else error.operator.operand; REGEX
flags read from the shared separator.
F08: WorkspacePath keeps its declared root; FileSystem takes Location
{path, within}; RealFileSystem proves WORKSPACE containment on the resolved
target before host grants (FsError::Escape -> error.value.out_of_range).
Conformance: match/glob-absolute-path-false corrected to -operand per
pattern_profiles/GLOB/input; MAPPING_DIGEST re-pinned.
```

# LCL-TASK-0017 IMPLEMENTATION RESULT

**Status: COMPLETE**

M8 — VERIFY/TEST/evidence/completion and executable semantic conformance.

## Repository state

| Fact | Value |
| --- | --- |
| Root | `/mnt/F/LCL` |
| Branch | `main` |
| Baseline HEAD (task start) | `307c6c53b98904dfb41dadeb33958d3ff2b4ae29` ("LCL task 16") |
| HEAD at this report | `36d1655e37a43849c4dad524a8f2588def1f113c` ("LCL task 17 part1") |
| Upstream | `307c6c53b98904dfb41dadeb33958d3ff2b4ae29` |
| Working tree | phases E and F uncommitted; nothing staged, committed, tagged or pushed |

The user committed phases A–D mid-task as `LCL task 17 part1`. Phases E and F are
in the working tree awaiting the user's Git closure.

## Predecessor evidence

LCL-TASK-0016 was verified closed from actual repository state before planning:
921 workspace tests passing, `cargo fmt --all -- --check` clean, `cargo clippy
--workspace --all-targets -- -D warnings` clean, canonical validator 31 PASS /
0 FAIL / 0 BLOCKED / 2 OUT_OF_SCOPE with `release_ready: true`, and
`sha256sum -c SHA256SUMS.txt` exit 0.

## Internal phases

All six completed.

| Phase | Subject | Gate |
| --- | --- | --- |
| A | Post-execution observation model | green |
| B | VERIFY and TEST | green |
| C | Evidence and success/failure | green |
| D | Terminal status and outputs | green |
| E | Executable conformance runner | green |
| F | Semantic execution gate | green |

## Architecture

M8 is a new crate, `lcl-completion`, confirmed with the owner before
implementation. `LCL_IMPLEMENTATION_ARCHITECTURE_AND_CONTRACTS.md` names
`lcl-completion` and defers crate-versus-module to Stage A evidence. The
decisive evidence: `lcl-runtime` states its own boundary at step 10 in crate
documentation and in `terminal_status()`; a `Completion` can only be constructed
from an `Execution`, which makes stage monotonicity a property of the types; and
M8 owns exactly the three `verification_or_completion` identifiers, which keeps
the runtime's parity-tested mirror at its existing 22.

One planned reason for the crate boundary turned out to be unnecessary and is
recorded here because it simplified the result. A `TEST` root's referenced
`TASK`/`ACTION` is execution-bearing in
`block_schemas#/execution_graph_contract`, so M3 already places it in the
candidate graph and M6 already runs it in step 10. Completion therefore performs
only the comparison, needs no operation dispatcher, and reaches no host at all.

## Files changed, in change order

Phases A–D (committed in `36d1655`):

1. `impl/Cargo.toml` — new workspace member and the M8 scope note.
2. `impl/crates/lcl-completion/Cargo.toml`
3. `impl/crates/lcl-completion/src/{lib,diagnostic,contracts,observe}.rs`
4. `impl/crates/lcl-completion/src/{syntax,engine,check,test_root}.rs`
5. `impl/crates/lcl-completion/src/{evidence,success}.rs`
6. `impl/crates/lcl-completion/src/{terminal,outputs,complete}.rs`
7. `impl/crates/lcl-completion/tests/*` — seven suites
8. `impl/crates/lcl-runtime/src/execute.rs` — root lifecycle repair, plus
   `position_of` made public
9. `impl/crates/lcl-runtime/src/lib.rs` — re-export
10. `impl/crates/lcl-runtime/tests/state_and_results.rs` — regression test

Phases E–F (uncommitted):

11. `impl/crates/lcl-conformance/Cargo.toml` — full-stack dependencies
12. `impl/crates/lcl-conformance/src/lib.rs` — module wiring and claim reason
13. `impl/crates/lcl-conformance/src/runner.rs` — the executable case runner
14. `impl/crates/lcl-conformance/src/report.rs` — the conformance report
15. `impl/crates/lcl-conformance/tests/{common,runner_seam,witness_cases,decision_witnesses}`
16. `impl/crates/lcl-conformance/tests/descriptive_index.rs` — corrected claim assertion
17. `impl/crates/lcl-completion/examples/m8_report.rs`
18. `impl/crates/lcl-conformance/examples/m8_conformance_report.rs`
19. `impl/README.md`

No file under `canonical/` was modified. No canonical repair was required.

## Behavior implemented

**Step 11.** Post-execution `VERIFY` and `TEST` selection against what the
execution actually activated and observed, never against the candidate graph;
prerequisite topological ordering with `error.reference.cycle` on a cycle;
`WHEN` applicability where absence means TRUE and FALSE skips; `REQUIRED`
controlling blocking rather than whether a selected check runs;
`error.verification.failed` for a required FALSE assertion. A skipped check has
no result record and is never bound into scope, so a later read yields MISSING
through the ordinary lookup path rather than through a remembered rule.

**Step 12.** `EVIDENCE` collection restricted to declarations that something
which actually ran referenced, with `error.evidence.missing` for an unresolved
required one; `SUCCESS` over `ALL`/`ANY`/`NONE` under three-valued logic;
`FAILURE` clause selection in source declaration order with
`error.required.missing` and `error.value.unknown` for MISSING and UNKNOWN
conditions, and `ERROR` treated as classification metadata that emits nothing.

**Step 13.** Exactly one terminal status per invocation, decided in canonical
precedence: a primary unhandled diagnostic first, then a declared `FAILURE`
mapping subject to terminal, non-success, `allowed_next` and root-scope checks
with `error.execution.order` on an illegal request, then success only when
`SUCCESS` is TRUE and every required output, rule and evidence obligation holds,
and otherwise `error.success.unsatisfied`. Status aliases resolve through
`BASE` before any of those contracts apply. Declared root outputs are published
under producer ownership, with a loop-local output reported ambiguous rather
than silently resolved.

## Canonical sources implemented

- `05_SEMANTICS/10_VERIFY_TEST_EVIDENCE_SUCCESS_FAILURE_AND_STATUS.txt`
- `statuses_and_errors_v0.1.0.json#/check_selection_contract` (all seven clauses)
- `statuses_and_errors_v0.1.0.json#/failure_lifecycle` (status, failure-mapping,
  terminal-invocation, output-binding and indeterminate-state rules)
- `statuses_and_errors_v0.1.0.json#/statuses` and `#/event_model/alias_rule`
- `05_SEMANTICS/05_INPUT_DATA_OUTPUT_RESULT_AND_FORMAT.txt`, output ownership
- `block_schemas_v0.1.0.json` and `field_signatures_v0.1.0.json` for
  VERIFY/TEST/EVIDENCE/SUCCESS/FAILURE/OUTPUT
- `04_GRAMMAR/07_RULE_CHECK_OUTPUT_AND_COMPLETION_FORM.txt`
- `09_CONFORMANCE/01_CONFORMANCE_REQUIREMENTS.txt` for the claim contract

## Verification

| Gate | Result |
| --- | --- |
| `cargo test --offline --workspace --all-targets` | **1015 passed, 0 failed, 0 ignored** |
| `cargo fmt --all -- --check` | clean |
| `cargo clippy --offline --workspace --all-targets -- -D warnings` | clean |
| `validate_release.py --scope all` | 31 PASS / 0 FAIL / 0 BLOCKED / 2 OUT_OF_SCOPE, `release_ready: true` |
| `sha256sum -c SHA256SUMS.txt` | exit 0 |
| `cargo run -p lcl-completion --example m8_report` | 13/13 examples, one terminal status each |
| `cargo run -p lcl-conformance --example m8_conformance_report` | executed evidence rendered |

Baseline was 921 tests; the task adds 94. `lcl-completion` contributes 73 and
`lcl-conformance` 29.

Runtime evidence, not inspection: all thirteen valid canonical examples run
bytes-to-terminal-status through the real engine. Seven end `status.succeeded`,
five `status.failed` and one `status.blocked`, each for a stated reason. A failing
example is truthful rather than hidden: the deterministic host installs no real
adapter, so an operation that needs one reports a limitation and the root's
required `VERIFY` then records FALSE.

## Acceptance criteria

| Criterion | Result | How |
| --- | --- | --- |
| VERIFY/TEST selection follows actual activation/observation | PASS | executed — `verify_and_test.rs`, targeted checks over activated producers, observed targets and unreached targets |
| Skipped checks are absent, not implicitly true | PASS | executed — a skipped check has no result and reads MISSING in `SUCCESS` |
| Earlier primary diagnostic is not overwritten by SUCCESS/FAILURE | PASS | executed — a division-by-zero run with a TRUE `FAILURE` clause keeps the diagnostic's status |
| Root/non-root skipped/blocked/failed/completed terminal rules | PASS | executed — alias resolution, `allowed_next`, root-scope refusal of `status.skipped` |
| Declared outputs have correct producer/ownership/publication | PASS | executed — published, unbound and ambiguous cases |
| `semantic_case_execution` is an actual implementation gate | PASS | executed — `decision_witnesses.rs` runs 64 probes over 53 witnesses |
| Conformance reports distinguish descriptive from executed | PASS | executed — separate types, separate counters, no total |

## Executable semantic conformance

All 66 decision witnesses are accounted for in exactly one population, and the
gate asserts the accounting is total and exact against the canonical catalog.

| Population | Witnesses | Probes |
| --- | --- | --- |
| Executable and passing | 46 | 57 |
| Executable, currently failing on a named gap | 7 | 7 |
| Descriptive only, with a stated reason | 13 | — |

The gate fails in both directions. A witness that fails without being listed
fails the suite, and a listed witness that starts passing also fails the suite
until the list is corrected. That second direction already earned its keep:
CLOSURE-025 was provisionally listed as a gap from a bad fixture of mine, and
the gate forced it back to executable once the fixture used the canonical `STEP`
nesting.

The 799-entry requirements index remains descriptive. `CaseState` still has one
variant, `NotExecuted`, so no indexed entry can carry a verdict.

## Defects found and repaired

**M6 execution root reached `status.succeeded` before completion ran.** Step 10
moved a finished root out of `status.running`, which pre-empted the completion
contract three ways: `check_selection_contract/lifecycle` requires a root to be
running when post-execution completion begins; `05_SEMANTICS/10` permits root
`status.succeeded` only when SUCCESS, outputs, rules and evidence all hold, none
of which step 10 evaluates; and `failure_mapping_rule` requires a declared
`FAILURE`'s status to be in the current state's `allowed_next`, which is empty
for a terminal status, so every declared mapping would have been illegal.
Repaired in `impl/crates/lcl-runtime/src/execute.rs`; non-root containers keep
their existing completion. Regression test added. No existing test asserted the
old behavior.

## Defects found and NOT repaired

Four pre-existing gaps in earlier milestones were discovered by the new gate.
Each is recorded in `witness_cases` with its exact missing behavior and owning
milestone, and each is outside this task's scope: M5, M6 and M7 are closed, and
contract section 3 directs a pre-existing out-of-scope failure to be reported
rather than fixed.

1. **Object-valued declarations never resolve** (M5, `lcl-semantics` data
   resolution). `Body::Nested` covers "an object-data value", but data resolution
   reads a `VALUE` only through `inline_expr`, which matches `Body::Inline`
   alone. Every read of an object-valued `DATA` or `INPUT` therefore yields
   MISSING. Blocks CLOSURE-005 directly and CLOSURE-021, CLOSURE-042 and
   CLOSURE-051 consequentially. This is the widest of the four.
2. **`core.read` ignores its `range` parameter** (M7, `lcl-stdlib`).
   `FileSystem::read` takes only a path and bounds, and no layer above applies
   the registered range contract, so the whole target content is returned and an
   inverted range raises no `error.value.out_of_range`. Blocks CLOSURE-048,
   CLOSURE-049 and CLOSURE-050.
3. **`core.append` does not enforce its content family** (M7, `lcl-stdlib`). The
   contract types `content` as `STRING|LIST[T]` and states that "BYTES is a count
   and is not content", yet a `BYTES` content parameter is accepted and the
   operation completes. Blocks CLOSURE-052.
4. **`SUM` over a declared empty typed collection yields UNKNOWN** (M6,
   `lcl-runtime`) instead of emitting `error.operator.operand`, although the
   registry gives `SUM` `minimum_count: 1` and names that identifier. Blocks
   CLOSURE-015.

## Remaining limitations

Thirteen witnesses stay descriptive, each with its reason recorded. The reasons
fall into three groups: witnesses needing a host scripted to fail (retry,
handler and partial-output witnesses, CLOSURE-022/023/024/027/059), witnesses
needing declaration syntax this build does not admit (CLOSURE-006, CLOSURE-055),
and witnesses needing test data the canonical text does not supply
(CLOSURE-004, CLOSURE-058, CLOSURE-060). Turning any of them into a pass would
require inventing the missing input, which the task's stop condition forbids.

No later task owns adjacent M8 functionality left incomplete. Steps 11 to 13 are
complete. The four gaps above belong to already-closed milestones and should be
scheduled as explicit repair tasks.

## Suggested commit message

```
Complete LCL execution lifecycle and semantic conformance

Add lcl-completion, canonical processing steps 11 to 13: post-execution
VERIFY and TEST against observed results, evidence resolution, SUCCESS and
FAILURE selection, and exactly one terminal invocation status with its
declared outputs.

Turn lcl-conformance from a descriptive index into an executable case
runner and add the semantic_case_execution gate, which runs 64 probes over
53 of the 66 decision witnesses and accounts for all 66.

Repair an M6 defect the gate exposed: step 10 moved an execution root to
status.succeeded before completion ran, pre-empting the completion contract
and making every declared FAILURE mapping an illegal transition.
```

The commit would contain only this task's changes.

## Confirmations

- No background, delegated, parallel, asynchronous, worker or sub-agent was used
  at any point, in planning or implementation.
- Nothing was staged, committed, tagged or pushed. The user controls Git
  closure.
- `canonical/` was not modified. Its validator and checksums are unchanged.
- No test was weakened, skipped or reclassified. One expected result was
  corrected: `descriptive_index.rs` asserted that the claim-blocked reason
  mentions "milestone M0", a sentence that became false once M8 supplied the
  engine it said did not exist. The replacement asserts the same blocked claim
  against the canonical threshold instead of a milestone name, and a new test
  pins the separation of indexed from executed evidence.

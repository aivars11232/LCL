# FINAL-01 result — FALLBACK correctness and test-gate hygiene

## Identity
- Pack: `/mnt/F/LCL_Final_PreTesting_Blocker_Closure_Pack_v3_825694f/`. All `MACHINE/SHA256SUMS.txt` entries verify.
- Entry HEAD: `825694f7316c20f0d2d4091d6ca03ab3878cf567` ("LCL p retest task 4"), the owner's commit of PRETEST-04. PRETEST-05 audited it read-only.
- Exit HEAD: unchanged.
- Entry status: clean (1,024 tracked files).
- Exit status: 9 modified tracked files plus this report (listed below).
- Git writes by agent: **NO**
- Agents or background workers: **NO**
- Network or new dependencies: **NO**
- Scratch: `/mnt/F/.lcl-pretest/f1/`, holding the gate script `f1-gate.sh`.
- Logs: `/mnt/F/.lcl-pretest/logs/F1-*`.
- `TMPDIR` was `/tmp/lcl-pretest-final01`, and `CARGO_TARGET_DIR` was the isolated `target-current`.

## Status
**PASS.** The B1 immediate root cause (the FALLBACK static check) and B5 (the gate race) are closed. O1 and O2 are closed.

B1's wider obligation, `semantics_conforming`, is not claimed here. It belongs to FINAL-02. The semantic probes are unchanged at 269 of 402.

## Findings

### B1 — FALLBACK operation-identifier sites were not checked
- **Canon.**
  - `05_SEMANTICS/06` says a FALLBACK "is a second invocation site with no target or parameter surface of its own … One operation identifier is legal only when the operation registers no required named parameter and its required target, if any, is supplied by the original handler-context binding above. An operation identifier that cannot satisfy its contract under those limits uses error.operation.parameter."
  - The HANDLER rule in `04_GRAMMAR/09_CORE_BLOCK_SCHEMAS_B.txt` and `06_STANDARD_LIBRARY/10` say the same.
- **Reproduction.**
  - The new red test fails: `FALLBACK core.move: []` (`F1-A-red-fallback.log`).
  - With the old binary (PRETEST-05), `lcl check` accepted `core.append`, `core.delete`, `core.write` and `core.move`.
- **Root cause.** `lcl-checker/src/operation.rs::invocation` judges ACTION and HANDLER *blocks*. FALLBACK is a HANDLER *field*, so nothing ever read it.
- **Repair.**
  - A new `operation::fallback` runs with each HANDLER. It reuses the existing `operation_identifier`, the registry contract (`check.contracts.operation`) and `admits_handler_context`.
  - An operation identifier gets `error.operation.parameter` for each required named parameter. It also gets one for a required TARGET whose type does not admit `REFERENCE[ACTION]` or `REFERENCE[meta.execution_unit]`.
  - The `REF` form stays judged where the referenced handler is written.
  - A custom operation's contract is its own, exactly as at ACTION.
  - Operation existence is already `error.operation.undefined` at resolution.
  - FALLBACK has no parameter or TARGET surface, so the unknown, duplicate, positional and value-rule checks cannot arise there.
- **Verification.**
  - `constructors_and_operations.rs::a_fallback_operation_identifier_is_an_invocation_site`:
    - illegal: `core.move`, `core.append`, `core.write`, `core.delete`, each `error.operation.parameter` and `status.invalid`;
    - legal: `core.stop`, whose target admits the execution unit, and `REF(handler.other)`.
  - `lcl check` on the PRETEST-04 and PRETEST-05 probe documents (`F1-A-cli-probes.log`): append, write and move report the missing `content` or `destination`, delete reports the missing TARGET, and `core.stop` passes.
- **Runtime.** No change is needed. A document with an illegal FALLBACK is now `status.invalid` before execution, so `handler.rs::fallback` never runs an unsatisfied contract.

### B5 — the `real_process` gate race had two root causes
1. **Test accounting.**
   - `many_flooding_children_in_sequence_leave_nothing_behind` asserted `zombie_children() == 0`. That count includes every child of the whole test process, and libtest runs sibling tests as threads of that process.
   - **Repair:** the test re-runs itself in a process of its own (`current_exe --exact <name> --test-threads=1`, env `LCL_PROCESS_FIXTURE`). This is the mechanism `inherited_pipe_case` in the same file already uses. It now counts only the children it started. The cleanup assertion itself is unchanged.
   - The isolated child is proven to run the body: `LCL_PROCESS_FIXTURE=<name> <bin> --exact <name>` gives 1 passed.
   - Correction to PRETEST-05: the "separate test process" comment at the former line 217 belongs to `inherited_pipe_case`, and it is accurate there.
2. **Product defect found while reproducing.**
   - Under concurrent load, a healthy command failed with `owned group observation failed: No such process (os error 3)`. It hit `many_flooding…` and `the_stream_bound_holds_while_the_output_is_being_read` (`F1-B-red-load.log`: 5 failed result lines).
   - **Root cause:** `process/linux.rs::remaining_group_members` lists `/proc` and then reads each `stat`. A process that exits in between fails the read with ESRCH, but only ENOENT was treated as "gone".
   - **Repair:** a small `vanished` predicate accepts ENOENT or ESRCH. A process that no longer exists is not a remaining group member. Every other I/O error still fails closed, and teardown and kill logic are untouched.
   - Unit test: `a_process_that_exits_during_the_group_scan_is_not_a_member`, red because the predicate was absent.
   - This is the same subsystem, and it directly blocked "no flaky required gate", so it was repaired here.
- **Verification.**
  - Stress: 8 concurrent copies of the binary × 5 rounds gave 40 of 40 ok (`F1-B-green-load.log`).
  - `--exact` ×5, `--test-threads=1` ×3 and default ×20 gave 28 of 28 ok and 0 failed (`F1-B-green-repeats.log`).
  - The full workspace gate, which runs this test in parallel under load, is green.

### O2 — misplaced `Contracts::load` doc
Documentation only. "Build the static-checking vocabulary…" moved back onto `load`.

### O1 — completion relabel: reachable, now repaired
- **Proof of reachability.**
  - I tabulated every `RuntimeError` the evaluator raises against `expression_demand_resolution.eligible_errors`, the registry stages and `CompletionError::ALL`.
  - `error.pattern.mismatch` is raised by `Evaluator::constrain`, the PRETEST-01 F01 path, and is demand-eligible ("A dynamically supplied value fails a declared GLOB or REGEX value constraint").
  - It was **not** mirrored by completion, so `report_demand_fault` returned false and dropped it. A required VERIFY then emitted `error.value.unknown`, and a FAILURE.WHEN or SUCCESS demand fault with it emitted nothing.
- **Red.** `verify_and_test.rs::a_required_verify_keeps_a_demanded_pattern_mismatch`, a supplied object violating its declared `PATTERN`, gave `left: ["error.value.unknown"]` (`F1-C-red-o1.log`).
- **Repair.** `CompletionError::PatternMismatch` was added in registry order, which preserves `stable_order`. The completion contracts load its row from the registry like every other mirrored id. The original registered demand diagnostic is now kept. No completion semantics changed.
- **Residual reachability.** Every identifier the evaluator raises is now either mirrored and demand-eligible, or `error.host.constraint` at the execution stage. The ineligible-fault branch cannot be reached from evaluator demand.
- **Conformance consequence.**
  - The `semantic/error_contract/error.pattern.mismatch` group enumerates each implementing component's default-status mirror. Completion is now one, so the runner executed a `completion` sub-run the pinned inventory did not list, and the probe became invalid (`F1-G-*`: 268 satisfied).
  - The pin was **extended** with `"completion"`, exactly as `error.value.out_of_range` and `error.pattern.resource_limit` already list it.
  - `MAPPING_DIGEST` is now `9b32a28b…a8ad`, the SHA-256 of the mapping file.
  - No run was removed, weakened or reclassified. The added run executes, compares `default_status` with the registry and passes, and the probe is satisfied again.

## Files changed
| File | Why | Reused mechanism |
|---|---|---|
| `impl/crates/lcl-checker/src/operation.rs` | `fallback` invocation-site check | `operation_identifier`, `Contracts::operation`, `admits_handler_context`, `StaticError::OperationParameter` |
| `impl/crates/lcl-checker/tests/constructors_and_operations.rs` | B1 regression | `common::check`, `ids` |
| `impl/crates/lcl-checker/src/contracts.rs` | O2 doc placement | — |
| `impl/crates/lcl-capabilities/src/process/linux.rs` | `vanished` (ESRCH) plus unit test | existing `/proc` scan |
| `impl/crates/lcl-capabilities/tests/real_process.rs` | isolated zombie accounting | the file's own `current_exe --exact` re-exec pattern |
| `impl/crates/lcl-completion/src/diagnostic.rs` | mirror `error.pattern.mismatch` | `CompletionError::ALL`, registry-loaded rows |
| `impl/crates/lcl-completion/tests/verify_and_test.rs` | O1 regression | `task_document`, `execute_with`, `emitted` |
| `impl/crates/lcl-conformance/src/obligations_v0.1.0_r2.json` | add the executed `completion` mirror sub-run to the `error.pattern.mismatch` pin | existing pin format |
| `impl/crates/lcl-conformance/src/obligations.rs` | `MAPPING_DIGEST` | — |
| `reports/tasks/FINAL-01_RESULT.md` | this report | pack template |

## Gates
| Command | Exit | Result | Log |
|---|---:|---|---|
| `cargo test -p lcl-checker` (red, then green) | 101 → 0 | 11 blocks, 122 passed | `F1-A-red-fallback.log`, `F1-A-green-checker.log` |
| `cargo test -p lcl-runtime -p lcl-conformance -p lcl-completion` (after A) | 0 | 36 blocks, 401 passed | `F1-A-green-neighbours.log` |
| `lcl check` FALLBACK probes | 1/1/0/1/1 | as canon requires | `F1-A-cli-probes.log` |
| `cargo test -p lcl-capabilities` | 0 | 8 blocks, 93 passed | `F1-B-green-capabilities.log` |
| `real_process` stress and repeats | — | 40/40 and 28/28 ok | `F1-B-green-load.log`, `F1-B-green-repeats.log` |
| `cargo test -p lcl-completion` (red, then green) | 101 → 0 | 9 blocks, 84 passed | `F1-C-red-o1.log`, `F1-C-green-completion.log` |
| `cargo test -p lcl-conformance` (after the pin) | 0 | green | `F1-C-green-conformance.log` |
| **Gate 1** `f1-gate.sh` | — | fmt 0, clippy 0, **tests 101** (1,673 passed, 1 failed: `production_report`, the pin above), MSRV check 0, semantic 268 of 402 | `F1-G-*` |
| **Gate 2** `f1-gate.sh` (final) | — | fmt 0, clippy `-D warnings` 0, **workspace tests 0: 168 blocks, 1,674 passed, 0 failed, 1 ignored**, MSRV 1.75 check 0, conformance 0, protected areas unchanged, 0 writes to `impl/target/test-tmp` or `/tmp/lcl-apps` | `F1-G2-*` |

Gate 2 did not run the MSRV *tests* (1.75.0), because the task asks for one current-toolchain test gate. The MSRV all-targets check is green. FINAL-04 and FINAL-05 own the full MSRV test gate.

## Identities
- Core 0.1: `00d648b162939d06c44838481a67c39bc12c64bdd6d105035c24150148fe67ed`. `canonical/LCL_Core_0.1.0` is untouched.
- Core 0.2: `6e7303157f5ba378b4e8c0b89852a7a81b00b6dd26b85cac36a8977532690ceb`. `canonical/LCL_Core_0.2.0` is untouched.

## Conformance
- Claim: `source_conforming`
- Source: 2,011 required, 2,011 satisfied
- Semantic required: 402
- Satisfied: 269
- Failed: 0
- Missing: 2
- Invalid: 131

These numbers are unchanged from PRETEST-05. `semantic/error_contract/error.operation.parameter` stays invalid. Its pinned sub-runs, including `behavior/fallback-target-omitted` and `behavior/fallback-operation-requires-named-parameter`, have no populated run yet. The checker now produces the canon outcome for them, so FINAL-02 can populate them without a further engine change.

## Residual and out-of-scope
- **FINAL-02:** the 750 unrun pinned sub-runs and the 54 records of `diagnostic_policy` and `failure_lifecycle`.
- **Unchanged:** O3 (spec package reads), F10 (the owner decision) and F28/F29.
- **Environment:** `/proc` ESRCH is Linux behaviour, and `linux.rs` is Linux-only.

## AI quota and reuse
- **Evidence reused:** the PRETEST-05 reproduction, the canon quotes and the `p4/docs/blocker` probe documents. No re-audit of F01–F09 or F11–F26.
- **Broad gates:** one integration gate, rerun once after the single conformance-pin fix. No MSRV test run.
- **Minimum code:** about 60 lines of production code across 4 files, with no new abstraction. Each test reuses its crate's helpers.

## Proposed commit message
```
LCL final task1

FINAL-01: FALLBACK invocation-site contract and gate hygiene

- B1: an operation-identifier FALLBACK is checked as an invocation site:
  a required named parameter, or a required TARGET no handler-context
  binding supplies, is error.operation.parameter (05_SEMANTICS/06).
- B5: real_process zombie accounting runs in its own process; the /proc
  group scan treats ESRCH like ENOENT for a process that exited mid-scan
  (a healthy command was reported as an observation failure under load).
- O1: completion mirrors error.pattern.mismatch, so a demanded declared
  PATTERN fault keeps its registered id instead of error.value.unknown;
  the error_contract pin gains the executed completion mirror sub-run.
- O2: Contracts::load documentation restored.
```

## Next task
FINAL-02 is **unlocked** after the owner commits.

No Git write operation (add, commit, push, tag, reset or any other) was performed.

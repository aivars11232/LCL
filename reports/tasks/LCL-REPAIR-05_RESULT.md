# LCL-REPAIR-05 — Result

## Identity

- Task: LCL-REPAIR-05, known semantic engine defects (B-15)
- Repository: `/mnt/F/LCL`
- Branch: `main`
- Entry HEAD: `9b3739e9968c2850201d3c7be4a214d762897e24` ("LCL repair task4"). The owner committed REPAIR-04: that commit holds exactly the 2 files and the result report that REPAIR-04 lists.
- Exit HEAD: unchanged
- Entry worktree: clean
- Exit worktree: 13 modified tracked files (listed below), plus this report. Nothing under `canonical/`, `releases/` or `reports/` changed apart from this report.
- Pack manifest: all 21 `MANIFEST_SHA256.md` hashes verified at the start.
- Git writes performed: **NO**
- Background/sub/parallel agents used: **NO**
- Evidence logs: `/mnt/F/.lcl-repair-6t/logs/R5-*`
- Scratch:
  - `/mnt/F/.lcl-repair-6t/tmp/r5/`:
    - `probe/`, a path-dependency-only crate that runs one assertion through `lcl_conformance::fixtures::assertion_task` and `Runner::run`;
    - the `*.decl` inputs.
  - `/mnt/F/.lcl-repair-6t/tmp/r5-gate.sh`

## Result

**Status:** BLOCKED (partial)

- Eight of the nine pinned sub-runs are FIXED_VERIFIED.
- `division/declared-bound` (root cause S4) is BLOCKED_WITH_OWNER_DECISION. On 2026-09-16 the owner chose to block S4 and continue with S5; see S4 below.

> Repair of the previously known semantic failures does not by itself establish full semantic conformance. Remaining missing/invalid/unexecuted semantic evidence is outside LCL-REPAIR-05.

## Phase 0 reconciliation

- The pinned failure set in `tests/production_report.rs` (`KNOWN_FAILED`) and `tests/semantic_cases.rs` (`KNOWN_FAILED_GROUPS`) matched the pack exactly: 8 probes, 9 sub-runs.
- RED baseline: `R5-BASE-semantic_cases.log`. Every one of the 9 sub-runs failed. 191 groups, 1,010 sub-runs executed.

| Sub-run | Observed on entry |
|---|---|
| `equality/path-address-form-identity` (`==`, `!=`) | check FALSE, `error.verification.failed` |
| `match/glob-workspace-path` | check FALSE, `error.verification.failed` |
| `constraint/negative-duration-result` | check FALSE, `error.verification.failed` |
| `division/declared-bound` | accepted, no diagnostic |
| `division/host-capacity`, `round/host-capacity` | check UNKNOWN, `error.verification.failed` |
| `pattern/resource-limit` | check UNKNOWN, `error.verification.failed` |
| `sum/unit-mismatch` | check UNKNOWN, `error.verification.failed` |

## Scoped findings

| Root cause / sub-runs | Reproduction | Repair | Verification | Disposition |
|---|---|---|---|---|
| **S1** PATH identity: `==` and `!=` `equality/path-address-form-identity` | Probe: `PATH(REF(workspace.case), "a.txt") == PATH("/case/a.txt")` was TRUE. Both evaluators flattened the WORKSPACE form into the absolute string `"/case/a.txt"`. | `lcl_semantics::Value::WorkspacePath { workspace, relative, resolved }`: identity is (workspace declaration id, exact relative STRING). `resolved` is the contained absolute target, used only for addressing. The runtime and preflight PATH constructors build it. `strict_equal` and preflight `equality` compare it. The stdlib address consumers (`params::classify`, `host::path_of`/`program_of`, `data` rename) take `resolved`. | Probe GREEN on all three conjuncts. `lcl-semantics` 136, `lcl-runtime` 225, `lcl-stdlib` 118 passed (`R5-S1-*`). | FIXED_VERIFIED |
| **S2** `match/glob-workspace-path` | Probe: `PATH(REF(workspace.case), "src/a.py") MATCHES GLOB("src/*.py")` was FALSE. The GLOB was matched against `"/case/src/a.py"`. | Runtime `matches`: a GLOB against a `WorkspacePath` uses its normalized relative segments (empty and `.` segments dropped). No root is inferred from an absolute string. | Probe GREEN, including `./src//a.py`. `lcl-runtime` 225 passed. `semantic_cases` shows the group passing (`R5-S2-*`). | FIXED_VERIFIED |
| **S3** `constraint/negative-duration-result` | Probe: `DURATION(1 s) - DURATION(2 s)` produced a negative DURATION. | Runtime `arithmetic`: a result that `order_profile::is_duration` identifies (the dedicated normalized unit, which no MEASURE shares) and that is negative faults with `error.value.out_of_range`. Negative MEASURE results are unaffected (probe). | GREEN after S5, primary `error.value.out_of_range` (`R5-S5-*`). | FIXED_VERIFIED |
| **S4** `division/declared-bound` | See the S4 section below. | None. | The sub-run stays pinned as the only known failure. | BLOCKED_WITH_OWNER_DECISION |
| **S5** `division/host-capacity`, `round/host-capacity`, `pattern/resource-limit`, `sum/unit-mismatch` | Faults were raised in a VERIFY `ASSERT`, but `lcl-completion::check::report_demand_fault` returned early. It dropped identifiers that `CompletionError` could not spell (`unit_mismatch`, `resource_limit`, `out_of_range`). It also dropped any identifier outside `expression_demand_resolution` (`host.constraint`, stage `execution`). | `CompletionError` mirrors the 7 remaining identifiers the shared evaluator can raise, in registry order, each with its registered stage verbatim: `host.constraint`, `literal.invalid`, `numeric.division_by_zero`, `numeric.non_terminating`, `numeric.unit_mismatch`, `pattern.resource_limit`, `value.out_of_range`. The gate still drops an ineligible identifier, except one registered at the `execution` stage itself. Completion does not relabel anything. | Probes GREEN, each with the original primary: `host.constraint` (status.blocked) ×2, `pattern.resource_limit`, `numeric.unit_mismatch`. `lcl-completion` 75 passed, including `every_mirrored_identifier_keeps_its_registered_stage` and `a_reused_identifier_is_not_relabelled_as_a_completion_error`. | FIXED_VERIFIED |

### Authority applied

- S1: `03_TYPES_AND_VALUES/03`, PATH equality: "WORKSPACE form uses the resolved workspace declaration identity and exact decoded relative STRING … Different forms are unequal. Dot-segment, link, and resource equivalence do not alter this identity."
- S2: `03_TYPES_AND_VALUES/07`: "A PATH input requires an explicit WORKSPACE root retained by the value or its declared context; its normalized relative segments are used. … No root or filesystem expansion is inferred."
- S3:
  - `03_TYPES_AND_VALUES/06`: "Subtracting one DURATION from another is valid only when the result is non-negative; otherwise evaluation produces error.value.out_of_range."
  - `operators_and_functions_v0.1.0.json#/operators/-/constraints[0]`.
- S5:
  - `statuses_and_errors_v0.1.0.json#/diagnostic_selection/expression_demand_resolution`: its context includes "verification, or completion step". Its `exclusion_rule` says eligible identifiers outside the map "retain their registered stage and default_status"; it does not say they are dropped.
  - `03_TYPES_AND_VALUES/06`: a host that cannot perform the exact computation "produces error.host.constraint".
  - `errors.error.host.constraint.stage` = `execution`.

### S4, blocked: why the pack's diagnosis does not match the code

The pinned source is `DATA OBJECT[REF(type.bounded)]`, where `ratio: 3 / 2` is declared against `FIELD … MAXIMUM: 1`, with assertion `TRUE`. Three facts, each probed (`r5/bounded*.decl`, `r5/blk-*.decl`):

1. **FIELD MINIMUM/MAXIMUM are never validated against object data, at any layer.**
   - `schema.rs` parses them into `FieldDecl`, but no code reads them.
   - `SchemaField.constraints` is always `Vec::new()`.
   - A literal `ratio: 1.5` is accepted as well.
   - `defining_schema` finds a type's schema by structural identity. Identity excludes constraints, so it cannot choose between two same-shape types with different bounds.
2. **The checker already knows `3 / 2` = 1.5 statically** (`expr.rs` `divide` carries the exact quotient). The value is not deferred, so the pack's premise ("repair the existing deferred-demand path") does not apply.
3. **No production code consumes `DemandObligation`s.** `Checked::deferred()` is read only by `examples/m4_report.rs`, so the checker's `DeclaredBound` deferrals are not enforced at demand.

Repairing S4 means implementing construction-time validation of object schema constraints: the type binding, nested objects, local SCHEMA, and supplied inputs. That is a feature-level change, not a bounded repair. Offered the choice, the owner chose to block S4 and continue with S5.

## Accounting changes (not oracle relaxation)

| File | Change | Why it is truthful |
|---|---|---|
| `tests/semantic_cases.rs`, `tests/production_report.rs` | The known-failure pins went from 8 probes / 9 sub-runs to 1 probe / 1 sub-run (`semantic/operator_invalid//` → `division/declared-bound`). Each removal was made only after that sub-run was observed GREEN. | The production report test still requires every other semantic probe to be satisfied, missing only for an unpopulated family, or invalid only for unrun pins. The claim is still asserted to be `source_conforming`. |
| `src/obligations_v0.1.0_r2.json`, `MAPPING_DIGEST` | `completion` was added after `runtime` in the 7 `semantic/error_contract/<id>` rows for the new mirrors. This follows the existing `error.operator.operand` row. The pinned sub-runs went from 3,717 to 3,724. The digest changed from `386c1499…6b20` to `32b3e634dd135784163b90dd94736fae4522b68945dbbfaede05e454296fbf8e` (447,081 bytes). | Each new pin is a real run: the completion mirror's `default_status` must equal the registry's. Without the pins, the report flagged them as unexpected sub-runs, and the probe became invalid. The r2 generator (`/mnt/F/.lcl-closure-4t-4c1cd4c659b7/review/r2/generate_r2.py`) depends on run labels recorded at T2-P4a, so the 7 rows were edited in place instead of regenerating 3,717 pins. |
| `lcl-runtime/tests/builtin_functions.rs` | `abs_keeps_a_durations_exact_unit` now uses `ABS(DURATION(90 s) - DURATION(30 s))`. It previously used `30 s - 90 s`. | That oracle contradicted `03_TYPES_AND_VALUES/06` (G-13): its intermediate is exactly the negative DURATION subtraction that S3 must reject. The test's subject, that ABS keeps a DURATION's unit, is unchanged. This failure appeared in the first integration gate, because the runtime suite had not been rerun directly after S3. |

## Files changed

| File | Why | Minimal change |
|---|---|---|
| `impl/crates/lcl-semantics/src/value.rs` | S1: the value model owns material identity | `WorkspacePath` variant; `text`, `family`, `Display` arms and a `strict_equal` arm |
| `impl/crates/lcl-semantics/src/eval.rs` | S1: the preflight PATH constructor and equality | build `WorkspacePath`; equality arm delegating to `strict_equal` |
| `impl/crates/lcl-runtime/src/eval.rs` | S1 constructor; S2 GLOB input form; S3 DURATION result range | three local edits |
| `impl/crates/lcl-stdlib/src/params.rs`, `host.rs`, `data.rs` | S1: the address consumers of a PATH value | one `WorkspacePath` arm each (`resolved`) |
| `impl/crates/lcl-completion/src/diagnostic.rs` | S5: the completion mirror | 7 reused variants, `ALL`, spellings, module doc |
| `impl/crates/lcl-completion/src/check.rs` | S5: the drop gate | keep execution-stage identifiers; doc comment |
| `impl/crates/lcl-conformance/tests/semantic_cases.rs`, `tests/production_report.rs` | pin accounting | see above |
| `impl/crates/lcl-conformance/src/obligations_v0.1.0_r2.json`, `src/obligations.rs` | mirror sub-run pins | see above |
| `impl/crates/lcl-runtime/tests/builtin_functions.rs` | test oracle contradicted canon | see above |

## Evidence

| Gate | Command/Test | Exit | Result |
|---|---|---:|---|
| RED baseline | `cargo test -p lcl-conformance --test semantic_cases -- --nocapture` (`R5-BASE-semantic_cases.log`) | 0 | 9 FAIL lines, matching the pins |
| S1 regressions | `cargo test -p lcl-semantics / lcl-runtime / lcl-stdlib --no-fail-fast` (`R5-S1-*`) | 0 | 136 / 225 / 118 passed |
| S2 regressions | `lcl-runtime`, `semantic_cases` (`R5-S2-*`) | 0 | 225 passed; 6 FAIL lines left, all pinned |
| S5 regressions | `lcl-completion`, `semantic_cases` (`R5-S5-*`) | 0 | 75 passed; only `division/declared-bound` fails |
| Conformance crate | `cargo test -p lcl-conformance --no-fail-fast` (`R5-S5-lcl-conformance.log`) | 0 | 71 passed, 9 targets; includes `production_report` (4) and `obligations` loader cross-checks |
| Integration: tests | `r5-gate.sh`: semantics, runtime, stdlib, completion, conformance, protocol, hardening, workspace, cli (`R5-INT-*`) | 0 except runtime 101 | runtime 224/1: the ABS oracle above. After the oracle correction: `R5-INT-lcl-runtime-rerun.log` exit 0, 225 passed. Others: 136, 118, 75, 71, 83, 81, 111, 63 passed, 0 failed. |
| Integration: clippy | `cargo clippy -p lcl-semantics -p lcl-runtime -p lcl-stdlib -p lcl-completion -p lcl-conformance --all-targets -- -D warnings`, then `-p lcl-runtime` again after the oracle edit | 0 | clean |
| Integration: fmt | `cargo fmt --all -- --check` (`R5-INT-fmt-rerun.log`) | 0 | clean. rustfmt was applied only to 3 edited files. |
| Production claim | `production_report` tests | 0 | claim `source_conforming`; failed set = `{semantic/operator_invalid//}` with exactly `division/declared-bound` |
| Protected | `R5-INT-protected.log` | 0 | see below |

## Protected-material check

- Core 0.1.0: `sha256sum -c SHA256SUMS.txt` OK. No diff. `package_identity == APPROVED_PACKAGE.identity_digest` is asserted by `production_report`.
- Core 0.2.0: `sha256sum -c SHA256SUMS.txt` OK. No diff.
- Existing candidates: no tracked or untracked change under `releases/`.
- Historical evidence:
  - no change under `reports/` except this new file;
  - the prior digest `386c1499…` remains cited in historical reports and candidate inventories, which is correct for those states.

## Out-of-scope findings

1. **Absolute-form PATH with GLOB.**
   - `03_TYPES_AND_VALUES/07` says: "An operand unable to supply this form uses error.operator.operand".
   - The runtime instead returns FALSE for `PATH("/case/a.txt") MATCHES GLOB("*.txt")`.
   - The existing passing sub-run in `semantic_cases.rs` (`(PATH("/case/a.txt") MATCHES GLOB("*.txt")) == FALSE`) pins that behavior. Its oracle appears to disagree with canon.
   - Separately, the stdlib operation-level `control.rs::matches` refuses every PATH subject.
   - Not changed.
2. **Object schema constraints are never validated** (S4 detail above): FIELD MINIMUM/MAXIMUM, and by the same code path TOLERANCE/PATTERN, on object data. Affected: `lcl-checker/src/schema.rs`, `declarations.rs` (`local_schema`, `defining_schema`, `object_body`).
3. **A DATA VALUE containing division is UNKNOWN at runtime.**
   - Preflight deliberately does not fold `/` (`lcl-semantics/src/eval.rs`: "M6 owns at demand").
   - The runtime's `declaration_value` returns the plan's resolution, UNKNOWN, and never evaluates the expression.
   - Probe: `DATA TYPE DECIMAL VALUE: 3 / 2`, then `REF(data.d) == 1.5`, records FALSE.
4. **`Checked::deferred()` has no production consumer.** The checker's `DeclaredBound`, `DeclaredPattern` and other deferrals are recorded but never enforced by a demanding layer.
5. **Living-document drift.** `reports/implementation/LCL_CONFORMANCE_OBLIGATIONS.md` still cites mapping digest `386c1499…` and 3,717 sub-runs. The current values are `32b3e634…` and 3,724. Left for REPAIR-06 or owner reconciliation, not edited here.
6. **Unverified observation.** When a VERIFY `ASSERT` faults, completion also emits `error.verification.failed` with the check recorded as UNKNOWN. This predates the task (see the RED log). It was not investigated against `05_SEMANTICS/10`.

## Remaining blockers

- S4 / `division/declared-bound`: BLOCKED_WITH_OWNER_DECISION (owner, 2026-09-16). It requires a dedicated task for object-schema constraint validation. Findings 2–4 are related.

## Owner actions

- Decide whether and when to schedule S4 and out-of-scope findings 2–4 as a dedicated semantic task, instead of folding them into REPAIR-06 (B-16).
- Review the obligations-mapping edit and the new `MAPPING_DIGEST` (7 `completion` pins).
- Review the ABS test-oracle correction.
- Commit boundaries (O-03).

## Quota/efficiency notes

- Reused:
  - `lcl_conformance::fixtures::assertion_task` and `Runner`, through a tiny scratch probe, instead of new repository tests;
  - `order_profile::is_duration` for S3;
  - the existing reused-mirror design (`OperatorOperand`) for S5;
  - the `r4-gate.sh` structure for the integration gate;
  - the Task 4 compile cache.
- Deferred: full workspace tests, the Rust 1.75 gate and the conformance report command are REPAIR-06's.
- Avoided: regenerating the r2 mapping (3,717 pins) for a 7-row change, and repeated full-suite runs during repair.

## Handoff

- REPAIR-05 has not passed its acceptance contract, because one scoped sub-run is BLOCKED.
- By `00_START_HERE`, REPAIR-06 may begin only once the owner explicitly resolves this BLOCKED result. The owner's decision to block S4 and continue is recorded above. Whether that also unlocks REPAIR-06 is the owner's call.
- If REPAIR-06 proceeds, it still owes:
  - the Rust 1.75 gate for the REPAIR-03, 04 and 05 changes;
  - out-of-scope finding 5.

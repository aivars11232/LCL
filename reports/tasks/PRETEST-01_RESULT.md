# PRETEST-01 — Result

## Identity

- Task: PRETEST-01, semantic value, schema, demand and completion correctness (F01–F05)
- Pack: `/mnt/F/LCL_PreTesting_Closure_Pack_v2_c50618a/`. All 20 `machine/SHA256SUMS.txt` entries verified.
- Repository: `/mnt/F/LCL`, branch `main`
- Entry HEAD: `c50618a4e1dc759e7adfd1e46aeb2201aeb62460` ("LCL repair task 6"), the audit baseline
- Entry worktree: clean
- Exit HEAD: unchanged
- Exit worktree: 22 modified tracked files, 4 new test files and this report. Nothing under `canonical/`, `releases/` or `assets/` changed.
- Git writes: **NO**. Publication: **NO**. Background, sub or parallel agents: **NO**. Network or new dependencies: **NO**.
- Evidence logs: `/mnt/F/.lcl-pretest/logs/P1-*`
- Scratch:
  - `/mnt/F/.lcl-pretest/p1/`: `env.sh`, `p1-gate.sh`, the reused REPAIR-05 probe, and `a*.decl` reproductions
  - `TMPDIR=/tmp/lcl-pretest-01`
  - `CARGO_TARGET_DIR` is the existing isolated `/mnt/F/.lcl-closure-4t-4c1cd4c659b7/target-current`. Checkout-local `impl/target` is not used.

## Result

**Status:** PASS WITH OUT-OF-SCOPE FINDINGS

- F01–F05 are FIXED_VERIFIED, each with red-first regressions.
- `division/declared-bound` no longer fails.
- The targeted gates passed.
- One stop condition was recorded: TOLERANCE acceptance semantics (finding 1 below).

> This task does not establish `semantics_conforming`. The production claim stays `source_conforming`, and PRETEST-04 owns conformance closure.

## Phase A — reproductions (before any edit)

Each was run through the full pipeline with the reused REPAIR-05 probe (`assertion_task` plus `Runner`), on the baseline.

| Finding | Reproduction | Observed at baseline |
|---|---|---|
| F01 | `type.bounded` has `FIELD ratio MAXIMUM: 1`, and DATA writes `ratio: 1.5` or `3 / 2`; also a local SCHEMA `MINIMUM`, a REGEX `PATTERN`, and a nested object | accepted, `status.succeeded` |
| F01 | combined SCHEMA identical to its `DEFINE`, including `DEFAULT: 1` | false `error.object.schema` |
| F02 | `DATA VALUE: 3 / 2`, then `REF(data.d) == 1.5` | FALSE, because the value read back is UNKNOWN |
| F02 | optional INPUT `DEFAULT: 3 / 2` | read as MISSING |
| F02 | `DATA VALUE: 1 / REF(input.z)` with z = 0, demanded | no `error.numeric.division_by_zero` |
| F03 | `Checked::deferred()` consumers | none in production; only `examples/m4_report.rs` and tests |
| F04 | required VERIFY `UNKNOWN AND TRUE` | `error.verification.failed` |
| F04 | required VERIFY reading a skipped check (MISSING) | `error.required.missing` **and** `error.verification.failed` |
| F04 | required VERIFY `1 / (REF(output.seed) - 1) == 1` | `division_by_zero` **and** `error.verification.failed` |
| F05 | `FAILURE.WHEN` faulting, then a later `WHEN: TRUE` clause | fault discarded; `status.stopped` selected |
| F05, same root | a faulting VERIFY `WHEN` | silently `SKIPPED`, no diagnostic |

## Root causes and repairs

### F01 — object FIELD constraints parsed then discarded

**Root cause.**
- `local_schema` built every `SchemaField` with `constraints: Vec::new()`, and object data (`object_body`) never judged a field value against its schema.
- `defining_schema` chose a `DEFINE` by *structural* object identity. Constraints are not identity (`object_type_contract/constraints`), so the choice between two same-shaped types was arbitrary.
- `DEFAULT` identity used `format!("{:?}")`, which includes source spans. An identical combined SCHEMA was therefore never identical.

**Repair.**
- `schema.rs`: `SchemaField` carries the rendered constraints (`MINIMUM`, `MAXIMUM`, `TOLERANCE`, `PATTERN`) for identity, plus `declared: FieldConstraints` (expressions, declaring unit, nested schema) for construction. Defaults and constraints render with the span-free `lcl_parser::syntax::render`.
- `declarations.rs`:
  - `schemas` builds every direct `BASE OBJECT` definition's schema first. It then resolves each declaration's schema **nominally** through `nominal_object`, which follows `OBJECT[REF(..)]` and transparent `BASE` aliases, from `TYPE` or `SCHEMA`.
  - `object_body` judges each property with `judge_constraints` and hands a nested property its own type's schema.
  - `judge_constraints` is the former `declared_constraints` body, shared by block and object field. It emits `error.value.out_of_range` or `error.pattern.mismatch` for statically known values. Otherwise it records `DeclaredBound`/`DeclaredPattern`.
- `lib.rs`: public `FieldConstraints`, `ObjectSchema`, and `Checked::object_schema(declaration)`. Constraints stay out of `ObjectType`, so structural identity is unchanged.
- Runtime consumer (F03): `Evaluator::constrain` uses the evaluator's own `<`, `>` and `MATCHES`, so no second ordering or pattern rule exists. `constrain_object` recurses into nested schemas. Every declaration read validates its object value, whether declared, supplied or default.

### F02 — demand-time DATA expressions frozen as UNKNOWN

**Root cause.**
- Preflight's evaluator deliberately does not fold `/` ("M6 owns at demand") and stores UNKNOWN for an undecided `VALUE`.
- The runtime's `declaration_value` returned that stored value and never evaluated the expression.
- An undecided `DEFAULT` was dropped to MISSING.
- Preflight readers (VALIDATE, DEPENDENCY) read the placeholder as a decided UNKNOWN.

**Repair.**
- `lcl-semantics`:
  - `Resolution.undecided` marks a written `VALUE` or `DEFAULT` preflight could not decide.
  - An undecided `DEFAULT` is recorded as undecided, and no ASSUME replaces it.
  - `reference_value` answers "not decidable here" (`None`) for it.
- `lcl-runtime`:
  - `Evaluator::demand_declaration` is the fallible read. It evaluates an undecided `VALUE`/`DEFAULT` in its declaring unit, with runtime semantics. `REF(x)` demands through it, so faults carry their registered identifiers.
  - `declaration_value` stays the infallible reader.
  - The executor pre-demands every declaration an operation's target or parameters reference (`demand_referents`) before dispatch, so the stdlib's 13 read-through sites need no change.
  - Object bodies are evaluated by `Evaluator::object`. `Executor::object_value` now delegates to it rather than duplicating it.
- Evaluation stays lazy: an undemanded faulting declaration raises nothing (test below).

### F03 — deferred obligations without a production consumer

| `DemandKind` | Producer (M4) | Production consumer | Test |
|---|---|---|---|
| `DivisionValue` | `/` with an unknown operand | `Evaluator::divide` | `demand_obligations::division_value` |
| `MeasureUnit` | `same_units` | `Evaluator::arithmetic`/`compare` | `demand_obligations::measure_unit` |
| `NonemptyReduction` | SUM/MIN/MAX over non-literal | runtime reductions | `demand_obligations::nonempty_reduction` |
| `SetMemberOrder` | direct `FOR EACH` over `SET` | `Executor` SET snapshot ordering | `demand_obligations::set_member_order` |
| `DeclaredBound` (constructor rows, ROUND digits) | `constructor`, `round_value` | `Evaluator::constructor`/`round` | `demand_obligations::declared_bound_of_a_constructor` |
| `DeclaredBound`/`DeclaredPattern` (object FIELD) | `judge_constraints` from `object_body` | **new** `Evaluator::constrain_object` at declaration demand | `declared_constraints::*_object_*` (3) |
| `DeclaredBound`/`DeclaredPattern` (PARAMETER `VALUE`) | `judge_constraints` from `declared_constraints` | **new** `Evaluator::constrain` in `Executor::parameters`, before dispatch | `declared_constraints::a_deferred_parameter_pattern_is_judged_before_dispatch` |
| `ConstructorValue` (DATE, TIME, DATETIME, URI, GLOB, REGEX text) | `constructor` | **new**: `Evaluator::constructor` applies `Lexicon::validate_constructor_argument`, the lexical stage's own closed literal profiles (`literal::argument`, now shared by `scan.rs`) | `demand_obligations::constructor_value` (red first) |
| `ConstructorValue` (one-STRING `PATH` form) | `path_value` | **new** arm in `Evaluator::constructor` | `demand_obligations::constructor_value_of_a_path` (red first) |
| `RequiredValue` | **none**: no code constructed it | redundant, **removed** | compile |

The audit found the `ConstructorValue` gap. Before the repair, `DATE(REF(input.text))` with `"not a date"` built a DATE value.

### F04 — required VERIFY conflated FALSE with UNKNOWN, MISSING and fault

**Root cause.** `CheckOutcome::blocks()` is `required && !held`, and `evaluate` emitted `error.verification.failed` for every blocking outcome. That included UNKNOWN, and MISSING and faults that had already been reported. `TEST` roots also discarded faults and MISSING.

**Repair.**
- `check.rs::demand_assertion` is shared by VERIFY `ASSERT` and TEST `ASSERT`. It returns the domain outcome and whether its cause was already reported: MISSING gives `error.required.missing`, and a fault gives the evaluator's identifier through `report_demand_fault`.
- A blocking outcome not already reported emits `error.verification.failed` only for FALSE, and `error.value.unknown` otherwise.
- TEST `EXPECTED`/`ACTUAL` faults are reported the same way.
- `report_demand_fault` now returns whether it emitted.

### F05 — FAILURE.WHEN silently discarded evaluator faults

**Root cause.**
- `select_failure` matched `Ok(_) | Err(_) => continue`.
- Same root in the same crate: VERIFY `WHEN` (`applicability`) mapped `Err(_)` to a silent skip, and root `SUCCESS` used `unwrap_or(Value::Unknown)`.

**Repair.** All three report the fault through `report_demand_fault`, which is the ordinary completion diagnostic path, before continuing. The emitted primary fixes status, so a later FAILURE clause cannot select one.

## Tests added or changed

| File | Tests | Red first |
|---|---|---|
| `lcl-checker/tests/object_constraints.rs` (new) | 9: named type, quotient (the pinned source), alias, same-shape types, local SCHEMA, nested, PATTERN, deferral, combined SCHEMA | 9/9 red (`P1-B-red-checker*.log`) |
| `lcl-runtime/tests/declaration_demand.rs` (new) | 6: quotient, constant, undecided DEFAULT, demanded fault, operation target, undemanded control | 5/5 red; the control passed (`P1-C-red-runtime.log`) |
| `lcl-runtime/tests/declared_constraints.rs` (new) | 4: deferred field bound, nested, supplied object, PARAMETER pattern | 4/4 red (`P1-B-red-runtime*.log`) |
| `lcl-runtime/tests/demand_obligations.rs` (new) | 7: one per retained `DemandKind` consumer | `constructor_value` and `constructor_value_of_a_path` red (`P1-C-audit-obligations.log`, `P1-C-red-path.log`); 5 audit existing consumers |
| `lcl-semantics/tests/data_resolution.rs` | +2: undecided DEFAULT recorded; DEPENDENCY invents no failure | the DEPENDENCY test is red with the guard removed (`P1-C-red-semantics.log`) |
| `lcl-completion/tests/verify_and_test.rs` | +6: required UNKNOWN, MISSING, fault, optional UNKNOWN control, faulted `WHEN`, TEST root fault | 5/5 red plus a passing control (`P1-D-red-completion2.log`) |
| `lcl-completion/tests/success_and_failure.rs` | +2: faulted `FAILURE.WHEN`, faulted `SUCCESS` | 2/2 red (`P1-D-red-completion.log`) |
| `lcl-conformance/tests/semantic_cases.rs`, `production_report.rs` | known-failure pins emptied | see below |

**Pin change, not relaxation.**
- Before editing the pins, `semantic_cases` was run with the old pin and observed the failed set `[]` (`P1-E-semantic_cases-pinned.log`): `division/declared-bound` now passes and no other group fails.
- Both tests' own comments require the list to change on repair. Every other assertion, including the `source_conforming` claim and `MAPPING_DIGEST`, is unchanged.
- No expected result was weakened, and no test was disabled.

Fixture corrections made during red runs, all to new tests before any product change:
- `ALL: [TRUE]` changed to `ALL: TRUE`;
- `INPUT` changed to `DATA` in a `kind.data` document;
- a chained property access replaced;
- an object `DEFAULT` replaced by `REF(const)`.

## Files changed

| File | Change |
|---|---|
| `lcl-checker/src/lib.rs` | `FieldConstraints`, `ObjectSchema`, `Checked::object_schema`; `DemandKind::RequiredValue` removed |
| `lcl-checker/src/schema.rs` | rendered and declared constraints on `SchemaField`; `Schema::constraints` |
| `lcl-checker/src/declarations.rs` | `schemas`, `nominal_object`, `local_schema`, `judge_constraints`, `constraint_value`, `object_body` |
| `lcl-checker/src/contracts.rs` | `Contracts::lexicon()` accessor |
| `lcl-parser/src/syntax.rs` | `render`, moved verbatim from `lcl-runtime/src/syntax.rs` |
| `lcl-runtime/src/syntax.rs` | `pub use lcl_parser::syntax::render` |
| `lcl-lexer/src/literal.rs`, `scan.rs`, `lexicon.rs` | `literal::argument` extracted from `scan.rs`; `Lexicon::validate_constructor_argument` |
| `lcl-semantics/src/plan.rs`, `data.rs`, `eval.rs` | `Resolution.undecided`; undecided DEFAULT; `reference_value` |
| `lcl-runtime/src/eval.rs` | `demand_declaration`, `demand_referents`, `undecided`, `object`, `constrain`, `constrain_object`, constructor value validation |
| `lcl-runtime/src/execute.rs` | referent pre-demand; PARAMETER constraints; `object_value` delegates |
| `lcl-completion/src/check.rs`, `test_root.rs`, `success.rs` | F04/F05 |
| tests | as listed above |

## Commands and exit results

Gate script `/mnt/F/.lcl-pretest/p1/p1-gate.sh`. Summary: `P1-E-gate.summary`. Toolchains: cargo 1.98.1 and 1.75.0.

| Gate | Command | Exit | Result |
|---|---|---:|---|
| fmt | `cargo fmt --all -- --check` | 0 | clean |
| workspace compile | `cargo check --offline --locked --workspace --all-targets` | 0 | every downstream crate compiles |
| clippy | `cargo clippy --offline --locked -p lcl-lexer -p lcl-parser -p lcl-checker -p lcl-semantics -p lcl-runtime -p lcl-stdlib -p lcl-completion -p lcl-conformance --all-targets -- -D warnings` | 0 | clean |
| owned tests | `cargo test --offline --locked` for the same 8 crates, `--no-fail-fast` | 0 | 89 targets, **999 passed, 0 failed** |
| downstream tests | `cargo test --offline --locked -p lcl-protocol -p lcl-workspace -p lcl-cli -p lcl-hardening --no-fail-fast` | 0 | 42 targets, **339 passed, 0 failed** |
| MSRV | Rust 1.75.0 `cargo check --offline --locked --workspace --all-targets` (own target dir `target-msrv`) | 0 | compiles |
| conformance | `lcl-conformance` (within owned tests; earlier alone as `P1-E-lcl-conformance.log`) | 0 | 71 passed; claim `source_conforming`; failed set `[]` |

Earlier per-crate runs during the repair:
- checker: 121
- semantics: 136
- runtime: 231, and then more as tests were added
- stdlib: 118
- completion: 83

The full workspace test gate and the MSRV test run are PRETEST-04's integration gate and were not run here.

## Protected material

- Core 0.1.0: `sha256sum -c --strict --quiet SHA256SUMS.txt` exits 0. `git status` shows no entry under `canonical/`. **Core 0.1 untouched.**
- Core 0.2.0: `sha256sum -c` exits 0 and it is unchanged.
- `releases/` and `assets/` are unchanged, and no candidate was built.

## Out-of-scope findings, recorded rather than fixed (07 contract, scope-expansion rule)

1. **TOLERANCE acceptance is canonically underdetermined (stop condition).** Canon says `03_TYPES_AND_VALUES/07`: "TOLERANCE is absolute and non-negative; it validates a permitted numeric difference". `06` calls it "an absolute acceptance constraint". Neither text, the keyword reference, nor any registry or conformance case says what the difference is measured *from* for a FIELD or PARAMETER. TOLERANCE therefore takes part in combined-schema identity and keeps its static value-kind check, but no acceptance rule was invented. A `DeclaredBound` recorded for a non-static TOLERANCE value has no consumer for the same reason. **Owner or canon decision required.**
2. **FIELD and PARAMETER `DEFAULT` values are never applied by any later layer.** A search of `lcl-semantics`, `lcl-runtime`, `lcl-stdlib` and `lcl-completion` finds no read of a FIELD or PARAMETER `DEFAULT`. `object_type_contract` says "Apply defaults before comparing values". An absent optional object field therefore stays MISSING, and a constraint over an unknown `DEFAULT` has nothing to consume it. `judge_constraints` deliberately does not defer a `DEFAULT` subject (`demanded = false`), so no consumer-less obligation was added; statically known defaults are still judged. This is a new root cause (default application at construction). **Suggested owner: PRETEST-04 conformance, or an owner decision.**
3. **Preflight VALIDATE cannot evaluate `/`.** `VALIDATE ASSERT: REF(data.d) == 1.5`, with `data.d` = `3 / 2`, reports `error.required.missing` ("cannot be evaluated before effects") at preflight. That is before F02 and unchanged by it (`P1` probe `a2-validate.decl`). The pre-effect evaluator's division gap is independent of DATA freezing, and closing it means division semantics in the preflight evaluator. **Suggested owner: PRETEST-04 conformance.**
4. **Supplied invocation data is never type-validated.** `Invocation` values enter resolution unchecked. PRETEST-01 validates *constraints* of supplied objects at demand, but not their family or shape. **New root cause; suggested owner: PRETEST-04 or owner.**
5. **A custom operation's `DEFINE` PARAMETER constraints** are not applied to an invoking ACTION's parameter values. Only the ACTION PARAMETER's own `MINIMUM`/`MAXIMUM`/`PATTERN` are. This was not traced further.
6. **Optional VERIFY reading MISSING** still emits `error.required.missing`, as it did before. The canonical sentence "Required demanded MISSING and UNKNOWN …" can be read as either the required *check* or the required *demand*, so the existing behavior was kept.
7. **Consumer-less read paths.** `Evaluator::declaration_value` reads an undecided declaration as UNKNOWN only if its demand faults. Every production site that can report a fault demands first: REF evaluation, operation targets and parameters, and completion. A stdlib read of a store or check result does not, and none of those holds an undecided value today.

## Residual risks

- Object values are validated on every declaration read. That is exact but repeated, and it costs time, not correctness.
- The runtime PATH arm rejects a one-STRING relative PATH at demand. PRETEST-02 reworks PATH and WORKSPACE authority and must keep this obligation.
- The checker's constraint values for object fields are computed with a silent judgement (`constraint_value`) when the walk has not yet reached the defining `DEFINE`. The same expression is judged again, with diagnostics, where it is written.

## Proposed commit message

```
PRETEST-01: object schema constraints, demand-time values, VERIFY/FAILURE diagnostics

F01: carry FIELD MINIMUM/MAXIMUM/PATTERN into construction; nominal schema
lookup through aliases; span-free combined-schema identity.
F02: preflight marks undecided VALUE/DEFAULT; runtime demands and evaluates it.
F03: runtime consumers for object/PARAMETER constraints and dynamic
constructor text; RequiredValue removed; per-DemandKind audit tests.
F04: required VERIFY/TEST keep FALSE, MISSING, UNKNOWN and faults distinct.
F05: FAILURE.WHEN, VERIFY.WHEN and SUCCESS report evaluator faults.
Unpins division/declared-bound.
```

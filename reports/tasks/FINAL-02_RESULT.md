# FINAL-02 result — semantic conformance completion

## Identity
- Pack: `/mnt/F/LCL_Final_PreTesting_Blocker_Closure_Pack_v3_825694f/`.
- Entry HEAD: `5ca774e2f04500a3440710debbd30f4e60c10396` ("LCL"), the owner's commit of FINAL-01.
- Exit HEAD: unchanged.
- Entry status: 10 modified tracked files and 3 untracked paths, the uncommitted FINAL-01 work.
- Exit status: 29 modified tracked files and 4 untracked paths — the three new conformance modules and this report (listed below).
- Git writes by agent: **NO**
- Agents or background workers: **NO**
- Network or new dependencies: **NO**
- Scratch: `/mnt/F/.lcl-pretest/f2/`, holding `env.sh`, `verdict.sh`, the gate script `f2-gate.sh`, the live ledger `ROOT_CAUSES.md`, and a scratch probe crate outside the repository.
- Logs: `/mnt/F/.lcl-pretest/logs/F2-*`.
- `TMPDIR` was `/tmp/lcl-pretest-final02`, and `CARGO_TARGET_DIR` was the isolated `target-current`.

## Status
**BLOCKED.** The claim stays `source_conforming`.

Every pinned sub-run this build can construct now executes and passes: **2,413 executed, 2,413 passed, 0 failed**. The semantic level is **357 satisfied, 0 failed, 0 missing, 45 invalid**, and every one of those 45 probes is invalid for exactly one reason — **79 pinned sub-runs have no constructible input in this build**. Each is recorded below under the capability it waits on. No probe fails, none is missing a population, and no record is duplicated or unexpected.

`semantics_conforming` therefore needs the six engine capabilities in *Blocked* below, not further repair of what exists. That is a `BLOCKED_NEW_ROOT_CAUSE` boundary: each one is an architectural capability, not a defect in a row.

## What changed since FINAL-01
FINAL-01 left 269 of 402 semantic probes satisfied and two families unpopulated. This task:

1. **Populated the last two families.** `semantic/diagnostic_policy/core.error_selection` (24 sub-runs) and `semantic/failure_lifecycle/core.failure_lifecycle` (30) had no executed population at all. `lcl-conformance/src/lifecycle_cases.rs` is new and carries both. `production_report.rs::UNPOPULATED` is now empty.
2. **Authored the remaining pinned sub-runs** across `operation_binding`, `operation_errors`, `operation_effects`, `result_schemas` and `error_contract` — the bulk in `operation_cases/clauses.rs`, `result_cases/engine.rs` and `semantic_cases/behaviors.rs`.
3. **Repaired every engine defect those runs exposed** (ten root causes, below), each reproduced red first with a focused regression that stays in the tree.

## Findings

Each row was reproduced before repair, repaired one root cause at a time, and verified by a named test. The full working ledger, including the canon quotation for each, is `/mnt/F/.lcl-pretest/f2/ROOT_CAUSES.md`.

| Finding | Reproduction | Root cause | Repair | Verification |
|---|---|---|---|---|
| D-mem-type / D-state-mode | `operation_binding/core.memory_write binding/merge-false-type-match`, `operation_effects/core.state_update precondition/0..1` | `core.memory_write`/`core.state_update` ignored `MODE mode.read_only`, never compared the written or merged value with the declared type, and used `error.type.mismatch` where the row states a precondition | `params::{declaration_index, value_matches}`; `data::store` applies both | `data_operations.rs::store_rows_check_mode_and_declared_type_before_effects` (5 cases) |
| D-key-missing | `operation_errors/core.{group,sort} path/missing-key` | `property_path` could not tell an unregistered path from a declared field a member omits, so a MISSING key became the row's precondition | `params::declared_member_path`; `pure::key_of` resolves a declared-but-absent path to MISSING | `pure_operations.rs::a_declared_key_path_with_no_value_is_a_missing_key_not_an_unregistered_path` |
| D-criteria | `operation_errors/core.compare path/type-mismatch` | `core.compare` never read a criteria REFERENCE through; the four parameter defects canon names used the row precondition; `error.type.mismatch` was unreachable | `control::criterion` reads through, names `error.operation.parameter` for unknown keys, malformed paths, an absent operator and an unregistered token, and `error.type.mismatch` for a resolved value of neither admitted form | `control_operations.rs::{an_unregistered_criterion_is_refused, a_malformed_criteria_object_is_a_parameter_defect, a_criteria_reference_to_neither_admitted_form_is_a_type_defect}` |
| D-validate-schema-kind | `operation_errors/core.validate path/wrong-kind-schema-reference` | `core.validate` ignored its `schema` parameter entirely | New `Checked::declared_object_type` exposes the object type a `DEFINE kind.type` defines, from the schema the checker already builds; `control::validate` requires a `kind.type` whose resolved type is OBJECT and applies it to the target | `control_operations.rs::{a_schema_reference_of_the_wrong_kind_is_a_reference_defect, a_declared_schema_is_applied_to_the_target}` |
| D-verify-evidence | `operation_binding/core.verify binding/evidence-references`, `operation_errors/core.verify error/evidence.missing`, `failure_lifecycle phase/mixed/evidence.missing` | `core.verify` never recorded its `evidence` parameter, and step 12a collected evidence only from an ACTION's own `EVIDENCE` field | `schema::verification` records the bound list; `lcl-completion::evidence` also collects what an activated invocation named through an `evidence` parameter | `success_and_failure.rs::evidence_an_operation_parameter_named_is_required_too` |
| D-ask | `operation_effects/core.ask answer/{equals-listed-option,no-valid-answer-missing}`, `operation_errors/core.ask path/{missing-authoritative-answer,option-incompatible-with-expected-type}` | `core.ask` accepted any answer and any option: no `expected_type` check, no closed-option check, and an absent answer surfaced as a host limitation | The option precondition is the row's (`control::incompatible_option`); answer validity is the adapter's (`host::compatible_answer`), and no authorized valid answer is `error.required.missing` | `external_operations.rs::{no_authorized_valid_answer_leaves_the_value_missing, a_question_that_was_put_records_its_message_effect, an_option_incompatible_with_the_expected_type_refuses_before_the_question}` |
| D-cancel-stop | `operation_effects/core.cancel transition/{allowed-cancel,reason-recorded}`, `operation_errors/core.cancel path/cancellation`, `operation_effects/core.stop postcondition/0`, `failure_lifecycle phase/mixed/cancelled` | A committed `core.cancel` raised no `error.cancelled` and evidenced no reason; a committed `core.stop` left the root evaluating SUCCESS and ending `status.succeeded` | `execute::request_terminal_status` raises `error.cancelled` and carries the reason as the state effect's evidence; `terminal::resolve` recognises a root a declared stop path already moved to `status.stopped` | `success_and_failure.rs::{a_committed_cancel_ends_the_root_cancelled_and_evidences_its_reason, a_committed_stop_ends_the_root_stopped}`, `control_operations.rs::a_cancellation_reason_travels_with_the_state_effect_it_justified` |
| D-test-shape | `operation_binding/core.test binding/{target-alone,target-with-actual,target-with-assertion}-rejected`, `operation_errors/core.test {error/block.conditional_requirement,path/invalid-comparison-shape}` | An unsatisfied `core.test` comparison form used `error.operation.precondition`, which that row's closed `errors` list does not admit | The structural half is the parser's `error.block.conditional_requirement` (`conditional::comparison_form`); the type-dependent half — a material TARGET beside `actual` or `assertion` — is the checker's `error.operation.parameter` (`operation::comparison_target`) | `control_forms.rs::core_test_requires_exactly_one_comparison_form`, `constructors_and_operations.rs::a_material_test_target_cannot_accompany_actual_or_assertion`, and the two corrected stdlib tests |
| RC-custom-axes | `error_contract/error.operation.precondition behavior/custom-operation-{effect-outside-maximum,no-concrete-effect}` | A custom `kind.operation` invocation crossed to the host with no axis resolution at all | `data::custom_axes` resolves the invocation effect set from the target's address class and checks it against the declared `SIDE_EFFECT` maximum before effects | The two behaviour sub-runs, plus `error_contract` unchanged elsewhere |
| D-convert-axes / D-move-axes / D-execute-profile-union / RC-02 | `operation_effects/core.convert resolution/exact-invocation-axes`, `operation_effects/core.move address/{output-source-state,uri-network-network}`, `operation_effects/core.execute mode/non-graph-axis-union`, `operation_errors/{core.analyze,core.report,core.verify,core.publish} precondition/profile-out-of-bounds` | `resolve_axes` observed a `core.move` source it in fact removes, mutated a `core.convert` target whose destination was omitted, ignored the selected execution profile's own axes, and never compared a selected profile with the row's maxima | One function, four clauses: source-removal is a mutation; an omitted `core.convert` destination is result-only; non-graph `core.execute` unions the profile's axes; `selected_axes` refuses a profile outside the row's maxima | The five effects sub-runs above and the four `profile-out-of-bounds` rows that now fail closed |

### Fixtures of mine that were wrong (the engine was right)
Recorded because each looked like a defect and was not:

- `core.ask path/option-incompatible-with-expected-type` asserted that no attempt is recorded. The action *is* entered and fails pre-effect, so the attempt exists; the assertion is now the diagnostic plus zero requests and no effect.
- `core.move address/uri-network-network` used one URI as both source and destination, which the row's own distinctness precondition refuses.
- `core.verify error/evidence.missing` and `failure_lifecycle phase/mixed/evidence.missing` used a declared `SOURCE`. Step 12a records a `SOURCE` as traceable-but-unfetched *by design* — it performs no effect — so both now use a `VALUE` that demands MISSING, which is the registry's "absent, unresolved".
- `core.execute mode/non-graph-axis-union` expected two *observed* effects. `MockHost` records exactly one applied effect; the resolved union is evidenced by the request the run already pins.

### Two corrected pre-existing tests
`lcl-stdlib/tests/control_operations.rs::core_test_requires_exactly_one_comparison_form` and `::an_assertion_may_not_accompany_an_actual_source` both asserted `error.operation.precondition`, which is **not** in `core.test`'s closed `errors` list. Both now assert the identifier and stage the registry does admit, with the canon quotation. No evidence was deleted or reclassified; both still assert a refusal, at the stage that owns it.

## Blocked: the 79 sub-runs, and what each waits on

| Cause | Sub-runs | What is missing |
|---:|---:|---|
| **G1** `error.scope.violation` is never emitted | 30 | The identifier is registered (preflight and runtime) and SCOPE records are collected, but nothing resolves an effective scope against a target. `operation_errors/*` `error/scope.violation`, `path/scope-violation`, `failure_lifecycle phase/scope-violation-pre-effect`, `core.ask path/out-of-scope-responder-or-request`, `core.compare path/scope-violation`. |
| **G2** graph mode for `core.execute` / `core.test` | 21 | A `REFERENCE[TASK\|ACTION\|PHASE\|SEQUENCE\|TEST]` target is not executed as a graph, so no transitive axis union, graph determinism category, graph error union or graph cycle exists. Includes `result.command engine/graph-*`, `error.reference.cycle behavior/{action,test}-cycle` and `behavior/before-graph-axis-resolution`. |
| **G4** `core.compare` / `core.validate` / `core.verify` never observe an addressable target through a host | 6 | `path/host-constraint`, `path/unauthorized-access`, `precondition/inaccessible-operand`, `error/host.constraint`. |
| **G5** MEMORY/STATE storage is engine-internal and infallible | 6 | `core.memory_write` / `core.state_update` `error/{execution.action,host.constraint,operation.postcondition}` need a store that can fail. |
| **G6** `error.determinism.mismatch` is never emitted | 3 | Registered, with no stated trigger for a `kind.operation`; `error_contract` behaviours and `core.validate error/determinism.mismatch`. |
| **G7** key-operation profiles | 4 | `core.sort precondition/key-operation-profile-*`. `axis_contract.custom_operation_resolution` says a custom `kind.operation` "selects no implementation profile", while `core.sort`'s own diagnostic triggers name key-operation profiles. Normative inconsistency, recorded rather than guessed. |
| **A** ambiguities with no constructible trigger | 5 | `core.group error/operator.operand` (no canon-determined trigger), `core.test path/incompatible-equality-operands` (`equality_compatible` admits any two material values or sentinels), `result.{test,verification} engine/unknown-never-binds` (the checker rejects an UNKNOWN read into a required material site, so UNKNOWN can only arrive through G2 or G4), `core.retry error/reference.cycle`. |
| **New, this task** | 2 | `core.calculate path/unresolved-expression-reference`: `error.reference.unresolved` is registered at the resolution stage and the pinned mirrors are `registry-contract` and `resolver` only; this build resolves no names inside a STRING fragment, so a fragment `REF` to an absent declaration reads MISSING. Naming it at execution would mirror a resolution-stage identifier in a layer that does not own it. `core.execute precondition/profile-out-of-bounds`: that row's maxima admit **every** registered effect class and **every** registered dependency, so no profile can declare axes outside them. |
| **Other graph-dependent** | 2 | `core.ask binding/authorized-in-scope-before-message` (G1), `core.test path/prohibited-graph-cycle` (G2). |

The `core.execute` out-of-bounds row deserves one more line, because the alternative reading was tried. `implementation_profile` also says a profile's axes "may narrow the row's but never widen it", so an invocation outside the *profile's* axes would be out of bounds. That check was implemented and **reverted**: the shipped profile catalog declares axes narrower than ordinary invocations resolve, and the check failed 23 unrelated pinned runs. Making it hold would mean restating every shipped profile's axes — a catalog change, not a repair, and outside this task.

## Files changed

| File | Why | Reused mechanism |
|---|---|---|
| `lcl-checker/src/lib.rs` | `Checked::declared_object_type`, so a receiving contract can name the object type a `DEFINE kind.type` defines | The `Schema::object_type()` the checker already builds |
| `lcl-checker/src/operation.rs` | `comparison_target`: a material `core.test` TARGET beside `actual` or `assertion` | `invocation`'s existing contract lookup and `catalog.binding` |
| `lcl-checker/tests/constructors_and_operations.rs` | Regression for the above | The file's own `invocation`/`ids`/`check` helpers |
| `lcl-completion/src/evidence.rs` | Collect evidence an activated invocation named through an `evidence` parameter | `referenced_evidence`, `syntax::reference_list` |
| `lcl-completion/src/terminal.rs` | A root a declared stop path moved ends `status.stopped` | `root_state`, the existing ordered rule |
| `lcl-completion/tests/success_and_failure.rs` | Regressions for evidence, cancel and stop | `complete`/`task_document` |
| `lcl-parser/src/conditional.rs` | `comparison_form`: the structural half of `core.test`'s comparison-form rule | `SchemaChecker`'s `emit_requirement` and `has_inline_identifier` |
| `lcl-parser/tests/control_forms.rs` | Regression for the above | `parse_bytes`/`id_list` |
| `lcl-runtime/src/capability.rs` | `CapabilityOutcome::Refused` carries an `Observation` | The `Failed` arm's existing shape |
| `lcl-runtime/src/execute.rs` | `error.cancelled` and the evidenced reason on a committed cancel; refusal phase from the observation | `Fault`/`fault`, `phase_of`, `effect_state_of` |
| `lcl-stdlib/src/control.rs` | criteria classification, `core.validate` schema, `core.verify` evidence, `core.ask` option precondition | `params`, `schema`, `pure::read_through` |
| `lcl-stdlib/src/data.rs` | `custom_axes`, the four `resolve_axes` clauses, `selected_axes` | `AddressClass::{observation,mutation}`, `Axes`, the existing profile selection |
| `lcl-stdlib/src/host.rs` | `core.ask` answer contract; `compatible_answer` | `Refused`, `applied`, `process_failure` |
| `lcl-stdlib/src/lib.rs` | A custom operation checks its own axis contract before crossing | The existing `family`/`Resolution::host` path |
| `lcl-stdlib/src/params.rs` | `declaration_index`, `value_matches`, `declared_member_path` | `referenced_declaration`, `lcl_checker::ty` |
| `lcl-stdlib/src/pure.rs` | A declared key path with no value resolves to MISSING | `key_of`, `property_path` |
| `lcl-stdlib/src/schema.rs` | `result.verification` records evidence | The constructor-per-schema shape |
| `lcl-stdlib/tests/{control,data,external,pure}_operations.rs` | Nine regressions; two corrected oracles | `common::{task,data,run,errors_of,result_of}` |
| `lcl-conformance/src/lifecycle_cases.rs` *(new)* | The two unpopulated families | `Runner`, `Expectation`, `judge` |
| `lcl-conformance/src/{semantic_cases,semantic_cases/behaviors.rs}` | 35 behaviour runs for the error contract | `execute_statuses_and_errors` |
| `lcl-conformance/src/{result_cases,result_cases/engine.rs}` | Engine-level runs for all nine result schemas | `Runner::run_on` |
| `lcl-conformance/src/{operation_cases,operation_cases/clauses.rs}` | The operation binding/errors/effects/lifecycle/capability clauses | `Runner`, `ProfileCatalog`, `MockHost` |
| `lcl-conformance/src/runner.rs` | `with_profiles`, `with_surface`, `shipped_profiles`, `stdlib_catalog`, `attempt_field` | `Runner::new` now delegates to `with_profiles` |
| `lcl-conformance/src/production.rs` | Records the lifecycle population; accepts the two new semantic families | `Coverage` |
| `lcl-conformance/src/lib.rs` | Module wiring | — |
| `lcl-conformance/tests/production_report.rs` | `UNPOPULATED` is now empty | The test's existing probe-state matrix |

Untracked, all new: `lcl-conformance/src/lifecycle_cases.rs`, `src/result_cases/`, `src/semantic_cases/`.

## Gates

All gates ran once, at the integration point, through `/mnt/F/.lcl-pretest/f2/f2-gate.sh`, adapted from the PRETEST-04 gate rather than rewritten. Logs are `/mnt/F/.lcl-pretest/logs/F2-G-*.log`.

| Command | Exit | Result | Log |
|---|---:|---|---|
| `cargo fmt --all -- --check` | 0 | clean | `F2-G-fmt.log` |
| `cargo clippy --workspace --all-targets -- -D warnings` | 0 | clean | `F2-G-clippy.log` |
| `cargo test --workspace --all-targets --no-fail-fast` (current toolchain) | 0 | 168 result blocks, **1,694 passed, 0 failed**, 1 ignored | `F2-G-test-workspace.log` |
| `cargo check --workspace --all-targets` (Rust 1.75, the declared MSRV) | 0 | clean | `F2-G-msrv-check.log` |
| `cargo test --workspace --all-targets --no-fail-fast` (Rust 1.75) | 0 | 162 result blocks, **1,694 passed, 0 failed**, 1 ignored | `F2-G-msrv-tests.log` |
| `sha256sum -c SHA256SUMS.txt` (Core 0.1.0) | 0 | every entry verifies | `F2-G-sha-0.1.0.log` |
| `validate_release.py --scope all` (Core 0.1.0) | 0 | `release_ready: true`, `scope_ready: true` | `F2-G-validate-0.1.0.log` |
| `sha256sum -c SHA256SUMS.txt` (Core 0.2.0) | 0 | every entry verifies | `F2-G-sha-0.2.0.log` |
| `validate_release.py --scope all` (Core 0.2.0) | 0 | `release_ready: false`, unchanged from PRETEST-04; Core 0.2 is not this task's | `F2-G-validate-0.2.0.log` |
| `validate_localization.py` (Core 0.2.0) | 0 | `passed: true` | `F2-G-localization-0.2.0.log` |
| Core 0.2.0 identity | 0 | `6e7303157f5ba378…690ceb`, 216 files | `F2-G-identity-0.2.0.log` |
| `sha256sum -c assets/brand/BRAND_ASSETS.sha256` | 0 | every entry verifies | `F2-G-brand.log` |
| `m8_conformance_report --json` | 0 | claim `source_conforming`; 2,413 executed, 2,413 passed | `F2-G-conformance-json.log` |
| `m8_conformance_report` (text) | 0 | source 2,011/2,011; semantics 357 satisfied, 0 failed, 0 missing, 45 invalid | `F2-G-conformance-text.log` |
| `lcl spec` + `lcl check --machine` on the localization fixture | 0 | identities match | `F2-G-identity.log` |
| protected-area check | 0 | no worktree change under `canonical`, `releases`, `assets`; every release candidate checksum verifies | `F2-G-protected.log` |

The gate wrote nothing into `impl/target/test-tmp` or `/tmp/lcl-apps`.

## Identities
- Core 0.1.0: `00d648b1…67ed`, verified by `SpecPackage` on every run; `canonical/**` is byte-identical to HEAD.
- Core 0.2.0: unchanged; FINAL-02 made no Core 0.2 change, which belongs to FINAL-03 after the owner's F10 decision.
- Obligations mapping: `obligations_v0.1.0_r2.json`, digest `9b32a28b…a8ad`, **unchanged**. No pin was added, removed or relabelled.

## Conformance
- claim: `source_conforming`
- source required 2,011: satisfied 2,011, failed 0, missing 0, invalid 0
- semantic required 402: **satisfied 357, failed 0, missing 0, invalid 45**
- records: executed 2,413, passed 2,413, failed 0
- blocked sub-runs: 79 across those 45 probes, each attributed above

## Residual and out-of-scope
- The claim cannot move without G1, G2, G4, G5 and G6. Each is a capability, not a defect: emitting scope violations, executing graph targets, observing addressable targets in the three check rows, a fallible store, and a determinism trigger. G7 is a normative inconsistency for the owner.
- `CapabilityOutcome::Refused` gained a field. Every existing construction site passes `Observation::none()`, which is exactly the previous behaviour; only `core.ask` uses the new arm.
- `error.block.conditional_requirement` now fires for `core.test` at the grammar stage. Two stdlib tests encoded the previous identifier and were corrected; no canonical example changed, and the whole workspace and MSRV suites pass.
- The custom-operation axis check resolves the effect set from the target alone. A custom row with declared destination arguments would need the destination side too; no pinned run exercises one.
- `lcl-completion` still does not fetch a declared evidence `SOURCE`. That is the layer's documented design (it performs no effect) and is consistent with `core.verify`, which does not fetch one either.

## AI quota and reuse
- Evidence reused: `verdict.sh` runs the existing `m8_conformance_report --json` entry point; no parallel reporting path was built. The scratch probe crate lives outside the repository and adds nothing to the build.
- Broad gates avoided: repairs were verified with a single crate test or a single filtered conformance dump. The workspace, MSRV, fmt and clippy gates ran only at the integration point, through `f2-gate.sh`, adapted from `p4-gate.sh` rather than rewritten.
- Minimum-code notes: every repair extends an existing function or adds one helper beside it. The only new public API is `Checked::declared_object_type`, and the only changed public shape is the `Refused` observation. No dependency was added.

## Proposed commit message

```
LCL final pretest task2

Complete the semantic conformance population and repair every engine
defect it exposed.

Populate the two families that had no executed population,
semantic/diagnostic_policy/core.error_selection and
semantic/failure_lifecycle/core.failure_lifecycle, and author the
remaining pinned sub-runs across operation_binding, operation_errors,
operation_effects, result_schemas and error_contract.

Repair the ten root causes those runs exposed, each red first:

- core.memory_write/core.state_update honour MODE and the declared type
- core.group/core.sort separate an absent declared key value from an
  unregistered property path
- core.compare classifies its criteria defects under the identifiers the
  row admits, and reads a criteria REFERENCE through
- core.validate resolves and applies its schema parameter
- core.verify records its evidence, and completion collects the evidence
  an invocation named through that parameter
- core.ask checks its options and its answer, and no authorized valid
  answer is error.required.missing
- a committed core.cancel raises error.cancelled and evidences its
  reason; a committed core.stop ends the root status.stopped
- core.test's comparison form is refused at the stages that own it
- a custom kind.operation invocation resolves and bounds its own axes
- core.move, core.convert and core.execute resolve their invocation axes
  as their rows state, and a selected profile outside the row's maxima
  fails closed

Every executed case passes: 2,413 of 2,413. The semantic level is 357
satisfied, 0 failed, 0 missing, 45 invalid, and every invalid probe is
invalid only because pinned sub-runs have no constructible input in this
build. The claim stays source_conforming.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
```

## Next task
FINAL-03 is **not** unlocked by this task alone: it turns on the owner's F10 decision, and `semantics_conforming` needs the six capabilities listed under *Blocked*. FINAL-02's own obligations — populate every semantic family, execute every constructible pinned sub-run, and repair what they expose — are complete.

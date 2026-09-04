# LCL-TASK-0005 Result — Finalize Result and OUTPUT Contracts

Date: 2026-09-04

Task status: COMPLETE FOR TASK SCOPE

Release status: NOT RELEASE READY

## 1. Scope, authority, and local source

LCL-TASK-0005 was executed only against the actual extracted candidate at
`/mnt/F/LCL/canonical/LCL_Core_0.1.0` and only within the bare-language
specification boundary.

The Claude web-session handoff was treated as a decision/change record. No
web-session edit was assumed to exist locally. The implementation followed the
owner's exact approval:

```text
APPROVED: USE EXPLICIT OUTPUT.PROPERTY PROJECTION FOR LCL-TASK-0005.
```

The pre-edit Git baseline was clean at `main`, commit
`c11bdaadf86d5fd61bc4c14ef3ae1ad2e7f86c87`. The retained baseline report is
`/mnt/F/LCL/reports/tasks/LCL-TASK-0005_BASELINE.md`. The completion-package
checksum manifest passed all 20 records before work and again at closure.

No parser, interpreter, compiler, runtime, semantic engine, UI, provider
integration, or executable conformance system was added. No successor task was
started.

The required native-platform research was completed before specification edits.
Qt's [QProcess](https://doc.qt.io/qt-6/qprocess.html) distinguishes failure to
start from a started process and its completion/exit observations. KDE's
[KJob](https://api.kde.org/kjob.html) provides asynchronous job result/error
signaling. Neither supplies LCL's schema cardinality, effect truth, or OUTPUT
projection model, so the language contract remains explicit and host-neutral.

## 2. Closed common result model

Every one of the nine result schemas now inherits these six required fields
exactly once:

| Field | Exact type | Cardinality |
|---|---|---|
| `status` | `qualified_identifier(status)` | exactly one |
| `output_binding` | `ENUM[not_requested\|unbound\|bound\|partial]` | exactly one |
| `execution_errors` | `LIST[qualified_identifier(error)]` | exactly one |
| `failure_phase` | `ENUM[none\|pre_effect\|post_effect\|indeterminate]` | exactly one |
| `effect_state` | `ENUM[none\|applied\|partial\|indeterminate]` | exactly one |
| `observed_effects` | `LIST[result.effect]` | exactly one |

`result.effect` is a closed auxiliary record with exactly one effect `class`,
exactly one `state`, zero-or-one `target`, and exactly one evidence list. It
records only concrete effect classes; `none` is represented by an empty
`observed_effects` list and the common `effect_state`.

Execution status is separate from domain outcome. Consequently, a producer may
complete with `status.succeeded` while `passed`, `valid`, `verified`,
`delivered`, or `changed` is `FALSE`. A command that started and completed with a
nonzero exit code is likewise a completed producer outcome unless a separate
producer-contract failure occurred.

A pre-effect failure requires `failure_phase: pre_effect`, `effect_state: none`,
an empty observed-effect list, and an unbound requested OUTPUT. Known effect
truth may be retained after post-effect or partial failure without binding it to
OUTPUT. Partial external effects and partial OUTPUT are independent.

All result and auxiliary effect schemas are closed; unknown fields are
forbidden.

## 3. Nine finalized schemas

In the table, `1` means exactly one and `0..1` means zero or one. The six common
fields above are additional to every listed schema-local field.

| Schema | Schema-local fields | Default OUTPUT property | Legal UNKNOWN outcome | Partial OUTPUT |
|---|---|---|---|---|
| `result.value` | `value: meta.material_value (0..1)`; `evidence: LIST[REFERENCE[EVIDENCE]] (1)` | `value` | none | unsupported |
| `result.collection` | `items: LIST[T] (0..1)`; `count: INTEGER (0..1)` | `items` | none | unsupported |
| `result.operation` | `changed: BOOLEAN\|UNKNOWN (1)`; `target: target_expression (1)`; `value: meta.material_value (0..1)` | `changed` | `changed` | unsupported |
| `result.command` | `mode: ENUM[non_graph\|graph] (1)`; `started`, `completed: BOOLEAN (0..1)`; `exit_code: INTEGER (0..1)`; `stdout`, `stderr: STRING (0..1)`; `value: meta.material_value (0..1)` | `stdout` | none | only `stdout` and `stderr` |
| `result.validation` | `valid: BOOLEAN (0..1)`; `errors: LIST[qualified_identifier(error)] (1)` | `valid` | none | unsupported |
| `result.verification` | `verified: BOOLEAN\|UNKNOWN (0..1)`; `observed: OBJECT (0..1)`; `errors` and `evidence` lists (1 each) | `verified` | `verified` | unsupported |
| `result.test` | `passed: BOOLEAN\|UNKNOWN (0..1)`; `expected`, `actual: meta.material_value (0..1)`; `evidence` list (1) | `passed` | `passed` | unsupported |
| `result.message` | `delivered: BOOLEAN\|UNKNOWN (1)`; `recipient: target_expression (1)`; `message_id: STRING\|NULL (1)` | `delivered` | `delivered` | unsupported |
| `result.transfer` | `source`, `destination: target_expression (1)`; `bytes: BYTES\|UNKNOWN (0..1)`; `checksum: STRING\|NULL (0..1)`; `value: meta.material_value (0..1)` | `bytes` | `bytes` | unsupported |

The schema-specific constraints close the required edge cases:

- `result.collection.count` is non-negative and equals the actual item count;
  `items: []` with `count: 0` is valid.
- `result.command` distinguishes `non_graph` from `graph`. Failure to start has
  `started: FALSE`, `completed: FALSE`, and no native observations. After start,
  both streams exist even when empty; completion adds `exit_code`. Graph mode
  synthesizes no native command observations and exposes `value` only for one
  completed material primary result.
- Validation and verification domain-error lists remain separate from common
  producer `execution_errors`.
- A test assertion form omits `expected`/`actual`; comparison form provides both.
  Tested `NULL` is present material data.
- `result.message.message_id: NULL` means no identifier was assigned; it does not
  mean the delivery outcome is missing.
- `result.transfer.bytes` is the non-negative byte count actually transferred,
  not content. `BYTES(0)` is valid. Optional `value` carries content only when an
  operation explicitly supplies it.

The verified UNKNOWN-compatible set is exactly:
`result.operation.changed`, `result.verification.verified`,
`result.test.passed`, `result.message.delivered`, and `result.transfer.bytes`.
UNKNOWN remains transient, cannot be persisted as a material value, and never
binds or partially binds OUTPUT.

## 4. Approved OUTPUT.PROPERTY projection

The accepted projection policy is now identical in block schemas, field
signatures, grammar-facing prose, result semantics, operation prose, examples,
and validation:

- zero `OUTPUT.PROPERTY` occurrences select the built-in result schema's declared
  default property;
- one occurrence selects that available projectable field as a scalar;
- two or more unique occurrences produce one closed OBJECT containing exactly
  those top-level fields in declaration order;
- common bookkeeping fields are not projectable;
- every selected field must be projectable and available for the operation and
  mode;
- a custom `kind.operation` with no PROPERTY selects its whole declared RESULT;
- a graph-mode `result.command` must explicitly select `value` or `mode`, because
  its non-graph default `stdout` is unavailable;
- the complete projection must be material and satisfy OUTPUT `TYPE`, `FORMAT`,
  `SCHEMA`, and `ENCODING` before binding;
- `MISSING` and `UNKNOWN` do not bind. `FALSE`, numeric zero, zero bytes, an empty
  string, and an empty collection are valid material values and may bind.

## 5. Operation, example, and catalog integration

All 39 Task-0004 operation rows resolve to one of the nine schemas. The mapping
counts are:

| Result schema | Operation count |
|---|---:|
| `result.value` | 9 |
| `result.collection` | 3 |
| `result.operation` | 18 |
| `result.command` | 1 |
| `result.validation` | 1 |
| `result.verification` | 1 |
| `result.test` | 1 |
| `result.message` | 1 |
| `result.transfer` | 4 |

Task-0005 changed only the result-population postconditions needed by
`core.generate`, `core.convert`, `core.execute`, and `core.download`; the other
35 Task-0004 operation rows remain byte-equivalent at the row level.

Four existing valid examples now make projection explicit:

- `03_IMPORTING_TASK.lcl`: `PROPERTY: destination`;
- `04_AUTOMATED_CODING_TASK.lcl`: `PROPERTY: exit_code`, `stdout`, and `stderr`;
- `06_EXPLICIT_CONTEXT_MEMORY_AND_STATE.lcl`: `PROPERTY: value`;
- `08_AUTHORITY_OVERRIDE_HANDLER_AND_RETRY.lcl`: `PROPERTY: value`.

The descriptive catalog remains exactly 795 entries. Existing IDs
`RESULT-SCHEMAS-0774` through `RESULT-SCHEMAS-0782` now specify the nine exact
contracts. `OPERATION-EFFECTS-0623` now reflects the closed command-result modes.
No executable-semantic claim is made for this requirements index.

## 6. Fresh validation evidence

Focused registry validation:

```text
PYTHONDONTWRITEBYTECODE=1 python canonical/LCL_Core_0.1.0/09_CONFORMANCE/TOOLS/validate_release.py \
  --root canonical/LCL_Core_0.1.0 --scope registry
```

Result: `13 PASS`, `0 FAIL`, `2 BLOCKED`, `0 OUT_OF_SCOPE`. The two blockers are
the retained Task-0006 diagnostic-selection and mixed-phase lifecycle contracts.
The process exit is therefore 1, but no Task-0005 check failed.

Task-specific registry evidence:

- `result_schema_cardinality_and_output_contracts`: PASS;
- result schemas: 9;
- inherited common fields: 6;
- valid operation result references: 39;
- violation list: empty;
- approved combined result-contract fingerprint:
  `21de485d9d618aee571d0204f49d03a32be41871350766c6709713237f4e36e4`.

Focused catalog validation:

```text
PYTHONDONTWRITEBYTECODE=1 python canonical/LCL_Core_0.1.0/09_CONFORMANCE/TOOLS/validate_release.py \
  --root canonical/LCL_Core_0.1.0 --scope catalog
```

Result: `1 PASS`, `0 FAIL`, `0 BLOCKED`, `1 OUT_OF_SCOPE`; scope-ready is true.
All nine Task-0005 requirements were checked. Their canonical fingerprint is
`2c65296a7da92f7d66c3fea373f20832976929476eb6e6f3c160185eba4dfb1d`.

The full static gate used the same command with `--scope all`. Fresh totals are:

```text
PASS:         20
FAIL:          2
BLOCKED:       2
OUT_OF_SCOPE:  2
release_ready: false
```

The 20 passes include filesystem/text/JSON/Python hygiene, all 15 source
fixtures, the 65-production EBNF graph, registry closure, operation contracts,
the complete Task-0005 result contract, and catalog integrity. The two failures
are the deliberately stale `MANIFEST.json` and `SHA256SUMS.txt`. The two blocked
checks belong to Task 0006. The parser/example parse matrix and semantic case
execution remain accurately OUT_OF_SCOPE.

Six in-memory adversarial mutation probes passed by proving the validator rejects:

1. an unregistered local result field;
2. `result.collection.count` changed from INTEGER to BYTES;
3. removal of the common `failure_phase` field;
4. illegal UNKNOWN support on `result.validation.valid`;
5. OUTPUT projection-rule drift; and
6. an operation reference to an unavailable result schema.

The probes did not write candidate or temporary package bytes. Python source
compilation via `compile()`, `git diff --check`, JSON parsing, and cache-hygiene
checks passed. No `__pycache__` or `.pyc` remains.

## 7. Files changed

Candidate files modified (25):

```text
00_RELEASE/00_CANONICAL_SOURCE_AND_PROVENANCE.txt
00_RELEASE/01_RELEASE_STATUS_AND_BOUNDARY.txt
00_RELEASE/03_COMPLETENESS_CRITERIA.txt
03_TYPES_AND_VALUES/09_MISSING_UNKNOWN_NULL_AND_OPTIONALITY.txt
04_GRAMMAR/07_RULE_CHECK_OUTPUT_AND_COMPLETION_FORM.txt
05_SEMANTICS/05_INPUT_DATA_OUTPUT_RESULT_AND_FORMAT.txt
05_SEMANTICS/09_VALIDATION_EXECUTION_FAILURE_AND_TERMINATION.txt
05_SEMANTICS/10_VERIFY_TEST_EVIDENCE_SUCCESS_FAILURE_AND_STATUS.txt
06_STANDARD_LIBRARY/02_MUTATING_AND_EXTERNAL_OPERATIONS.txt
06_STANDARD_LIBRARY/05_CORE_STATUS_IDENTIFIERS.txt
06_STANDARD_LIBRARY/10_CORE_OPERATION_PARAMETER_RULES.txt
08_EXAMPLES/VALID/03_IMPORTING_TASK.lcl
08_EXAMPLES/VALID/04_AUTOMATED_CODING_TASK.lcl
08_EXAMPLES/VALID/06_EXPLICIT_CONTEXT_MEMORY_AND_STATE.lcl
08_EXAMPLES/VALID/08_AUTHORITY_OVERRIDE_HANDLER_AND_RETRY.lcl
09_CONFORMANCE/01_CONFORMANCE_REQUIREMENTS.txt
09_CONFORMANCE/CASES/core_conformance_cases_v0.1.0.json
09_CONFORMANCE/TOOLS/validate_release.py
10_REGISTRIES/block_schemas_v0.1.0.json
10_REGISTRIES/built_in_groups_and_results_v0.1.0.json
10_REGISTRIES/field_signatures_v0.1.0.json
10_REGISTRIES/operations_v0.1.0.json
10_REGISTRIES/statuses_and_errors_v0.1.0.json
CHANGELOG.txt
README.txt
```

Candidate files created, deleted, renamed, or moved: none. Candidate inventory
remains 172 files.

External task reports created:

```text
/mnt/F/LCL/reports/tasks/LCL-TASK-0005_BASELINE.md
/mnt/F/LCL/reports/tasks/LCL-TASK-0005_RESULT.md
```

## 8. Integrity and remaining boundary

The candidate's current sorted `./path` plus content-SHA-256 stream is:

```text
696f09a4be0a48eaeceb226678392f4f852b7f55789fbc7d30ea5813fbe0655e
```

Frozen integrity artifacts were not regenerated and retain their pre-edit bytes:

```text
bee3d2f71a1668b7e5794b54be9fe08687ca42b3da4e5b7dd03ef9632a2a8685  MANIFEST.json
4716d5993417506577643e2ea12d65bb0b7f0219944b6fd1f094948577630ac5  SHA256SUMS.txt
318e4fc95e836cdde1d771638d77ba77be10ede4f41588414536aa0dd90c5921  VALIDATION_REPORT.txt
```

The immutable historical 152-file tree still has aggregate fingerprint
`7712e2e981e7140cdfde2b679b45b3a96d2b00834541ba6148946376f7d6a7d8`.
The completion package's `SHA256SUMS.txt` remains
`bd0fa1acc9a475ec07ee01b386cf65f8e47a981bb8f870bf910436916cc340ec`
and all 20 records verify.

The candidate remains blocked only by the Task-0006 bare-language findings
`LCL-AUDIT-014` and `LCL-AUDIT-015`, followed by the separately gated final
integrity/release task. The current integrity failures are expected stale-byte
evidence, not passes. No manifest, checksum set, validation report, release ZIP,
commit, or push was produced.

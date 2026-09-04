# LCL-TASK-0006 Result — Finalize Diagnostics, Failure Timing, and the Bare Specification

Date: 2026-09-04

Task status: COMPLETE

Language-definition status: `BARE_SPECIFICATION_COMPLETE`

Release status: NOT RELEASE READY (packaging and integrity are gated to the
corrected final release task)

## 1. Scope, authority, and local source

LCL-TASK-0006 was executed only against the actual extracted candidate at
`/mnt/F/LCL/canonical/LCL_Core_0.1.0`, and only within the bare-language
specification boundary.

The Claude web-session handoff was treated as a decision/change record only. No
web-session edit was assumed to exist locally; every accepted change was
reproduced against the inspected local bytes. No parser, interpreter, compiler,
runtime, semantic engine, UI, provider integration, or executable conformance
system was added, and no successor task was started.

The pre-edit baseline is retained at
`/mnt/F/LCL/reports/tasks/LCL-TASK-0006_BASELINE.md`:

- Git baseline: `main`, commit `cd8d6293ef2d57d481bcfbd9e1a154d2eb727833`
  (`task5`), clean working tree.
- Candidate baseline: 172 files, sorted `./path` plus content-SHA-256 stream
  `696f09a4be0a48eaeceb226678392f4f852b7f55789fbc7d30ea5813fbe0655e`.
- Baseline validator: 20 PASS, 2 expected stale-integrity FAIL, 2 BLOCKED,
  2 OUT_OF_SCOPE; `release_ready=false`, `scope_ready=false`.
- Baseline blockers: `diagnostic_selection_contract` (`LCL-AUDIT-014`) and
  `mixed_phase_lifecycle_contracts` (`LCL-AUDIT-015`).
- Completion-package `SHA256SUMS.txt`
  (`bd0fa1acc9a475ec07ee01b386cf65f8e47a981bb8f870bf910436916cc340ec`) verified
  all 20 records before work and again at closure.

## 2. Decisions applied

No new user decision was required. Both decision gates were resolved from
internal evidence in the actual registries, grammar, and normative prose, so no
question was raised under Global Rule 3.

Preserved accepted decisions D-001 through D-005 and the accepted
operation/result design constraints were not reopened. Open items O-006
(diagnostic selection) and O-007 (mixed failure timing) are the two gates
closed by this task.

### Gate A — diagnostic selection (`LCL-AUDIT-014`, closed)

Defined in `10_REGISTRIES/statuses_and_errors_v0.1.0.json` under
`diagnostic_selection`, which is authority rank 1 in
`00_RELEASE/02_NORMATIVE_AUTHORITY_ORDER.txt`:

| Question | Resolution |
|---|---|
| Are all applicable diagnostics emitted? | Every independent unsuppressed diagnostic at the earliest failing stage is emitted; later stages of the same failed unit or path are not evaluated. |
| Stage order | Closed seven-stage order: `lexical`, `grammar_or_schema`, `resolution`, `static_or_expression`, `validation`, `execution`, `verification_or_completion`. |
| Severity | Core 0.1.0 has exactly one severity, `error`, default for every error. |
| Specificity / rank | Default rank 100, `higher_is_more_specific`, with eight exact overrides at 200. |
| Supersession | Eight explicit `supersedes` edges, applied transitively, and only for the same cause, canonical locus, stage, producer path, iteration index, and retry-attempt index. |
| Duplicate suppression | A closed seven-field `duplicate_key`; evidence is merged in canonical order and byte-identical records removed. |
| Stable output order | Seven ordered keys ending in identifier by Unicode scalar value; discovery time and host completion order are never ordering keys. |
| Primary vs secondary | First unhandled diagnostic in stable order is primary; every other unhandled diagnostic is secondary; handled and retried diagnostics remain ordered secondary evidence. |
| Ties | Fully broken by the ordering key list; the vector suite proves determinism. |
| Locations | Zero-based byte offsets, with exact zero-width positions defined for omitted fields, omitted child blocks, omitted required top-level blocks, and the absent final line feed. |

Specificity/supersession overrides distinguish `error.source.tab`,
`error.indentation.jump`, `error.indentation.width`, `error.literal.unclosed`,
`error.literal.escape`, `error.field.required`, `error.field.duplicate`, and
`error.block.duplicate` from their generic consequences.

### Gate B — mixed failure timing (`LCL-AUDIT-015`, closed)

Defined under `failure_lifecycle` in the same registry.

- **Phase scope:** `failure_phase` is measured at the exposed result producer. A
  delegated child or retry attempt retains its own local phase and effect
  observations as ordered evidence; the aggregate producer accounts for every
  effect begun within its invocation.
- **Phase resolution:** allowed phases are resolved for all 77 registered
  errors from stage defaults plus exact identifier overrides. `none` is never an
  allowed failure phase when an error exists.
- **Four baseline ambiguous errors:** `error.dependency.unsatisfied` and
  `error.scope.violation` are `pre_effect` only; `error.execution.order` and
  `error.required.missing` are resolved from observed producer timing across
  `pre_effect`, `post_effect`, and `indeterminate`.
- **Eleven additional mixed-phase errors** revealed by Tasks 0004-0005 are
  profiled: `error.cancelled`, `error.evidence.missing`,
  `error.execution.action`, `error.host.constraint`,
  `error.operation.postcondition`, `error.operation.precondition`,
  `error.permission.denied`, `error.retry.exhausted`,
  `error.success.unsatisfied`, `error.value.unknown`,
  `error.verification.failed`.
- **Status:** the primary unhandled diagnostic supplies its registered
  `default_status`; secondary diagnostics never override it; recovery promotes
  the next unhandled diagnostic; only full recovery lets a declared handler
  result control status. Phase never changes status and status never infers
  phase.
- **Effect state:** `pre_effect` requires `effect_state none` and an empty
  `observed_effects`; `post_effect` requires at least one known begun effect;
  `indeterminate` phase requires `indeterminate` effect state unless
  independent evidence proves otherwise.
- **OUTPUT binding:** `pre_effect` leaves OUTPUT unbound and forbids partial
  OUTPUT; `post_effect` does not bind OUTPUT merely because an effect exists;
  partial OUTPUT only where the selected result schema declares it;
  indeterminate phase binds only independently proven values.
- **Retry safety:** every ACTION resolving exactly once stays syntactically
  eligible for bounded RETRY. Proven `pre_effect` attempts may repeat under
  LIMIT/WHEN. After known effects, exact evidence must prove safe repetition,
  resumption, or reconciliation within the wrapped action's original authority,
  scope, and post-state. Indeterminate phase or effects prohibit another attempt
  until reconciled. Missing proof → `error.required.missing`; UNKNOWN proof →
  `error.value.unknown`; proved unsafe → `error.operation.precondition`.
  `error.retry.exhausted` requires that every permitted attempt was actually
  made and failed; a safety-blocked unmade attempt is not exhaustion.
- **Evidence:** seven common retained records for every failure, plus exact
  additional evidence for each of the four baseline errors.
- **Indeterminate state:** fail closed — absence of evidence never proves
  absence of effects or OUTPUT, and reconciliation may replace `indeterminate`
  only with exact retained evidence.

## 3. Bare-spec closure work

- Normative prose was reconciled with the registries under the authority order.
  `01_FOUNDATION/03_NORMATIVE_PROCESSING_MODEL.txt`,
  `05_SEMANTICS/09_VALIDATION_EXECUTION_FAILURE_AND_TERMINATION.txt`,
  `06_STANDARD_LIBRARY/06_CORE_ERROR_IDENTIFIERS_PART_1.txt`, and
  `06_STANDARD_LIBRARY/07_CORE_ERROR_IDENTIFIERS_PART_2.txt` now point at the
  closed registry contract instead of restating a second precedence table, so
  each accepted decision is represented exactly once at its authoritative rank.
- `06_STANDARD_LIBRARY/03_CONTROL_OPERATIONS.txt` and
  `10_REGISTRIES/operations_v0.1.0.json` were kept in exact parity for the
  `core.retry` attempt-evidence, exhaustion, precondition, and error-resolution
  contracts.
- Four invalid-example expectations were corrected to the resolved selection
  rules: `02_TAB_INDENTATION` → `error.source.tab`,
  `10_CONTEXT_WITHOUT_SCOPE` → `error.field.required`, `11_DUPLICATE_FIELD` →
  `error.field.duplicate`, `15_MISSING_TASK_STRUCTURE` →
  `error.reference.unresolved`.
- `09_CONFORMANCE/SOURCE_FIXTURES/expected_results.json` and
  `validate_source_fixtures.py` were aligned with the closed excluded-lexeme
  inventory, so `;`, `#`, `'`, `%`, and bare `=` select `error.symbol.invalid`
  while the adopted `==`, `!=`, `<=`, and `>=` operators are unaffected.
- Stale claims were removed from `README.txt`, `VERSION.txt`,
  `CHANGELOG.txt`, `00_RELEASE/00_CANONICAL_SOURCE_AND_PROVENANCE.txt`,
  `00_RELEASE/01_RELEASE_STATUS_AND_BOUNDARY.txt`,
  `00_RELEASE/03_COMPLETENESS_CRITERIA.txt`,
  `00_RELEASE/04_CHANGE_CONTROL.txt`, and
  `09_CONFORMANCE/01_CONFORMANCE_REQUIREMENTS.txt`, including the corrected
  catalog count (795 → 797), the corrected selection-vector count (ten →
  twelve), and the corrected focused-diagnostic count (four → seven).
  `00_RELEASE/03_COMPLETENESS_CRITERIA.txt` now separates the
  `BARE_SPECIFICATION_COMPLETE` boundary from packaged-release completeness.
- The descriptive requirements index grew from 795 to 797 entries with
  `DIAGNOSTIC-POLICY-0796` and `FAILURE-LIFECYCLE-0797`.
- `09_CONFORMANCE/TOOLS/validate_release.py` gained two real registry gates:
  `diagnostic_selection_contract` and `mixed_phase_lifecycle_contracts`. They
  pin both policy fingerprints, resolve metadata for all 77 errors, execute
  twelve concrete selection vectors through an implementation of the published
  ordering rules, and assert registry↔prose parity. Neither gate contains a
  pass-through branch: a tamper check that changed one specificity override and
  one phase override turned both gates `FAIL`
  (`specificity-rank contract is not exact`, `phase overrides differ`).

## 4. Files created, modified, deleted

Candidate files modified (23; no candidate file was created, deleted, renamed,
or moved; inventory remains 172 files):

```text
00_RELEASE/00_CANONICAL_SOURCE_AND_PROVENANCE.txt
00_RELEASE/01_RELEASE_STATUS_AND_BOUNDARY.txt
00_RELEASE/03_COMPLETENESS_CRITERIA.txt
00_RELEASE/04_CHANGE_CONTROL.txt
01_FOUNDATION/03_NORMATIVE_PROCESSING_MODEL.txt
05_SEMANTICS/09_VALIDATION_EXECUTION_FAILURE_AND_TERMINATION.txt
06_STANDARD_LIBRARY/03_CONTROL_OPERATIONS.txt
06_STANDARD_LIBRARY/06_CORE_ERROR_IDENTIFIERS_PART_1.txt
06_STANDARD_LIBRARY/07_CORE_ERROR_IDENTIFIERS_PART_2.txt
08_EXAMPLES/INVALID/02_TAB_INDENTATION.invalid.lcl.expected.txt
08_EXAMPLES/INVALID/10_CONTEXT_WITHOUT_SCOPE.invalid.lcl.expected.txt
08_EXAMPLES/INVALID/11_DUPLICATE_FIELD.invalid.lcl.expected.txt
08_EXAMPLES/INVALID/15_MISSING_TASK_STRUCTURE.invalid.lcl.expected.txt
09_CONFORMANCE/01_CONFORMANCE_REQUIREMENTS.txt
09_CONFORMANCE/CASES/core_conformance_cases_v0.1.0.json
09_CONFORMANCE/SOURCE_FIXTURES/expected_results.json
09_CONFORMANCE/TOOLS/validate_release.py
09_CONFORMANCE/TOOLS/validate_source_fixtures.py
10_REGISTRIES/operations_v0.1.0.json
10_REGISTRIES/statuses_and_errors_v0.1.0.json
CHANGELOG.txt
README.txt
VERSION.txt
```

External task reports created:

```text
/mnt/F/LCL/reports/tasks/LCL-TASK-0006_BASELINE.md
/mnt/F/LCL/reports/tasks/LCL-TASK-0006_RESULT.md
```

Deleted: none.

## 5. Exact validation commands and exit codes

All commands were run from `/mnt/F/LCL` with `PYTHONDONTWRITEBYTECODE=1`.

```text
python3 canonical/LCL_Core_0.1.0/09_CONFORMANCE/TOOLS/validate_release.py \
  --root canonical/LCL_Core_0.1.0 --scope all                              exit 1
python3 .../validate_release.py --root canonical/LCL_Core_0.1.0 --scope filesystem  exit 0
python3 .../validate_release.py --root canonical/LCL_Core_0.1.0 --scope text        exit 0
python3 .../validate_release.py --root canonical/LCL_Core_0.1.0 --scope structured  exit 0
python3 .../validate_release.py --root canonical/LCL_Core_0.1.0 --scope grammar     exit 0
python3 .../validate_release.py --root canonical/LCL_Core_0.1.0 --scope registry    exit 0
python3 .../validate_release.py --root canonical/LCL_Core_0.1.0 --scope catalog     exit 0
python3 .../validate_release.py --root canonical/LCL_Core_0.1.0 --scope integrity   exit 1
python3 canonical/LCL_Core_0.1.0/09_CONFORMANCE/TOOLS/validate_source_fixtures.py   exit 0
python3 canonical/LCL_Core_0.1.0/09_CONFORMANCE/TOOLS/validate_ebnf.py \
  canonical/LCL_Core_0.1.0/04_GRAMMAR/10_COMPLETE_EBNF.ebnf                        exit 0
sha256sum -c SHA256SUMS.txt   (in /mnt/F/LCL_Completion_Task_Package_v2.0)         exit 0
```

The only non-zero exits are `--scope all` and `--scope integrity`, and both are
caused solely by the two deliberately frozen integrity artifacts.

## 6. Resolved requirements and findings

- `LCL-AUDIT-014` — closed by `diagnostic_selection_contract`.
- `LCL-AUDIT-015` — closed by `mixed_phase_lifecycle_contracts`.
- `O-006` and `O-007` from `03_OPEN_DECISIONS.md` — closed.
- Acceptance criteria:
  - every error has complete phase/severity/selection metadata — 77 of 77
    resolved (`error_metadata_resolved_count` and
    `error_phase_metadata_resolved_count` are both 77);
  - same-stage diagnostic behavior is deterministic — twelve executed selection
    vectors pass;
  - mixed pre/post/partial/indeterminate behavior is complete — four baseline
    plus eleven additional profiles, with status, effect, OUTPUT, evidence,
    retry-safety, and indeterminate rules;
  - zero unresolved bare-language grammar/type/registry/semantic blockers —
    `BLOCKED` count is 0;
  - static validation has zero unexpected FAIL.

## 7. Remaining issues

Nothing bare-language remains open. The remaining items are explicitly
classified as reference-implementation or packaging work, not unresolved
language semantics:

| Item | Classification |
|---|---|
| `complete_example_parse_matrix` (OUT_OF_SCOPE) | reference lexer/parser — LCL-TASK-0007 |
| `semantic_case_execution` (OUT_OF_SCOPE) | reference semantic interpreter and executable conformance — LCL-TASK-0008/0009 |
| `manifest_set_size_hash` (FAIL) | deliberately frozen `MANIFEST.json`; regeneration is gated to LCL-TASK-0010 |
| `checksum_set_and_hash` (FAIL) | deliberately frozen `SHA256SUMS.txt`; regeneration is gated to LCL-TASK-0010 |

`VALIDATION_REPORT.txt` is likewise a retained pre-Task-0006 snapshot and is not
current validation evidence; `README.txt` now says so.

## 8. Current validator summary

`--scope all`: 23 PASS, 2 FAIL, 0 BLOCKED, 2 OUT_OF_SCOPE;
`release_ready=false`, `scope_ready=false`.

```text
PASS  filesystem  path_mode_and_cache_hygiene
PASS  text        utf8_lf_and_control_hygiene
PASS  text        concrete_source_fixtures          (15/15 fixtures)
PASS  text        static_example_contracts          (12 valid, 21 invalid pairs)
PASS  structured  strict_json_parse
PASS  structured  python_source_compile
PASS  grammar     static_ebnf_graph
OOS   grammar     complete_example_parse_matrix
PASS  registry    keyword_grammar_parity
PASS  registry    block_field_schema_parity
PASS  registry    collection_value_and_item_contract
PASS  registry    field_value_kind_closure
PASS  registry    nested_parent_closure
PASS  registry    operation_reference_closure
PASS  registry    priority_contract
PASS  registry    operation_contracts
PASS  registry    operation_prose_contracts
PASS  registry    result_schema_cardinality_and_output_contracts
PASS  registry    division_semantics
PASS  registry    set_and_sort_semantics
PASS  registry    constructor_and_pattern_profiles
PASS  registry    diagnostic_selection_contract     (LCL-AUDIT-014 closed)
PASS  registry    mixed_phase_lifecycle_contracts   (LCL-AUDIT-015 closed)
PASS  catalog     requirements_index_integrity      (797 cases, 23 categories)
OOS   catalog     semantic_case_execution
FAIL  integrity   manifest_set_size_hash            (expected stale bytes)
FAIL  integrity   checksum_set_and_hash             (expected stale bytes)
```

Policy fingerprints now pinned by the validator:

```text
diagnostic_selection  0c52f53d57e540c034838a05640bc0f16e816c5ca5393dc513db7fd2a1ced7c2
failure_lifecycle     a6e247aa8fc0278e2514e0fbced9958f50cce14ae243815ef171e43b4ce85afe
```

## 9. Preservation of historical inputs

- Immutable historical extracted tree `/mnt/F/LCL/LCL_Core_0_1_Final`: still 152
  files with aggregate fingerprint
  `7712e2e981e7140cdfde2b679b45b3a96d2b00834541ba6148946376f7d6a7d8`, unchanged
  from the recorded baseline.
- Historical ZIP: absent, consistent with the owner-confirmed intentional
  deletion recorded by LCL-TASK-0001. Its verified lineage hash is retained in
  `00_RELEASE/00_CANONICAL_SOURCE_AND_PROVENANCE.txt`.
- Completion package `/mnt/F/LCL_Completion_Task_Package_v2.0`: `SHA256SUMS.txt`
  is still `bd0fa1acc9a475ec07ee01b386cf65f8e47a981bb8f870bf910436916cc340ec`
  and all 20 records verify.
- Earlier task reports under `/mnt/F/LCL/reports/tasks/` were not modified.
- Frozen integrity artifacts retain their pre-edit bytes:

```text
bee3d2f71a1668b7e5794b54be9fe08687ca42b3da4e5b7dd03ef9632a2a8685  MANIFEST.json
4716d5993417506577643e2ea12d65bb0b7f0219944b6fd1f094948577630ac5  SHA256SUMS.txt
318e4fc95e836cdde1d771638d77ba77be10ede4f41588414536aa0dd90c5921  VALIDATION_REPORT.txt
```

- Current candidate fingerprint (172 files, sorted `./path` plus content
  SHA-256 stream):

```text
82514489c94c4314d2d8160eeafe56b283015ca130357ebec8f71468aed1bdc0
```

No manifest, checksum set, validation report, or release ZIP was regenerated,
and the candidate is not Final, Verified, packaged, or release-ready.

## 10. Next permitted task

`LCL-TASK-0007_REFERENCE_LEXER_AND_PARSER` — the reference lexer and parser.
It is now unblocked because the bare language definition is closed. It must not
be started automatically.

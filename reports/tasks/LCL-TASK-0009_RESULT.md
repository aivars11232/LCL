# LCL-TASK-0009 Result — Close the Event / Handler Activation Model

Date: 2026-09-05

Task status: `COMPLETE` (for the Stage 1 scope)

Bare-spec status: NOT YET COMPLETE — Stages 2 through 7 of the completion plan remain.

Release status: NOT RELEASE READY. Integrity artifacts are deliberately left stale;
regeneration is gated to the final release stage by
`00_RELEASE/03_COMPLETENESS_CRITERIA.txt`.

## 1. Scope and baseline

Stage 1 of the bare-language completion plan: close **BLK-2** (no normative rule defined
what raises `event.*`, how HANDLERs match, how propagation works, or how `RETRY.HANDLER`
participates) and the event-dependent part of **DEF-5**.

Verified from disk before the first edit:

- branch `main`, HEAD `487da7815d3b0e734528efc986d2aa4730dccf2a`, working tree clean,
  0 ahead / 0 behind `origin/main`.
- Canonical fingerprint `7b741c54f5b93cbb69cffcbf7dd914f1a605d66e7cee3fa5dff3b1a04e0ec157`,
  172 files.
- Historical tree `/mnt/F/LCL/LCL_Core_0_1_Final`: 152 files, fingerprint
  `7712e2e981e7140cdfde2b679b45b3a96d2b00834541ba6148946376f7d6a7d8`.
- Completion package `/mnt/F/LCL_Completion_Task_Package_v2.0`: `sha256sum -c` 20/20 OK.
- Baseline validator: 25 PASS, 2 FAIL (stale integrity), 0 BLOCKED, 2 OUT_OF_SCOPE.

No parser, interpreter, compiler, runtime, execution engine, UI, IDE, provider
integration, or executable conformance system was added.

## 2. Invariant repaired

> Emitting a diagnostic is the sole producer of an event; the diagnostic-to-event mapping
> is total over the closed error registry and bijective onto the closed event vocabulary;
> handler selection is a deterministic total function of (raised event, producer path,
> declaration order); and recovery is defined without duplicating diagnostic selection.

## 3. Design applied

**Event source.** Only the emission of a diagnostic raises an event. No timer, host signal,
external notification, or successful completion raises one. This keeps the model inside the
bare-language boundary: it requires no runtime.

**Mapping.** Every one of the 77 registered errors now carries an exact `event` field: one
registered event identifier, or `null` meaning it raises nothing and can never be recovered.
The mapping is total, and `event != null` holds **exactly when**
`recoverable_with_declared_handler` is true. The result is a bijection:

| error | stage | event |
|---|---|---|
| `error.required.missing` | execution | `event.missing` |
| `error.value.unknown` | static_or_expression | `event.unknown` |
| `error.dependency.unsatisfied` | execution | `event.dependency_failure` |
| `error.host.constraint` | execution | `event.host_constraint` |
| `error.retry.exhausted` | execution | `event.execution_error` |

**Event vocabulary narrowed 9 -> 5.** `event.conflict`, `event.validation_error`,
`event.verification_failure`, and `event.cancelled` were removed. Each had exactly one
natural source error, and in every case that error is one recovery must not reach:

- `error.conflict.hard` (resolution) and `error.validation.failed` (validation) are
  document-validity failures that complete *before the first side effect*
  (`05_SEMANTICS/09`); letting a handler recover them would let a document paper over its
  own invalidity.
- `error.verification.failed` (verification_or_completion) — recovering it would allow a
  success claim where a required check failed (`05_SEMANTICS/10`).
- `error.cancelled` (execution) — cancellation is an invoking-authority decision; a document
  must not override it.

Removing them is the honest closure: every remaining event is reachable and every
recoverable error has exactly one trigger. Observation-only handlers (which would let a
handler *see* an unrecoverable condition without recovering it) are deliberately **not**
introduced — that needs its own status, result-ownership and evidence contract, and belongs
to a later version, not to this closure.

**Handler activation.** Closed attachment set: `TASK.HANDLER`, `STEP.HANDLER`,
`RETRY.HANDLER`, `DEPENDENCY.HANDLER`, `FAILURE.HANDLER`. A declared but unattached HANDLER
never activates. Each site's scope is stated exactly. Candidates are ordered by a total
three-key sequence — attachment proximity innermost-to-outermost (local DEPENDENCY/FAILURE,
then RETRY, then STEP, then TASK), then source declaration order in the referenced list,
then handler ID in Unicode scalar order — so there is no tie. A candidate matches on EVENT
equality with an absent or TRUE `WHEN`; exactly the first match runs, so one diagnostic
causes at most one handler invocation. No match leaves the diagnostic unhandled under the
unchanged diagnostic-selection contract.

**RETRY.HANDLER retained with a distinct purpose**: it is in scope only for diagnostics
raised by an attempt of the execution unit declaring that RETRY, including the first
attempt. That is a genuine scope no other attachment site expresses, so the field is not
redundant and was not removed.

**Recovery.** A diagnostic is recovered exactly when the selected handler's invocation
records `status.succeeded`. Any other outcome leaves it unhandled and records the handler's
own diagnostics independently at their registered stages. Status remains governed by
`failure_lifecycle.status_rule`; event selection never reorders, suppresses, or re-ranks
diagnostics, so Task-0006 ordering is preserved intact and not duplicated.

**Non-reentrancy.** Handler selection is not re-entered for diagnostics raised while
evaluating a candidate `WHEN`, resolving a handler invocation contract, or executing a
handler invocation or its FALLBACK for the same originating diagnostic. Those raise no
event. This makes activation acyclic and terminating.

## 4. Files changed (16)

```text
10_REGISTRIES/statuses_and_errors_v0.1.0.json      total event field on 77 errors; new event_model contract
10_REGISTRIES/built_in_groups_and_results_v0.1.0.json   events 9 -> 5
10_REGISTRIES/formats_encodings_units_v0.1.0.json       builtins.events 9 -> 5
06_STANDARD_LIBRARY/08_BUILT_IN_IDENTIFIER_GROUPS.txt   EVENTS list 9 -> 5
05_SEMANTICS/06_..._HANDLER_RESOLUTION.txt              EVENT MODEL section; selection sentence corrected
06_STANDARD_LIBRARY/07_CORE_ERROR_IDENTIFIERS_PART_2.txt  recovery bound to the event model
10_REGISTRIES/field_signatures_v0.1.0.json              HANDLER + RETRY conditional requirements
10_REGISTRIES/block_schemas_v0.1.0.json                 identical mirror
04_GRAMMAR/09_CORE_BLOCK_SCHEMAS_B.txt                  identical mirror; DEF-27 also closed (see 6)
10_REGISTRIES/keywords_v0.1.0.json                      EVENT and HANDLER meanings
02_LEXICAL/05_KEYWORD_REFERENCE_A_TO_M.txt              identical mirror
08_EXAMPLES/VALID/08_AUTHORITY_OVERRIDE_HANDLER_AND_RETRY.lcl   EVENT retargeted (see 5)
09_CONFORMANCE/CASES/core_conformance_cases_v0.1.0.json  events case specialised; EVENT-MODEL-0798 added
09_CONFORMANCE/TOOLS/validate_release.py                new event_model_contract check; 18 pins updated
09_CONFORMANCE/01_CONFORMANCE_REQUIREMENTS.txt          797 -> 798
README.txt                                              797 -> 798
```

No file was created, deleted, renamed, or moved. Candidate inventory remains 172 files.

## 5. Example 08 corrected

`08_AUTHORITY_OVERRIDE_HANDLER_AND_RETRY.lcl` declared `EVENT: event.execution_error` on a
handler guarding `core.download`. Under the closed mapping, `core.download`'s applicable
errors raise exactly one event — `event.host_constraint` — because `error.host.constraint`
is its only handler-recoverable error. The handler could therefore never activate. The
example now declares `EVENT: event.host_constraint`, which makes it the canonical
demonstration: a transient host/network failure raises the event, the handler invokes
`core.retry` authorizing the declared `RETRY LIMIT: 2` (total attempts exactly 3), and
`FALLBACK` applies only if `core.retry` cannot execute. The retry arithmetic and the
`core.retry`/`RETRY` composition were verified unchanged.

## 6. Adjacent defect closed opportunistically

**DEF-27** — `04_GRAMMAR/09` carried 1 of the 3 OUTPUT rules held by both JSON registries.
It was found by the three-way parity checker used to verify this task's own edits, in the
same file this task was editing. Leaving a known parity break behind a green check would
have been unsound, so it was closed. All 41 blocks now agree across `block_schemas`,
`field_signatures`, and `04_GRAMMAR/08`+`09` — a stronger invariant than the shipped
validator enforces, which compares only the two JSON registries.

## 7. Validation

Full gate: `validate_release.py --root canonical/LCL_Core_0.1.0 --scope all`

```text
PASS:         26   (25 baseline + 1 new event_model_contract)
FAIL:          2   (pre-existing stale MANIFEST.json / SHA256SUMS.txt)
BLOCKED:       0
OUT_OF_SCOPE:  2   (parser matrix, semantic execution — retired package tasks)
```

The two failures are the deliberately stale integrity artifacts. They are not repaired here:
`00_RELEASE/03_COMPLETENESS_CRITERIA.txt` requires manifest, validation report and checksums
to be regenerated in that acyclic order only after substantive files are stable.

`registry/event_model_contract` PASS reports: 77 mapped errors, 5 event-raising errors,
5 registered events, 1 checked shipped handler, policy fingerprint
`5012d94b66dedf5fa3541e3cf6c36bd90f427d45c69554acd3f365dfcc5d3914`.

**Honest coverage statement.** The check computes the invariants from the registries rather
than comparing against a duplicated literal: mapping totality, the event/recoverability
equivalence, cross-surface vocabulary agreement (both registries and the prose list),
bijection onto the registered vocabulary, and reachability of every shipped HANDLER event
from the operations that handler can guard. It does not parse or execute LCL, and it states
that limit in its own `static_limit` field.

**Non-vacuity proven.** Eight mutation probes on isolated copies each produced FAIL with the
exact naming violation; the unmutated tree passes:

| probe | result |
|---|---|
| example 08 event reverted to `event.execution_error` | FAIL — names an event no guarded operation can raise |
| example 08 event set to a removed identifier | FAIL — unregistered event |
| an error made recoverable without an event | FAIL — invariant violated |
| two errors mapped to the same event | FAIL — not a bijection |
| one registry's vocabulary shortened | FAIL — surfaces disagree |
| prose event list shortened | FAIL — surfaces disagree |
| `event_model.non_reentrancy_rule` deleted | FAIL — contract key absent |
| one error's `event` field deleted | FAIL — mapping not total |

The first probe initially passed: the reachability set wrongly included the handler's own
`OPERATION`, so it would not have caught the original defect. The check was tightened to
exclude handler-side operations, which is exactly what the new non-reentrancy rule states,
and the probe then failed correctly. This is recorded rather than silently fixed.

Independent re-verification outside the validator: event field total over 77 errors; the
event/recoverability equivalence; bijection onto 5 events; vocabulary identical across both
registries and the prose; 13 `event_model` keys; 141/141 keyword registry/prose parity;
41/41 block rule parity.

## 8. Validator pins updated honestly

Adding a real normative field broke three checks that compare whole error-registry entries
against pinned literals. All 18 pinned error dicts in `validate_release.py` gained the new
`event` key with the value the registry actually holds, resolved per-error and asserted
against the registry's own recoverability flag while patching. No check was weakened,
skipped, or given a pass-through branch; the three whole-dict comparisons still compare
whole dicts, and the subset comparisons are now *stronger* because they additionally assert
the event mapping.

Catalog count pins were updated honestly rather than avoided: 797 -> 798 cases and 23 -> 24
categories, in the catalog, the validator's `expected_category_counts`, `expected_sources`,
the `case_count` assertion, the 24-category message, `09_CONFORMANCE/01`, and `README.txt`.

## 9. Known items deliberately left to later stages

- **DEF-5 (extension events).** `qualified_identifier(event)` still admits `defined_kind:
  kind.event`, so an extension may declare an event that no registered error raises and that
  therefore can never activate a handler. Stage 1 makes this precisely diagnosable for the
  first time; Stage 7 must either give `kind.event` declarations the handler-selection
  metadata the model needs or remove the capability.
- **BLK-5/BLK-6, DEF-15, DEF-20, DEF-25** (invocation sites) — Stage 2.
- Integrity artifacts, `VALIDATION_REPORT.txt`, `CHANGELOG.txt`, `MANIFEST.json` counts and
  the `RELEASE CANDIDATE` headers — the final release stage.
- A third invocation-site surface was found while reading error prose:
  `06_STANDARD_LIBRARY/06_CORE_ERROR_IDENTIFIERS_PART_1.txt` lines 28-34 still scope
  `error.operation.parameter` to **ACTION** only, alongside the already-known
  `04_GRAMMAR/03:14`. Recorded here for Stage 2.

## 10. Preservation

- `/mnt/F/LCL/LCL_Core_0_1_Final`: unchanged, 152 files, fingerprint
  `7712e2e981e7140cdfde2b679b45b3a96d2b00834541ba6148946376f7d6a7d8`.
- `/mnt/F/LCL_Completion_Task_Package_v2.0`: unchanged, 20/20 checksum records verify.
- Earlier task reports under `reports/tasks/`: unmodified.
- No commit, push, stage, tag, PR, or release was performed. All work is left in the
  working tree.

## 11. Next permitted task

Stage 2 — invocation-site closure (BLK-5, BLK-6, DEF-15, DEF-20, DEF-25, plus the
`06_STANDARD_LIBRARY/06` surface found in section 9).

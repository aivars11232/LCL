# LCL-TASK-0008 Result — Close Custom Operation and Non-ACTION Invocation Contracts

Date: 2026-09-04

Task status: COMPLETE (for the approved D+C scope)

Bare-spec status: NOT YET COMPLETE — verified audit findings A1, A2, A3, B, E1–E4,
F1, parts of G, H, and the `core.calculate` embedded-expression concern remain open.

Release status: NOT RELEASE READY. No release task was authorized and release
integrity artifacts were deliberately left stale.

## 1. Scope and approval

- Task package: `/mnt/F/LCL-TASK-0007_D_C_SEMANTIC_REPAIR`.
- Approved plan: the read-only plan produced under `PLANNING_AND_APPROVAL_GATE.md`,
  recommending option **D-2** (declare custom-operation axes on `DEFINE`; retire the
  custom-operation implementation-profile obligation) and option **C-1** (`ACTION`,
  `HANDLER`, and `FALLBACK` are invocation sites, with one declared handler-context
  target binding and a `RETRY`-authoritative retry composition).
- Owner approval: message `approved`, given after the plan was presented. No file was
  modified before that approval.
- Confirmations: exactly one foreground agent; no background, parallel, delegated, or
  sub-agent was used; no background or parallel shell execution; bare LCL specification
  and its static conformance material only. No lexer, parser, interpreter, compiler,
  runtime, execution engine, UI, IDE, workspace application, provider integration,
  OS integration, daemon, deployment, or product code was written.

### Task identifier correction (required by `README_FIRST.md`)

The package expected `LCL-TASK-0007`. That ID was **already taken** in the actual local
tree by a different, completed task (static validation, integrity regeneration, and
bare-language release packaging): `reports/tasks/LCL-TASK-0007_BASELINE.md`,
`reports/tasks/LCL-TASK-0007_RESULT.md`, and commit
`c541874 LCL-TASK-0007: validate, regenerate integrity, and package the bare-language release`.

Nothing was overwritten or renumbered. The mismatch was reported in the planning
response and this work is recorded as **LCL-TASK-0008**.

## 2. Baseline

- Repository root: `/mnt/F/LCL`
- Candidate root: `/mnt/F/LCL/canonical/LCL_Core_0.1.0` (172 files, unchanged in count)
- Branch: `main`, ahead 2 of `origin/main`
- Pre-edit HEAD: `c5418742ea7ba02802f1456c46bd59665feeccb2`
- Pre-edit Git status: clean; no modified, staged, or untracked files
- Pre-edit validation: `validate_release.py --scope all` -> exit 0,
  **25 PASS / 0 FAIL / 0 BLOCKED / 2 OUT_OF_SCOPE**, `release_ready=true`
- Read-only planning phase changed no file. (Disclosure: `.git/FETCH_HEAD` was written
  during the session by an external git client, not by this task. It is git metadata,
  not a specification or working-tree file.)

## 3. Finding D resolution

### Contradiction verified

- `03_TYPES_AND_VALUES/05_CUSTOM_TYPES_SCHEMAS_AND_DEFINITIONS.txt` required every
  `kind.operation` definition to select one immutable profile with `profile_role
  implementation`.
- `operations_v0.1.0.json#/axis_contract/implementation_profile.required_properties`
  demanded ten properties, including `implementation_id`, `implementation_version`,
  `target_class`, `determinism_source`, `possible_dependencies`, `possible_effects`,
  and `invocation_resolution`.
- The closed `DEFINE` surface (`field_signatures_v0.1.0.json`, `unknown_fields:
  forbidden`) offered no field for any of them, and no `PROFILE` keyword existed among
  the 141 reserved words.
- Therefore `SIDE_EFFECT: TRUE` in `08_EXAMPLES/VALID/07_DOMAIN_EXTENSION_OPERATION.lcl`
  was required to supply information that LCL could not express, and
  `12_SET_SORTING.lcl`'s `sort.identity_key` could not state the
  `{declared_state_only}` dependency set that `core.sort` demands of a key operation.

Every part of Finding D reproduced against current bytes.

### Approved design (option D-2) and the exact normative rule after repair

A custom `kind.operation` **selects no implementation profile**. It declares its complete
axis contract in its own `DEFINE` block:

- `SIDE_EFFECT: FALSE` declares possible effects exactly `{none}`.
- `SIDE_EFFECT: [<effect classes>]` declares one or more distinct concrete effect
  classes from the closed `effect_classes` vocabulary, excludes `none`, and is the
  operation's possible-effect maximum. **Bare `SIDE_EFFECT: TRUE` is retired** — this is
  the one compatibility-affecting syntax change, called out in the plan before approval.
- `DEPENDENCY: [<dependency classes>]` declares the possible-dependency maximum from the
  closed `dependency_classes` vocabulary; when omitted it declares exactly
  `{declared_state_only}`.
- The invocation effect set resolves from the address classes of the resolved `TARGET`
  and declared destination arguments, bounded by the declared maximum. A resolved class
  outside that maximum, or a declared effect maximum that resolves no concrete class,
  emits `error.operation.precondition` before effects.
- `DETERMINISTIC TRUE` is verified against the declared contract (no permitted
  variation) rather than a profile category; `error.determinism.mismatch` behavior is
  unchanged.

Core-operation profiles are untouched. A core profile is a host binding resolved at
invocation; it was never an obligation the document had to express. That distinction is
why Finding D applied to custom operations only.

### Syntax and registry representation

Zero new keywords were introduced, so the conformance catalog kept all 797 cases and
every pinned category count (`keyword_valid/invalid: 141`, `block_*: 41`). `[state]` and
`[model]` already parse: `COLLECTION_LITERAL` admits `EXPRESSION`, and `VALUE_PRIMARY`
admits `IDENTIFIER`. **The EBNF required no change**, verified by `--scope grammar`.

New machine-readable surface: named value kinds `side_effect_declaration` and
`dependency_class_list`, and the optional `DEFINE.DEPENDENCY` field. Field totals moved
334 -> 335 and distinct value kinds 65 -> 67, recomputed from the registry and
synchronized into `04_GRAMMAR/13_EXACT_FIELD_SIGNATURES.txt`.

### Affected examples

- `07_DOMAIN_EXTENSION_OPERATION.lcl`: `SIDE_EFFECT: TRUE` -> `SIDE_EFFECT: [state]`
  (the `OUTPUT` target's address class) plus `DEPENDENCY: [model]`.
- `12_SET_SORTING.lcl`: **unchanged**, and now valid for its documented reason — its
  omitted `DEPENDENCY` normatively resolves to `{declared_state_only}`, which is exactly
  what `core.sort` requires of a key operation.

### Focused validation evidence

New check `registry/custom_operation_declaration_contract` (PASS) verifies that the
`DEFINE` surface admits both axis declarations, that the named value kinds resolve
against the closed vocabularies, that `custom_operation_resolution` governs
`SIDE_EFFECT`/`DEPENDENCY`/`DETERMINISTIC` and is no longer nested under
`implementation_profile`, that six normative surfaces no longer assert any retired
custom-operation claim, and that both shipped `kind.operation` definitions declare axis
values inside the closed vocabularies with the exclusivity rules enforced.

The check was proven non-vacuous on a scratch copy: reintroducing
`SIDE_EFFECT: TRUE` and a retired profile sentence produced FAIL with both violations
named. The real tree was not touched by that test.

## 4. Finding C resolution

### Contradiction verified

- `error.operation.parameter` was scoped to `ACTION` in prose
  (`06_STANDARD_LIBRARY/10`), in `statuses_and_errors_v0.1.0.json`, and in the validator.
- `core.retry` requires `TARGET REFERENCE[ACTION]` and a required `limit` parameter;
  `core.stop` requires a `TARGET`.
- Example 08's handler supplied neither a `TARGET` nor a `limit` parameter, and no rule
  mapped handler `LIMIT` to the operation parameter `limit`.
- No prose defined whether `HANDLER.OPERATION` was invoked at all; the only statements
  were "5. matching HANDLER" and the failure-lifecycle mentions.
- No rule composed the ACTION-level `RETRY` block with a handler running `core.retry`,
  so total attempts were genuinely undefined.

**One audit claim was corrected:** `HANDLER` already carried optional `TARGET` and
repeatable `PARAMETER` fields. The audit's "HANDLER/FALLBACK structurally cannot carry
invocation data" is true only of `FALLBACK`. The defect was nonetheless real, because the
enforcing error was scoped to `ACTION`, so nothing made those fields mandatory.

### Approved design (option C-1)

- **ACTION semantics:** unchanged.
- **HANDLER semantics:** a HANDLER is an invocation site. `OPERATION` selects and invokes
  one operation under the same contract that binds an ACTION. It is not a bare
  reference and not a handler selector; `EVENT` alone selects the handler.
- **Target contract:** `HANDLER.TARGET` supplies the target. Exactly one omission is
  permitted — the **handler-context binding**: when the operation's registered target type
  is `REFERENCE[ACTION]` or `REFERENCE[meta.execution_unit]` and `TARGET` is omitted, the
  target binds to the execution unit that raised the handled event. This binding is
  declared in the specification, never left to host convention, and applies to no other
  target type. Any other omission uses `error.operation.parameter`.
- **Parameter contract:** `HANDLER.PARAMETER` supplies named parameters, and
  `HANDLER.LIMIT` **is** the `limit` named parameter of the selected operation — legal
  only when that operation registers `limit`, and a duplicate when both forms appear.
- **FALLBACK semantics:** a second invocation site with no target or parameter surface,
  admitting exactly two forms: one `REF` to a HANDLER (which carries its own complete
  invocation data), or one operation identifier that registers no required named
  parameter and whose required target the handler-context binding supplies. Anything else
  uses `error.operation.parameter` and never executes with an unsatisfied contract.
- **`core.retry` vs `RETRY`:** the ACTION or STEP `RETRY` block is the only source of
  attempt bounds. **Total attempts are exactly `1 + LIMIT`.** A handler running
  `core.retry` authorizes that declared RETRY and never opens a second budget; its
  resolved `limit` must equal the RETRY `LIMIT`, and an unequal limit or a wrapped ACTION
  with no RETRY block emits `error.operation.precondition`. `DELAY` and `WHEN` come from
  the RETRY block, their only declaration site.

`error.operation.parameter` now reads "An ACTION, HANDLER, or FALLBACK invocation site
…", so no path bypasses a required operation contract except by the one explicit,
semantically justified exemption above.

### Example 08

**Unchanged, byte for byte**, with exactly one interpretation:

| Element | Resolution |
|---|---|
| `OPERATION: core.retry`, no `TARGET` | handler-context binding -> `action.download` |
| `LIMIT: 2` | binds the required `limit` parameter = 2 |
| ACTION `RETRY LIMIT: 2` | equals the handler limit -> precondition satisfied |
| Total attempts | **3** (`1 + 2`), `DELAY` 1s, `WHEN` from the RETRY block |
| `FALLBACK: core.stop` | legal: no required named parameter; `REFERENCE[meta.execution_unit]` supplied by the binding |

### Focused validation evidence

New check `registry/invocation_site_contract` (PASS) verifies that the widened rule is
stated on the error registry and all three prose surfaces, that HANDLER carries the
fields its rule needs and that both block-schema surfaces state the invocation-site,
binding, FALLBACK, and attempt-bound rules, that `core.retry` binds the composition rule
and the limit-equality precondition, and that every shipped HANDLER can actually satisfy
the contract it names — required target, required parameters, LIMIT legality, no
duplicate or unregistered parameter, and FALLBACK admissibility.

Proven non-vacuous on a scratch copy: `FALLBACK: core.cancel` (required `reason`),
`OPERATION: core.continue` (no `limit` parameter) with `LIMIT: 2`, and a weakened retry
composition sentence each produced a named FAIL.

## 5. Changed files

Twenty files, all inside the candidate tree. Each was edited alone and verified before
the next was opened.

| File | Purpose | Verification |
|---|---|---|
| `10_REGISTRIES/field_signatures_v0.1.0.json` | new value kinds; `DEFINE.SIDE_EFFECT` kind; `DEFINE.DEPENDENCY`; HANDLER + RETRY conditional requirements; FALLBACK value-kind definition | strict JSON parse with duplicate-key detection, targeted assertions, `--scope registry` |
| `10_REGISTRIES/block_schemas_v0.1.0.json` | mirror `DEFINE.DEPENDENCY`, HANDLER rules, RETRY rules | JSON parse, `--scope registry` block/field parity |
| `10_REGISTRIES/operations_v0.1.0.json` | `custom_operation_resolution` moved to `axis_contract` top level and rewritten; `role_resolution`, `bounds`, `graph_resolution` updated; `core.retry` composition rule and precondition | JSON parse, `--scope registry` |
| `10_REGISTRIES/statuses_and_errors_v0.1.0.json` | `error.operation.parameter` widened to all invocation sites | JSON parse, `--scope registry` |
| `10_REGISTRIES/keywords_v0.1.0.json` | `SIDE_EFFECT`, `DEPENDENCY`, `OPERATION`, `HANDLER`, `FALLBACK` meanings | JSON parse, `--scope registry` |
| `03_TYPES_AND_VALUES/05_CUSTOM_TYPES_SCHEMAS_AND_DEFINITIONS.txt` | `kind.operation` rules rewritten for the declared-axis model | exact diff, `--scope text`, prose-token parity |
| `05_SEMANTICS/11_DETERMINISM_EQUIVALENCE_AND_INTERPRETER_VARIATION.txt` | custom operations resolve from their declaration | exact diff, `--scope text` |
| `05_SEMANTICS/06_MISSING_UNKNOWN_NULL_DEFAULT_ASSUME_AND_HANDLER_RESOLUTION.txt` | new normative HANDLER/FALLBACK invocation semantics | exact diff, `--scope text` |
| `05_SEMANTICS/08_PHASE_SEQUENCE_STEP_BRANCH_LOOP_RETRY_AND_CONCURRENCY.txt` | `RETRY` / `core.retry` composition and attempt count | exact diff, `--scope text` |
| `06_STANDARD_LIBRARY/10_CORE_OPERATION_PARAMETER_RULES.txt` | invocation-site rule; custom-operation axis rule | exact diff, `--scope text` |
| `06_STANDARD_LIBRARY/06_CORE_ERROR_IDENTIFIERS_PART_1.txt` | custom-operation precondition trigger | exact diff, `--scope text` |
| `06_STANDARD_LIBRARY/03_CONTROL_OPERATIONS.txt` | `core.retry` prose synced to the registry line | exact diff, registry/prose parity check |
| `04_GRAMMAR/08_CORE_BLOCK_SCHEMAS_A.txt` | `DEFINE` optional list gains `DEPENDENCY` | three-way parity check against both registries |
| `04_GRAMMAR/09_CORE_BLOCK_SCHEMAS_B.txt` | HANDLER and RETRY prose rules | rule-set parity against `block_schemas` |
| `04_GRAMMAR/13_EXACT_FIELD_SIGNATURES.txt` | field/value-kind counts 334/65 -> 335/67 | recomputed from the registry and compared |
| `02_LEXICAL/05_KEYWORD_REFERENCE_A_TO_M.txt` | `DEPENDENCY`, `FALLBACK`, `HANDLER` meanings | registry/prose parity |
| `02_LEXICAL/06_KEYWORD_REFERENCE_N_TO_Z.txt` | `SIDE_EFFECT`, `OPERATION` meanings | registry/prose parity; full 141/141 sweep |
| `08_EXAMPLES/VALID/07_DOMAIN_EXTENSION_OPERATION.lcl` | declares `[state]` effects and `[model]` dependency | `--scope text`, `static_example_contracts` |
| `09_CONFORMANCE/CASES/core_conformance_cases_v0.1.0.json` | six case texts updated; counts unchanged | JSON parse, `--scope catalog` |
| `09_CONFORMANCE/TOOLS/validate_release.py` | pinned literals resynced; two new real checks added | AST syntax check, all scopes, negative tests |

Unchanged and verified as already conformant: `08_EXAMPLES/VALID/12_SET_SORTING.lcl` and
`08_EXAMPLES/VALID/08_AUTHORITY_OVERRIDE_HANDLER_AND_RETRY.lcl`.

### Validator literals resynced (disclosed in full)

These pinned constants encoded the retired contracts and were updated to the approved
text. None of them weakens a check; each still asserts an invariant, and two token lists
were **strengthened**:

1. `expected_axis_contract` — structural and textual sync.
2. `03_TYPES_AND_VALUES/05` prose tokens — two retired tokens replaced by five tokens
   asserting the new rule. Two other tokens that failed only because of line wrapping
   were **restored by rewrapping the prose**, not deleted.
3. `ERROR-CONTRACT-0727` and `ENUM-GROUPS-0764` catalog tokens.
4. `ERROR-CONTRACT-0725` catalog tokens — expanded from six to nine.
5. `TASK_0002_NAMED_VALUE_KIND_DEFINITIONS["operation_identifier_or_handler_reference"]`.
6. `core.retry` pinned precondition list (new limit-equality precondition).
7. `EXPECTED_OPERATION_ROW_FINGERPRINTS["core.retry"]` and
   `EXPECTED_OPERATION_CONTRACT_FINGERPRINT`, recomputed independently.
8. Task-0004 catalog fingerprint, recomputed twice as case text changed.

## 6. Exact commands and exit codes

All runs serial, with `PYTHONDONTWRITEBYTECODE=1`. Per-file narrow checks are listed in
section 5; the closure runs were:

| Command | Exit | Result |
|---|---|---|
| `python3 09_CONFORMANCE/TOOLS/validate_release.py --scope filesystem` | 0 | 1 PASS |
| `... --scope text` | 0 | 3 PASS |
| `... --scope structured` | 0 | 2 PASS |
| `... --scope grammar` | 0 | 1 PASS, 1 OUT_OF_SCOPE |
| `... --scope registry` | 0 | 17 PASS |
| `... --scope catalog` | 0 | 1 PASS, 1 OUT_OF_SCOPE |
| `... --scope integrity` | 1 | 2 FAIL (stale integrity) |
| `... --scope all` | 1 | 25 PASS / 2 FAIL / 0 BLOCKED / 2 OUT_OF_SCOPE |

Failed commands are not omitted: the two intermediate `--scope registry` runs that
reported `operation_contracts`, `operation_prose_contracts`, `field_value_kind_closure`,
and `set_and_sort_semantics` failures were the pre-identified literal-sync points, each
inspected before proceeding and each closed by the corresponding sync. One deviation from
the plan: the plan pre-announced literal-sync failures at steps D3 and C1 only; the
catalog-fingerprint and `ERROR-CONTRACT-0725`/`0727` token pins produced two further
expected failures of the same class, which were inspected and resolved rather than
worked around.

A `python3 -m py_compile` invocation wrote `09_CONFORMANCE/TOOLS/__pycache__` into the
candidate tree. It was removed immediately, and the remaining syntax checks used
`ast.parse`, which writes nothing. `path_mode_and_cache_hygiene` passes and the tree
contains zero bytecode artifacts.

## 7. Full static gate

- Command: `PYTHONDONTWRITEBYTECODE=1 python3 09_CONFORMANCE/TOOLS/validate_release.py --scope all`
- Exit code: **1**
- PASS: **25** (23 pre-existing + 2 new checks added by this task)
- FAIL: **2**
- BLOCKED: **0**
- OUT_OF_SCOPE: **2**
- `release_ready`: `false`

Remaining failures:

| Check | Cause | Pre-existing or introduced |
|---|---|---|
| `integrity/manifest_set_size_hash` | `MANIFEST.json` describes pre-edit bytes | **Introduced by this task, deliberately.** Rule 11 forbids regenerating release integrity artifacts. Not repaired to obtain a green result. |
| `integrity/checksum_set_and_hash` | `SHA256SUMS.txt` describes pre-edit bytes | Same. |

Out-of-scope checks are unchanged from baseline: `grammar/complete_example_parse_matrix`
and `catalog/semantic_case_execution` both require an executable parser/interpreter,
which this task is forbidden to build.

### Honest coverage limits

- The validator does **not** parse or execute LCL. The two new checks operate on
  registries, prose, and targeted field scans of shipped examples. Example conformance to
  the repaired rules is argued from the normative text plus those scans, not machine-proved
  by a parser.
- `04_GRAMMAR/08`, `09`, and `13` prose remain unchecked against the registries by the
  validator generally; parity for the specific blocks this task touched was verified by
  targeted comparison, and the new `invocation_site_contract` check now does assert the
  HANDLER and RETRY rule presence in `block_schemas`.
- Full verification of `DETERMINISTIC TRUE` is not statically decidable. Only the
  declared-axis coherence half is checked.
- Neither new check claims coverage it does not perform; each reports its limit in its
  own `static_limit` field in the validator output.

## 8. Deferred audit findings

Unresolved and explicitly **not** addressed by this task:

- **A1** — defined scalar/enum types cannot be written as `TYPE: <defined type>`.
- **A2** — bare `NULL` grammar ambiguity.
- **A3** — dead named-call-argument syntax.
- **B** — unreachable `error.keyword.case`.
- **E1** — `result.` / `meta.` are normative namespaces but unreserved.
- **E2** — `result.effect` is a tenth `result.*` schema against a stated nine.
- **E3** — extension statuses/errors cannot participate in closed namespaces.
- **E4** — conformance cases use unregistered `core.error_selection` and
  `core.failure_lifecycle`.
- **F1** — invalid examples 06 and 15 test the same condition.
- **G** — environment-looking core vocabulary and `core.execute` graph re-entry.
- **H** — stale RELEASE CANDIDATE headers, changelog omissions, dates, provenance,
  superseded root artifacts, hard-coded Gate-8 coverage count.
- **`core.calculate`** — expression text accepted as STRING with no defined embedded
  grammar.

Two observations recorded rather than repaired, since both are outside the approved
scope: core operations still resolve host-supplied implementation profiles (a host
binding, not a document obligation, which is why Finding D did not apply to them); and
custom operations can now claim only address-derivable effect classes, so `process`,
`package`, and `message` effects remain reachable only through the core rows that own
them. That narrowing is stated normatively rather than left implicit.

Completing D and C does **not** make the bare specification complete.

## 9. Repository state

- Post-edit Git status: 20 modified files, all under `canonical/LCL_Core_0.1.0/`; no
  untracked files; no deletions; candidate file count still 172.
- Staged files: none.
- Commits: none.
- Pushes, tags, PRs, releases: none.
- Manifest/checksum regeneration: none.
- Final ZIP generation: none.
- HEAD unchanged at `c5418742ea7ba02802f1456c46bd59665feeccb2`.
- No user work was discarded; no destructive git operation was run.

## 10. Stop condition

LCL-TASK-0008 is closed for its approved scope. No successor task was started.

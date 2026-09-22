# LCL implementation conformance obligations — Core 0.1.0

Status, 2026-09-14 (LCL-CLOSURE-4T, Task LCL-CLOSE-02): conformance accounting
and production reporting are complete; full semantic conformance is BLOCKED.
This document defines required evidence; it does not report that the engine
has passed that evidence. Actual outcomes are in
`reports/tasks/LCL-CLOSE-02_RESULT.md` and
`reports/implementation/LCL_RESIDUAL_REPAIR_REPORT.md`.

Authority is the unchanged, verified `canonical/LCL_Core_0.1.0` package,
identity `00d648b162939d06c44838481a67c39bc12c64bdd6d105035c24150148fe67ed`.
`09_CONFORMANCE/01_CONFORMANCE_REQUIREMENTS.txt` requires lexical, grammar,
block and field evidence for source conformance, and additional type, operator,
function, operation, status, error and execution evidence for semantics.
Registry/grammar authority outranks illustrative examples under
`00_RELEASE/02_NORMATIVE_AUTHORITY_ORDER.txt`.

## Required evidence and accounting

| Obligation family | Canonical basis | Concrete evidence required |
| --- | --- | --- |
| Lexical fixtures | `09_CONFORMANCE/SOURCE_FIXTURES/expected_results.json` | All 15 exact byte fixtures, with their specified lexical outcomes. |
| Lexical rules | `04_GRAMMAR` lexical grammar and keyword/symbol registries | Admitted tokens, case rejection, identifiers, numeric/string/constructor forms, whitespace, indentation, boundaries and malformed controls. |
| Grammar examples | `08_EXAMPLES/VALID`, `08_EXAMPLES/INVALID` | All 13 valid sources and all 21 invalid sources at their applicable earliest stage; a later-stage defect requires clean parsing, not successful execution. |
| Document and expression forms | `04_GRAMMAR` productions | Document kinds and ordering; expressions, collections, positional calls, selectors, type expressions and control bodies, including malformed controls. |
| Blocks | All 41 registered block signatures | Legal contexts and required minimal bodies; forbidden fields, conditional requirements and nested bodies. No skipped unsynthesizable blocks. |
| Fields | All 334 block/field uses | Each admitted field form and cardinality, missing required fields, forbidden/duplicate fields and invalid forms in an otherwise valid containing block. Shared cases may establish multiple clauses, but exact membership is recorded. |
| Semantic contracts | The type, operator, function, operation and status/error registries and `05_SEMANTICS` | Concrete typed values and observed results for the applicable contracts, including failure, effect, execution and completion controls. Registry descriptions alone cannot establish behavior. |
| Decision witnesses | All 66 entries of `language_decision_cases_v0.1.0.json` | Every explicitly required probe of each witness. A single passing ID prefix cannot replace omitted probes. |

The obligation inventory is derived from the verified package and a versioned
implementation mapping. Its construction must reject an unverified or different
package, incomplete mappings and duplicate IDs. Callers cannot choose a smaller
inventory and thereby certify the whole version. Reports identify the mapping
digest separately from the canonical package identity.

The runner and production report use the same inventory and concrete case
population. Required case IDs are exact, including sub-probe labels. A missing,
failed or duplicated required record blocks the affected claim. Irrelevant
records do not fill gaps. Source and semantic completeness are evaluated
separately; a semantic failure cannot manufacture source completeness.

The 799 descriptive requirements, 66 indexed witnesses, executed probes,
established unique witnesses and missing obligations are separate populations.
One concrete probe may cover several normative clauses. This does not create
799 duplicate executions or turn catalog entries into executed tests. Tests of
report arithmetic are instrument tests, never evidence about the LCL engine.

## The implementation mapping, revision r3

The mapping is `impl/crates/lcl-conformance/src/obligations_v0.1.0_r3.json`,
SHA-256 `296fc2bef03cb4a4ec45f00d5b43601f5c234a9d55a22c188ce477630013c3f3`,
pinned in `obligations.rs` as `MAPPING_DIGEST`. It replaces revision r2 under
an explicit owner approval; r2 replaced r1, which pinned probe identifiers only.

Two figures in this section were stale against r2 and are corrected here rather
than carried forward: the digest read `27e3271f…`, and the sub-run total read
3,724 where r2 pinned 3,725.

### What r3 changes, and what it does not

r3 removes **exactly four** sub-run pins from
`semantic/operation_errors/core.sort`:

    precondition/key-operation-profile-missing
    precondition/key-operation-profile-ambiguous
    precondition/key-operation-profile-incomplete
    precondition/key-operation-profile-out-of-bounds

Those four words are the registry's closed vocabulary for a **profile-role
selection** fault, and `core.sort` selects no such role: it has no entry in
`operations_v0.1.0.json#/axis_contract/implementation_profile/required_roles_by_operation`,
and `role_resolution` states that "A core operation absent from
required_roles_by_operation requires no local core profile." A custom
`kind.operation` key likewise "selects no profile role and resolves under
axis_contract.custom_operation_resolution". With no role to select, none can be
missing, ambiguous, incomplete or out of bounds, so no input reaches the
condition. An earlier revision closed these by reading the four words as
properties of the declared key contract; that equivalence was withdrawn in
`reports/tasks/POST-FINAL-05_CORRECTIVE_02_RESULT.md` §7 as not
canon-authorized, and the pins were reopened rather than left closed on it.

Nothing else changed. The row and its probe remain; all 2,413 probes remain;
980 rows remain; no level was reclassified; and the key-contract checks that
rest on their own stated requirements —
`precondition/incompatible-key-operation-signature` and
`precondition/invalid-key-operation-axes` — are still pinned. The correction
does not alter Core 0.1 semantics and does not expand any operation's
capabilities. `core.execute precondition/profile-out-of-bounds` is a separate
obligation and is **not** covered by this approval: it remains pinned and
unestablished.

- **Probes.** All 2,413 probe IDs are kept: 2,011 source and 402 semantics.
- **Sub-runs.** Each of the 319 semantic contract rows also pins its exact
  required sub-run labels, **3,721** in total. They are derived from the row's
  canonical requirement clauses and registry facts, and include clauses that no
  run exercises yet.
- **Establishing a probe.** A grouped record establishes its probe only when
  its judged runs carry exactly the pinned labels, each once, and every run
  passes. The report names every missing, unexpected, duplicated or failed
  sub-run.
- **Loader cross-checks.** The loader refuses a mapping that leaves a semantic
  row unpinned, or whose registry-derivable pins differ from the registries:
  - status successor sets;
  - error registry contracts and stage-owner mirrors;
  - operation binding, default, bound, store, determinism and profile runs;
  - registered error identifiers and condition indices.
- **Known failures.** A failing sub-run that exposes an engine defect stays
  failed evidence in the report. The conformance tests pin those probes and
  sub-runs exactly, so any change to that set fails the tests.

## Previously unestablished decision witnesses

All 66 witnesses are established in the current production report
(LCL-CLOSE-02). The table records the discriminating cases that were required.

| ID | Required discriminating case |
| --- | --- |
| CLOSURE-004 | Read `REF(output.copy).TARGET` before output binding; observe declaration metadata without reading the unbound value. |
| CLOSURE-006 | Read a stored REFERENCE once; distinguish two references with equal referent values and preserve equal reference identity. |
| CLOSURE-021 | Group three typed OBJECT values with tags a, b, a; check exact closed key/items records, first-group order and member order. |
| CLOSURE-022 | Deterministic initial failure then success, LIMIT 2: exactly two attempts and no exhaustion. |
| CLOSURE-023 | Initial failure, LIMIT 2 WHEN FALSE: one attempt, original unhandled failure and no exhaustion. |
| CLOSURE-024 | LIMIT 2 and repeated failure: exactly three unsuccessful attempts followed by `error.retry.exhausted`. |
| CLOSURE-027 | Successful selected `core.continue` handler recovers the originating diagnostic and advances to the declared successor. |
| CLOSURE-055 | A second sequential sibling requests BEFORE the first: `error.execution.order` before effects; legal order control. |
| CLOSURE-058 | A FOR EACH producer's output read outside the body is unresolved; an inside-body control is valid and does not imply aggregation. |
| CLOSURE-059 | Permitted partial command output followed by an authorized safe retry: next attempt starts unbound, old partial evidence stays ordered/local, final projection follows bound/partial/unbound policy. |
| CLOSURE-060 | Real imported targetless VERIFY FALSE remains inactive on import alone; an explicit prerequisite activates it. |

Failure schedules and typed invocation inputs are deterministic test data, not
new language syntax. Retry-after-effect proof must come from the host boundary,
be bound to the request, prior attempt, observed effect and original authority,
and default to refusal when absent, stale, mismatched or unsafe. It cannot be a
program-supplied safe flag. Real operations and the shared engine remain in use.

## Closure criteria

The mapping and claim-accounting regressions must pass independently of the
behavior probes. All applicable missing witnesses must then execute and pass;
an honest lower claim alone does not close B4. Any unresolved canonical conflict
requires exact higher-authority clauses and a minimal discriminating source.
Actual gate outcomes, counts, logs and limitations belong in the residual repair
report and generated conformance report, not in this requirement definition.

Under the LCL-CLOSE-02 exit contract, reporting-correctness may close while full
conformance remains BLOCKED. That is not the full objective passing. The exact
remaining sub-runs and the engine defects they expose are listed in
`reports/tasks/LCL-CLOSE-02_RESULT.md`.

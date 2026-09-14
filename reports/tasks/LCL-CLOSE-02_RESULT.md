# LCL-CLOSE-02 — result

| Field | Value |
| --- | --- |
| Task | LCL-CLOSE-02, conformance evidence and audit reconciliation (LCL-CLOSURE-4T v1.1) |
| Result | **CLOSED: reporting-correct; full semantic conformance BLOCKED** |
| Closed | 2026-09-14 |
| Repository | `/mnt/F/LCL`, branch `main`, HEAD `298d1e2b01a0dcd4dbecb5bf401143948b1cf716` ("LCL repair task2", the owner's checkpoint). Nothing is staged, and this task made no Git write |
| Successor | LCL-CLOSE-03 stays **locked**: this closure is not B4 passing |

## Verdict

- **Reporting is complete and verified.** B4 obligation accounting, production integration and reporting are done.
  The production report claims exactly what real execution evidence supports: `source_conforming`.
- **Full semantic conformance is BLOCKED.** 813 pinned semantic sub-runs have no passing run:
  - 750 have not been run;
  - 54 belong to the two families with no population;
  - 9 fail on engine defects that faithful sub-runs expose.
- **The contract permits this closure.** The exit contract says "Task 02 may report reporting-correctness complete
  while full conformance remains BLOCKED, but that is not permission to declare this task's full objective passed or
  start final-release Task 03."
- **Owner decision, 2026-09-14:** stop expanding Task 2's scope, record the exact remaining gaps, and close.

## Findings

| Finding | Status | Evidence |
| --- | --- | --- |
| B4 (combined F-02) | Reporting-correct; full conformance BLOCKED | Obligation mapping r2 with pinned sub-runs; the closed report policy; the production entry; pinned known failures; the final report and independent reconciliation (below) |
| DOC-01 (combined F-04) | Fixed by an external, non-normative count erratum | `reports/LCL_Core_0.1.0_ERRATUM_2026-09-13.md` |
| HISTORY-01 (combined F-07) | Resolved by a dated retrospective index. The original reports remain absent | `reports/tasks/LCL-TASK-0011_TO_0016_RETROSPECTIVE_INDEX.md` |
| COVERAGE-01 | Scoped, hash-bound ledger delivered. Direct callers are not enumerated | `reports/implementation/LCL_REVIEW_COVERAGE_LEDGER.md`, `.tsv` |
| Reopened LCL-CLOSE-01 items | Q-READ closed under D1; STORE-ROLE-01 closed under amendment D2; the UI-03 test oracle corrected under A-T2-2 | `LCL_RESIDUAL_REPAIR_REPORT.md`, Phases 1a and 1b, and the Phase 1 closing gates |

## Final gates

| Gate | Toolchain | Actual exit | Result | Log SHA-256 |
| --- | --- | --- | --- | --- |
| T2-FINAL-workspace-tests | rustc 1.98.1 (48a229cea 2026-09-01) (Arch Linux rust 1:1.98.1-1) | 0 | Every workspace target: 144 targets, 1,520 passed, 0 failed, 1 ignored (the child helper of `manifest_input_bounds`, run by its parent tests) | `d54ecddce40aa3498328120b61a9f2cf3bccbeb5870753035f0a393a28239b3a` |
| T2-FINAL-msrv-workspace-tests | rustc 1.75.0 (82e1608df 2023-12-21) | 0 | The same 144 targets: 1,520 passed, 0 failed, 1 ignored | `f9f09aa4c738e5287739233f1918c03ff5df44bc9899888e64d645b57c570f14` |
| T2-FINAL-fmt | rustc 1.98.1 (48a229cea 2026-09-01) (Arch Linux rust 1:1.98.1-1) | 0 | `cargo fmt --all -- --check` | `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` |
| T2-FINAL-clippy | rustc 1.98.1 (48a229cea 2026-09-01) (Arch Linux rust 1:1.98.1-1) | 0 | `cargo clippy --workspace --all-targets -- -D warnings` | `578104a1cea4b93a4d68f8111cb63c2dc0254fe9967c24850cd7898ebbc9a822` |
| T2-FINAL-m8 | rustc 1.98.1 (48a229cea 2026-09-01) (Arch Linux rust 1:1.98.1-1) | 0 | The production report (text) | `6b3ee59d42a0425ca694ae5c514f0a494b617cc497ad6f108625bc79f0c1ae0e` |
| T2-FINAL-m8-json | rustc 1.98.1 (48a229cea 2026-09-01) (Arch Linux rust 1:1.98.1-1) | 0 | The production verdict (JSON) | `247dd091c5ff07d2578276052c392abd410422a754878a57d48ba309fe1d8c7e` |
| T2-FINAL-canonical-checksums | rustc 1.98.1 (48a229cea 2026-09-01) (Arch Linux rust 1:1.98.1-1) | 0 | `sha256sum -c SHA256SUMS.txt`: 175 matching | `7800f1b938e27b2c6b1f0d9bc295bbed7f3406ac5d42ef868c7037f3e1abb0ae` |
| T2-FINAL-brand-checksums | rustc 1.98.1 (48a229cea 2026-09-01) (Arch Linux rust 1:1.98.1-1) | 0 | `sha256sum -c assets/brand/BRAND_ASSETS.sha256`: 17 matching | `ec531a8e0a91cc08447e722a727175d34ed72c383bf8b0f487d9415e827ea7b7` |
| T2-FINAL-protected | rustc 1.98.1 (48a229cea 2026-09-01) (Arch Linux rust 1:1.98.1-1) | 0 | 192 of 192 protected files match (canonical 176, assets 4, releases 12) | `f741e089563249f51de32bbcec43faff59f2f860e8ff79671a83f7d203269500` |

The worktree content identity at the production report gate was `2755ddb2e1668829d74717a0d1f2a4ec19232376d9b7d818de959f9af9800e8d`. Only report
files changed after these gates, and no code reads `reports/`.

## The final production report

- **Source snapshot:** `impl-tree-sha256:04a8e853175d389398b3bf1a5d2c3b23cb33af688c52e39fee1c1daba376f5aa`, the SHA-256 of every tracked and untracked file under `impl/` (325 files),
  recorded at build time through `LCL_SOURCE_SNAPSHOT`.
- **Canonical package identity:** `00d648b162939d06c44838481a67c39bc12c64bdd6d105035c24150148fe67ed`.
- **Obligation mapping:** `impl/crates/lcl-conformance/src/obligations_v0.1.0_r2.json`, digest
  `386c14994f032a57143ef731ea7ac55db3dfffde52b54f0d38e39e7b5e126b20`. It holds 3,717 pinned semantic sub-runs.
- **Obligations and probes:** 980 required obligations, 2,413 required probes.
- **Source level:** 2,011 required, 2,011 satisfied.
- **Semantics level:** 402 required = 261 satisfied + 8 failed + 2 missing + 131 invalid.
- **Witnesses:** 66 of 66 established.
- **Claim:** `source_conforming`.
- **Independent reconciliation: PASS.** A Python check of the JSON verdict, sharing no code with the report:
  - the verdict's digest equals the mapping file's digest;
  - at each level the four state lists are disjoint, and their union is exactly that level's inventory;
  - `required` equals the total of the four lists;
  - `problems` names exactly the unsatisfied probes;
  - no problem carries an unexpected or duplicated sub-run.

## What LCL-CLOSE-02 changed

Every gate and the full rationale are in `reports/implementation/LCL_RESIDUAL_REPAIR_REPORT.md`, from "LCL-CLOSURE-4T
Task LCL-CLOSE-02 entry and Phase 0" to "LCL-CLOSE-02 final gates".

- **Phase 1:**
  - Q-READ, where `core.read` fails closed (D1);
  - STORE-ROLE-01, a storage profile for the engine's own stores (D2);
  - the UI-03 test oracle (A-T2-2).
- **Phase 2:** formatting of 10 inherited files.
- **Conformance instrument:**
  - run labels;
  - a closed report policy with exact sub-run membership;
  - handling of inputless evidence, unrequired records and run identity;
  - a JSON verdict.
- **P4a:** the populations moved into the library behind the single `production::report` entry, with production-path
  tests.
- **P4b:** mapping revision r2 pins the sub-runs of all 319 semantic contract rows, the loader checks the
  registry-derivable pins against the canonical registries, and r1 was removed.
- **P4c:** 161 new type, operator and function sub-runs, with oracles from canon and round trips guarded against
  sentinel false passes. Repairs made under amendment A-T2-3, each reproduced red first:
  - the conformance runner records the registered stage spelling;
  - `lcl-parser` requires one identifier in `REF(...)`;
  - `lcl-checker` reports unregistered unit identifiers as `error.reference.unresolved` (with a cited oracle
    correction), and MEASURE ordering across units as `error.numeric.unit_mismatch`;
  - `lcl-semantics` and `lcl-runtime` share one REGEX flag representation;
  - `lcl-semantics` constructs WORKSPACE-form PATH values and reports escapes.

## Engine defects left open: the 9 failing sub-runs

| Probe | Sub-run | Defect |
| --- | --- | --- |
| `semantic/operator_valid/==` and `semantic/operator_valid/!=` | `equality/path-address-form-identity` | A WORKSPACE-form PATH value is stored as its joined absolute spelling, so it equals an absolute PATH. Canon: "Different forms are unequal" |
| `semantic/operator_valid/MATCHES` | `match/glob-workspace-path` | The same representation keeps no WORKSPACE root, so a WORKSPACE PATH cannot match a relative GLOB |
| `semantic/operator_valid/-` | `constraint/negative-duration-result` | DURATION subtraction yields a negative DURATION instead of `error.value.out_of_range`. A runtime repair was reverted: an existing runtime test relies on a negative intermediate DURATION |
| `semantic/operator_invalid//` | `division/declared-bound` | FIELD MINIMUM and MAXIMUM are not enforced on object values |
| `semantic/operator_invalid//` | `division/host-capacity` | VERIFY assertion faults are dropped by `lcl-completion` unless it mirrors their identifier, so the check records UNKNOWN |
| `semantic/function_invalid/ROUND` | `round/host-capacity` | The same completion defect |
| `semantic/operator_invalid/MATCHES` | `pattern/resource-limit` | The same completion defect |
| `semantic/function_invalid/SUM` | `sum/unit-mismatch` | The same completion defect |

Two tests pin exactly these failures: `tests/production_report.rs` and `tests/semantic_cases.rs` in `lcl-conformance`.
A repair or a new failure fails the tests until the set is reviewed.

**Recorded but not repaired:**

- **DATA division.** The semantic layer does not fold division in declared DATA, so such a value reads UNKNOWN.
- **Textual escape check.** The runtime's WORKSPACE escape check is textual.
- **REGEX separator.** A NUL joins non-empty REGEX flags to the pattern, which is ambiguous for a pattern containing an
  escaped NUL.
- **Possible oracle question, for owner review.** The earlier oracle `match/glob-absolute-path-false` expects FALSE,
  while the GLOB input rule names `error.operator.operand` for an input without a WORKSPACE root.

## Remaining work for full conformance

- **750 sub-runs with no run:** error_contract 40, operation_binding 127, operation_effects 219, operation_errors 302,
  result_schemas 62. The operation families include the D3 fixture capabilities for `core.analyze`, `core.report`,
  `core.generate`, `core.convert`, `core.install` and `core.uninstall`.
- **54 pins with no population:** diagnostic_policy 24, failure_lifecycle 30.
- **The 9 failing sub-runs above,** and the engine repairs they need.

The appendix lists every remaining sub-run.

## Deliverables

| Contract deliverable | Location |
| --- | --- |
| Updated production conformance implementation and regression coverage | `impl/crates/lcl-conformance` and the engine repairs above |
| The pinned obligation, probe and sub-run map | `impl/crates/lcl-conformance/src/obligations_v0.1.0_r2.json` |
| The exact remaining list | The appendix of this report |
| The production report with input and tool identities | T2-FINAL-m8 and T2-FINAL-m8-json, with the source snapshot, mapping digest, package identity and toolchain above |
| The external count erratum | `reports/LCL_Core_0.1.0_ERRATUM_2026-09-13.md` |
| The historical cross-reference | `reports/tasks/LCL-TASK-0011_TO_0016_RETROSPECTIVE_INDEX.md` |
| The scoped coverage ledger | `reports/implementation/LCL_REVIEW_COVERAGE_LEDGER.md` and `.tsv` |
| Task 02 result and handoff | This report; `/mnt/F/.lcl-closure-4t-4c1cd4c659b7/LCL_CLOSE_02_HANDOFF.txt` |
| Protected-input verification | T2-FINAL-canonical-checksums, T2-FINAL-brand-checksums, T2-FINAL-protected |
| Correction of earlier claims | The dated note in `impl/README.md`; the status of `reports/implementation/LCL_CONFORMANCE_OBLIGATIONS.md` |

## Owned resources

- **Scratch** `/mnt/F/.lcl-closure-4t-4c1cd4c659b7` keeps the gate logs, tools and build caches. Its temporary
  directories were removed at close.
- **Build caches** that eleven Phase 3 gates wrote under `impl/target/debug` remain for the owner to decide on. That
  deviation is recorded in the residual report.

## Appendix: the exact remaining sub-runs

Generated from the final verdict, T2-FINAL-m8-json. Each line gives the probe, its state, the kind of gap and the
labels.

- `semantic/diagnostic_policy/core.error_selection` — missing, no population (24): `metadata/all-registered-errors`, `severity/single-error-value`, `stage/registered-order`, `stage/earliest-failing-stage-only`, `multiplicity/independent-diagnostics-emitted`, `supersession/same-cause-only`, `supersession/independent-occurrence-kept`, `duplicate/exact-key-suppressed`, `order/stage`, `order/locus`, `order/iteration`, `order/retry-attempt`, `order/severity`, `order/specificity`, `order/identifier`, `order/no-discovery-time-influence`, `primary/first-unhandled-after-recovery`, `secondary/unhandled-retained-in-order`, `evidence/handled-retained`, `evidence/successful-substitution-locals-retained`, `retry/retried-failure-stays-unhandled`, `primary/none-when-all-recovered`, `demand/eligible-value-domain-failure-resolved`, `demand/excluded-trigger-keeps-classification`
- `semantic/error_contract/error.determinism.mismatch` — invalid, no run (2): `behavior/deterministic-false-never-mismatches`, `behavior/deterministic-true-resolves-nondeterministic`
- `semantic/error_contract/error.numeric.division_by_zero` — invalid, no run (3): `behavior/measure-denominator`, `behavior/round-quotient`, `behavior/scalar-denominator`
- `semantic/error_contract/error.numeric.non_terminating` — invalid, no run (1): `behavior/quotient-outside-round`
- `semantic/error_contract/error.numeric.unit_mismatch` — invalid, no run (2): `behavior/exact-unit-mismatch-same-category`, `behavior/unit-outside-required-category`
- `semantic/error_contract/error.operation.parameter` — invalid, no run (11): `behavior/action-target-omitted`, `behavior/constraint-rejection-keeps-its-own-error`, `behavior/duplicate-named-parameter`, `behavior/fallback-operation-requires-named-parameter`, `behavior/fallback-target-omitted`, `behavior/handler-target-omitted`, `behavior/positional-argument`, `behavior/required-named-parameter-omitted`, `behavior/sort-comparator-forbidden`, `behavior/sort-stable-forbidden`, `behavior/unregistered-named-parameter`
- `semantic/error_contract/error.operation.precondition` — invalid, no run (10): `behavior/custom-operation-effect-outside-maximum`, `behavior/custom-operation-no-concrete-effect`, `behavior/determinism-incompatible-profile`, `behavior/false-precondition`, `behavior/missing-precondition`, `behavior/profile-ambiguous`, `behavior/profile-incomplete`, `behavior/profile-missing`, `behavior/profile-out-of-bounds`, `behavior/unknown-precondition`
- `semantic/error_contract/error.pattern.resource_limit` — invalid, no run (1): `behavior/pattern-resource-exhaustion`
- `semantic/error_contract/error.permission.denied` — invalid, no run (2): `behavior/prohibited-effect`, `behavior/unauthorized-access`
- `semantic/error_contract/error.reference.cycle` — invalid, no run (6): `behavior/action-cycle`, `behavior/before-graph-axis-resolution`, `behavior/phase-cycle`, `behavior/sequence-cycle`, `behavior/task-cycle`, `behavior/test-cycle`
- `semantic/error_contract/error.type.mismatch` — invalid, no run (2): `behavior/declared-type-incompatible`, `behavior/set-for-each-without-total-order`
- `semantic/failure_lifecycle/core.failure_lifecycle` — missing, no population (30): `phase/stage-defaults`, `phase/identifier-overrides`, `phase/measured-at-exposed-producer`, `evidence/child-and-attempt-local-phase-retained`, `phase/dependency-unsatisfied-pre-effect`, `phase/scope-violation-pre-effect`, `phase/execution-order-from-timing`, `phase/required-missing-from-timing`, `phase/mixed/cancelled`, `phase/mixed/evidence.missing`, `phase/mixed/execution.action`, `phase/mixed/host.constraint`, `phase/mixed/operation.postcondition`, `phase/mixed/operation.precondition`, `phase/mixed/permission.denied`, `phase/mixed/retry.exhausted`, `phase/mixed/success.unsatisfied`, `phase/mixed/value.unknown`, `phase/mixed/verification.failed`, `status/primary-default-after-demand-resolution`, `status/promotion-after-recovery`, `status/handler-result-controls-only-when-all-recovered`, `independence/status-effect-output`, `evidence/phase-effect-output-exact`, `indeterminate/fail-closed`, `retry/known-effects-require-safety-evidence`, `retry/safety-blocked-attempt-is-not-exhaustion`, `terminal/blocked-result-terminal`, `terminal/later-invocation-distinct`, `aggregate/child-failure-resolved-through-handler`
- `semantic/function_invalid/ROUND` — failed, failed (pinned engine defect) (1): `round/host-capacity`
- `semantic/function_invalid/SUM` — failed, failed (pinned engine defect) (1): `sum/unit-mismatch`
- `semantic/operation_binding/core.analyze` — invalid, no run (1): `fixture/success`
- `semantic/operation_binding/core.append` — invalid, no run (2): `binding/memory-target-rejected`, `binding/state-target-rejected`
- `semantic/operation_binding/core.ask` — invalid, no run (3): `binding/authoritative-responder`, `binding/authorized-in-scope-before-message`, `binding/options-compatible`
- `semantic/operation_binding/core.calculate` — invalid, no run (5): `binding/default/bindings`, `binding/exists-exception`, `binding/final-missing-rejected`, `binding/final-unknown-rejected`, `binding/skipped-operand-exception`
- `semantic/operation_binding/core.compare` — invalid, no run (3): `binding/default/criteria`, `binding/omitted-criteria-strict-equality`, `binding/supplied-criteria-exact`
- `semantic/operation_binding/core.continue` — invalid, no run (1): `binding/success-path`
- `semantic/operation_binding/core.convert` — invalid, no run (2): `binding/default/preserve`, `fixture/success`
- `semantic/operation_binding/core.copy` — invalid, no run (4): `binding/copy-profile-role`, `binding/default/overwrite`, `binding/destination-absent-unless-overwrite`, `binding/overwrite-true-replaces`
- `semantic/operation_binding/core.create` — invalid, no run (6): `binding/default/fail_if_exists`, `binding/fail-if-exists-false-reconciles`, `binding/fail-if-exists-true-requires-absent`, `binding/memory-target-rejected`, `binding/state-target-rejected`, `binding/target-profile-role`
- `semantic/operation_binding/core.delete` — invalid, no run (8): `binding/default/recursive`, `binding/default/require_exists`, `binding/delete-profile-role`, `binding/memory-target-rejected`, `binding/recursive-policy`, `binding/require-exists-false-permits-absent`, `binding/require-exists-true-rejects-absent`, `binding/state-target-rejected`
- `semantic/operation_binding/core.download` — invalid, no run (5): `binding/checksum`, `binding/default/overwrite`, `binding/destination-absent-unless-overwrite`, `binding/source-profile-role`, `binding/transfer-profile-role`
- `semantic/operation_binding/core.execute` — invalid, no run (6): `binding/default/arguments`, `binding/default/environment`, `binding/executable-target`, `binding/execution-profile-selection`, `binding/graph-target`, `binding/only-registered-parameters`
- `semantic/operation_binding/core.filter` — invalid, no run (2): `binding/predicate-reference-contract`, `binding/set-input-rejected`
- `semantic/operation_binding/core.generate` — invalid, no run (7): `binding/default/variation`, `binding/format`, `binding/generation-profile-role`, `binding/memory-target-rejected`, `binding/state-target-rejected`, `binding/variation-bounds`, `fixture/success`
- `semantic/operation_binding/core.group` — invalid, no run (2): `binding/key-reference-contract`, `binding/set-input-rejected`
- `semantic/operation_binding/core.inspect` — invalid, no run (2): `binding/default/depth`, `binding/required-existing-target`
- `semantic/operation_binding/core.install` — invalid, no run (1): `fixture/success`
- `semantic/operation_binding/core.memory_write` — invalid, no run (4): `binding/default/merge`, `binding/merge-false-type-match`, `binding/merge-true-merged-object-type-match`, `binding/storage-profile-role`
- `semantic/operation_binding/core.modify` — invalid, no run (5): `binding/change-profile-role`, `binding/expected-before-guard`, `binding/memory-target-rejected`, `binding/selection`, `binding/state-target-rejected`
- `semantic/operation_binding/core.move` — invalid, no run (7): `binding/default/overwrite`, `binding/destination-absent-unless-overwrite`, `binding/distinct-addresses`, `binding/memory-source-rejected`, `binding/move-profile-role`, `binding/overwrite-never-permits-same-address`, `binding/state-source-rejected`
- `semantic/operation_binding/core.publish` — invalid, no run (4): `binding/default/replace`, `binding/destination-absent-unless-replace`, `binding/publication-profile-role`, `binding/visibility`
- `semantic/operation_binding/core.rename` — invalid, no run (6): `binding/default/overwrite`, `binding/destination-absent-unless-overwrite`, `binding/memory-target-rejected`, `binding/new-name-differs`, `binding/rename-profile-role`, `binding/state-target-rejected`
- `semantic/operation_binding/core.report` — invalid, no run (3): `binding/default/format`, `binding/default/include_evidence`, `fixture/success`
- `semantic/operation_binding/core.retry` — invalid, no run (1): `binding/success-path`
- `semantic/operation_binding/core.select` — invalid, no run (1): `binding/predicate-reference-contract`
- `semantic/operation_binding/core.sort` — invalid, no run (6): `binding/comparator-rejected`, `binding/default/direction`, `binding/key-operation-reference-contract`, `binding/list-target`, `binding/set-target`, `binding/stable-rejected`
- `semantic/operation_binding/core.start` — invalid, no run (2): `binding/default/arguments`, `binding/default/environment`
- `semantic/operation_binding/core.stop` — invalid, no run (1): `binding/default/force`
- `semantic/operation_binding/core.test` — invalid, no run (7): `binding/assertion-form`, `binding/expected-actual-form`, `binding/expected-target-form`, `binding/graph-target-executes-before-comparison`, `binding/target-alone-rejected`, `binding/target-with-actual-rejected`, `binding/target-with-assertion-rejected`
- `semantic/operation_binding/core.uninstall` — invalid, no run (2): `binding/default/purge_data`, `fixture/success`
- `semantic/operation_binding/core.upload` — invalid, no run (4): `binding/checksum`, `binding/default/overwrite`, `binding/destination-absent-unless-overwrite`, `binding/transfer-profile-role`
- `semantic/operation_binding/core.validate` — invalid, no run (3): `binding/default/rules`, `binding/rules-reference-validate-declaration`, `binding/schema-reference-object-type`
- `semantic/operation_binding/core.verify` — invalid, no run (6): `binding/assertion-reference-invokes-nothing`, `binding/boolean-expression-assertion`, `binding/default/evidence`, `binding/evidence-references`, `binding/reference-boolean-assertion`, `binding/verification-profile-role`
- `semantic/operation_binding/core.write` — invalid, no run (5): `binding/create-if-missing-policy`, `binding/default/create_if_missing`, `binding/memory-target-rejected`, `binding/state-target-rejected`, `binding/write-profile-role`
- `semantic/operation_effects/core.analyze` — invalid, no run (4): `axes/no-effects`, `determinism/base-nondeterministic`, `determinism/verified-deterministic-profile`, `resolution/exact-invocation-axes`
- `semantic/operation_effects/core.append` — invalid, no run (7): `address/output-state`, `address/path-host-filesystem`, `address/uri-network-network`, `postcondition/0`, `precondition/0`, `precondition/1`, `resolution/exact-invocation-axes`
- `semantic/operation_effects/core.ask` — invalid, no run (4): `answer/compatible-with-expected-type`, `answer/equals-listed-option`, `answer/no-valid-answer-missing`, `resolution/exact-invocation-axes`
- `semantic/operation_effects/core.calculate` — invalid, no run (3): `postcondition/0`, `precondition/0`, `resolution/exact-invocation-axes`
- `semantic/operation_effects/core.cancel` — invalid, no run (4): `resolution/exact-invocation-axes`, `transition/allowed-cancel`, `transition/disallowed-cancel`, `transition/reason-recorded`
- `semantic/operation_effects/core.compare` — invalid, no run (5): `criteria/non-equality-missing`, `criteria/omitted-strict-equality`, `criteria/supplied-registered-rules`, `criteria/unknown-result-value-unknown`, `resolution/exact-invocation-axes`
- `semantic/operation_effects/core.continue` — invalid, no run (3): `postcondition/0`, `precondition/0`, `resolution/exact-invocation-axes`
- `semantic/operation_effects/core.convert` — invalid, no run (4): `postcondition/0`, `postcondition/1`, `precondition/0`, `resolution/exact-invocation-axes`
- `semantic/operation_effects/core.copy` — invalid, no run (5): `address/path-destination-filesystem`, `address/path-side-host`, `address/source-observation-dependency-only`, `address/uri-side-network`, `resolution/exact-invocation-axes`
- `semantic/operation_effects/core.create` — invalid, no run (9): `address/output-state`, `address/path-host-filesystem`, `address/uri-network-network`, `postcondition/0`, `postcondition/1`, `precondition/0`, `precondition/1`, `precondition/2`, `resolution/exact-invocation-axes`
- `semantic/operation_effects/core.delete` — invalid, no run (9): `address/output-state`, `address/path-host-filesystem`, `address/uri-network-network`, `postcondition/0`, `postcondition/1`, `precondition/0`, `precondition/1`, `precondition/2`, `resolution/exact-invocation-axes`
- `semantic/operation_effects/core.download` — invalid, no run (5): `address/destination-path-host-filesystem`, `address/local-source-host`, `address/remote-source-network`, `determinism/source-identity-and-profiles`, `resolution/exact-invocation-axes`
- `semantic/operation_effects/core.execute` — invalid, no run (15): `mode/graph-category-copied`, `mode/graph-cycle-rejected-before-axes`, `mode/graph-no-local-process-effect`, `mode/graph-transitive-axes`, `mode/non-graph-axis-union`, `mode/non-graph-process-effect`, `mode/non-graph-profile-category`, `resolution/exact-invocation-axes`, `result/completed-exit-code`, `result/failure-to-start`, `result/graph-no-native-observations`, `result/graph-value-single-primary`, `result/independent-phase-effect-output`, `result/nonzero-exit-completed`, `result/started-streams`
- `semantic/operation_effects/core.filter` — invalid, no run (4): `axes/declared-state-only`, `axes/predicate-adding-axis-rejected`, `resolution/exact-invocation-axes`, `result/all-true-members-in-source-order`
- `semantic/operation_effects/core.generate` — invalid, no run (8): `address/output-state`, `address/path-host-filesystem`, `address/uri-network-network`, `postcondition/0`, `postcondition/1`, `precondition/0`, `precondition/1`, `resolution/exact-invocation-axes`
- `semantic/operation_effects/core.group` — invalid, no run (6): `axes/declared-state-only`, `axes/key-adding-axis-rejected`, `resolution/exact-invocation-axes`, `result/groups-by-first-key`, `result/partition-exactly-once`, `result/source-order-within-group`
- `semantic/operation_effects/core.inspect` — invalid, no run (3): `postcondition/0`, `precondition/0`, `resolution/exact-invocation-axes`
- `semantic/operation_effects/core.install` — invalid, no run (3): `postcondition/0`, `precondition/0`, `resolution/exact-invocation-axes`
- `semantic/operation_effects/core.memory_write` — invalid, no run (5): `merge/current-only-fields-preserved`, `merge/false-replaces`, `merge/nested-objects-not-merged`, `merge/true-shallow-right-biased`, `resolution/exact-invocation-axes`
- `semantic/operation_effects/core.modify` — invalid, no run (8): `address/output-state`, `address/path-host-filesystem`, `address/uri-network-network`, `postcondition/0`, `precondition/0`, `precondition/1`, `precondition/2`, `resolution/exact-invocation-axes`
- `semantic/operation_effects/core.move` — invalid, no run (8): `address/independent-source-and-destination`, `address/memory-source-prohibited`, `address/output-source-state`, `address/path-host-filesystem`, `address/profile-strategy`, `address/state-source-prohibited`, `address/uri-network-network`, `resolution/exact-invocation-axes`
- `semantic/operation_effects/core.publish` — invalid, no run (5): `address/path-destination`, `address/source-dependencies-only`, `address/uri-destination`, `determinism/profile-final-category`, `resolution/exact-invocation-axes`
- `semantic/operation_effects/core.read` — invalid, no run (4): `postcondition/0`, `postcondition/1`, `precondition/0`, `resolution/exact-invocation-axes`
- `semantic/operation_effects/core.rename` — invalid, no run (11): `address/output-state`, `address/path-host-filesystem`, `address/uri-network-network`, `postcondition/0`, `postcondition/1`, `precondition/0`, `precondition/1`, `precondition/2`, `precondition/3`, `precondition/4`, `resolution/exact-invocation-axes`
- `semantic/operation_effects/core.report` — invalid, no run (4): `axes/no-effects`, `determinism/base-nondeterministic`, `determinism/verified-deterministic-profile`, `resolution/exact-invocation-axes`
- `semantic/operation_effects/core.retry` — invalid, no run (10): `postcondition/0`, `postcondition/1`, `postcondition/2`, `postcondition/3`, `precondition/0`, `precondition/1`, `precondition/2`, `precondition/3`, `precondition/4`, `resolution/exact-invocation-axes`
- `semantic/operation_effects/core.return` — invalid, no run (3): `postcondition/0`, `precondition/0`, `resolution/exact-invocation-axes`
- `semantic/operation_effects/core.select` — invalid, no run (6): `axes/declared-state-only`, `axes/predicate-adding-axis-rejected`, `resolution/exact-invocation-axes`, `result/cardinality-bounds`, `result/no-repeated-occurrence`, `result/true-members-only`
- `semantic/operation_effects/core.send` — invalid, no run (3): `postcondition/0`, `precondition/0`, `resolution/exact-invocation-axes`
- `semantic/operation_effects/core.sort` — invalid, no run (8): `axes/declared-state-only`, `determinism/key-operation`, `determinism/ordered-type-rules`, `determinism/string-key-projection`, `resolution/exact-invocation-axes`, `result/distinct-keys-for-set-members`, `result/equal-keys-keep-source-position`, `result/list-output`
- `semantic/operation_effects/core.start` — invalid, no run (3): `postcondition/0`, `precondition/0`, `resolution/exact-invocation-axes`
- `semantic/operation_effects/core.state_update` — invalid, no run (5): `postcondition/0`, `precondition/0`, `precondition/1`, `precondition/2`, `resolution/exact-invocation-axes`
- `semantic/operation_effects/core.stop` — invalid, no run (5): `postcondition/0`, `postcondition/1`, `precondition/0`, `precondition/1`, `resolution/exact-invocation-axes`
- `semantic/operation_effects/core.test` — invalid, no run (7): `mode/comparison-only-deterministic`, `mode/graph-category-copied`, `mode/graph-cycle-rejected-before-axes`, `mode/graph-executes-before-comparison`, `mode/strict-equality-comparison`, `mode/transitive-axes-normalized`, `resolution/exact-invocation-axes`
- `semantic/operation_effects/core.uninstall` — invalid, no run (4): `purge/false-preserves-data`, `purge/installation-absent`, `purge/true-removes-data`, `resolution/exact-invocation-axes`
- `semantic/operation_effects/core.upload` — invalid, no run (4): `address/path-destination-host-filesystem`, `address/path-source-host-without-filesystem-effect`, `address/uri-destination-network`, `resolution/exact-invocation-axes`
- `semantic/operation_effects/core.validate` — invalid, no run (3): `postcondition/0`, `precondition/0`, `resolution/exact-invocation-axes`
- `semantic/operation_effects/core.verify` — invalid, no run (5): `assertion/evaluated-against-snapshot`, `axes/bounded-observation-dependencies`, `axes/no-effects`, `determinism/profile-final-category`, `resolution/exact-invocation-axes`
- `semantic/operation_effects/core.write` — invalid, no run (7): `address/output-state`, `address/path-host-filesystem`, `address/uri-network-network`, `postcondition/0`, `precondition/0`, `precondition/1`, `resolution/exact-invocation-axes`
- `semantic/operation_errors/core.analyze` — invalid, no run (7): `error/host.constraint`, `error/permission.denied`, `error/scope.violation`, `precondition/profile-ambiguous`, `precondition/profile-incomplete`, `precondition/profile-missing`, `precondition/profile-out-of-bounds`
- `semantic/operation_errors/core.append` — invalid, no run (6): `error/execution.action`, `error/host.constraint`, `error/operation.postcondition`, `error/permission.denied`, `error/scope.violation`, `forbidden-state-target`
- `semantic/operation_errors/core.ask` — invalid, no run (5): `path/host-constraint`, `path/missing-authoritative-answer`, `path/option-incompatible-with-expected-type`, `path/out-of-scope-responder-or-request`, `path/unauthorized-request`
- `semantic/operation_errors/core.calculate` — invalid, no run (12): `error/reference.kind`, `path/declared-bound`, `path/demanded-missing-operand`, `path/demanded-unknown-operand`, `path/division-by-zero`, `path/division-operand`, `path/host-capacity`, `path/non-terminating-quotient`, `path/required-unknown-result`, `path/unit-mismatch`, `path/unresolved-expression-reference`, `unresolved-target`
- `semantic/operation_errors/core.cancel` — invalid, no run (3): `path/cancellation`, `path/insufficient-authority`, `path/status-disallows-cancel`
- `semantic/operation_errors/core.compare` — invalid, no run (9): `path/host-constraint`, `path/matches-resource-limit`, `path/non-equality-missing`, `path/propagated-unknown`, `path/scope-violation`, `path/type-mismatch`, `path/unauthorized-access`, `path/unsupported-operator-operands`, `precondition/inaccessible-operand`
- `semantic/operation_errors/core.continue` — invalid, no run (3): `error/operation.precondition`, `path/unhandled-event-or-absent-continuation`, `positional-earliest-stage`
- `semantic/operation_errors/core.convert` — invalid, no run (6): `error/execution.action`, `error/host.constraint`, `error/operation.postcondition`, `error/operation.precondition`, `error/permission.denied`, `error/scope.violation`
- `semantic/operation_errors/core.copy` — invalid, no run (6): `error/execution.action`, `error/host.constraint`, `error/operation.postcondition`, `error/operation.precondition`, `error/permission.denied`, `error/scope.violation`
- `semantic/operation_errors/core.create` — invalid, no run (6): `error/execution.action`, `error/host.constraint`, `error/operation.postcondition`, `error/permission.denied`, `error/scope.violation`, `forbidden-state-target`
- `semantic/operation_errors/core.delete` — invalid, no run (7): `error/execution.action`, `error/host.constraint`, `error/operation.postcondition`, `error/permission.denied`, `error/scope.violation`, `forbidden-state-target`, `positional-earliest-stage`
- `semantic/operation_errors/core.download` — invalid, no run (13): `error/execution.action`, `error/host.constraint`, `error/operation.postcondition`, `error/permission.denied`, `error/scope.violation`, `precondition/source-profile-ambiguous`, `precondition/source-profile-incomplete`, `precondition/source-profile-missing`, `precondition/source-profile-out-of-bounds`, `precondition/transfer-profile-ambiguous`, `precondition/transfer-profile-incomplete`, `precondition/transfer-profile-missing`, `precondition/transfer-profile-out-of-bounds`
- `semantic/operation_errors/core.execute` — invalid, no run (13): `error/execution.action`, `error/host.constraint`, `error/operation.postcondition`, `error/permission.denied`, `error/scope.violation`, `path/graph-error-union`, `path/graph-retry-exhausted-only-through-union`, `path/reference-cycle`, `positional-earliest-stage`, `precondition/profile-ambiguous`, `precondition/profile-incomplete`, `precondition/profile-missing`, `precondition/profile-out-of-bounds`
- `semantic/operation_errors/core.filter` — invalid, no run (8): `error/operator.operand`, `error/reference.kind`, `path/missing-predicate`, `path/referenced-predicate-error-union`, `path/set-target-type-mismatch`, `path/unknown-predicate`, `path/unresolved-predicate-reference`, `precondition/predicate-contract`
- `semantic/operation_errors/core.generate` — invalid, no run (6): `error/execution.action`, `error/host.constraint`, `error/operation.postcondition`, `error/permission.denied`, `error/scope.violation`, `forbidden-state-target`
- `semantic/operation_errors/core.group` — invalid, no run (8): `error/operator.operand`, `error/reference.kind`, `path/missing-key`, `path/referenced-key-operation-error-union`, `path/set-target-type-mismatch`, `path/unknown-key`, `path/unresolved-key-reference`, `precondition/key-contract`
- `semantic/operation_errors/core.inspect` — invalid, no run (5): `path/host-constraint`, `path/scope-violation`, `path/unauthorized-access`, `positional-earliest-stage`, `precondition/absent-target`
- `semantic/operation_errors/core.install` — invalid, no run (7): `error/execution.action`, `error/host.constraint`, `error/operation.postcondition`, `error/operation.precondition`, `error/permission.denied`, `error/scope.violation`, `positional-earliest-stage`
- `semantic/operation_errors/core.memory_write` — invalid, no run (8): `error/execution.action`, `error/host.constraint`, `error/operation.postcondition`, `error/permission.denied`, `error/scope.violation`, `precondition/merge-current-not-object`, `precondition/merge-new-not-object`, `precondition/merged-object-type-mismatch`
- `semantic/operation_errors/core.modify` — invalid, no run (6): `error/execution.action`, `error/host.constraint`, `error/operation.postcondition`, `error/permission.denied`, `error/scope.violation`, `forbidden-state-target`
- `semantic/operation_errors/core.move` — invalid, no run (7): `error/execution.action`, `error/host.constraint`, `error/operation.postcondition`, `error/permission.denied`, `error/scope.violation`, `forbidden-state-target`, `precondition/same-address`
- `semantic/operation_errors/core.publish` — invalid, no run (9): `error/execution.action`, `error/host.constraint`, `error/operation.postcondition`, `error/permission.denied`, `error/scope.violation`, `precondition/profile-ambiguous`, `precondition/profile-incomplete`, `precondition/profile-missing`, `precondition/profile-out-of-bounds`
- `semantic/operation_errors/core.read` — invalid, no run (7): `error/value.out_of_range`, `path/host-constraint`, `path/scope-violation`, `path/unauthorized-access`, `positional-earliest-stage`, `precondition/absent-target`, `precondition/unreadable-target`
- `semantic/operation_errors/core.rename` — invalid, no run (9): `error/execution.action`, `error/host.constraint`, `error/operation.postcondition`, `error/permission.denied`, `error/scope.violation`, `forbidden-state-target`, `precondition/disallowed-existing-destination`, `precondition/illegal-new-name`, `precondition/same-name`
- `semantic/operation_errors/core.report` — invalid, no run (8): `error/host.constraint`, `error/permission.denied`, `error/scope.violation`, `positional-earliest-stage`, `precondition/profile-ambiguous`, `precondition/profile-incomplete`, `precondition/profile-missing`, `precondition/profile-out-of-bounds`
- `semantic/operation_errors/core.retry` — invalid, no run (15): `error/execution.action`, `error/reference.cycle`, `path/action-resolution-before-inheritance`, `path/attempt-evidence-in-order`, `path/exhausted-after-all-attempts`, `path/false-when-preserves-failure`, `path/missing-safety-proof`, `path/missing-when-condition`, `path/safety-blocked-attempt-is-not-exhaustion`, `path/unknown-safety-proof`, `path/unknown-when-condition`, `path/wrapped-action-error-union`, `precondition/limit-unequal-retry-limit`, `precondition/no-retry-block`, `precondition/proved-unsafe-repetition`
- `semantic/operation_errors/core.return` — invalid, no run (3): `path/resolved-missing`, `path/resolved-unknown`, `positional-earliest-stage`
- `semantic/operation_errors/core.select` — invalid, no run (6): `error/operation.precondition`, `error/operator.operand`, `error/reference.kind`, `path/missing-predicate`, `path/referenced-predicate-error-union`, `path/unknown-predicate`
- `semantic/operation_errors/core.send` — invalid, no run (6): `error/execution.action`, `error/host.constraint`, `error/operation.postcondition`, `error/operation.precondition`, `error/permission.denied`, `error/scope.violation`
- `semantic/operation_errors/core.sort` — invalid, no run (19): `path/comparator-unregistered`, `path/direction-outside-enum`, `path/equal-keys-for-distinct-set-members`, `path/incompatible-key-results`, `path/missing-key`, `path/omitted-natural-order`, `path/referenced-key-operation-error-union`, `path/stable-unregistered`, `path/unknown-key`, `path/unresolved-key-reference`, `path/wrong-kind-key-reference`, `positional-earliest-stage`, `precondition/incompatible-key-operation-signature`, `precondition/invalid-key-operation-axes`, `precondition/key-operation-profile-ambiguous`, `precondition/key-operation-profile-incomplete`, `precondition/key-operation-profile-missing`, `precondition/key-operation-profile-out-of-bounds`, `precondition/malformed-property-path`
- `semantic/operation_errors/core.start` — invalid, no run (7): `error/execution.action`, `error/host.constraint`, `error/operation.postcondition`, `error/operation.precondition`, `error/permission.denied`, `error/scope.violation`, `positional-earliest-stage`
- `semantic/operation_errors/core.state_update` — invalid, no run (6): `error/execution.action`, `error/host.constraint`, `error/operation.postcondition`, `error/operation.precondition`, `error/permission.denied`, `error/scope.violation`
- `semantic/operation_errors/core.stop` — invalid, no run (8): `error/execution.action`, `error/execution.order`, `error/host.constraint`, `error/operation.postcondition`, `error/operation.precondition`, `error/permission.denied`, `error/scope.violation`, `positional-earliest-stage`
- `semantic/operation_errors/core.test` — invalid, no run (9): `error/block.conditional_requirement`, `path/false-comparison-is-not-an-error`, `path/graph-error-union`, `path/host-constraint`, `path/incompatible-equality-operands`, `path/invalid-comparison-shape`, `path/prohibited-graph-cycle`, `path/unresolved-typed-value-reference`, `unresolved-target`
- `semantic/operation_errors/core.uninstall` — invalid, no run (7): `error/execution.action`, `error/host.constraint`, `error/operation.postcondition`, `error/operation.precondition`, `error/permission.denied`, `error/scope.violation`, `positional-earliest-stage`
- `semantic/operation_errors/core.upload` — invalid, no run (6): `error/execution.action`, `error/host.constraint`, `error/operation.postcondition`, `error/operation.precondition`, `error/permission.denied`, `error/scope.violation`
- `semantic/operation_errors/core.validate` — invalid, no run (10): `error/determinism.mismatch`, `error/host.constraint`, `error/permission.denied`, `error/scope.violation`, `error/validation.failed`, `path/unresolved-rule-reference`, `path/unresolved-schema-reference`, `path/wrong-kind-rule-reference`, `path/wrong-kind-schema-reference`, `positional-earliest-stage`
- `semantic/operation_errors/core.verify` — invalid, no run (10): `error/evidence.missing`, `error/host.constraint`, `error/permission.denied`, `error/scope.violation`, `error/verification.failed`, `path/unresolved-assertion-reference`, `precondition/profile-ambiguous`, `precondition/profile-incomplete`, `precondition/profile-missing`, `precondition/profile-out-of-bounds`
- `semantic/operation_errors/core.write` — invalid, no run (6): `error/execution.action`, `error/host.constraint`, `error/operation.postcondition`, `error/permission.denied`, `error/scope.violation`, `forbidden-state-target`
- `semantic/operator_invalid//` — failed, failed (pinned engine defect) (2): `division/declared-bound`, `division/host-capacity`
- `semantic/operator_invalid/MATCHES` — failed, failed (pinned engine defect) (1): `pattern/resource-limit`
- `semantic/operator_valid/!=` — failed, failed (pinned engine defect) (1): `equality/path-address-form-identity`
- `semantic/operator_valid/-` — failed, failed (pinned engine defect) (1): `constraint/negative-duration-result`
- `semantic/operator_valid/==` — failed, failed (pinned engine defect) (1): `equality/path-address-form-identity`
- `semantic/operator_valid/MATCHES` — failed, failed (pinned engine defect) (1): `match/glob-workspace-path`
- `semantic/result_schemas/result.collection` — invalid, no run (6): `engine/closed-record`, `engine/empty-list-count-zero-binds`, `engine/items-and-count-on-success`, `engine/items-default-projection`, `engine/partial-output-unsupported`, `engine/unknown-rejected`
- `semantic/result_schemas/result.command` — invalid, no run (10): `engine/closed-record`, `engine/completed-exit-code`, `engine/failure-to-start-record`, `engine/graph-no-native-fields`, `engine/graph-output-requires-explicit-selection`, `engine/nonzero-exit-completed`, `engine/partial-only-streams`, `engine/started-streams-present`, `engine/stdout-default-projection`, `engine/unknown-rejected`
- `semantic/result_schemas/result.message` — invalid, no run (6): `engine/closed-record`, `engine/delivered-default-projection`, `engine/null-message-id-means-unassigned`, `engine/partial-output-unsupported`, `engine/success-with-false-or-unknown`, `engine/unknown-never-binds`
- `semantic/result_schemas/result.operation` — invalid, no run (6): `engine/changed-default-projection`, `engine/changed-kept-after-failure-unbound`, `engine/closed-record`, `engine/partial-output-unsupported`, `engine/unknown-changed-never-binds`, `engine/value-only-when-exposed`
- `semantic/result_schemas/result.test` — invalid, no run (8): `engine/assertion-form-without-comparison`, `engine/closed-record`, `engine/comparison-form-with-both`, `engine/partial-output-unsupported`, `engine/passed-default-projection`, `engine/success-with-false-or-unknown`, `engine/tested-null-is-material`, `engine/unknown-never-binds`
- `semantic/result_schemas/result.transfer` — invalid, no run (8): `engine/bytes-absent-before-transfer`, `engine/bytes-default-projection`, `engine/bytes-zero-valid`, `engine/closed-record`, `engine/interrupted-count-retained-without-output`, `engine/partial-output-unsupported`, `engine/unknown-never-binds`, `engine/value-only-when-content-supplied`
- `semantic/result_schemas/result.validation` — invalid, no run (6): `engine/closed-record`, `engine/domain-errors-distinct-from-execution-errors`, `engine/partial-output-unsupported`, `engine/success-with-valid-false`, `engine/unknown-rejected`, `engine/valid-default-projection`
- `semantic/result_schemas/result.value` — invalid, no run (6): `engine/closed-record`, `engine/falsy-material-values-bind`, `engine/partial-output-unsupported`, `engine/unknown-rejected`, `engine/value-default-projection`, `engine/value-required-on-success`
- `semantic/result_schemas/result.verification` — invalid, no run (6): `engine/closed-record`, `engine/partial-output-unsupported`, `engine/success-with-false-or-unknown`, `engine/unknown-never-binds`, `engine/verified-and-observed-after-run`, `engine/verified-default-projection`

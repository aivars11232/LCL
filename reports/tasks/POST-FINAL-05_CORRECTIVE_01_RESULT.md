# POST-FINAL-05 corrective task — result (pre-build, pre-commit)

## Identity

- Entry HEAD: `878187d2cd95d644462bcb30c8035a59a1624392` ("LCL final pretest task 5"), clean, 1,042 tracked files.
- Exit HEAD: **unchanged** — no Git write of any kind occurred.
- Exit worktree: 24 modified, 2 deleted, 4 untracked (listed under *Files changed*).
- Governing pack: `/mnt/F/LCL_Final_PreTesting_Blocker_Closure_Pack_v3_825694f/`; this task is the corrective pass its FINAL-05 verdict called for, not a new pack.
- Core 0.1 identity: `00d648b162939d06c44838481a67c39bc12c64bdd6d105035c24150148fe67ed` (176 files) — **unchanged**, canonical tree untouched.
- Core 0.2 identity: `00daee8de1919c4945ef04ff65edb22164bd8046a493be08a87d5fa3b4c3e604` (216 files) — **unchanged**; no canonical file was edited, so no regeneration, no anchor change.
- F10 decision A (atomic UTF-8 decode) and its original-byte behaviour are untouched.
- Scratch: `/mnt/F/.lcl-pretest/c1/`. Logs: `/mnt/F/.lcl-pretest/logs/C1-*`.

## Status

**Repairs complete and verified; 4 of the original 79 sub-runs remain, each an obligation this build cannot construct without an owner decision.** No candidate was built: §6 of the instruction fixes the order — source repairs and this report first, the owner's commit next, then a candidate built from that frozen revision.

| Measure | Entry (`878187d`) | Exit |
|---|---:|---:|
| source obligations | 2,011 / 2,011 | **2,011 / 2,011** |
| semantic obligations satisfied | 357 / 402 | **398 / 402** |
| semantic failed / missing / invalid | 0 / 0 / 45 | **0 / 0 / 4** |
| pinned sub-runs with no run | 79 | **4** |
| claim | `source_conforming` | `source_conforming` |
| mapping digest | `9b32a28b79d9c3872cb3f810365a3275583ce5731a74c98a8d2013d07633a8ad` | unchanged |
| workspace tests | 1,695 passed | **1,715 passed, 0 failed** |

The claim is still `source_conforming` because a claim is all-or-nothing: 4 invalid obligations keep it there. The readiness gate added under §8 says exactly that, and refuses.

## 1. Disposition of the 79-sub-run baseline

Every row below was recomputed from the live report at entry (`/mnt/F/.lcl-pretest/c1/gaps.tsv`, 79 sub-runs across 45 probes) and re-measured at exit (`gaps5.tsv` → 4).

| Group | Sub-runs | Class | Disposition |
|---|---:|---|---|
| **G1** scope enforcement | 31 | (a) missing implementation | **Closed.** Step 6 now resolves an action's effective scope against its target. |
| **G2** graph execution | 22 | (a) missing implementation | **Closed.** `core.execute`/`core.test` execute a referenced unit on the one executor; cycles are refused before axis resolution. |
| **G4** host-backed observation | 5 | (a) missing implementation | **Closed.** An addressable operand of `core.compare`/`core.validate`/`core.verify` is observed through the host that owns it. |
| **G5** store failure behaviour | 6 | (a) + (b) | **Closed.** The selected storage profile decides how a store is performed; an externally backed one crosses the boundary. |
| **G6** determinism mismatch | 3 | (a) missing implementation | **Closed.** `core.validate` verifies a referenced `kind.operation`'s `DETERMINISTIC` assertion. |
| **G7** key-operation profiles | 4 | (d) apparent conflict, resolved | **Closed.** The four profile-fault words apply to the declared key contract; see §2. |
| **Group A** | 5 | mixed | 1 closed (`core.retry error/reference.cycle`), **4 open** — see §3. |
| **FINAL-02 additions** | 2 | (c)/(d) | 1 closed (`core.calculate path/unresolved-expression-reference`), 1 open (`core.execute precondition/profile-out-of-bounds`). |
| **dependent rows** | 2 | (a) | **Closed** with their capabilities. |

Group totals differ by one or two from the instruction's labels because the live register assigns `binding/authorized-in-scope-before-message` to G1's scope work and `core.test path/host-constraint` to G2's graph work; the sub-run identities are exactly the 79.

## 2. Repairs, with their canonical basis

Each was reproduced first, repaired at one root cause, and pinned by a focused regression that fails without the repair.

### G1 — effective scope against the target (31 sub-runs)

- **Canon.** `05_SEMANTICS/02`: "SCOPE is computed as INCLUDE minus EXCLUDE. Exact references/paths identify one entity … EXCLUDE wins within the same SCOPE." `statuses_and_errors_v0.1.0.json`: `error.scope.violation` is "An action targets an entity outside applicable SCOPE", **pre_effect only**, "effective scope resolves at processing step 6 before the first authorized effect". `02_LEXICAL/06`: SCOPE declares "the exact set of entities to which a clause may apply".
- **Reproduction (red).** `data_resolution.rs::an_action_targeting_an_entity_its_scope_excludes_is_a_scope_violation` and `::exclude_wins_over_include_within_the_same_scope` — both `left: []`, no diagnostic.
- **Root cause.** `order.rs::authorize_actions` recorded the action's `SCOPE` but never compared it with the target; and it read the field with `field_text`, which returns `None` for the `REF(...)` form every scope reference uses, so the decision carried no scope at all.
- **Repair.** `lcl-semantics/src/scope.rs:140` `selector_of` is now `pub(crate)` and also normalizes the WORKSPACE path form (new `Selector::Expression`, `authority.rs`); `scope.rs:182` `admits` implements INCLUDE minus EXCLUDE; `scope.rs:201` `applicable` honours `SCOPE.OPERATION`; `order.rs:735` refuses the action before authorizing it.
- **Boundary decided by canon, not by convenience.** An enclosing TASK `SCOPE` is *not* applied to every action under it: canonical valid example `04_AUTOMATED_CODING_TASK` declares TASK `SCOPE: REF(scope.source)` (one source file) while `action.test` under it targets `PATH("/usr/bin/python3")`. Widening the rule would reject a canonical valid example, so the applicable scope is the one the ACTION itself names.
- **Stated limitation.** A `GLOB`/`REGEX` selector never decides a refusal here: resolving its finite set means enumerating a workspace, which this pre-effect layer explicitly does not do. An unresolved pattern admits, and never excludes.
- **Green.** 4 semantics tests; 29 `error/scope.violation` rows (the exact set of registry rows admitting it — derived from `contract.admits_error`, not a hand-kept list), plus `path/scope-violation` ×3, `path/out-of-scope-responder-or-request`, `binding/authorized-in-scope-before-message`, `phase/scope-violation-pre-effect`.

### G2 — graph execution (22 sub-runs)

- **Canon.** `operations_v0.1.0.json#/contracts/core.execute`: a referenced TASK/PHASE/SEQUENCE/ACTION/TEST "has no mandatory local process effect and resolves the final determinism category and normalized transitive dependency and effect unions of its reachable graph"; `result.command` in graph mode "never synthesizes started, completed, exit_code, stdout, or stderr and records value exactly when the completed graph exposes one material primary result"; `graph_resolution` requires a prohibited reference cycle to "emit error.reference.cycle and fail **before axis resolution**".
- **Reproduction.** A graph-target `core.execute` crossed to a host and reported `error.host.constraint: no process capability is installed`; a self-referential pair was accepted.
- **Repair.** One executor, not two: `operations.rs` adds `Resolution::Graph(String)` and `GraphOutcome`/`GraphInvocation`; `execute.rs:1554` `execute_graph` enters the referenced node on the engine's own queue under an iteration path of its own, then `execute.rs:1220` invokes the row again with what it did; `data.rs:163` `graph_mode` and `data.rs:181` `graph_result` build `result.command`; `control.rs:693` makes `core.test` run its graph first and carry its effects. Cycles are refused in preflight (`order.rs:71,88`), where the identifier's mirrors already live.
- **Why the cycle check is not in the runtime.** `error.reference.cycle`'s reviewed mirrors are `registry-contract`, `resolver`, `preflight`, `completion` — no `runtime`. Adding a runtime mirror would have added an unexpected component sub-run to its own error-contract probe. Step 9 is also literally "before axis resolution".
- **Green.** `data_operations.rs::core_execute_in_graph_mode_runs_the_referenced_unit`, `ordering_graph.rs::a_delegating_target_that_leads_back_to_itself_is_a_reference_cycle` (execute/test/retry, each with a non-cyclic control), and 22 conformance sub-runs.

### G4 — host-backed observation (5 sub-runs)

- **Canon.** `core.compare` "Resolve[s] host or network independently for the target and against operands; material-value-only comparison resolves declared_state_only", precondition "both operands are accessible"; `core.validate` "only for addressable targets"; `core.verify` "from the target and evidence sources".
- **Repair.** `control.rs:125` `observed_by_host` resolves the observation axes of a PATH/URI/host-bound operand and crosses; MEMORY, STATE and OUTPUT stay engine-internal.
- **Discriminator that shaped the rule.** `MATCHES` does **not** cross: its own profile rule consumes a workspace-relative path as text, which PRETEST-02 F07 pins (`matches_consumes_a_workspace_path_by_its_relative_segments`). Making every criterion cross broke exactly those two pinned tests; the exception restored them and is quoted in the code.
- **Note for the owner.** This is a behaviour change for documents that compare two addressable operands with a non-`MATCHES` criterion: they now resolve a host dependency instead of comparing addresses locally. It follows the row's own invocation resolution, and it is recorded here because it is visible.
- **Green.** `control_operations.rs::an_addressable_compare_operand_is_observed_through_the_host` (crossing, host limitation, MATCHES control, material-value control) and 5 conformance sub-runs.

### G5 — store failure behaviour (6 sub-runs)

- **Canon.** Both store rows declare `possible_dependencies: ["host"]` and "Resolve the authorized MEMORY/STATE storage profile"; their postconditions speak of "the persistent value".
- **Determination.** Production behaviour *and* test support were both missing, and the profile decides which: the shipped storage profile declares no dependency and states "Write the declared engine-owned store in place within one invocation", so it cannot be limited, fail, or miss its postcondition. A profile that declares the row's `host` dependency is the other store the row's maxima describe.
- **Repair.** `data.rs:773` — when the selected storage profile declares a dependency, the write crosses the boundary carrying the resolved value; `execute.rs:1364` `apply_store_write` updates the engine's own view **only** once the profile reports the effect applied, so a refused or failed store leaves the previous value in place.
- **Test support.** `clauses.rs::external_store_profiles` is conformance-only, in the same sense as the D3 fixture profiles: it fabricates no observation, it changes which registered kind of storage the engine is asked to perform. The three failures are then the *existing* generic helpers (`host_constraint`, `execution_action`, `postcondition`), unchanged.
- **Green.** `data_operations.rs::an_externally_backed_storage_profile_writes_through_the_host`, including the refusal case that must leave the store unwritten.

### G6 — determinism mismatch (3 sub-runs)

- **Canon.** `05_SEMANTICS/11`: "Validation emits error.determinism.mismatch exactly when DETERMINISTIC TRUE is declared and that resolved contract is nondeterministic"; `03_TYPES_AND_VALUES/05`: TRUE is valid "only when its declared axes and parameters admit no permitted variation"; `06_STANDARD_LIBRARY/06`: "emitted only during contract validation".
- **Which axis admits variation, and why that is not invented.** The registry defines `model` as "Obtains inference or generation from the selected LC or model capability" and `human` as "Obtains an authoritative response or decision from a human". Every core row declaring either is registered *nondeterministic* (`core.analyze`, `core.generate`, `core.report`, `core.ask`); no row registered deterministic declares one; and `host`/`network` are not such axes — `core.read` declares both and is deterministic over "the exact requested content … from the resolved target snapshot".
- **Where.** `control.rs:488-522`, inside `core.validate`, whose meaning is "Check syntax, type, reference, **dependency**, and constraints" and whose postcondition is that "all detected failures use registered error identifiers". A preflight refusal was implemented first and **reverted**: it would have rejected the document before any `core.validate` could run, making an identifier the row's own closed list admits unreachable.
- **Green.** `control_operations.rs::validate_reports_a_determinism_mismatch_as_a_detected_finding` — model and human findings, the `DETERMINISTIC FALSE` control, and the `host, network` control that must *not* be a finding.

### G7 — key-operation profiles (4 sub-runs): the apparent conflict, resolved

- **The two clauses.** `core.sort`'s `error.operation.precondition` trigger names "a missing, ambiguous, incomplete, or out-of-bounds immutable profile" for the key operation, while `axis_contract.custom_operation_resolution` says a custom `kind.operation` "selects no implementation profile".
- **Authority order.** Both are the same registry, and neither is subordinate; so they are read together rather than one over the other. The four words are the registry's closed vocabulary for a profile-role selection fault, and the key constraint states exactly which properties stand in for a profile here: "exactly one PARAMETER accepting T, and exactly one RESULT of a concrete registered ordered type", plus an installed implementation.
- **Mapping.** missing → no installed implementation (already the engine's behaviour); ambiguous → more than one PARAMETER; incomplete → no RESULT; out of bounds → a RESULT outside `operators_and_functions#/ordered_types`. Each is `error.operation.precondition`, the only identifier the row admits.
- **Repair.** `pure.rs:668` `incomplete_contract`, branching by parameter so `core.filter`/`core.select` keep their "exactly one BOOLEAN RESULT" rule (a first version refused their predicates and was caught by their own probes).
- **Green.** `pure_operations.rs::a_key_operation_contract_must_be_complete_unambiguous_and_ordered` — all four faults and the working control.

### Singles closed

- `core.retry error/reference.cycle` — `06_STANDARD_LIBRARY/10`: "a prohibited wrapped-ACTION reference cycle uses error.reference.cycle". The same preflight check, with `core.retry` added to the delegating rows.
- `core.calculate path/unresolved-expression-reference` — `expression_fragment_contract/environment`: "REF references resolve in the enclosing document … an unknown binding name produces error.reference.unresolved". The identifier's reviewed mirrors are `registry-contract` and `resolver` only, so the run exercises the reference the resolver owns: the `bindings` OBJECT the fragment's environment is built from. **Residual:** a bare name *inside* the fragment string is not resolved statically by this build; naming it at execution would mirror a resolution-stage identifier in a layer that does not own it. A fragment-resolution pass in the resolver would close it; it is not attempted here.
- `result.verification engine/unknown-never-binds` — "UNKNOWN verified never binds OUTPUT". Now constructible because an addressable `core.verify` target is observed through the host (G4), and a host that cannot establish the assertion answers UNKNOWN through the production boundary.

## 3. The 4 remaining obligations — owner decision requested

None is closed, none is reclassified, and none is hidden: each is listed `invalid` in the live report and refused by the readiness gate. For each I give the competing clauses and a precise impossibility argument. **I am asking for one decision, not proposing one.**

| Sub-run | Why this build cannot construct it | What I recommend deciding |
|---|---|---|
| `core.execute precondition/profile-out-of-bounds` | Mechanically proven: `core.execute`'s maxima are `{host, network, model, human}` and `{filesystem, network, process, package, message, memory, state}`, which is **the entire closed vocabulary** of both axes (`axis_contract.dependency_definitions` minus `declared_state_only`, `effect_definitions` minus `none`). A profile's axes "may narrow the row's but never widen it", so no profile can declare axes outside them. Declaring an exclusive sentinel beside a concrete class is malformed or incomplete, which are different faults with their own sub-runs. | Either an approved mapping correction that drops this one sub-run for `core.execute` (the identity of the row's other profile faults is unaffected), or a canon clarification naming a trigger. |
| `core.group error/operator.operand` | Group keys "are compared by the registered strict == equality"; `==` admits `equality_compatible` = "Any two material values or permitted singleton sentinels". Every value this engine can produce is material or MISSING/UNKNOWN, and the row already routes MISSING to `error.required.missing` and UNKNOWN to `error.value.unknown` under its own invocation resolution. A malformed or unregistered property path is `error.operation.precondition` by the row's own constraint. Nothing is left outside the operand domain. | Same shape of decision: an approved mapping correction, or canon naming the operand case. |
| `core.test path/incompatible-equality-operands` | Identical domain argument; additionally the checker refuses a non-material `actual` before execution, and `06_STANDARD_LIBRARY/03` scopes the identifier to "operands outside its equality_compatible domain". | Same. |
| `result.test engine/unknown-never-binds` | `passed` is UNKNOWN only if the comparison resolves UNKNOWN. A declared UNKNOWN is refused at static checking ("UNKNOWN cannot bind a required INTEGER/BOOLEAN destination" — reproduced); `==`/`!=` over sentinels are FALSE by `09_MISSING_UNKNOWN_NULL`; other operators over MISSING raise `error.required.missing`; and `core.test`'s target type is `REFERENCE[TASK\|ACTION]\|meta.material_value`, with no addressable form, so — unlike `core.verify` — it never crosses a boundary that could supply an UNKNOWN. | Either approve extending `core.test`'s target to admit a host-observed actual (a canon change), or an approved mapping correction. |

Until one of these is decided, the claim cannot reach `semantics_conforming`, and `TESTING_READY` cannot be issued. That is stated plainly rather than worked around.

## 4. F5-N1 and F5-N2 — verified, removed, prevented

- **Verified against the current files.** `archive-EWXwoU/gk_3.1.75_linux_amd64.zip` was a tracked 9,406,024-byte ZIP holding one member, `gk` (24,043,668 bytes, the GitKraken CLI), added in `5ca774e`, referenced by nothing in the tree. `.directory` was a tracked 84-byte KDE folder-settings file whose `Icon=` line names a path on one person's computer. Both were inside the candidate's `SOURCE_INVENTORY.tsv` and its source archive, because the release process records every tracked file outside `releases/`.
- **Removed from the working tree only**, left unstaged for the owner's commit. The archive was never executed; no unrelated directory was touched; nothing was uninstalled.
- **Prevention, narrowly scoped.** New `.gitignore` with exactly three desktop-metadata patterns (`.directory`, `.DS_Store`, `Thumbs.db`); verified that it ignores a *new* `reports/.directory` and that **no tracked file is ignored by it**.
- **Packaging guard.** `packaging/build_release.sh:321,325` refuses a source set containing desktop metadata, or a binary archive (`.zip`, `.tar.gz`, `.tgz`, `.7z`, `.rar`) **outside `releases/`** — the published archives under `releases/` are explicitly exempt, so no legitimate release artifact is affected.
- **Regressions.** `release_build.rs::desktop_metadata_and_stray_archives_are_refused_before_the_source_is_recorded` (four cases, driven through the real script) and `::a_published_release_archive_is_not_treated_as_stray` (the control).
- **History is not rewritten.** Removing these files does not remove their historical copies. The three earlier candidates and the published `releases/` archives still contain them; they are identified here as contaminated and are **not** repackaged under their old identities. `releases/candidates/lcl-0.2.0-linux-x86_64-68529c1420ba` is superseded by anything built after this commit, for the same reason.

## 5. F5-N3 — provenance, and the order of the next steps

F5-N3 is an identity question, so nothing was relabelled. The four identities are kept apart:

| Identity | Value |
|---|---|
| build-source commit of the existing candidate | `4dead55f15d537ae5952c054e3d6ab8acb86432c` |
| its source inventory digest (source id) | `68529c1420babf24d85d857c280a18ff683444b515462c903dfd6437b1844b17` |
| its artifact digest | `bfbec817053fcbd952314e89dd4179a61d320fcf885bb4a74295506214099272` |
| the later report/artifact-recording commits | `9ddaadb` (candidate + FINAL-04 report), `878187d` (FINAL-05 report + ledger) |

No newer SHA was substituted into any provenance. The existing candidate stays exactly what it is: built from `4dead55`, and now also **superseded** — this task changes engine source, so a candidate built from it will differ.

**The sequence from here, which is why this report stops where it does.** Source repairs are finished and this is the pre-build report. The owner commits (§7 below). I then resume in the same task, inspect the resulting clean tree, freeze that revision, build the candidate from it, and audit the candidate against that same frozen source before any newly generated artifact is committed. Building now would produce a candidate whose provenance names a commit that does not exist yet — the exact defect F5-N3 records.

## 6. Owner-only items, unchanged

- **B3 / F29 — Core 0.2 independent review.** `validate_release.py --scope all` over `canonical/LCL_Core_0.2.0` still reports 31 PASS, 0 FAIL, 2 OUT_OF_SCOPE, **1 BLOCKED**: `language_decisions_and_release_state`, `pending_decisions: ["independent_review"]`, `release_gate_permitted: false`, `UNRELEASED_CANDIDATE`. No acceptance was recorded, and none is implied by this task's repairs. Core 0.1 remains `release_ready: true`.
- **F10** decision A and its original-byte diagnostics are untouched; no canonical byte changed, so no metadata, validation evidence, identity reference or trust anchor needed regeneration.
- **F30** (real KDE desktop-menu and file-association acceptance) and **F31** (branch protection) remain separately documented owner acceptance items; nothing here converts them into technical checks.

## 7. Files changed

No canonical file, no release artifact and no historical report was modified.

**Production source (9)** — `lcl-semantics/src/{scope.rs, order.rs, authority.rs}`, `lcl-runtime/src/{operations.rs, execute.rs}`, `lcl-stdlib/src/{control.rs, data.rs, pure.rs, params.rs, schema.rs}`.
**Conformance evidence (6)** — `lcl-conformance/src/{lib.rs, operation_cases.rs, operation_cases/clauses.rs, lifecycle_cases.rs, result_cases/engine.rs, semantic_cases/behaviors.rs}`.
**Tests (7)** — `lcl-semantics/tests/{data_resolution.rs, ordering_graph.rs}`, `lcl-stdlib/tests/{control_operations.rs, data_operations.rs, pure_operations.rs, coverage.rs}`, `lcl-hardening/tests/release_build.rs`.
**Packaging (1)** — `packaging/build_release.sh`.
**New (4)** — `.gitignore`, `lcl-conformance/src/acceptance.rs`, `lcl-conformance/examples/m8_conformance_gate.rs`, `lcl-conformance/tests/acceptance.rs`.
**Deleted (2)** — `.directory`, `archive-EWXwoU/gk_3.1.75_linux_amd64.zip`.
**This report** — `reports/tasks/POST-FINAL-05_CORRECTIVE_01_RESULT.md`.

## 8. The readiness assertion

`m8_conformance_report` exits 0 whenever it can assemble a report, including on a `source_conforming` verdict — which a pipeline would read as readiness. The gate added here reads that same rendered verdict and fails closed.

- `lcl-conformance/src/acceptance.rs:39` `accept(verdict, mapping_digest, package_identity)` refuses: a verdict that is absent, malformed or of an unrecognized format; a claim below `semantics_conforming`; any failed, missing or invalid obligation (each named); any obligation counted in two states; a level whose states do not account for its required count; a mapping digest that is not the reviewed inventory's; a package identity that is not the approved anchor's.
- `examples/m8_conformance_gate.rs` runs the production report and exits 0 accepted / 1 refused / 2 report-unavailable. Nothing is hardcoded and the production report is not replaced.
- `tests/acceptance.rs` — 8 tests over deliberately nonconforming verdicts, including a `source_conforming` verdict of the exact shape the exit-zero command produces, and one test that drives the gate over the **real** production verdict and asserts its answer matches that verdict's own claim.
- Run today: **REFUSED** — "the claim is `source_conforming`, and readiness requires `semantics_conforming`" plus the 4 invalid obligations by name. That is the correct answer at this HEAD.

The 2,011 source / 402 semantic baseline is unchanged; no obligation was added, removed or reclassified.

## 9. Gates

Isolated: `TMPDIR=/tmp/lcl-corrective`, out-of-tree `CARGO_TARGET_DIR`. Script `/mnt/F/.lcl-pretest/c1/c1-gate.sh` (adapted from FINAL-05's). Logs `/mnt/F/.lcl-pretest/logs/C1-*`.

| Command | Exit | Result |
|---|---:|---|
| `cargo fmt --all -- --check` | 0 | clean |
| `cargo clippy --offline --locked --workspace --all-targets -- -D warnings` | 0 | clean (two findings of mine repaired) |
| `cargo test --offline --locked --workspace --all-targets --no-fail-fast` | 0 | 164 blocks, **1,715 passed, 0 failed, 1 ignored** |
| MSRV 1.75.0 `cargo check --workspace --all-targets` | 0 | clean |
| MSRV 1.75.0 `cargo test --workspace --all-targets --no-fail-fast` | 0 | 164 blocks, **1,715 passed, 0 failed, 1 ignored** |
| `real_process` stress (12 sequential + 3 × 6 concurrent) | 0 | **30 of 30**, 0 failures |
| Core 0.1 `sha256sum -c --strict --quiet` / `validate_release.py --scope all` | 0 / 0 | verifies; `release_ready: true` |
| Core 0.2 `sha256sum -c --strict --quiet` | 0 | verifies |
| Core 0.2 `validate_release.py --scope all` | 1 | 31 PASS, 0 FAIL, 2 OUT_OF_SCOPE, 1 BLOCKED (`independent_review`) — unchanged owner item |
| `validate_language_contracts.py` | 0 | 514 checks, 0 violations |
| `validate_localization.py` / `validate_source_fixtures.py` | 0 / 0 | `passed: true` |
| Core 0.1 / Core 0.2 identity | 0 / 0 | `00d648b1…` 176 files / `00daee8d…` 216 files |
| `sha256sum -c assets/brand/BRAND_ASSETS.sha256` | 0 | verifies |
| `m8_conformance_report` | 0 | source 2,011/2,011; semantic 398 satisfied, 0 failed, 0 missing, 4 invalid |
| **`m8_conformance_gate`** | **1** | **REFUSED**, with the claim and the 4 obligations named |
| protected areas + candidate checksums | 0 | no change under `canonical`, `releases`, `assets`; every candidate `.sha256` verifies |
| `impl/target/test-tmp`, `/tmp/lcl-apps` | — | 0 entries written |

**Not run, and why.** The candidate build and its installed smoke: §6 puts them after the owner's commit, against the frozen revision. The every-file ledger and the independent read-only audit: FINAL-05's contract requires a separate reviewer, and this task is the repair pass — my own verification is not that review, and no agent was spawned to imitate one.

## 10. Residual risks and limitations

1. **4 obligations open** (§3) — the claim cannot reach `semantics_conforming` until one decision is made.
2. **Behaviour changes visible to existing documents.** Comparing two addressable operands with a non-`MATCHES` criterion now resolves a host dependency; an action whose declared `SCOPE` excludes its target is now refused at step 6; a `core.execute`/`core.test` graph target now runs on the engine instead of failing at a host. Each follows the row's own canon, and each is pinned by a regression.
3. **GLOB/REGEX scope selectors** never decide a refusal at preflight (stated limitation, §2).
4. **Bare names inside expression fragments** are not resolved statically (§2, singles).
5. **Superseded candidates.** Every existing candidate predates these repairs and contains the two stray files; none was repackaged.
6. **No independent audit** has been run over this tree.

## 11. AI quota and reuse

Reused rather than rebuilt: FINAL-05's gate script, the FEATURE-04 acceptance harnesses, PRETEST-03's `identity.py`, the existing generic clause helpers (`host_constraint`, `execution_action`, `postcondition`, `failed_without_effects`, `on_files`, `on_mock`, `with_declarations`), the D3 fixture-profile pattern, and the runtime's own executor and retry machinery. New production code is 9 files and under ~450 lines in total; no second evaluator, executor or report was written; every new test extends an existing file except the acceptance suite, which is the one new capability.

## Proposed commit message

```
LCL post-FINAL-05 corrective: close 75 of 79 semantic obligations

Semantic conformance goes from 357/402 satisfied with 45 invalid probes to
398/402 with 4, and the 79 pinned sub-runs with no run become 4. Source
conformance stays 2,011/2,011 and the claim stays source_conforming,
because 4 obligations remain.

Engine repairs, each reproduced red first and pinned by a regression:
step 6 now resolves an action's effective SCOPE against its target and
refuses error.scope.violation pre-effect (31 sub-runs); core.execute and
core.test execute a referenced unit as a graph on the one executor, with
result.command graph mode, transitive effects and preflight refusal of a
prohibited reference cycle, which core.retry's wrapped ACTION now shares
(22); core.compare, core.validate and core.verify observe an addressable
operand through the host that owns it, with MATCHES staying local as
PRETEST-02 F07 pins (5); a storage profile that declares the row's host
dependency makes a store write cross the boundary, and the engine's view
follows the profile (6); core.validate verifies a referenced
kind.operation's DETERMINISTIC assertion (3); and a key operation's
declared contract is checked for the four properties core.sort's key
constraint names (4).

FINAL-05's two hygiene findings are fixed: the tracked GitKraken CLI
archive and the .directory file are removed from the working tree, a
narrow .gitignore prevents recurrence, and build_release.sh refuses
desktop metadata or a binary archive outside releases/ in a source set,
with regressions through the real script.

A readiness gate is added: m8_conformance_gate reads the production
verdict and fails closed on an absent, malformed, source_conforming or
incompletely accounted one. It refuses today, naming the 4 obligations.

fmt, clippy -D warnings, the workspace (1,715 passed) on 1.98.1 and on
MSRV 1.75.0, 30 real_process runs, both packages' checksums, the four 0.2
validators, brand assets and protected areas are green. No canonical byte
changed: Core 0.1 stays 00d648b1..., Core 0.2 stays 00daee8d..., and F10
decision A is untouched. No candidate was built: it belongs to the commit
after this one.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
```

## No Git writes

No `add`, `commit`, `amend`, `merge`, `rebase`, `reset`, `checkout`, `stash`, `clean`, `tag`, `push`, `pull`, `fetch` or configuration change was performed. HEAD is `878187d2cd95d644462bcb30c8035a59a1624392`, exactly as at entry. Every change above is in the working tree, unstaged, for the owner to review and commit.

## Next action after the commit

Resume this same task: inspect the clean tree, record the new HEAD as the frozen build source, build the candidate from it with the existing offline release process, verify its provenance and inventory against that frozen revision, run the installed-candidate smoke, and re-run the readiness gate. The 4 open obligations still gate `TESTING_READY`.

# LCL-TASK-0020 IMPLEMENTATION RESULT

**Status: COMPLETE**

M11 — full-product hardening, staged application validation, packaging and
release closure.

## Repository state

| Fact | Value |
| --- | --- |
| Root | `/mnt/F/LCL` |
| Branch | `main` |
| Baseline HEAD (task start) | `093d64c8980e55ce7edbf4cf5fd5a6e24b43acd3` ("LCL task 19") |
| HEAD at this report | `093d64c8980e55ce7edbf4cf5fd5a6e24b43acd3`, unchanged |
| Upstream | `origin/main`, same commit |
| Working tree | 28 modified files and 7 new paths, uncommitted; nothing staged, committed, tagged or pushed |

## Predecessor evidence

`LCL-TASK-0019` was verified closed from actual repository state before
planning, not inferred from numbering. Its result report records
**Status: COMPLETE**, and its work is committed as `093d64c`. Its gates were
re-run independently at the start of this task:

| Gate | Result |
| --- | --- |
| `cargo test --offline --workspace --all-targets` | 120 suites, **1235 passed, 0 failed, 0 ignored** — exactly what task 19 reported |
| `cargo fmt --all -- --check` | clean |
| `cargo clippy --offline --workspace --all-targets -- -D warnings` | clean |
| `validate_release.py --scope all` | 31 PASS / 0 FAIL / 0 BLOCKED / 2 OUT_OF_SCOPE, `release_ready: true` |
| `sha256sum -c SHA256SUMS.txt` | exit 0, 175 files |

The working tree was clean; the six-file M1 dirty state the pack expects was
committed long ago.

## Owner decisions obtained before any implementation

Stage A put five questions to the owner. All five were answered before a file
was touched.

1. **The five open engine defects are repaired in this task**, rather than
   waived or deferred, because the zero-defect acceptance criterion cannot
   otherwise pass.
2. **Packaging is user-level**: a tarball and an install script that need no
   elevation.
3. **I author the application ladder**, small through large, plus the
   decomposed variant.
4. **The release candidate is version `0.1.0`**, stated as distinct from the
   language version.
5. **The checker's expression walk is made iterative** to close the stack
   overflow phase B found, rather than declaring a stack bound or shipping the
   crash. This question was asked mid-task, when the crash was found, with the
   measurements in hand.

## Internal phases

Seven, in order, each gate green before the next began. Two are additions to
the approved plan, both authorized by the owner decisions above.

| Phase | Subject | Gate |
| --- | --- | --- |
| A | Release baseline and threat model | green |
| A prime | Defect closure, the five registered defects | green |
| B | Fuzz, property and adversarial robustness | green |
| B prime | The stack overflow phase B found | green |
| C | Performance, repeatability and security | green |
| D | Staged application validation | green |
| E | Packaging, install and recovery | green |
| F | Final release gate and evidence | green |

## Files changed, in change order

**Phase A**

1. `impl/Cargo.toml` — the product version, pinned to `0.1.0`
2. `reports/implementation/LCL_RELEASE_BASELINE.md` — candidate identity,
   platform and capability scope, threat model, every threshold later phases
   assert, and the known-defect register

**Phase A prime — defect 1, object-valued declarations**

3. `impl/crates/lcl-semantics/src/eval.rs` — read an indented `VALUE` body as an object
4. `impl/crates/lcl-semantics/src/data.rs` — use it in data resolution
5. `impl/crates/lcl-semantics/tests/data_resolution.rs` — three regression tests
6. `impl/crates/lcl-conformance/tests/witness_cases/mod.rs` — CLOSURE-005 and CLOSURE-042 promoted to executable, CLOSURE-021's reason corrected

**Phase A prime — defect 2, a discarded demand fault**

7. `impl/crates/lcl-completion/src/diagnostic.rs` — `error.operator.operand` as a reused identifier, and the resolved stage beside the registered one
8. `impl/crates/lcl-completion/src/engine.rs` — demand resolution at emission
9. `impl/crates/lcl-completion/src/{check.rs,evidence.rs,success.rs,terminal.rs}` — surface the fault a check's own `ASSERT` raises
10. `impl/crates/lcl-completion/tests/verify_and_test.rs` — two regression tests
11. `witness_cases/mod.rs` — CLOSURE-015 promoted

**Phase A prime — defect 3, the unchecked expression fragment**

12. `impl/crates/lcl-checker/src/contracts.rs` — the lexicon and grammar, so the checker reads a fragment with the same parser M2 exposes
13. `impl/crates/lcl-checker/src/operation.rs` — check a written fragment
14. `impl/crates/lcl-checker/tests/constructors_and_operations.rs` — regression tests
15. `impl/crates/lcl-stdlib/tests/pure_operations.rs` — two expectations corrected, with the authority
16. `witness_cases/mod.rs` — CLOSURE-019 promoted

**Phase A prime — defect 4, the ignored range**

17. `impl/crates/lcl-runtime/src/capability.rs` — a contract refusal that carries the identifier the row lists
18. `impl/crates/lcl-runtime/src/execute.rs` — object-valued parameters, and the refusal's diagnostic
19. `impl/crates/lcl-runtime/src/handler.rs`, `impl/crates/lcl-workspace/src/execution.rs` — the new outcome
20. `impl/crates/lcl-stdlib/src/host.rs` — the whole range contract
21. `impl/crates/lcl-stdlib/tests/external_operations.rs` — five regression tests
22. `witness_cases/mod.rs` — CLOSURE-048, 049 and 050 promoted

**Phase A prime — defect 5, the unchecked parameter family**

23. `impl/crates/lcl-checker/src/operation.rs` — judge a declared scalar family against the row
24. `impl/crates/lcl-checker/src/contracts.rs` — render a contract type as the registry spells it
25. `impl/crates/lcl-checker/tests/constructors_and_operations.rs` — regression test
26. `witness_cases/mod.rs` — CLOSURE-051 and 052 promoted

**Phase B — the hardening crate**

27. `impl/Cargo.toml`, `impl/crates/lcl-hardening/Cargo.toml`
28. `impl/crates/lcl-hardening/src/{lib.rs,rng.rs,corpus.rs,invariant.rs}`
29. `impl/crates/lcl-hardening/tests/{fuzz_stages.rs,adversarial_inputs.rs,protocol_and_tooling.rs}`

**Phase B prime — the stack overflow**

30. `impl/crates/lcl-checker/src/expr.rs` — the expression walk, iterative
31. `impl/crates/lcl-checker/src/lib.rs` — the driver's own invariant, asserted in every test build
32. `impl/crates/lcl-parser/src/syntax.rs` — `Expr`'s `Clone`, iterative
33. `impl/crates/lcl-checker/src/declarations.rs` — read statements where they are, instead of copying a subtree per level

**Phase C**

34. `impl/crates/lcl-hardening/tests/{performance.rs,repeatability.rs,security_matrix.rs}`

**Phase D**

35. `apps/small-invoice-total/`, `apps/medium-release-notes/`,
    `apps/large-release-pipeline/`, `apps/large-release-pipeline-decomposed/`
36. `impl/crates/lcl-hardening/tests/applications.rs`

**Phase E**

37. `packaging/{build_release.sh,install.sh,uninstall.sh,lcl.desktop,README.md}`
38. `impl/crates/lcl-hardening/tests/packaging_smoke.rs`

**Phase F**

39. `reports/implementation/LCL_RELEASE_REPORT.md`
40. `impl/README.md` — the M11 section and the crates table
41. `releases/lcl-0.1.0-linux-x86_64.tar.gz`, its checksum and its provenance
42. this report

No file under `canonical/` was modified. No canonical repair was required, and
no canonical contradiction was found.

## Behavior implemented

**Five defects closed, at the stage each belongs to.** An object written as an
indented `VALUE` block now resolves, in declarations and in operation
parameters. A fault raised by a check's own assertion is now emitted with its
registered identifier and the execution stage the demand map resolves, instead
of being flattened to UNKNOWN. A malformed expression fragment is refused where
the contract says it is checked, statically, rather than reaching execution and
being reported under a substituted identifier. `core.read` applies its range
contract. A declared parameter family the row does not register is refused.

**A crash from untrusted source, closed.** Every nesting shape ended the process
by `SIGABRT` between 2,000 and 16,000 levels. The static checker's expression
walk now judges descendants through an explicit worklist, and the syntax tree's
`Clone` is iterative like the `Drop` beside it. Every shape now survives 50,000
levels, the depth M2 proved for the parser.

**A hardening crate that drives the product, not a mock of it.** Every generated
case is a pure function of a seed, so a failure is reproducible from the number
the failure message prints and no corpus is stored. Every report is checked for
span containment, registry closure, stage monotonicity and the absence of a
false success claim.

**Four applications, run as a user runs them**: the real binary, an empty
environment, a working directory outside the repository, and exactly the
capabilities each document declares.

**A release that installs somewhere clean.** A tarball, a checksum, a provenance
record naming the source commit, the toolchain, the package identity and the
checksum of every payload file, and an installer that needs no elevation and is
reversed exactly by its uninstaller.

## Canonical sources implemented

- `03_TYPES_AND_VALUES/10`, OBJECT — "An object uses an indented VALUE block
  containing unique lowercase property names. Property order has no semantic
  effect", which is defect 1 and the shape of both object readers
- `03_TYPES_AND_VALUES/03` — OBJECT equality by field, which is why the reader
  keys rather than orders, and what CLOSURE-042 now executes
- `statuses_and_errors_v0.1.0.json#/diagnostic_selection/expression_demand_resolution`
  — its context covers a demand "during a reachable invocation, condition,
  verification, or completion step", its eligible map gives
  `error.operator.operand` to an empty reduction, and its `resolved_stage` is
  `execution`: defect 2 entire
- `#/exclusion_rule` — "No source structure, token, name resolution ... defect
  qualifies", which is why a malformed fragment could not be demand-resolved and
  had to be checked statically
- `operations_v0.1.0.json#/expression_fragment_contract` — "consume exactly one
  EXPRESSION ... followed only by optional whitespace", "Malformed fragment
  syntax and invalid bindings produce error.operation.parameter", and "Static
  checks cover the complete fragment": defect 3
- `#/contracts/core.read/parameters/range` — the closed unit set, the
  representation each indexes, line terminators retained, and
  "0 <= start <= end <= sequence length; otherwise error.value.out_of_range":
  defect 4
- `#/contracts/core.append/parameters/content` — "BYTES is a count and is not
  content", and `#/contracts/core.validate/parameters/schema` — "Material OBJECT
  schema encodings are not admitted": defect 5
- `06_STANDARD_LIBRARY/10` — what `error.operation.parameter` covers at an
  invocation site, and what is not remapped to it
- `04_GRAMMAR/10` and `04_GRAMMAR/02` — the shape of nesting and the absence of
  any bound on it, which made a depth limit unavailable as a repair
- `04_GRAMMAR/12` — "VALUE contains an inline expression or an indented
  OBJECT-data body"
- `05_SEMANTICS/02` — "Ambient current directory and implied nearby files do not
  exist in portable LCL", which the packaging follows by shipping the
  specification package rather than searching for one
- `block_schemas_v0.1.0.json#/schemas/FORBID` and `#/schemas/ALLOW` — the
  prohibition a host grant does not open
- `07_VERSIONING_AND_EXTENSIONS/05` — no ignore-unknown mode, which the manifest
  fuzzing asserts

## Verification

| Gate | Result |
| --- | --- |
| `cargo test --offline --workspace --all-targets` | 129 suites, **1294 passed, 0 failed, 0 ignored** |
| `cargo fmt --all -- --check` | clean |
| `cargo clippy --offline --workspace --all-targets -- -D warnings` | clean |
| Executable conformance | 61 executed, **61 passed, 0 failed**, claim `semantics_conforming` |
| `validate_release.py --scope all` | 31 PASS / 0 FAIL / 0 BLOCKED / 2 OUT_OF_SCOPE, `release_ready: true` |
| `sha256sum -c SHA256SUMS.txt` | exit 0 |
| `git status --short canonical/` | empty |

The baseline was 120 suites and 1235 tests. This task adds **9 suites and 59
tests**, and changes no existing test's expectation except three corrected with
their authority, described below.

## Acceptance criteria

| Criterion | Result | How |
| --- | --- | --- |
| No known crash or panic from untrusted source within defined resource bounds | PASS | executed — one crash found and closed; every nesting shape now survives 50,000 levels, and nested bodies carry a declared bound of 512 with its measurement |
| Fuzz, property and adversarial suites pass their thresholds | PASS | executed — 2,000 mutations, 2,000 generated documents, 2,000 arbitrary byte strings, every truncation of every canonical example, and the adversarial shapes, with zero panics and zero invariant violations |
| Repeat runs with identical inputs produce equivalent results | PASS | executed — byte-identical in one process, across three processes, from two working directories, with an empty environment |
| No tested capability escape bypasses language authorization or host permission | PASS | executed — six path escapes refused, a read grant does not write, and a `FORBID`den effect with the host grant given is refused at preflight before the host is consulted |
| Small, medium and larger application validations succeed | PASS | executed — all four reach `status.succeeded` with their declared outputs and evidence |
| Whole-program against task-decomposed shows explainable equivalent semantics | PASS | executed — identical stage, outcome, terminal status, checks, evidence, diagnostics and outputs; the one difference is the workspace each was told to use, which the comparison names |
| UI and CLI agree with the engine | PASS | executed — the M10 equivalence suite compares the workspace against the `lcl` binary byte for byte and still passes; this task adds the CLI against the in-process engine over the ladder |
| Clean install works without repository-local build state | PASS | executed — installed into a temporary home with an empty environment, run from outside the repository, and no installed file names the build directory |
| Release artifacts have reproducible provenance and checksums | PASS | executed — the tarball, its checksum, and a provenance record with the source commit, toolchain, package identity and every payload file's checksum. Bit-identical rebuild is **not** claimed and is recorded as a limitation |
| Zero known release-blocking defects remain | PASS | the crash found in phase B is closed; every remaining limitation is bounded, measured and recorded |

## Expected results corrected, with the authority

Three, all in `lcl-stdlib`, all caused by a defect repair moving a decision to
an earlier stage.

**Two fragment tests asserted a runtime identifier that is no longer reachable.**
They were written when no stage checked a written fragment, so a malformed one
executed and the runtime substituted an identifier from the row. The fragment
contract says "Static checks cover the complete fragment", and M4 now refuses it
there, so a written fragment cannot reach M7 at all. Both tests now assert the
static refusal and say why; the runtime's arm stays as totality for a fragment
that arrives at demand.

**One `core.sort` test needed no change but exposed a check that was too eager.**
The first version of the parameter-family check refused a `STRING` direction for
a parameter the registry types `ENUM[ascending|descending]`. The registry's own
`contract_type_notation` says "registry-only unions, metatypes, and schema names
do not become source forms", so an `ENUM[...]` contract type has no source
spelling and a site must write something else. The check now judges only a row
whose every alternative is spellable in a source `TYPE` field, and skips the
rest rather than refusing what the registry gives no way to write.

No test was weakened, skipped, ignored or reclassified.

## Additions to closed milestones, and why

Four, all additive, each required by a defect the owner authorized closing.

1. **`CompletionError::OperatorOperand` and a resolved stage on M8's
   diagnostic.** `expression_demand_resolution` says to resolve before stage
   selection and covers a demand made during a verification step. Without this,
   the identifier the registry names for that demand was raised by no layer.
2. **`CapabilityOutcome::Refused` in M6.** The boundary already carried two
   identifier-selecting arms for the host's two ways of saying no. This is the
   same shape for the standard library's implementation of a row, which can only
   discover part of `core.read`'s range contract once the representation is in
   hand.
3. **Object-valued parameters in M6's invocation reader.** A `PARAMETER` whose
   `VALUE` is an indented body was skipped entirely, so the operation received
   nothing.
4. **The lexicon and grammar in M4's contracts.** The checker reads a fragment
   with the parser M2 already exposes rather than a second reader, and
   `Checker::new` is unchanged.

No dependency was added. The workspace remains std only, 17 packages, none
third-party.

## Remaining limitations, and who owns them

Eight, each measured and recorded in `reports/implementation/LCL_RELEASE_REPORT.md`
section 10. The two that matter most:

- **Nested indented bodies are bounded at 512 levels**, because the static
  checker's declaration walk still recurses per level and `Statement`'s `Clone`
  is still derived. Past the bound the process aborts. The repair is the one
  this task applied to expressions, extended to statements, and it is the single
  highest-value follow-up.
- **A `ROUND` chain is quadratic in its depth.** Every other shape is linear.
  Bounded, measured, not repaired.

No later task owns these: `LCL-TASK-0020` is the last in the pack, so both are
recommended as a bounded follow-up rather than assigned.

## `git diff --stat`

```
 28 files changed, 1874 insertions(+), 194 deletions(-)
```

Plus seven new paths: `impl/crates/lcl-hardening/`, `apps/`, `packaging/`,
`reports/implementation/`, this report, and the three release artifacts under
`releases/`.

## Suggested commit message

```
Harden and release the LCL product

Close the five engine defects that were withholding the conformance claim.
An object written as an indented VALUE block now resolves, in declarations
and in operation parameters, so an object-valued declaration is no longer
MISSING with nothing missing. A fault raised by a check's own ASSERT now
carries its registered identifier and the execution stage the demand map
resolves, instead of being flattened to UNKNOWN by the layer that demanded
it. A malformed expression fragment is refused where the contract says it is
checked, statically, rather than reaching execution under a substituted
identifier. core.read applies its range contract. A declared parameter
family the row does not register is refused. The executable conformance gate
now runs 61 cases, passes all of them, and claims semantics_conforming.

Repair a crash from untrusted source that hardening found. Every expression
nesting shape ended the process by SIGABRT between two and sixteen thousand
levels, and canonical LCL declares no maximum nesting depth, so a limit was
not available as a repair. The static checker's expression walk now judges a
node's descendants through an explicit worklist, and the syntax tree's Clone
is iterative like the Drop beside it, because every consumer that copies a
subtree was paying derived recursion. Every shape now survives fifty
thousand levels, the depth M2 proved for the parser.

Add lcl-hardening: a seeded generator whose every case is reproducible from
one number, mutation and adversarial corpora, the cross-layer invariants
every report must hold, the performance and repeatability measurements, the
product-level security matrix, the application ladder and the packaging
smoke test. It defines no language rule and nothing depends on it.

Add four applications under apps/ and run them as a user runs them, with an
empty environment and exactly the capabilities each declares. The large one
is written twice, whole and decomposed into sub-tasks, and the two agree on
every observable the engine decides.

Add packaging: a release build that is offline and locked, a tarball with a
checksum and a provenance record, and an installer that needs no elevation
and is reversed exactly by its uninstaller.
```

The commit would contain only this task's changes.

## Confirmations

- **No background, delegated, parallel, asynchronous, worker or sub-agent was
  used** at any point, in planning or implementation. Every command ran in one
  sequential session; where a long build or test run was placed in a shell
  background job, it was the same session's own tooling, never a second agent.
- Nothing was staged, committed, tagged or pushed. The user controls Git
  closure.
- `canonical/` was not modified. Its validator and checksums are unchanged and
  `git status` reports it clean.
- No dependency was added; the workspace remains std only.
- No test was weakened, skipped, ignored or reclassified. Three expected results
  were corrected, each against the exact canonical authority that proves the
  previous expectation wrong.
- Nothing was installed on the system. Every install was into a temporary home,
  and every process started during verification was stopped.

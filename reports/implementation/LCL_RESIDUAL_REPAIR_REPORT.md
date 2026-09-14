# LCL-RESIDUAL-REPAIR-01 — execution record

Date: 2026-09-12. Status: **IN PROGRESS — B3 / SORT-01, A1 and A3 verified;
B1 / VR-01 and B2 / VR-04 retained; A2 / UI-01 verified including actual browser
observation; B4 is in progress (66/66 decision witnesses, claim accounting still
open); B5 and final integrated/package acceptance remain open.**

The latest owner instruction explicitly approves A3 and necessary supporting
callers/helpers/tests, and directs continuation through A2, B4, B5 and final
verification. Earlier approval-pending stop records below are historical.
Current continuation checkpoint: `3d55de33d17adf2dd65606dbad171d0f1359e228`.
See the latest continuation section at the end of this report.

The owner approved the Stage A plan with
`APPROVED: IMPLEMENT LCL-RESIDUAL-REPAIR-01 EXACTLY AS PLANNED.`
The adopted task is `/home/aivars/Downloads/LCL_Astra_Residual_Repair_Prompt.txt`.
Supporting independent report SHA-256:
`a3ee6ff604e66a181e533fb0ab412824e3485ec8c0bdd5685850c2b9b433e593`.

## Starting state and execution boundaries

- Root: `/mnt/F/LCL`; branch: `main`; HEAD and locally recorded `origin/main`:
  `2a20cc058264b223595deb687937bb0f26f49532` (`LCL finish`). No remote refresh.
- Starting tracked, staged and nonignored untracked state: clean.
- One primary session; sequential gates; no delegated agents, staging, commits,
  pushes, publication, global installation or personal desktop changes.
- Owned disk-backed scratch: `/mnt/F/.lcl-residual-repair-01-lxd8dwu8`.
  `TMPDIR` is its `tmp/`; targets are `target-current/` and `target-msrv/`.
  Core dumps disabled for build/test commands. Starting available space: 157 GiB.
- `starting-state.json` in scratch records SHA-256 values for all 176 canonical
  files, all 12 previous release files, and the four root brand files.
  All 192 still match after baseline execution.
- Canonical package identity:
  `00d648b162939d06c44838481a67c39bc12c64bdd6d105035c24150148fe67ed`.
  Original logo SHA-256:
  `6c930f61a0db7c7e2e1d7e5926b18a4d74dee1f921c0937f8c8e8092d3a96b3d`.

## B0 — fresh baseline

Commands below are separate gates. Complete stdout/stderr logs are in the
scratch `logs/` directory; displayed excerpts did not replace captured exit codes.
Rust commands ran in `impl/`, with the explicit scratch environment above.

| Command | Toolchain | Exit | Result | Log |
| --- | --- | --- | --- | --- |
| `cargo fmt --all -- --check` | system Rust 1.98.1 | 0 | PASS | `B0-fmt.log` |
| `cargo clippy --offline --workspace --all-targets -- -D warnings` | system Rust 1.98.1 | 0 | PASS | `B0-clippy.log` |
| `cargo test --offline --workspace --all-targets` | system Rust 1.98.1 | 0 | 132 suites; 1350 passed; 0 failed; 0 ignored | `B0-tests.log` |
| `cargo check --offline --locked --workspace --all-targets` | actual Rust 1.75.0 | 0 | PASS | `B0-msrv-check.log` |
| `cargo test --offline --locked --workspace --all-targets` | actual Rust 1.75.0 | 0 | 132 suites; 1350 passed; 0 failed; 0 ignored | `B0-msrv-tests.log` |
| `PYTHONDONTWRITEBYTECODE=1 python3 -B canonical/LCL_Core_0.1.0/09_CONFORMANCE/TOOLS/validate_release.py --root canonical/LCL_Core_0.1.0 --scope all` (repository root) | Python 3 | 0 | 31 PASS; 2 OUT_OF_SCOPE; 0 FAIL/BLOCKED | `B0-canonical-validator.log` |
| `sha256sum -c SHA256SUMS.txt` (canonical root) | system | 0 | 175 matching entries | `B0-canonical-checksums.log` |
| `sha256sum -c assets/brand/BRAND_ASSETS.sha256` (repository root) | system | 0 | 17 matching entries | `B0-brand-checksums.log` |

The minimum toolchain was absent. As explicitly approved, the official
`static.rust-lang.org/dist/` Rust 1.75.0 rustc, cargo and rust-std archives were
downloaded, checked against their official SHA-256 files, and installed only
under scratch `toolchains/1.75.0/`, with `--disable-ldconfig` and a private
configuration prefix. `toolchains/download-record.json` records URLs, sizes and
hashes; `logs/B0-toolchain-install.log` records commands and versions.
Compiler: `rustc 1.75.0 (82e1608df 2023-12-21)`; Cargo:
`cargo 1.75.0 (1d8b05cdd 2023-11-20)`. Both PATH and RUSTC explicitly selected it.
System compiler: Rust 1.98.1; system Cargo 1.98.1.

These are starting-revision results. Existing packaging/installed-launcher tests
use historical payload/development inputs, not a new final candidate. Canonical
OUT_OF_SCOPE classifications do not establish executable conformance.

## B1 — process supervision and effect truth

**FIXED within the approved Linux x86_64 process-group scope.** Other platforms
retain their previous implementation and are not covered by this evidence.
Process groups do not provide a sandbox against deliberately escaped sessions.

The actual-adapter regressions initially failed with exit 101:
`cargo test --offline -p lcl-capabilities --test real_process inherited_ -- --test-threads=1`.
An exited child with a descendant retaining stdout, stderr or both returned a
successful completion only after approximately 3.003 seconds despite a 200 ms
declared deadline. The live-child case returned while its holder was still alive.
`B1-inherited-before.log` preserves all four failures. Each case uses a separate
test subprocess, an eight-second external watchdog and finite three-second
holders; failed fixtures settle before their owned directory is removed.

The repair replaces Linux x86_64 reader threads with bounded nonblocking turns
over both descriptors, retains bytes only up to the configured cap and drains
excess. `waitid(WNOWAIT)` retains the direct child's PID/PGID ownership until the
last group signal. Collection continues after child exit. Normal completion also
requires the owned group to finish, including descendants that closed their
streams. Declared deadlines cover this entire wait; `None` adds no execution
timeout. After termination is requested, cleanup has an explicit one-second
allowance; failures are reported rather than treated as successful empty output.
Capture descriptors close synchronously and the direct child is reaped.

`Process::run_observed` carries start/output/cleanup observations without breaking
the existing `run` error API. `Observation::host_limited` carries a host fact,
while the runtime retains authority over the registered error and phase.
`core.execute` and `core.start` preserve post-start evidence and report unknown
effect extent as indeterminate. A legacy bounded error without observations
is also indeterminate, not proof that no effect began. Handler host limitations
reuse the same result conversion. No canonical bytes or error identifiers changed.

Integration revealed that `host.rs` ignored the runtime's existing normalized
DURATION representation. A recording wrapper around the real adapter proved a
declared second became the fallback 200 ms (`B1-normalized-timeout-before.log`,
exit 101). `nanoseconds` now accepts the runtime's `DURATION_UNIT` constant.
The exact 1,000,000,000 ns bound is verified at the adapter boundary. Authority:
`03_TYPES_AND_VALUES/07_FORMATS_ENCODINGS_UNITS_BOUNDS_AND_PATTERNS.txt:100`
and `formats_encodings_units_v0.1.0.json#/duration_normalization`.
The initial new assertion expected the written unit spelling; it was corrected
to the canonically equivalent normalized magnitude and strengthened with the
adapter-bound assertion, not merely relaxed to accept the observed fallback.

Other corrected failures are retained explicitly:

- `B1-affected-tests.log`, exit 101: the new socket fixture assumed dropping one
  descriptor guaranteed immediate EOF during parallel subprocess creation.
  A controlled duplicate-descriptor experiment established that this assumption
  is false. Explicit writer shutdown now synchronizes EOF; cap/content/EOF
  assertions remain intact. Both subsequent toolchain runs pass.
- `B1-eof-work-before.log`, exit 101: the first supervisor revision terminated a
  descendant that had closed its streams before it wrote its marker. Normal
  completion now waits for the owned group; the marker case passes in about
  0.21 seconds (`B1-eof-work-after.log`). This defect was introduced and repaired
  within B1; it is not attributed to the audited revision.
- `B1-legacy-bound-before.log`, exit 101: a bounded error without observations
  was mapped to PreEffect. It now maps to Indeterminate; the full external
  operation target passes (`B1-legacy-bound-after.log`, 20 tests).
- An initial single-file formatter invocation used the wrong relative path;
  absolute-path formatting/checking then passed. Formatting checks also found
  three new constructor layouts; only those affected files were formatted.
  `B1-fmt.log` retains the workspace formatting failure, and closure formatting
  passes. No workspace-wide formatting mutation was performed.

Final B1 gates (complete logs in the scratch `logs/` directory):

| Command | Exit / result | Log |
| --- | --- | --- |
| `cargo fmt --all -- --check` | 0 | `B1-closure-fmt.log` |
| `cargo clippy --offline -p lcl-capabilities -p lcl-runtime -p lcl-stdlib --all-targets -- -D warnings` | 0 | `B1-closure-clippy.log` |
| `cargo test --offline -p lcl-capabilities -p lcl-runtime -p lcl-stdlib --all-targets` | 0; 28 suites, 371 passed, 0 failed/ignored | `B1-closure-tests.log` |
| actual Rust 1.75.0: `cargo test --offline --locked -p lcl-capabilities -p lcl-runtime -p lcl-stdlib --all-targets` | 0; 28 suites, 371 passed, 0 failed/ignored | `B1-closure-msrv-tests.log` |
| actual Rust 1.75.0: `cargo check --offline --locked --workspace --all-targets` | 0; before the last private supervisor/host corrections, which the closure tests compile and exercise | `B1-msrv-check.log` |

Coverage includes all four inherited-pipe cases, old flood/truncation/repeated
child cases, eight native supervision cases (including read/setup errors,
waitable ownership, unbounded execution and closed-stream descendant work),
real partial stdout/effect uncertainty, exact timeout delivery and absent
observation compatibility. Synthetic primitive/compatibility fixtures are not
claimed as real subprocess evidence. The final full workspace and packaged
application gates remain pending.

All 192 protected file hashes match after B1. No processes executing scratch
binaries or using scratch/tmp as cwd were found after the gates; the inherited
pipe regressions separately assert their recorded holders are gone and reader
thread counts do not grow. Five baseline test homes were removed only from this
run's owned scratch/tmp; `B1-tmp-cleanup.json` records their exact names. Targets,
the private toolchain and evidence logs remain for subsequent phases.

## B2 — atomic creation and safe persistence

**FIXED.** The old predictable temporary path followed a preexisting symlink and
overwrote an unrelated ordinary text file. The controlled regression failed with
exit 101 in `B2-temporary-before.log`. Temporary files now use bounded unique
names reserved with `create_new`; writes complete before publication. An owned
guard removes only its own temporary file on success and failure.

Create uses a same-directory hard link for atomic, non-overwriting publication;
existing destinations produce the typed `AlreadyExists` error and HTTP 409.
There is no overwrite fallback. Ordinary save retains atomic replacing rename.
The route's preliminary existence check is only an early response, never the
reservation. Normalization, containment, exact legacy save paths and ordinary
text-file preservation remain in place.

Two prepared low-level writers synchronize publication and prove exactly one
complete winner. Authenticated HTTP tests prepare both request bodies before a
barrier releases their final byte. They cover identical and normalized names,
two separate workspace processes sharing a root, and overlapping legacy saves.
The create loser receives 409; saves each return their own correct digest and
leave one whole payload. Existing files, dangling links and directories remain
unchanged; success/failure cases leave no owned temporary debris. Product
subprocess guards bound startup and reap only their owned children.

| Command (in `impl/`) | Exit / result | Log |
| --- | --- | --- |
| `cargo test --offline -p lcl-workspace --lib --test persistence` | 0; 2 unit and 19 persistence tests | `B2-document-publication.log` |
| `cargo test --offline -p lcl-workspace --test routes` | 0; 19 tests, including strengthened conflict/naming checks | `B2-route-conflicts.log` |
| `cargo test --offline -p lcl-workspace --test concurrent_persistence` | 0; 3 actual product HTTP tests | `B2-concurrent-http.log` |
| `cargo fmt --all -- --check` | 0 | `B2-fmt.log` |
| `cargo clippy --offline -p lcl-workspace --all-targets -- -D warnings` | 0 | `B2-clippy.log` |
| `cargo test --offline -p lcl-workspace --all-targets` | 0; 10 suites, 86 passed, 0 failed/ignored | `B2-workspace-tests-confirmed.log` |
| actual Rust 1.75.0: `cargo check --offline --locked -p lcl-workspace --all-targets` | 0 | `B2-msrv-check.log` |
| actual Rust 1.75.0: `cargo test --offline --locked -p lcl-workspace --all-targets` | 0; 10 suites, 86 passed, 0 failed/ignored | `B2-msrv-tests.log` |

The earlier `B2-workspace-tests.log` contains passing suites, but its final tool
exit response was lost during context compaction. The gate was rerun once to
capture an authoritative exit code; the confirmed log above and `.exit` file
are the closure evidence. This was an evidence gap, not an observed test failure.
All 192 protected hashes still match, HEAD remains the starting revision and the
owned `tmp/` directory is empty after B2. No changes are staged.

## B3 — earlier checkpoint stopped at the checker boundary

**BLOCKED — PLAN AMENDMENT REQUIRED, not a canonical contradiction.** No sort
production code or checker code has been edited. The previous named-enum
descending regression passed during B0 and B1. The newly added matrix tests
omission, bare and named enum literals, DATA reads and constant reads, with a
wrapper that records the actual request and delegates to the real standard
library. It preserves the expected ordering and multiplicity assertions.

`cargo test --offline -p lcl-stdlib --test pure_operations sort_direction_forms -- --nocapture`
failed with exit 101 (`B3-direction-matrix.log` and `.exit`). Omitted direction
reached the operation with `None` and returned `[1, 1, 2, 3]`. The next case,
bare ENUM ascending, failed static checking with `error.type.mismatch`. Later
matrix entries have **NOT EXECUTED**; the matrix remains red, with no exemption,
deleted case or weakened assertion.

A focused diagnostic pass reduced the rejection to the 40-line standalone
`/mnt/F/.lcl-residual-repair-01-lxd8dwu8/sort-bare-descending.lcl.txt`.
Source SHA-256: `c44e826517ec34e481da7915e3824e92957e5419156f06477f16248acdee745c`.
It sorts `[3, 1, 2, 1]` with `PARAMETER.NAME: direction`, `TYPE: ENUM`,
`REQUIRED: FALSE`, `VALUE: descending`; the expected ordered result is
`[3, 2, 1, 1]`.

After a successful current CLI build (`B3-cli-build.log`, exit 0), this exact
command ran with an empty environment and cwd equal to the owned scratch root:

```text
/mnt/F/.lcl-residual-repair-01-lxd8dwu8/target-current/debug/lcl run --machine --spec /mnt/F/LCL/canonical/LCL_Core_0.1.0 /mnt/F/.lcl-residual-repair-01-lxd8dwu8/sort-bare-descending.lcl.txt
```

The CLI exited **1**, reached `static_checking` and rejected the `ENUM` token
at line 21, column 15, bytes 366–370. Exact diagnostic: `error.type.mismatch`,
cause `unconstrained_type`, detail `bare ENUM does not declare a material value
type` (the emitted detail includes backticks around ENUM).
`B3-cli-bare-descending.stdout.json`, `.stderr.log` and `.record.json` preserve
the complete response, command, environment, real process exit and identities.
The diagnostic collector's own exit 0 is not the CLI result.
CLI binary SHA-256: `e5353895ccae21fa55602bfad1711b8f3424a6dd0ae9366c1833688518e6240c`.
This is a development binary, not a final packaged candidate.

Authority and traced cause:

- `03_TYPES_AND_VALUES/05_CUSTOM_TYPES_SCHEMAS_AND_DEFINITIONS.txt:29–31`
  explicitly permits bare ENUM when an operation contract supplies one exact
  enum domain. Its 25–29 lines require that domain and reject nonmembers.
- `10_REGISTRIES/operations_v0.1.0.json#/contracts/core.sort/parameters/direction`
  supplies `ENUM[ascending|descending]`, default ascending. The row's
  `diagnostic_triggers/error.type.mismatch` rejects values outside that domain.
- `lcl-checker/src/types.rs:200–205` rejects generic bare ENUM as unconstrained.
  That generic rule is appropriate outside the stated exception.
- `lcl-checker/src/declarations.rs:898–963` resolves nested PARAMETER types
  generically and provides no enclosing operation's enum context. The TYPE
  field then reaches the ordinary type-expression checker. `operation.rs`
  already loads the operation row but does not supply the missing context.
- Checker paths are unchanged against the starting HEAD. This is a newly
  reproduced existing checker defect, not a regression introduced by B1/B2.

The reported descending-returned-ascending symptom is **not established** by
this rejection. Repairing this earlier boundary exceeds the approved B3
production scope (`pure.rs` only after a proved sort-boundary defect).
Per the task's failure/amendment gate, no later workstream has advanced.

### Proposed amendment LCL-RESIDUAL-REPAIR-01-A1 (not implemented)

1. Extend `impl/crates/lcl-checker/tests/constructors_and_operations.rs` with
   registry-backed positive direction cases, invalid/nonmember cases and a
   control proving bare ENUM remains invalid without an exact receiving domain.
   Preserve nominal user enums, ordinary reference reads and old diagnostics.
2. Extend `impl/crates/lcl-checker/src/operation.rs` to expose the selected
   operation's exact registered enum parameter context to the shared checker
   walk, derived from its already verified contract. Do not infer a domain
   from the value spelling or special-case fixture names.
3. Update `impl/crates/lcl-checker/src/declarations.rs` to apply that context
   only to the receiving invocation PARAMETER's TYPE/VALUE; retain the generic
   unconstrained-type rejection elsewhere and use the existing type/expression
   machinery. No canonical, dependency, parser or general type-policy change.
4. Resume the unchanged red `pure_operations.rs` matrix and the original B3
   engine/CLI investigation. Repair `pure.rs` only if its own narrow defect is
   then demonstrated; retain LIST stability/multiplicity and SET rules.

Each file still requires inspection, announced single-file edits and focused
verification. The first gate after the added checker reproducer is its focused
test; after production repair, rerun that gate and the currently red direction
matrix, then CLI cases, affected checker/stdlib tests and clippy, formatting,
and actual Rust 1.75.0 checks/tests. Only green B3 phase closure permits B4.
All other original stages and safeguards remain unchanged. A further boundary
expansion still requires a revised plan; no blanket checker refactor is proposed.

## 2026-09-12 continuation — A1/A2 approved; SET boundary reproduced

The owner replied **approved** to the proposal containing existing A1 and new
A2 (UI-01). Both approvals persist. On resumption, local root was `/mnt/F/LCL`,
branch `main`, with HEAD and locally recorded upstream `origin/main` at
`0ff51b7df73639e0a9189b6c8a536545d4b8f5fb`; the working tree and index were
clean. This is the owner's committed checkpoint, one commit after the original
baseline, not an assistant commit. No remote refresh was performed.

A1 now derives the exact contextual enum domain from the verified operation
row and keys it to the receiving PARAMETER's source locus. The ordinary
expression checker handles VALUE and the other typed fields. The written bare
ENUM TYPE is recorded as that domain; generic unconstrained-type rejection and
other parameter paths are preserved. Its internal domain identity is the
registered slot's pointer, whose separators cannot collide with a source ID.
No checker type-model, canonical, dependency or production sorting change was
made.

Fresh gates, system Rust 1.98.1, existing owned scratch environment:

| Command (in `impl/`) | Exit / result | Log |
| --- | --- | --- |
| `cargo test --offline -p lcl-checker --test constructors_and_operations contextual_enum -- --nocapture` before repair | 101; admitted ENUM still rejected | `A1-checker-before.log`, `A1-checker-red-confirmed.log` |
| `cargo check --offline -p lcl-checker --all-targets` after additive context helper | 0 | `A1-context-check.log` |
| `cargo test --offline -p lcl-checker --test constructors_and_operations` after repair/corrected new test loci | 0; 20 passed, 0 failed/ignored | `A1-checker-confirmed.log` |
| `cargo test --offline -p lcl-stdlib --test pure_operations sort_direction_forms -- --nocapture` | 0; all seven matrix entries executed | `A1-sort-matrix.log` |
| `cargo test --offline -p lcl-stdlib --test pure_operations` with new tie/SET controls | 101; 22 passed, 1 failed, 0 ignored | `A1-sort-controls.log` |
| `cargo test --offline -p lcl-stdlib --test pure_operations sort_direction_preserves_set_distinct_key_requirement -- --nocapture` with observation-only trace | 101; SET target became LIST before sorting | `A1-set-diagnostic.log` |
| `cargo build --offline -p lcl-cli --bin lcl` for standalone diagnosis | 0 | `A1-cli-diagnostic-build.log` |

Each gate has a separate `.exit` file and complete log. Two new-test defects
were corrected without weakening product assertions: the key-parameter negative
control must preserve both its existing unregistered-family and unconstrained
type diagnostics; and locating invalid VALUE `TRUE` with `rfind` mistakenly
selected a later SUCCESS member. The test now locates its exact `VALUE:` field.
`A1-checker-after.log` retains that latter failure. The full 20-test checker
target passes after both corrections.

The unchanged direction matrix now observes:

- omitted: no supplied direction, `[1, 1, 2, 3]`;
- bare and named ascending: `Identifier("ascending")`, `[1, 1, 2, 3]`;
- bare/named descending and DATA/constant references:
  `Identifier("descending")`, `[3, 2, 1, 1]`.

New LIST key-projection controls pass for both directions, preserving the exact
source order of distinct equal-key members and duplicate LIST members. No
production `pure.rs` edit is justified by the recorded direction symptom.
The A1 change has focused evidence, but B3 phase closure, current broad gates
and the actual Rust 1.75.0 rerun are still pending because the required SET
control failed. Do not present earlier B1/B2/MSRV counts as those pending gates.

### SORT-01-SET — newly reproduced existing preflight defect

For distinct SET strings `apple` and `apricot`, the declared deterministic pure
key fixture returns `a` for both. The canonical core.sort row requires
`error.operation.precondition`; the actual result has no execution error.
The test preserves that expected error and stays red. Its diagnostic wrapper
only records observations and delegates to the actual Stdlib:

```text
checked=Some(Set(String))
planned=List([Text("apple"), Text("apricot")])
request_target=Some(List([Text("apple"), Text("apricot")]))
read=List([Text("apple"), Text("apricot")])
```

Cause traced to `lcl-semantics/src/data.rs::declared_value`: inline collections
unconditionally become `Value::List`. The corresponding expression and
multiline-property paths in `lcl-semantics/src/eval.rs` also construct LIST
without using the checker's receiving-family annotation. The runtime reads
the preflight resolution as stored. `pure.rs` correctly selects its distinct-key
guard only for a `Value::Set`, so changing the sorter alone would mask the
upstream family loss. These preflight files are unchanged against `0ff51b7`.

Authority: `03_TYPES_AND_VALUES/10_COLLECTION_OBJECT_ENUM_AND_SCHEMA_FORMS.txt`
SET[T] at lines 13–20 and bracket typing at 52–57; also
`03_TYPES_AND_VALUES/03_COLLECTIONS_OBJECTS_ENUMS_AND_EQUALITY.txt:8–11` and
`operations_v0.1.0.json#/contracts/core.sort` distinct-key precondition and
member-preservation postconditions. No canonical contradiction is alleged.

Standalone CLI proof uses ordinary registered sorting, no custom key or enum
parameter: `/mnt/F/.lcl-residual-repair-01-lxd8dwu8/sort-set-duplicates.lcl.txt`.
It declares `SET[INTEGER]` with `[3, 1, 1]`, binds the sorted LIST output and
verifies its member count is two. Actual published output is **`[1, 1, 3]`**;
VERIFY is FALSE, diagnostic `error.verification.failed`, CLI exit **2**.
Expected result is `[1, 3]`. Exact command, empty environment, cwd, output and
exit are in `A1-cli-set-observable.record.json`, `.stdout.json`, `.stderr.log`.
Source SHA-256: `b669fa17a99c0155221de1dfe324664125dedc3c1b8f2aa291af6fa6c749978d`.
Exercised development CLI SHA-256:
`39903c8617a707520172208eb07a562165dc0de547e789a2cde7562d8fb557ca`.
No final packaged binary exists. The initial `A1-cli-set-duplicates.*` fixture
had no bound output and an inadequate SUCCESS oracle; it is retained but is
not the standalone proof. The observable fixture above supersedes it.

### Proposed amendment LCL-RESIDUAL-REPAIR-01-A3 (awaiting approval)

Extend B3 to restore typed collection materialization at preflight, before A2.
Use the already checked receiving type and each declaration's actual SourceId;
do not infer SET from member spelling or repair it inside the sorter.

Sequential file scope:

1. New `impl/crates/lcl-semantics/tests/collection_materialization.rs`:
   direct preflight regressions for SET family and duplicate collapse, paired
   LIST order/multiplicity controls, inline/multiline/nested forms, constants,
   applicable default paths and imported-source annotation identity. Retain
   existing object/constant/robustness regressions; do not restart those tasks.
2. If shared collection construction/equality support is necessary, place it
   on the existing Value model in `lcl-semantics/src/value.rs` and have
   `lcl-runtime/src/eval.rs` delegate through its compatible existing interface.
   This explicitly permits only the value-level sharing needed for canonical
   duplicate collapse, not a new evaluator, crate, dependency or runtime rewrite.
3. `impl/crates/lcl-semantics/src/eval.rs`: preserve the checked family when
   constructing expression and multiline collections, evaluating members before
   strict-equal duplicates collapse. Apply the existing canonical value rules;
   add no ordering significance to SET storage and retain LIST multiplicity.
4. `impl/crates/lcl-semantics/src/data.rs`: route declared collections through
   that same path with the declaring source identity; preserve existing
   value/source/default/assumption precedence and object/constant behavior.
5. Reuse the red SET test in `lcl-stdlib/tests/pure_operations.rs`, expand only
   discriminating source-to-sort controls as necessary, and update this report.

Required gates: the new preflight target; the exact currently red SET target
and full pure_operations target; all direction cases through the current CLI;
affected checker/semantics/runtime/stdlib checks, tests and clippy; formatting;
actual Rust 1.75.0 checks/tests. The standalone duplicate program must publish
`[1, 3]`, pass its VERIFY and exit 0. Only then close B3 and begin approved A2.
Canonical/equality questions that cannot be resolved from existing authority
still require an explicit decision; A3 is not blanket permission to change
language meaning or unrelated evaluators.

A2 remains approved with its existing five-file scope: new workspace
`tests/editor_save.cjs`, new `tests/editor_save.rs`, production `assets/app.js`,
`impl/README.md`, and this report. Test-only Node >=22 using built-in modules
was included in that approval; installed Node is 26.8.2. A2 binds saves to exact
documents/submitted bytes, propagates failure, preserves later edits, handles
line-feed normalization/tab switches, serializes per-document saves and closes
only when the intended document has no unsaved/pending work. Real frontend/HTTP
regressions, exact-candidate reuse and separate graphical acceptance remain
required. Do not ask for A1/A2 approval again.

## Approved workstreams and remaining acceptance

| Phase / finding | Current applicability and evidence limit | Status |
| --- | --- | --- |
| B1 / VR-01 | Actual inherited-pipe, capture, cleanup and effect-observation regressions pass; scope and limits recorded above. | FIXED for Linux x86_64 process-group scope |
| B2 / VR-04 | Atomic non-overwriting publication and exclusive temporary reservation; low-level and actual concurrent HTTP evidence passes on both toolchains. | FIXED |
| B3 / SORT-01 | A1 checker/direction matrix passes; SET materialization loses its family and duplicates. Exact failed gate and CLI proof above. | BLOCKED — A3 approval required; SET regression red |
| A2 / UI-01 | Save/close integrity amendment approved; implementation waits for B3 closure. | APPROVED, NOT STARTED |
| B4 / VR-03 | Source claim still accepts sparse category evidence and witnesses use prefix accounting. Version-bound obligations and faithful missing cases pending. | OPEN |
| B5 / VR-02 | Script still builds/copies live checkout and Git-free inventory can be empty. Frozen-source and exact-candidate rebuild/install evidence pending. | OPEN |

Required unestablished witnesses remain CLOSURE-004, CLOSURE-006, CLOSURE-021,
CLOSURE-022, CLOSURE-023, CLOSURE-024, CLOSURE-027, CLOSURE-055, CLOSURE-058,
CLOSURE-059 and CLOSURE-060. Baseline green tests do not fill these obligations.

Next: owner decision on A3, then its first preflight collection regression gate.
The plan's architecture and canonical decision gates remain binding. No later
workstream may advance while a preceding required gate fails.

## Final acceptance still pending

No new source snapshot, candidate, archive checksum or exercised installed binary
identity exists yet. Final integrated regression, source-export rebuild, exact
candidate installation and graphical acceptance are not executed. A stub browser
opener or HTTP result will not be counted as observed graphical acceptance.

Changed repository files in first-touch order (later corrections documented above):
this report; `impl/crates/lcl-capabilities/tests/real_process.rs`;
`impl/crates/lcl-capabilities/src/process/linux.rs` (new);
`impl/crates/lcl-capabilities/src/process.rs`;
`impl/crates/lcl-runtime/src/capability.rs`;
`impl/crates/lcl-runtime/src/execute.rs`; `impl/crates/lcl-runtime/src/handler.rs`;
`impl/crates/lcl-stdlib/src/host.rs`;
`impl/crates/lcl-stdlib/tests/external_operations.rs`;
`impl/crates/lcl-workspace/tests/persistence.rs`;
`impl/crates/lcl-workspace/src/document.rs`;
`impl/crates/lcl-workspace/src/project.rs`;
`impl/crates/lcl-workspace/src/routes.rs`;
`impl/crates/lcl-workspace/tests/routes.rs`;
`impl/crates/lcl-workspace/tests/concurrent_persistence.rs` (new);
`impl/crates/lcl-stdlib/tests/pure_operations.rs` (new failing investigation).
All changes remain unstaged. Scratch toolchains, build targets and logs remain
owned by this run. Retain evidence before exact-path cleanup; preserve old releases.

At the B3 stop, root/HEAD/local upstream remain unchanged; the staged diff is
empty. All 192 protected files still match; owned `tmp/` is empty, and no process
executing a scratch binary or using owned tmp as cwd remains. Available disk
space is 147,653,734,400 bytes. `logs/B3-stop-preservation.json` records these
observations. Full final gates and packaging/GUI acceptance remain NOT EXECUTED.
The continuation handoff is
`/mnt/F/.lcl-residual-repair-01-lxd8dwu8/LCL_RESIDUAL_REPAIR_01_HANDOFF.txt`.

Continuation change order after the owner's `0ff51b7` checkpoint:
`lcl-checker/tests/constructors_and_operations.rs`, `lcl-checker/src/operation.rs`,
`lcl-checker/src/declarations.rs`, `lcl-stdlib/tests/pure_operations.rs`, this
report (all crate paths under `impl/crates/`). All five changes remain unstaged.
Current root/branch/HEAD/upstream remain as recorded for this continuation.
`logs/A1-stop-state.json` records matching hashes for all 192 protected files,
empty owned tmp, no owned product processes, empty staged diff and approximately
147.5 GB free disk. The original toolchain, targets and logs are retained.
The new current handoff is
`/mnt/F/.lcl-residual-repair-01-lxd8dwu8/LCL_RESIDUAL_REPAIR_01_HANDOFF_A3.txt`;
the earlier handoff remains historical and must not revoke A1/A2 approval.


## 2026-09-12 continuation — A3 approved, B3 verified

The owner's direct instruction in attachment
`83011386-09b0-4b4f-b4dc-a9112263b683/pasted-text.txt` explicitly approves A3,
retains A1/A2/base approval and includes necessary existing callers, shared
helpers and regressions. It directs implementation through final verification;
an old handoff's pending approval is superseded. No new architecture,
dependency or canonical decision was needed here.

Fresh starting inspection confirmed `/mnt/F/LCL`, `main`, HEAD and locally
recorded `origin/main` at `3d55de33d17adf2dd65606dbad171d0f1359e228`
(`LCL finish v3`), with no staged, unstaged or untracked files. This is the
owner's checkpoint containing A1 and the previously red SET controls. No pull,
branch switch or Git mutation was performed. Existing contracts and current
files were reconciled; Tasks 11–20 and completed B1/B2 were not restarted.

### Implemented boundary repair

The checked receiving annotation determines LIST versus SET for ordinary,
multiline, nested and schema-received brackets. All source members are evaluated
before strict-equal duplicates collapse; LIST order and duplicates remain.
VALUE, DEFAULT and applicable ASSUME use the same field evaluator with the
actual declaring SourceId. Existing constant and source/default/assumption
precedence is retained. Reference identity contexts use the resolved binding;
value dependencies are resolved by a finite worklist and completed resolutions
are available to later reads. Resolution report order is retained, and this
adds no producer activation or execution graph edges.

Canonical normalization now lives beside the existing shared Value model.
Runtime's order-profile module re-exports that implementation; duration factors
load from the verified registry in preflight. SET construction and runtime
strict equality share the same helper, including unordered nested SET equality
and normalized TIME/DATETIME/DURATION identity. No crate or dependency was
added. Runtime and preflight no longer carry separate normalization code.

New `lcl-semantics/tests/collection_materialization.rs` executes 11 regressions:
inline/multiline family and duplicates, empty collections, nested SET/LIST
identity, constants/defaults and supplied/UNKNOWN precedence, normalized
ordered values, retained reference identity, actual imported-source annotations,
invalid later-member diagnostics, malformed multiline separators, value REF in
both declaration orders, and a schema-received multiline SET property.

### Fresh evidence and corrected expectations

All Rust commands below ran from `impl/`, offline, using the existing explicit
scratch TMPDIR and target directories. Full logs and actual `.exit` files remain
under `/mnt/F/.lcl-residual-repair-01-lxd8dwu8/logs/`.

| Gate | Exit / result | Log |
| --- | --- | --- |
| New preflight regressions, before repair | 101; 1 passed, 7 failed | `A3-collection-before.log` |
| Verified duration contract loading | 0; 12 passed | `A3-contracts.log` |
| New collection + existing data-resolution targets | 0; 11 + 26 passed, no failed/ignored | `A3-reference-resolution.log` |
| Runtime evaluation target | 0; 60 passed, no failed/ignored | `A3-runtime-evaluation-canonical.log` |
| Full stdlib pure_operations | 0; 23 passed, including the original equal-key SET failure and direction/stability controls | `A3-sort-full.log` |
| `cargo fmt --all -- --check` | 0 | `A3-fmt-workspace.log` |
| `cargo clippy --offline -p lcl-checker -p lcl-semantics -p lcl-runtime -p lcl-stdlib --all-targets -- -D warnings` | 0 | `A3-phase-clippy-corrected.log` |
| `cargo test --offline -p lcl-checker -p lcl-semantics -p lcl-runtime -p lcl-stdlib --all-targets` | 0; 46 suites, 559 passed, 0 failed/ignored; actual Rust 1.98.1 | `A3-phase-tests.log` |
| Same four-crate all-target `cargo check --offline --locked` | 0; actual Rust 1.75.0 | `A3-msrv-check.log` |
| Same four-crate all-target `cargo test --offline --locked` | 0; 46 suites, 559 passed, 0 failed/ignored; actual Rust 1.75.0 | `A3-msrv-tests.log` |
| `cargo build --offline -p lcl-cli --bin lcl` | 0 | `A3-cli-build.log` |
| Nine actual CLI source programs, empty inherited environment | Every process exit 0, VERIFY TRUE, terminal status.succeeded, exact expected published list | `A3-cli-matrix.json`, per-case `A3-cli-*.record.json`, `.stdout.json`, `.stderr.log` |

Failures remain recorded and were diagnosed before later changes:

- Two new multiline fixtures omitted the commas required by
  `04_GRAMMAR/10_COMPLETE_EBNF.ebnf:67–68`. Corrected source spelling and added a
  discriminating rejection control for comma-free members. `A3-collection-after.log`
  retains the grammar failures; `A3-collection-corrected-fixtures.log` passed.
- One invocation from repository root failed to find Cargo.toml
  (`A3-collection-confirmed.log`, exit 101). It was rerun from verified `impl/`;
  this command-location error was not counted as a product regression.
- The old runtime test asserted FALSE for equivalent DATETIME instants, with
  a representation-identity rationale. Both `03_TYPES_AND_VALUES/03:17–22` and
  `operators_and_functions_v0.1.0.json#/ordered_value_equality` require TRUE.
  The assertion was corrected and unequal-instant/signed TIME displacement
  controls added. `A3-runtime-evaluation.log` retains 59 passed/1 failed.
- The new value-REF regression initially produced MISSING because all
  resolutions were hidden in a local list until the source loop finished.
  `A3-collection-reference-before.log` retains that red case. Dependency order
  and incremental publication repair it without reading through stored identities.
- `A3-phase-clippy.log` records mixed module documentation attributes and two
  manual-contains lints. Corrected without suppression; the exact gate passed.

The unchanged original standalone SET source retains SHA-256
`b669fa17a99c0155221de1dfe324664125dedc3c1b8f2aa291af6fa6c749978d`.
It now publishes `[1, 3]`, verifies TRUE and exits 0. The LIST counterpart keeps
`[1, 1, 3]`. The seven CLI forms are omitted, bare/named ascending and descending,
and DATA/constant references; their four-member LIST preserves multiplicity
and publishes the exact expected ascending/descending list. The development
CLI actually exercised has SHA-256
`caeea88ab65b26cdadf366ceb755a18c17e3c55a6b6a051f1e9a65468a9de55f`.
This is development evidence, not final candidate acceptance.

A3 first-touch change order: new semantics `tests/collection_materialization.rs`;
semantics `src/value.rs`; runtime `src/order_profile.rs`; semantics
`src/contracts.rs`, `src/eval.rs`, `src/data.rs`; runtime `src/eval.rs`,
`tests/evaluation.rs`; this report. Later corrections above stay in that scope.
All current changes remain unstaged. HEAD/branch/upstream are unchanged.
`A3-preservation.json` confirms all 192 protected files still match the original
inventory. No canonical, artwork or historical release file was changed.

**Current phase result: B3 / SORT-01, A1 and A3 FIXED with the above phase
verification.** A2/UI-01 is approved and next. B4's version-bound claims and
11 unestablished witnesses, B5's source-bound build/export/exact candidate,
final workspace-wide checks and graphical acceptance remain required. No
release-complete or full-language-conformance claim is made by this closure.


## 2026-09-12 continuation — A2 / UI-01 verified

A2 now captures the intended Doc, immutable submitted text and edit revision at
Save, serializes that document's saves and retains a count of pending work.
Only the response's acknowledged content (including explicit final LF) updates
the saved baseline. Newer edits remain dirty, and an inactive save cannot
replace the active textarea. The close modal passes its own Doc to Save and
closes only after success with no dirty or pending work. Discard stays explicit;
stale close callbacks cannot delete a newly reopened Doc. Save returns a Boolean
result, checks the acknowledgement's document/byte count and separates later
refresh failure from persistence failure. Input and reload maintain revision
identity. Backend atomic create/save publication from B2 remains unchanged.

Changes in first-touch order: new workspace `tests/editor_save.cjs`, new
`tests/editor_save.rs`, production `assets/app.js`, `impl/README.md`, this report.
The JS harness loads the whole unmodified production script, with a controlled
DOM and transport. Its server mode instead fetches the actual served JavaScript,
uses authenticated real HTTP and checks filesystem bytes. The Rust driver owns
and bounds startup, execution and teardown; Node >=22 is an explicit test-only
prerequisite, with no npm or new Rust dependency. Paired
`LCL_EDITOR_WORKSPACE_BIN` / `LCL_EDITOR_SPEC` overrides select an exact candidate
and bundled specification for the later release gate. Missing prerequisites or
only one override fail the test.

### Fresh acceptance evidence

Logs below are in the existing scratch `logs/` directory; command exits are
captured separately in `.exit` files. Rust commands use the same explicit scratch
TMPDIR/targets and run from `impl/`.

| Gate | Exit / observed result | Log |
| --- | --- | --- |
| `node impl/crates/lcl-workspace/tests/editor_save.cjs`, before repair, from repo root | 1; 2 controls pass, 9 acceptance cases fail | `A2-editor-controlled-before.log`, `A2-editor-controlled-red-confirmed.log` |
| `cargo test --offline -p lcl-workspace --test editor_save -- --nocapture`, before repair | 101; the same 9 cases fail against served JS + real HTTP/disk | `A2-editor-http-before.log` |
| Controlled whole-script harness after repair | 0; 11 pass, 0 fail/skip | `A2-editor-controlled-after.log` |
| Actual served-script HTTP/disk target after repair | 0; all 11 cases pass inside one Rust integration test | `A2-editor-http-after.log` |
| `cargo clippy --offline -p lcl-workspace --all-targets -- -D warnings` | 0 | `A2-clippy.log` |
| `cargo test --offline -p lcl-workspace --all-targets` | 0; 11 suites, 87 passed, 0 failed/ignored; Rust 1.98.1 | `A2-workspace-tests.log` |
| `cargo check --offline --locked -p lcl-workspace --all-targets` | 0; actual Rust 1.75.0 | `A2-msrv-check.log` |
| `cargo test --offline --locked -p lcl-workspace --all-targets` | 0; 11 suites, 87 passed, 0 failed/ignored; actual Rust 1.75.0 | `A2-msrv-tests.log` |
| `cargo fmt --all -- --check` | 0 | `A2-fmt.log` |
| Owned Firefox graphical acceptance script | 0; three demonstrated UI-01 cases pass in real headed Firefox | `A2-firefox-acceptance.log`, `A2-firefox-report.json` |

The 11 harness cases cover failed close-save, closing inactive A with dirty
active B, newer edits during a delayed acknowledgement, LF handling after a tab
switch, serialized overlapping saves, edits during close-save, pending save
with text reverted to the old baseline, discard/reopen before an old response,
Cancel/Discard, successful toolbar Save for a legacy name containing spaces,
and project-list refresh failure after successful persistence. The new harness
was tightened before product edits to settle held promises even on failed
assertions and assert the newer buffer state before the new return-value rule.
The same nine acceptance failures remained; no product oracle was weakened.

### Actual browser observation, not the DOM double

The preferred browser plugin could not initialize: its client import was refused
with `Importing module "node:process" is not allowed in node_repl`. Its prescribed
bootstrap troubleshooting offered no applicable recovery before initialization.
The available native fallback was then verified from Mozilla's documentation:
[direct Firefox BiDi connection](https://developer.mozilla.org/en-US/docs/Web/WebDriver/How_to/Create_BiDi_connection)
and [isolated instance/profile arguments](https://firefox-source-docs.mozilla.org/browser/CommandLineParameters.html).

An owned scratch script `A2-firefox-acceptance.cjs` started the real product and
Firefox 155.0.1 with a fresh profile, private HOME/XDG paths, `--no-remote`, a
loopback debugging port and the existing Wayland display. Firefox's returned
capabilities explicitly record `moz:headless: false`. It drove pointer and
keyboard actions on actual UI controls, without replacing production save logic:

- Dirty inactive A saved and closed; B's textarea stayed unchanged and dirty;
  disk held A's submitted text and B's previous saved text.
- A controlled filesystem refusal (the test document path temporarily occupied
  by an owned directory) produced the real error toast and retained the visible
  buffer/tab. The fixture's original saved file was preserved and restored.
- A BiDi network intercept held the actual PUT response after the server had
  persisted it; further keyboard edits remained visible and modified after the
  acknowledgement was released. Disk still held only the submitted version.

The three screenshots were opened and visually inspected:
`A2-firefox-inactive-saved.png`, `A2-firefox-save-refused.png`, and
`A2-firefox-newer-edits.png`. Additional initial/modal captures and the full
`A2-firefox-protocol.jsonl` are retained. The deliberately plain editing strings
are lexically invalid LCL, which the real diagnostics panel reports; these are
save-integrity checks, not valid-program execution evidence. The header artwork
was visibly present and decoded at its original 97-pixel width. Final packaged
application/desktop launcher checks still belong to B5.

The exact development workspace exercised has SHA-256
`9de91ec4ea1c9dd2bdfc0b9a8dc1aecef274b4a385d3a39073a449eacd234a77`;
served production `app.js` has SHA-256
`f281ec49f8f60a7f95a379df157b31e3251168c04789b009045584d6e5254b65`.
The owned browser/project profile is retained under scratch
`A2-firefox-s73VUP/`. Both process groups were terminated/reaped;
`A2-firefox-cleanup.json` records no remaining members. No real installation,
existing browser profile, desktop default or artwork was changed.
`A2-preservation.json` confirms all 192 protected file hashes match. HEAD remains
`3d55de33d17adf2dd65606dbad171d0f1359e228`; all current changes remain unstaged.

**Current phase result: A2 / UI-01 FIXED, with controlled frontend, actual
HTTP/disk and actual graphical evidence.** Next is B4: the canonical-backed
claim inventory and all 11 previously unestablished witnesses. B5 and final
integrated/export/exact candidate/desktop acceptance remain required.

## 2026-09-12 continuation — B4 ten-witness checkpoint, not phase closure

HEAD remains `3d55de33d17adf2dd65606dbad171d0f1359e228`, branch `main`.
The approved sequential implementation continues. All edits remain unstaged.
`LCL_CONFORMANCE_OBLIGATIONS.md` now records the canonical basis, required source
and semantic evidence families, exact-subprobe accounting requirements and the
eleven previously unestablished witnesses. The verified obligation constructor,
complete source-case population and replacement claim logic are **still open**.

The following ten witnesses now have passing concrete probes in the shared
production/test runner: CLOSURE-004, 006, 021, 022, 023, 024, 027, 055, 058 and
060. CLOSURE-059 remains unestablished; its old descriptive explanation is not
accepted as justification and the approved retry-evidence repair remains next.

- CLOSURE-004 exposed an implementation defect: the runtime returned declaration
  field source spelling as STRING although the checker assigned PATH. Runtime
  metadata evaluation now evaluates the selected expression in its declaring
  source context, preserving its value type without reading the unbound OUTPUT.
  The witness checks both the PATH metadata and the still-MISSING output value.
- CLOSURE-006 compares same and different stored reference identities while the
  referents have equal INTEGER values. The existing engine passes.
- CLOSURE-021 groups three typed OBJECT declarations through ordinary value
  references into an a/b/a LIST, and checks group keys, group order, duplicate
  occurrences and member order. The existing engine passes.
- CLOSURE-055 executes legal AFTER and rejects reversed BEFORE ordering.
- CLOSURE-058 exposed absent static loop-output scope validation. The resolver's
  candidate-graph pass now checks value reads against producer loop paths.
  Seven focused tests cover outside assertions/material values/defaults,
  metadata/reference identity exemptions, operator context boundaries, nested
  parameters, EXECUTE exports, conditions and same-iteration reads. Conditions,
  nested parameters and EXECUTE exports were initially missed by the first
  implementation; those discriminating tests were red and now pass.
- CLOSURE-022/023/024 use a bounded host read fixture and the actual standard
  library: one failure then success gives exactly two attempts; WHEN FALSE
  leaves one failed attempt and an unbound output; three failures exhaust LIMIT
  2 with exactly three attempts. Host permission restricts the fixture to its
  exact PATH and operation; no fixture process or network service is used.
- CLOSURE-027 observes recovery, retained original diagnostic, successor
  invocation and output publication. Its absent-successor control refuses with
  error.execution.order under the canonical core.continue contract.
- CLOSURE-060 imports a complete valid kind.task document with targetless VERIFY
  FALSE. Import alone leaves it inactive; an explicit prerequisite selects it.
  A kind.library fixture was correctly rejected because that kind forbids
  VERIFY; the corrected fixture preserves the witness's actual requirement.

The runner now accepts source bytes, a provider and typed invocation data through
one shared entry point and records actual invocation/attempt/result/event
evidence. Its expectations can require every assertion, exact attempt sequences,
retained diagnostic presence/absence, recovery, result fields and publications.
Eleven instrument tests cover raw invalid UTF-8, imports, supplied data and exact
attempt accounting. These tests remain instrument evidence, not additional
language witnesses. Both decision_witnesses and m8_conformance_report now call
the same probe executor and fixture setup.

Fresh current-toolchain gates (rustc/cargo 1.98.1, offline; each actual exit 0):

| Log stem in owned scratch `logs/` | Command / evidence |
| --- | --- |
| B4-partial-clippy | `cargo clippy --offline -p lcl-resolver -p lcl-runtime -p lcl-conformance --all-targets -- -D warnings` |
| B4-partial-tests | `cargo test --offline -p lcl-resolver -p lcl-runtime -p lcl-conformance --all-targets`: 30 suites, 357 passed, 0 failed/ignored |
| B4-output-contexts-typed | `cargo test --offline -p lcl-resolver --test output_instances -- --nocapture`: 7 passed |
| B4-runner-input-evidence-valid-default | `cargo test --offline -p lcl-conformance --test runner_seam`: 11 passed |
| B4-ten-witness-report | `cargo run --offline -p lcl-conformance --example m8_conformance_report`: 80 executed probes, 80 passed, 0 failed; 65/66 unique witnesses |

All full logs, actual `.exit` files, and failed predecessors remain under
`/mnt/F/.lcl-residual-repair-01-lxd8dwu8/logs/`. The new, nonoverwritten
`B4-ten-witness-checkpoint.json` records every B4 gate exit, test totals and log
hash. It also verifies that all 192 protected canonical/artwork/historical
release files still match the original preservation inventory.

Failed predecessors are not acceptance evidence. They include the original
metadata and output-scope reds; malformed test syntax corrected against the
canonical grammar/field registry; one edit attempted from the wrong directory
that changed no file; a Rust reference-list inference error; fixture integer
constructor/type mistakes; and the supplied-value fixture's initially wrong
precedence expectation. The latter now retains a discriminating control proving
explicit VALUE wins, followed by an optional DEFAULT case proving supplied data
wins over DEFAULT, exactly as 05_SEMANTICS/06 requires. No language expectation
was weakened to accommodate an implementation defect.

**B4 remains IN PROGRESS.** The report's current `source_conforming` line is
generated by the old category-hit accounting and is not accepted as proof of
source completeness. The required inventory/claim repair, remaining CLOSURE-059,
final formatting and actual Rust 1.75 phase/integrated verification remain open.
B5 has not begun and no new final candidate has been built or tested.


## 2026-09-12 continuation — all decision witnesses executable; B4 accounting open

The shared production/test runner now executes 83 passing probes covering all 66
canonical decision witnesses. CLOSURE-059 adds three bounded partial-command
cases: successful proven retry, absent-proof refusal, and exhausted final partial
attempt. They assert initial MISSING bindings, exact attempt order, retained
partial stdout, accepted proof count, final bound/partial publication and correct
exhaustion presence/absence. No remaining decision-witness ID is descriptive.
This closes the previously missing eleven witnesses, not the full B4 obligation
or conformance-accounting requirement.

Two demonstrated retry-path defects were repaired. Host retry evidence is now
queried and checked against the exact prior request/result, original authority,
scope, target and parameters. Missing/unknown/proved-unsafe evidence maps to the
canonical required.missing/value.unknown/operation.precondition diagnostics.
Mismatched or incomplete proof cannot authorize another attempt. Indeterminate
state requires concrete reconciliation. Accepted proof is retained separately;
prior attempt results are unchanged, and original/resolved requests are guarded
at the next dispatch. Existing hosts supply no retry proof by default.

An incomplete native command stream was incorrectly marked fully bound. Runtime
projection now applies the schema's partial-output policy to incomplete stdout
and stderr; a projection mixing an incomplete stream with a non-partial field
stays unbound. Pre-effect failures cannot bind output. Full completed projections
retain ordinary binding. The CLOSURE-059 red exposed this rather than weakening
its expectation.

Fresh current-toolchain (Rust 1.98.1) gates, logs under the same owned `logs/`:

| Log stem | Actual exit | Result |
| --- | --- | --- |
| B4-retry-proof-matrix-before | 101 | Intentional red: two tests failed, 24 passed; host evidence never queried |
| B4-retry-proof-handler | 0 | 26 failure-handling tests passed |
| B4-retry-proof-reconciliation | 0 | 27 passed; exact proof, reconciliation and fourteen refusal controls |
| B4-retry-observations | 0 | 11 runner instrument tests passed |
| B4-059-partial-command | 101 | Fixture compile error: Decimal module path; corrected against existing API |
| B4-059-command-decimal | 101 | Runtime defect: incomplete streams incorrectly fully bound |
| B4-059-partial-projection | 0 | Full semantic_case_execution gate, including all three new cases |
| B4-all-witness-runtime-gate | 0 | `cargo test --offline -p lcl-runtime -p lcl-stdlib -p lcl-conformance -p lcl-completion --all-targets`: 38 suites, 433 passed, 0 failed/ignored |
| B4-all-witness-clippy | 0 | Same four crates, `cargo clippy --offline ... --all-targets -- -D warnings` |
| B4-66-witness-report | 0 | Production m8_conformance_report: 83 probes passed, 0 failed, 66/66 witnesses |

All 192 protected files still match the starting inventory at this checkpoint.
The production report still uses the old category/prefix accounting; its claim
line is NOT accepted as source or semantic completeness. Next: version-bound
complete obligation inventory, source/semantic evidence population and exact
sub-probe accounting, followed by B5 and final integrated/candidate verification.
Formatting and Rust 1.75 verification of the B4 changes remain pending.

## 2026-09-12 continuation — verified source obligations and claim accounting

B4 accounting now uses the embedded, digest-pinned 0.1.0 inventory bound to the
approved canonical package identity: 980 obligations requiring 2,413 exact probe
IDs. The caller cannot construct a shortened authoritative inventory. All required
sub-probes must pass exactly once; missing, failed, duplicate, irrelevant, empty
input and falsely supplied verdict records cannot fill the tested gaps. Legacy
caller-count reports are descriptive only. Synthetic accounting tests remain
instrument tests, never engine conformance evidence.

The actual shared source runner executes all 2,011 source probes: all canonical
fixtures and examples, exact/case keyword checks, admitted/excluded symbols, all
registered blocks and parent contexts, all 334 field uses with every admitted
form and absence/cardinality/invalid-form controls, plus the grammar families.
A faithful local SCHEMA fixture exposed a parser defect: its nested FIELD bodies
were processed as object data. The parser now validates local FIELD declarations
against their registered schema; eleven document-structure tests pass including
legal local fields and forbidden/malformed controls. The earlier multiline-string
fixture failures were corrected against canonical content indentation.

Fresh logs and actual exits in the owned logs directory:
- B4-obligation-inventory, B4-claim-obligations, B4-verified-witness-report: 0.
- B4-local-schema-before: 101 (intentional red); B4-local-schema-after: 0.
- B4-complete-source-matrix: 0, all 2,011 source probes pass.
- B4-source-production-report: 0, 2,094 executed/passed, zero failed, all 66
  witnesses established, no missing source probes, 319 missing semantic probes.
  The report now supports source_conforming under the complete source inventory.
- B4-source-phase-tests: 0, parser and conformance all-target gate.

All 192 protected files match the starting inventory. B4 remains incomplete:
the 319 additional semantic obligations require concrete executable evidence;
formatting, actual Rust 1.75 and final integrated gates remain pending. B5 and
the exact final candidate verification have not started. No Git closure occurred.

## 2026-09-12 continuation — additional semantic cases and two checker repairs

The additional semantic population is in progress, not yet included in the
production report. `tests/semantic_cases.rs` executes 191 type, operator,
function, status-transition and diagnostic-contract groups with 849 sub-runs.
`tests/operation_cases.rs` executes 117 current operation binding/error/effect
groups with 1,708 sub-runs. Each group retains ordered exact sub-inputs, expected
results and observations. Component contracts (registry metadata, lifecycle,
default application and profile selection) are explicitly distinguished from
source-through-engine executions. The 799 descriptive entries remain descriptive.

Two faithful positive cases exposed additional existing checker defects:
- Registered MEASURE times INTEGER/DECIMAL, and scalar times MEASURE, were rejected
  because result promotion discarded the measure family. The result-type branch
  now preserves the measure operand and exact unit after overload admission.
- A bare OBJECT declaration's written field types were unavailable to property
  and index selection. The checker now follows the selected schema-free field
  path and source references, preserves declaring SourceId, and returns MISSING
  for absent schema-free keys. The traversal uses an explicit cursor and rejects
  declaration cycles. Closed schemas, exact receiving types and runtime-varying
  index rejection remain covered. Nested/forwarded selection controls pass.

New gates under the existing owned logs directory (real exits):
- B4-grouped-runner: 0; grouped evidence rejects missing/failed/inputless runs.
- B4-semantic-types: 101, fixture issues; B4-semantic-types-canonical: 0.
  Corrected ITEM simple identifiers, object-body whitespace, and the exact
  collection.heterogeneous diagnostic for an incompatible declared item type.
- B4-semantic-expressions and B4-expression-fixture-controls: 101. Faithful
  measure/object reds remained; glob full-string and metadata/type fixtures were
  corrected against canonical contracts, not weakened to accept engine defects.
- B4-measure-products-fixed: 0; B4-schema-free-selection: 0.
- B4-object-selection-controls: 101 for an ambiguous dotted fixture spelling;
  B4-object-selection-grouped: 0 after parenthesizing the intermediate selector.
- B4-checker-phase: 0, full checker all-target gate.
- B4-status-error-contracts: 0, 191 groups and 849 sub-runs.
- B4-operation-binding: 101, fixture JSON number API compile mismatch;
  B4-operation-binding-json: 101, qualified format default typed as STRING in
  the oracle; B4-operation-binding-default-types: 0, 39 groups/525 sub-runs.
- B4-operation-binding-errors: 0, 78 groups/739 sub-runs.
- B4-operation-effects: 101, fixture set/slice API compile mismatch;
  B4-operation-effects-sets: 0, 117 groups/1,708 sub-runs.

The operation population includes concrete required/duplicate/unknown binding
failures, unresolved targets, declared numeric bounds, forbidden memory mutation,
exact fresh-filesystem post-states, default absence/MISSING/preserved-value cases,
resolved effect bounds, determinism modes, and required-profile missing/ambiguous/
incomplete/out-of-bounds controls. Absent analysis, reporting, generation,
conversion and package profiles are recorded as explicit precondition refusals,
not as successful capability execution. The remaining operation-specific error
clauses and capability coverage still require a completeness review.

OPEN: nine result-schema groups, diagnostic-policy and failure-lifecycle groups;
remaining overload/special-value and operation-specific sub-probe completeness;
private inventory pinning of exact semantic sub-run membership and rejection of
omitted sub-runs; production population integration; formatting, clippy, actual
Rust 1.75 and integrated gates. All 192 protected files still match. B5 and exact
final candidate verification have not started. No staging, commit or push.

## 2026-09-13 continuation — LCL-CLOSURE-4T Task LCL-CLOSE-01 entry and Phase A

New package: **LCL-CLOSURE-4T v1.1**, task **LCL-CLOSE-01** (correctness, safety
and persistence). The owner approved the presented plan with decisions D1 (safe
std-only Unix `O_NOFOLLOW | O_EXCL` for FS-01, no unsafe FFI or parent walking),
D2 (verify the canonical timeout contract first and stop if it is silent),
D3 (per-path publication ordering plus an optional baseline-digest precondition),
D4 (probe Q-JSON before proposing any repair) and D5 (new owned disk-backed
scratch; the residual scratch is reused read-only for the Rust 1.75.0 toolchain).

Root `/mnt/F/LCL`, branch `main`, HEAD `7f13aecd4af639f1373bb1180cb50f9b508cfb05`
— the same commit the packaged audit used as its historical baseline. The
worktree was **clean** at entry: the B4 work the previous continuation describes
was committed at 18:57 on 2026-09-12 as `LCL final 0.1`, so nothing was pending
in the working tree and nothing was recovered or reset. Upstream is recorded as
`origin/main`; no fetch, staging, commit or push occurred.

Owned scratch: `/mnt/F/.lcl-closure-4t-4c1cd4c659b7` (mode 700, ext2, disk-backed, 136 GB free).
`TMPDIR` and the Cargo target directories are inside it. `/tmp` is tmpfs on this
machine and is therefore excluded from build and test artifacts. The residual
scratch `/mnt/F/.lcl-residual-repair-01-lxd8dwu8` was read only, for its verified
private Rust 1.75.0 toolchain (`rustc 1.75.0 (82e1608df 2023-12-21)`,
`cargo 1.75.0 (1d8b05cdd 2023-11-20)`) and its protected-file inventory.
System toolchain: rustc/cargo 1.98.1. Python 3.14.7. Node v26.8.2.

All **192** protected canonical, brand and previous-release files were re-hashed
at entry and match the residual repair's recorded inventory exactly.

### Entry baseline, with two pre-existing failures recorded

| Gate | Actual exit | Result |
| --- | --- | --- |
| `cargo fmt --all -- --check` | 1 | **FAIL, pre-existing.** 23 files unformatted |
| `cargo check --offline --locked --workspace --all-targets` | 0 | PASS |
| `cargo clippy --offline --locked --workspace --all-targets -- -D warnings` | 101 | **FAIL, pre-existing.** One `clippy::type_complexity` at `lcl-conformance/tests/semantic_cases.rs:35` |
| `cargo clippy` over the seven crates this task changes | 0 | PASS — the valid lint baseline for this task |
| `cargo test --offline --locked --workspace --all-targets` | 101 | FAIL — one stale test oracle, closed below |
| the same, after that closure | 0 | 139 suites, 1416 passed, 0 failed, 0 ignored |
| actual Rust 1.75.0 `cargo check ... --workspace --all-targets` | 0 | PASS |
| actual Rust 1.75.0 `cargo test ... --workspace --all-targets` | 0 | 139 suites, 1416 passed, 0 failed, 0 ignored |
| canonical `validate_release.py --scope all` | 0 | 31 PASS, 2 OUT_OF_SCOPE, 0 FAIL/BLOCKED |
| canonical `sha256sum -c SHA256SUMS.txt` | 0 | 175 matching entries |
| `sha256sum -c assets/brand/BRAND_ASSETS.sha256` | 0 | 17 matching entries |

Canonical package identity was verified by the implementation's own algorithm,
not by a guessed concatenation: `lcl-spec`'s `approved_package_matches_the_anchor`
and `identity_digest_is_reproducible` executed and passed inside the green
baseline suite, binding `compute_identity_digest` over the real canonical root to
`00d648b162939d06c44838481a67c39bc12c64bdd6d105035c24150148fe67ed`.

**The two failing workspace-wide gates are pre-existing and belong to B4, which
is LCL-CLOSE-02's item, not this task's.** Every one of the 23 unformatted files
was touched by the two B4 commits (`0ff51b7..7f13aec`) and no file outside them
is unformatted; the single clippy error is in a test file the last continuation
added. That continuation already records "formatting, clippy, actual Rust 1.75
and integrated gates" as open. They are not mass-formatted or lint-patched here:
the working rule confines formatting changes to the file currently being edited,
and those files are outside this task's approved scope. Every file this task
edits is format-checked individually. Both gates are carried into LCL-CLOSE-02's
entry state and must close there before any completeness claim.

### Entry baseline closure — one stale test oracle

`lcl-protocol/tests/engine_stages.rs::every_valid_example_reaches_completion`
asserted that exactly **7** canonical valid examples reach `status.succeeded`.
Eight now do. This was diagnosed read-only before anything else was edited.

A control build of the previous revision `2a20cc0` was extracted into owned
scratch (`git archive`, never a checkout or reset) and the existing
`lcl-completion --example m8_report` was run against both trees over the same
unchanged canonical package. Exactly one example differs:
`03_IMPORTING_TASK.lcl`, `status.failed` at 2a20cc0 versus `status.succeeded`
now; the other twelve are identical, and the current production M8 report itself
counts eight. Logs `C01-diag-control-2a20cc0-m8` and
`C01-diag-m8-example-statuses`, both exit 0.

The cause is the CLOSURE-004 repair this report already records. That example's
ACTION binds `destination` to `REF(output.copy).TARGET`. `05_SEMANTICS/01`:
"A metadata read such as REF(output.copy).TARGET reads the declaration field
without requiring the OUTPUT's result binding." The runtime used to answer that
read with the field's *source spelling* as a STRING, so `core.copy` received a
STRING where its contract requires a PATH and refused with
`error.operation.precondition`; the example failed, and `verify.copy` was FALSE.
Evaluating the selected field in its declaring source context restored its
declared PATH type, so a valid canonical example whose SUCCESS is satisfiable
now succeeds. The count was stale, not the engine.

The oracle was corrected to assert the exact **named set** of succeeding
examples rather than a count, because a count cannot distinguish a repaired
example from a regressed one — which is precisely how this went unnoticed: no
full workspace test ran after B0, and `lcl-protocol` was in none of B4's scoped
runs. No assertion was weakened and no expectation was changed to match
implementation output; the primary assertions, that every example reaches
completion with exactly one terminal status, are untouched.
Gate `C01-baseline-oracle-fixed`, exit 0, 21 passed.

### Phase A — denial and cancellation cannot be lost

**RUN-02 — FIXED_VERIFIED.** Reproduced first, in
`lcl-workspace/tests/debugging.rs`, over the real loopback product with an
actual event stream. With operation pauses on, effect pauses off, an otherwise
valid and authorized `core.write`, and the operator answering **deny**, the
denial was dropped: `WatchedOperations::invoke` returned `Resolution::host` with
no record of the refusal, and `WatchedHost::permits` — not pausing, because
effect breaks were off — consulted the real host. Gate `A-run02-before-corrected`,
exit 101, preserved three distinct failures:

- the run reported `error.host.constraint`, a host *limitation*, instead of the
  operator's registered refusal. The effect did not occur only because the
  unresolved request failed a downstream grant check — an unrelated downstream
  failure, not the denial being honoured;
- a denied **pure** row (`core.calculate`) was likewise handed to a host that
  never had that capability, producing `error.host.constraint`;
- with both breaks on, no report ever arrived: the operator was asked a **second**
  time at the effect boundary, so one denial did not answer the invocation.

The repair records the refusal against the exact `(InvocationId, operation)` and
consults it at the host gate, which is where a refusal already has a registered
meaning. `permits` returns `Permission::Denied` with the operator's reason and
`invoke` is never reached; `invoke` carries the same guard for any caller that
skips the gate. A retry is a different attempt index and a loop pass a different
iteration path, so a recorded refusal binds one invocation and never a later
decision. Gate `A-run02-after-formatted`, exit 0, 13 passed, including the
pre-existing effect-deny, cancel, stale-answer and consent-cannot-open-the-gate
controls and new controls proving `continue` still performs the effect exactly
once and that an unrelated later run is unaffected.

One correction is recorded rather than concealed: the first version of the new
pure-operation assertion read `report.status`, which is not in the report
schema, and panicked in the test helper. The terminal status is
`report.completion.terminal_status`. That was a mistake in the new test, not an
observation about the product, and the reproduction was rerun after correcting
it (`A-run02-before` then `A-run02-before-corrected`, both exit 101).

**RUN-01 — FIXED_VERIFIED.** `Session::hold` checked cancellation, released the
lock, and only then registered the pause and cleared the answer field. Six unit
regressions in `execution.rs` place a cancellation at each point the run thread
and an answering thread can meet, using a deterministic barrier or a test seam
that is `#[cfg(test)]` only and compiled out of the product, each bounded by a
five-second external watchdog so a lost cancellation fails instead of hanging.
Gate `A-run01-before`, exit 101, recorded two deterministic failures:

- a cancellation landing in the check/register window returned `Err(Timeout)`:
  the reset of the answer field discarded the `Cancel` the operator had already
  given, and the run parked forever;
- a hold outstanding when `finish` ran also parked, because the wait condition
  watched only the answer field and not the run's terminal state.

Cancellation before registration, during waiting, after a resume, and an
ordinary answer were already correct and are retained as controls.

The repair makes the liveness check and the pause registration one critical
section, so a cancellation either precedes the section and is seen by it or
follows it and finds a registered pause; there is no third interleaving. Waiting
now observes `cancelled` and `finished` as well as the answer, a cancelled run
answers `Cancel` whatever the answer field holds, and the pause is cleared on
every exit path so no later answer can hit it. The sequence number is still
allocated only when a pause is actually registered.
Gate `A-run01-after`, exit 0, 8 passed.

Phase A closure: `cargo test --offline --locked -p lcl-workspace --all-targets`
exit 0, 11 suites, 98 passed, 0 failed, 0 ignored (`A-phase-closure-workspace`);
`cargo clippy --offline --locked -p lcl-workspace --all-targets -- -D warnings`
exit 0 (`A-phase-closure-clippy`). `rustfmt --check` passes on every edited file.
All 192 protected files still match. Phases B to G remain open.

### Phase B — filesystem confinement, bounded I/O and effect truth

**FS-01 — FIXED_VERIFIED.** Reproduced against the real adapter on owned
directories in the new `lcl-capabilities/tests/real_filesystem.rs`. Gate
`B-fs-before` (exit 101) and `B-fs-races-before` (exit 101) recorded six
failures, three of which are unauthorized effects rather than wrong labels:

- a `core.create`-mode write through a **dangling** symbolic link inside the
  grant **actually created its target outside every granted scope**. The link
  resolves to nothing, so `canonicalize` failed, the path was judged by the
  link's own location — which is inside — and the open then followed the link
  out. The failing assertion names the escaped file;
- a replacing write through the same link escaped identically;
- two racing creates with overwrite disabled both returned `Ok(())`, and so did
  two racing copies and two racing renames. A prior existence check admits every
  writer that passes it, so the second silently destroyed the first.

The repair, within decision D1 — safe standard library only, no unsafe FFI and
no directory-relative syscalls:

- `resolve` now follows a dangling final link to the destination it names, so
  containment is judged against where the bytes would actually go;
- every open of a resolved path passes `O_NOFOLLOW`, closing the window in which
  another process swaps the final component for a link after the decision. The
  flag's value is part of each platform's ABI, so it is stated per target and is
  zero where it is not known, which is recorded rather than guessed;
- `Create` is reserved with `create_new` — one atomic step that cannot replace a
  file and cannot be satisfied by an existing link, dangling or not;
- a non-overwriting copy reserves its destination with `create_new` before any
  byte moves; a non-overwriting rename reserves with a same-directory hard link
  and unlinks the source only once the destination is in place, the same
  primitive already used for atomic document publication.

**The remaining limit is stated, not implied**: nothing here defends against
another process replacing a *directory* along the path between resolution and
the operation, and a non-overwriting **directory** move still rests on its
preceding check because a directory cannot be hard-linked. Both are documented
in the module itself.

**FS-02 — FIXED_VERIFIED.** The cap was applied to `metadata.len()` and the read
was then unbounded. Reproduced with a file whose reported size is not its
content: a zero-byte bound over `/proc/self/cmdline` returned 95 bytes. The read
now takes at most one byte past the cap from the open file and checks the bytes
actually collected, so the bound constrains what this process retains. Empty,
exact-cap and cap-plus-one cases are controls.

Gate `B-fs-after-2`, exit 0, 16 passed. Consumers unaffected: `B-fs-consumers`
(`lcl-capabilities`, `lcl-stdlib`, `lcl-cli`), exit 0, 232 passed.

**EF-01 — FIXED_VERIFIED.** Reproduced through the real `HostAdapter` into the
runtime's own result record. A `core.write` to `/dev/full` — which accepts an
open and refuses every write with `ENOSPC`, and is used precisely so that no
filesystem is filled and no user data is touched — produced:

```
ResultRecord { status: "status.failed", output_binding: Unbound,
  execution_errors: ["error.execution.action"],
  failure_phase: PreEffect, effect_state: None, observed_effects: [] }
```

The adapter had already opened the target for modification, and for a replacing
write already truncated it, when the write failed. It cannot prove nothing
began, yet the record asserted exactly that. `05_SEMANTICS/09` permits the
effect-free claim only for a failure positively established as pre-effect, and
`Observation::none()` makes that claim explicitly through `proven_effect_free`.

The adapter now distinguishes an I/O failure *before* the target was opened for
modification from one *after* it, and the standard library maps the latter to a
failure carrying an observed filesystem effect in the `indeterminate` state,
preserving any effect already recorded — a completed network transfer before a
failed local write keeps both. The extent is indeterminate rather than partial
because `write_all` does not report how far it got; claiming `partial` would be
as invented as claiming `none`. Gate `B-ef01-after-2`, exit 0, 23 passed.

Two controls hold the other half of the contract and both passed before and
after: a grant refusal is still `PreEffect` with `effect_state: None` and writes
nothing, and an ordinary granted write still completes with its content on disk.

Three authoring corrections in the new tests are recorded rather than concealed:
an assertion called a `ResultRecord` method that does not exist and was also
vacuous for a document declaring no OUTPUT, replaced by asserting the observed
effects the record actually carries; a fixture placed its `ALLOW` block before
the `LCL:` header and was rejected with `error.block.context`, corrected by
passing it as a declaration to the existing task helper; and a bounded read used
`Read::take` without the trait in scope. All three were mistakes in new test or
adapter code, not observations about the product.

Phase B closure: `cargo test --offline --locked -p lcl-capabilities -p lcl-stdlib
-p lcl-runtime -p lcl-cli -p lcl-workspace --all-targets` exit 0, 553 passed, 0
failed, 0 ignored (`B-phase-closure`); clippy over the three changed crates with
`-D warnings` exit 0 (`B-phase-closure-clippy`); `rustfmt --check` passes on every
edited file; all 192 protected files still match.

### Phase C — correct, bounded transport and guarded ingress

**NET-01 — FIXED_VERIFIED.** Reproduced in the new
`lcl-capabilities/tests/transport.rs`, each case one real HTTP/1.1 response over
a real bounded loopback listener. Gate `C-net-before`, exit 101:

- a chunked response returned its body as
  `5\r\nhello\r\n6\r\n world\r\n0\r\n\r\n` — **the chunk-size delimiters were
  the downloaded content**, which C06 forbids in those words;
- a response declaring eleven bytes and delivering five returned the five as a
  completed transfer;
- an unterminated chunked response returned what had arrived;
- a response declaring **both** `Content-Length` and `Transfer-Encoding` was
  accepted, the disagreement resolved silently;
- `Transfer-Encoding: gzip, chunked` was ignored rather than reported.

The repair implements RFC 9112 §6.3 for determining the body length, §7.1 for
decoding the chunked coding including chunk extensions and the trailer section,
and §8 for incomplete messages. Both framings present is an error rather than a
choice; disagreeing `Content-Length` values are an error; a coding this
std-only transport does not implement is reported instead of passed through;
and a close-delimited response is still read to the close, which is legal for a
response. Gate `C-net01-closure`, exit 0. Controls for an ordinary length, a
zero-length body, a close-delimited body and a non-2xx status all pass.

**Q-NETDOMAIN — REPRODUCED, then FIXED_VERIFIED. It was a defect, not only a
review question.** `core.download` against a `404` produced:

```
ResultRecord { schema: "result.transfer", status: "status.succeeded",
  execution_errors: [], failure_phase: None, effect_state: Applied,
  observed_effects: [Network Applied, Filesystem Applied],
  fields: { bytes: 23, destination: PATH(...), source: URI(...) } }
```

— a completed transfer of a 23-byte HTML error page, written to the
destination. The same held for `500` and for a `302` whose body is a note about
somewhere else.

The repair is taken from the row, not invented.
`operations_v0.1.0.json#/contracts/core.download` registers the precondition
"source is accessible" and the postcondition "destination bytes equal received
source"; `statuses_and_errors_v0.1.0.json` defines `error.operation.precondition`
as "a registered operation precondition ... is false, missing, or unknown" and
places it where "an immediate operation precondition fails **before** that
operation's effects". A non-2xx status says the source was not accessible, so
the row's own identifier is refused before anything is written — the same shape
as `core.read`'s range, which the outcome type already documents as a contract
decided "over a representation already read without effect".

For `core.upload` to a URI destination the content has already been sent, so a
non-2xx is not a precondition but an unestablished postcondition: the registry
admits exactly that — "a postcondition may fail ... after known effects, or when
the effect extent cannot be established" — and the network effect is recorded
as `indeterminate` rather than dropped. Gate `C-qnetdomain-after`, exit 0, 27
passed, including the control that a `200` still writes exactly the received
content.

**WS-01 — FIXED_VERIFIED.** Reproduced against the real product server. Gate
`C-ws01-before`, exit 101, five failures, three of which are security gates
being satisfied by being offered a choice:

- two disagreeing `Content-Length` lines were accepted, the **longer** believed,
  and the echoed body came back as `{}GET /x\r\n\r\n` — the next request's bytes
  swallowed into this one's body. This server's own module documentation says a
  request whose framing is ambiguous is refused rather than guessed at;
- `Host: evil.example` **and** the real address returned **200**. That gate is
  the defence against DNS rebinding;
- two `Origin` lines, one hostile, returned **200**;
- a connection that sent nothing was still held after 20 seconds;
- sixty-four silent connections locked the owner out: their own authenticated
  request was reset.

The repair refuses a repeated `Host`, `Origin`, `Content-Length`,
`Transfer-Encoding` or `X-LCL-Token` as malformed while the request is being
read — earlier and stricter than the gate — and leaves genuinely list-valued
fields alone. Separately, an unfinished request now has a ten-second ingress
bound, which is lifted the moment a complete request passes all three gates, so
an authenticated event stream stays connected exactly as a debugging session
needs. Gate `C-ws01-after`, exit 0, 20 passed, including the control proving an
authenticated stream survives past the ingress bound and the workspace keeps
serving.

One oracle correction is recorded: the duplicate-`Host` and duplicate-`Origin`
cases first expected 403, the gate's status. The request is refused at parse
time with 400 and the exact reason, which is earlier and stricter; the
assertions were corrected to the stage that actually applies, and now also
assert the refusal text. The saturation case was likewise corrected from
"never refused" to "recovers": while silent peers hold the slots the owner is
refused, and that is the ceiling working; what the repair must guarantee is
that the lockout **ends** without anyone restarting the workspace.

**NET-02 — REPRODUCED; repair deliberately NOT attempted. Owner decision
required.** A server answering every read inside a 300 ms declared bound took
**2.003 s** and completed successfully. `Deadline` reaches the socket only as
`set_read_timeout`/`set_write_timeout`, which are per-read; `TcpStream::connect`
takes no timeout at all, so resolution and connection are unbounded even when a
deadline is supplied.

The authority was checked before touching anything, as decision D2 required:
`core.execute`, `core.start` and `core.stop` each register a `timeout`
parameter, and the process adapter honours those. **`core.download` and
`core.upload` register no timeout parameter at all**, and no registry defines a
network deadline. Canonical is therefore *silent* on whether a supplied bound is
a total budget across resolution, connection, send and receive, or a per-phase
limit — and choosing would be inventing a public timeout policy. Under D2 this
stops at that boundary.

The evidence is preserved as an explicitly labelled EXPECTED_REPRODUCTION,
`expected_reproduction_net02_a_declared_bound_does_not_cover_the_whole_transfer`,
which asserts the known-bad behaviour and says in its own documentation that a
passing run of it is **not** a product acceptance pass. It fails the moment the
bound begins to cover the transfer, at which point it must be replaced by the
acceptance test for whichever policy is chosen.

Phase C closure: `cargo test --offline --locked -p lcl-capabilities -p lcl-stdlib
-p lcl-workspace -p lcl-runtime -p lcl-cli --all-targets` exit 0, 573 passed, 0
failed, 0 ignored (`C-phase-closure`); clippy over the three changed crates exit
0 (`C-phase-closure-clippy`); `rustfmt --check` passes on every edited file.

### Phase D — authenticate the same specification bytes that are parsed

**SPEC-01 — FIXED_VERIFIED.** The loader read the package twice. It read and
parsed the manifest, the checksums, all twelve registries and both catalogs,
and only then walked the tree again to hash it. The hashes that decided trust
therefore described the *second* read, while the object handed to every later
layer held the first.

Reproduced deterministically on an owned copy of the canonical package, using a
`#[cfg(test)]`-only seam that runs in exactly that window — the same technique
as RUN-01, and compiled out of the product. A registry was left holding content
that is not the release's, and restored to the release's own bytes at the moment
parsing finished. Gate `D-spec01-before`, exit 101:

> the package reports itself verified and authoritative while holding a registry
> that was never verified

`SpecPackage::open_with_anchor` returned **Ok**. The package matched its
manifest, its checksums, its declared counts and the external trust anchor
`00d648b1…67ed` in every particular, and its in-memory `types` registry was
`{"types":{},"note":"not the released registry"}`. The anchor could not see it,
because by the time the anchor looked the evidence had been put back.

The repair reads every file exactly once into one capture, and derives
everything from it: the per-file hashes, the identity digest, the manifest, the
checksums, every registry and every catalog. Nothing reads the package a second
time, so there is no second state for the hashes to describe. The two helpers
that performed the separate read are removed rather than left available.

Existing behaviour is preserved deliberately rather than incidentally: a file
absent from the capture still produces the same `SpecError::Io` a direct read
produced, so `detects_missing_file`'s fail-closed distinction between a missing
file and a malformed one still holds and that test needed no change. Malformed
JSON, duplicate checksum records, unsafe paths, symlinks, inventory closure,
count mismatches, the forged-but-internally-consistent package and the
`OUT_OF_SCOPE` classifications are all unchanged.

Controls: an untouched owned copy still opens, is authoritative and still
matches the anchor; a genuinely altered copy is still rejected. Gates
`D-spec01-closure` exit 0 and `D-spec01-clippy` exit 0. The real engine still
opens the unchanged canonical package: `D-spec-consumers` over `lcl-protocol`,
`lcl-cli` and `lcl-checker`, exit 0, 230 passed. All 192 protected files match.

### NET-02 — closed under the owner's decision

The owner chose the total-budget reading: one declared bound covers resolution,
connection, send and receive, with remaining-time accounting. That policy is now
stated once, in `net.rs`, together with the fact that canonical does not decide
it and why this layer therefore may not decide it silently.

A `Budget` is taken at the start of the exchange and spent by every phase. Each
socket wait is given what is left rather than the whole bound, so a peer cannot
stay inside a per-read limit indefinitely and still be inside the bound it was
given. Resolution and connection run on their own thread with a bounded wait,
because `ToSocketAddrs` and `TcpStream::connect` have no timeout in the standard
library; the worker owns nothing but its own attempt, its send fails silently
into a dropped receiver, and it closes any socket it opened. With no declared
deadline the budget is a finite thirty seconds rather than unbounded.

The EXPECTED_REPRODUCTION was replaced by the acceptance test its own
documentation demanded. Gate `C-net02-after`, exit 0, 12 passed: the trickle is
now stopped as a host limitation near the bound instead of completing after
2.003 s; an address that never answers is stopped by the bound instead of
waiting on the operating system; and a prompt exchange well inside its bound
still completes, which a budget that stopped everything would fail. The whole
transport suite now runs in 0.50 s rather than 2.01 s, because the trickle stops.
Consumers unaffected: `C-net02-consumers`, exit 0, 192 passed;
`C-net02-clippy`, exit 0.

### Phase E — editor read/write ownership survives asynchronous work

**UI-02 — FIXED_VERIFIED.** `reload()` assigned the response to `doc.text` and
`doc.saved` unconditionally after awaiting it, so anything typed while the
request was out was overwritten **and marked clean**. `doc.revision` was
incremented but never checked.

Reproduced against the complete unmodified production `app.js` in the existing
Node harness, extended to hold a GET as it already held a PUT, and with a
bounded `until` helper that re-checks after everything else has run — so a
condition that is briefly true and then clobbered fails rather than passes.
Gate `E-ui02-before`, exit 101, two failures:

- reload from a clean buffer, typing while the GET is held: the typed text was
  gone;
- a confirmed discard followed by more typing: "held briefly and was then
  undone", the guard catching the overwrite as it happened.

The repair captures the revision the reload is answering for and checks it at
the actual update point. A response that finds the document closed and reopened
returns without touching the new one; a response that finds newer edits still
records what is on disk as the saved baseline — which it now knows — leaves the
edits in place, and says so with a warning rather than silently. Only a reload
that still matches what it was asked about replaces the buffer.

Gate `E-ui02-closure`, exit 0, 15 cases passed through `editor_save.rs`, which
runs the **real product server over real HTTP with disk assertions**, including
all eleven pre-existing UI-01 cases and a new control proving a reload of an
inactive tab does reload it without touching the active tab's textarea.

The wrapper's `output.contains("11 passed; 0 failed; 0 skipped")` was the same
stale-count pattern as the entry baseline oracle. It now asserts that nothing
failed and nothing was skipped, and that the suite has not shrunk below the
coverage it had — so adding a case cannot break the gate and losing one must.

**UI-03 — FIXED_VERIFIED.** The publication boundary was an unconditional
rename with no ordering at all. Reproduced at that boundary, with the exact
interleaving the finding names: an older write held **before its bytes are
published**, the document closed and reopened, a newer save accepted, published
and acknowledged, and only then the older write released. With ordering disabled
as a discrimination check, gate `E-ui03-before-discrimination`, exit 101:

> the acknowledged newer content must still be on disk
> left: "older text\n"   right: "newer text\n"

Under decision D3 the repair is per-path publication ordering. Each write takes
a number when it is **accepted**, and publication happens under one lock that
refuses a number older than the one already published for that path; the check
and the publication are the same critical section, because a check that releases
its lock before publishing is the defect again with more steps. A superseded
write is told so — `DocumentError::Superseded`, HTTP 409 — rather than reporting
a success it did not have.

Two boundaries are deliberate. Ordering governs **replacing saves only**: a
create is a reservation, not newer content, and B2's atomic create-only
publication must keep refusing with `AlreadyExists` rather than "too late",
which the preserved unit test asserts. And ordering is **not** a disk-content
precondition, because the task is explicit that later external edits must not be
conflated with the stale in-flight ordering being repaired; the optional
precondition D3 allows therefore stays optional and the editor does not send
one, so an external edit behaves exactly as before.

Scope stated rather than implied: this orders the writes one process accepted.
Two workspace processes sharing a root do not order against each other through
it, and atomic create-only publication remains what protects them from
destroying each other's files.

One unexpected failure was diagnosed rather than retried: the first seam was a
process-global hook, and cargo runs unit tests in parallel, so another test's
write could trip it and the case passed or failed depending on scheduling. The
seam is now handed the destination and fires only for its own document. Three
consecutive full `--lib` runs pass (`E-ui03-after-repeat-1..3`).

Phase E closure: `cargo test --offline --locked -p lcl-workspace --all-targets`
exit 0, 107 passed, 0 failed, 0 ignored (`E-phase-closure`); clippy exit 0
(`E-phase-closure-clippy`); `rustfmt --check` passes on every edited file.

### Phase F — preflight cannot alter value meaning

**SEM-01 — FIXED_VERIFIED.** The preflight evaluator's own contract says its
`None` "is **not** `MISSING`" and that the caller "must leave the obligation to
the layer that demands it rather than inventing an outcome". Its caller wrote
`value.unwrap_or(Value::Missing)`, which invented exactly that outcome — and
`05_SEMANTICS/06` then admits `DEFAULT` for MISSING, so the declared value was
not merely lost but replaced.

The probes change only the grouping around one explicit input, so each is the
same value written differently, and the evaluator's budget was read from its
behaviour rather than assumed. Gate `F-before`, exit 101:

> 129 parentheses around an explicit 7 took the DEFAULT

0, 2 and 128 parentheses resolved to 7; at **129** — one past the evaluator's
own counted budget — the explicit 7 became the DEFAULT 42. Nothing in the
document was missing.

The repair distinguishes a declaration that writes no `VALUE` at all from one
whose written expression this layer could not fold. The first is MISSING, "no
value/source exists", and `DEFAULT` applies to it. The second is UNKNOWN, "value
exists but cannot be determined" — the canonical name for exactly this state —
and `DEFAULT` does not, which is the asymmetry `05_SEMANTICS/06` states and
which the pre-existing `a_default_never_replaces_unknown` already relied on. No
LCL depth restriction was invented and no stack was enlarged to hide a result.

Controls: a genuinely absent optional input still takes its DEFAULT, and every
grouping that resolves at all resolves to 7. One oracle of mine needed
correcting and is recorded: its guard excluded MISSING but not UNKNOWN, so it
demanded a value from a case that is explicitly undecided; it now excludes both
sentinels and additionally asserts there is no third answer.

**MEASURE-01 — FIXED_VERIFIED.** Preflight arithmetic returned `Integer` when
both operands were integers and `Decimal` otherwise, discarding the MEASURE
family and the exact unit. Gate `F-before`, exit 101: `5 m + 3 m` gave
`Decimal(8)`, `5 m - 3 m` gave `Decimal(2)`, `5 m * 3` gave `Decimal(15)`.

The repair takes the result family from the same rule the runtime already
applies — a quantity keeps its exact unit identifier, a percentage stays a
percentage, a byte count stays a byte count, and INTEGER promotes to DECIMAL
only when paired with DECIMAL — so the two stages agree rather than one of them
being patched for one family. Two quantities in different units produce no
arithmetic result here, because the checker already rejects that pairing with
the registered unit-mismatch diagnostic and inventing a number in the meantime
would decide a question that has an answer elsewhere.

The cross-stage claim is evidenced end to end through the real engine, not only
at the unit boundary. A fixture whose OUTPUT is `MEASURE(5, unit.meter) +
MEASURE(3, unit.meter)` and whose VERIFY asserts equality with
`MEASURE(8, unit.meter)`, run by the actual CLI against the unchanged canonical
package with an empty environment:

| | terminal status | verify.total | verify.scaled | published output | real exit |
| --- | --- | --- | --- | --- | --- |
| preflight family preservation disabled | `status.failed` | FALSE | FALSE | `'8'` | **2** |
| repaired | `status.succeeded` | TRUE | TRUE | `'8 unit.meter'` | **0** |

Two `error.verification.failed` diagnostics in the first row: the document's own
verification failed because preflight and runtime disagreed about what kind of
value it had computed. That is the cross-stage agreement on "value family,
value, exact unit" the value-fidelity contract requires, demonstrated by its
absence and then its presence. Fixture SHA-256
`112cc4ef8308e120cec4fa8b0adc0004146fb379c441c54f880a910b0a33e6aa`; the CLI is a
development binary and not a packaged candidate. Gates
`F-measure-cli-discrimination` (exit 2) and `F-measure-cli-restored` (exit 0).

Phase F closure: the **full workspace** gate, `cargo test --offline --locked
--workspace --all-targets`, exit 0, 141 suites, 1481 passed, 0 failed, 0 ignored
(`F-phase-closure`) — up from the 1416 of the entry baseline, with no suite lost.
Clippy over `lcl-semantics` exit 0. `rustfmt --check` passes on every edited file.

### Phase G — result parity and the bounded questions

**HANDLER-01 — was a verification gap; is now a FIXED_VERIFIED defect.** The
ordinary dispatch path checks a completed result against its registered schema
before anything is bound. The handler path built the record and returned it
unchecked. A field set that is closed on only one path is not closed, and this
is the worse path to leave open: the record decides whether a registered
diagnostic is kept or discarded.

Locating the evidence took a correction worth recording. The first oracle looked
for the handler's result in `execution.invocations()`, where it does not appear,
so three malformed cases "passed" while proving nothing — and the well-formed
control caught it by failing. The answer lives in the event disposition:
`Disposition::Selected { recovered }` is "true exactly when the handler
invocation ... recorded `status.succeeded`". Rewritten against that, gate
`G-handler01-before-4`, exit 101, showed all three malformed claims recovering
the diagnostic:

- a completed `result.value` with **no fields at all**, where the schema
  requires `value` for a success;
- one carrying `exit_code`, which that schema forbids;
- one whose `evidence` is a STRING where `LIST[REFERENCE[EVIDENCE]]` is
  registered.

Each produced the identical `handler.read recovered it`.

The repair runs the same `schema_violations` check the ordinary path runs, and
on violation records the registered `error.host.constraint`, its default status,
and the failure phase and effect state derived from the observation — so a
malformed claim of success neither recovers the failure it was called for nor
takes the diagnostic with it. Gate `G-handler01-after-7`, exit 0, 5 passed,
including two controls: a well-formed handler result still recovers, and the
ordinary dispatch path already refused the same malformed success, which is what
makes the asymmetry evidence rather than assertion.

Two fixture corrections are recorded: the "well-formed" claim initially omitted
`evidence`, which `result.value` registers `exactly_one` — so it was not
well-formed, and the control was right to reject it; and a "wrong family" claim
used an INTEGER for `value`, which is typed `meta.material_value` and admits
one. Both were mistakes in the fixture, corrected against the registry.

**Q-JSON — REPRODUCED, then FIXED_VERIFIED.** Probed first, as the decision
required. `lcl.project.json` is read from the user's own project directory, so
its bytes are ordinary input, and the JSON reader descends recursively per
nested value. Each depth was parsed in a child process so a crash could be
observed rather than taking the runner with it. Gate `G-qjson-probe`, exit 101:

> the child was killed by signal 6 at depth 10000
> thread 'child_parses_nested_manifest' has overflowed its stack
> fatal runtime error: stack overflow, aborting

Depth 1,000 survived. Depth 10,000 aborted the process. A one-line manifest
anyone can write ends the project shell before it can report anything.

The repair bounds the **host JSON reader**, not the language: no LCL depth limit
was introduced, and the bound produces an ordinary `JsonError` that callers
already surface as a malformed-file diagnostic. The value was chosen against
measured reality — the deepest JSON in the canonical package is **six** levels
and the deepest anywhere in the repository is **seven**, so the bound of 128 is
an order of magnitude above every legitimate file and far below the depth at
which the descent was observed to fail. Gate `G-qjson-after`, exit 0.

**Q-READ — split by evidence. One half FIXED_VERIFIED, one half REPRODUCED and
referred.** Both halves were probed before anything was changed.

*Excessive range bound — repaired.* A bound of 2^70 was reported as
`error.operation.parameter`. `INTEGER` is "an unbounded signed whole number", so
such a bound is well typed and merely outside every sequence, and the row is
explicit: "Require 0 <= start <= end <= sequence length; otherwise
**error.value.out_of_range**", reserving `error.operation.parameter` for "an
incompatible unit/representation or wrong key/type". A non-integral value still
reports a wrong type. Gates `G-qread-probe` (exit 101, observed
`error.operation.parameter`) and `G-qread-bound-after` (exit 0, observed
`error.value.out_of_range`).

*Lossy decoding — reproduced, repair referred to the owner.* `core.read` of the
bytes `61 FF 62` returns `"a\u{FFFD}b"` with `status.succeeded` and no
diagnostic: `String::from_utf8_lossy` substitutes and cannot fail. That
contradicts the row's meaning, "Retrieve accessible **exact content** without
changing its source", its postcondition that the result is "the **exact**
requested representation", and its "No clipping or ambient encoding conversion
occurs".

The deviation is established; the **repair is not chosen here**, because both
halves of it are policy this layer may not invent. `core.read`'s registered
`errors` list does not admit `error.operation.postcondition`, which is the
identifier that most plainly describes "the exact representation could not be
produced"; the nearest admitted reading is "an incompatible unit/representation",
which the row states for ranges rather than for the whole read. And refusing
would change what happens to every read of a file that is not UTF-8, which is a
product behaviour change and not only a diagnostic one. It is therefore recorded
as the labelled EXPECTED_REPRODUCTION
`expected_reproduction_qread_a_non_utf8_read_substitutes_silently`, which states
in its own documentation that a passing run is **not** an acceptance pass and
which fails the moment the behaviour changes.

### Final gates for LCL-CLOSE-01

| Gate | Toolchain | Actual exit | Result |
| --- | --- | --- | --- |
| `cargo test --offline --locked --workspace --all-targets` | rustc/cargo 1.98.1 | 0 | 143 suites, **1492 passed**, 0 failed, 0 ignored |
| `cargo check --offline --locked --workspace --all-targets` | **actual Rust 1.75.0** | 0 | PASS |
| `cargo test --offline --locked --workspace --all-targets` | **actual Rust 1.75.0** | 0 | **1492 passed**, 0 failed, 0 ignored |
| `cargo clippy ... -D warnings` over the eight crates this task changed | 1.98.1 | 0 | PASS |
| `rustfmt --check` on every file this task edited | 1.98.1 | 0 | PASS |
| `cargo test -p lcl-conformance --test decision_witnesses` | 1.98.1 | 0 | 7 passed |
| `cargo run -p lcl-conformance --example m8_conformance_report` | 1.98.1 | 0 | see below |
| canonical `validate_release.py --scope all` | Python 3.14.7 | 0 | 31 PASS, 2 OUT_OF_SCOPE, 0 FAIL/BLOCKED |
| canonical `sha256sum -c SHA256SUMS.txt` | system | 0 | 175 matching |
| `sha256sum -c assets/brand/BRAND_ASSETS.sha256` | system | 0 | 17 matching |

The suite grew from the entry baseline's 1416 to 1492 with no suite lost and
nothing ignored or skipped. The MSRV run is the full workspace under the
verified private `rustc 1.75.0 (82e1608df 2023-12-21)` and
`cargo 1.75.0 (1d8b05cdd 2023-11-20)`, both explicitly selected.

The production conformance report's structured verdict was inspected rather than
its exit code trusted. It claims **`source_conforming`**, against 980 required
obligations and 2,413 required probe IDs: 2,094 executed, 2,094 passed, **0
failed**, **66 of 66** decision witnesses established, **0 missing
source_conforming probes**, and **319 missing semantics_conforming probes**,
each enumerated by ID in its own "limited by" lines. That is exactly the state
the previous continuation recorded. This task neither raised nor reduced the
achieved claim: it preserved it, and the 319 missing semantic probes remain
LCL-CLOSE-02's B4 obligation.

**Preservation.** All **192** protected canonical, brand and previous-release
files re-hashed at task close: 0 mismatches. Canonical identity re-verified by
the implementation's own algorithm inside the green suite —
`approved_package_matches_the_anchor` and `identity_digest_is_reproducible` both
executed and passed, binding `compute_identity_digest` over the real canonical
root to `00d648b1…67ed`. No canonical byte, trust anchor, frozen archive or
release artifact was changed. No staging, commit, push, tag, merge, publication,
global install or change to the owner's real installation occurred, and HEAD is
still `7f13aecd4af639f1373bb1180cb50f9b508cfb05`.

**Resources.** No owned server, product process or fixture process was running
at close. Eleven leftover fixture directories inside the owned scratch `tmp/`
were removed one at a time, each verified to be inside that exact directory and
owned by this user; nothing outside it was touched, and the residual repair's
scratch was left intact. Evidence — every gate's `.log`, `.exit` and
`.record.json` — is retained.

**Still open, and owned elsewhere.** The two pre-existing workspace-wide gates
from the entry baseline remain failing and belong to B4, which is
LCL-CLOSE-02's item: `cargo fmt --all -- --check` over 23 files, every one of
them touched by the B4 commits and none outside them, and one
`clippy::type_complexity` at `lcl-conformance/tests/semantic_cases.rs:35`.
Neither is repaired here, because the working rule confines formatting changes
to the file currently being edited and those files are outside this task's
approved scope.

## 2026-09-13 continuation — LCL-CLOSURE-4T Task LCL-CLOSE-02 entry and Phase 0

Task **LCL-CLOSE-02** (conformance evidence and traceability) began with a
read-only reconciliation and a file-by-file plan. The owner approved that plan
("Plan is approved"). The approved plan carried these decisions:

- **D1, Q-READ:** repair it now as a narrow LCL-CLOSE-01 reopening. `core.read`
  fails closed with the row's registered `error.host.constraint` when the bytes
  are not valid UTF-8 or a `format` other than `format.plain_text` is requested.
  No new byte representation is introduced.
- **D2, STORE-ROLE-01** (found by source reading during planning, not yet
  executed): a bounded amendment. Reproduce first, add a `storage` profile for
  the engine's own stores, install it wherever profiles are installed, and
  enforce role selection in `data::store` last.
- **D3:** conformance-only deterministic fixture capabilities for the six rows
  the shipped host cannot perform (`core.analyze`, `core.report`,
  `core.generate`, `core.convert`, `core.install`, `core.uninstall`). Shipped-host
  refusal and fixture success are recorded as separate evidence classes; no
  product, provider or model code.
- **D4:** a new mapping revision pins the exact sub-run membership of every
  semantic row. All 2,413 existing probe IDs are kept, and the superseded
  mapping file is removed after the switch.
- **D5:** a scoped, hash-bound review ledger over every tracked file. Full
  review covers only the files LCL-CLOSE-01 and LCL-CLOSE-02 change, the
  conformance crate and their direct callers; everything else is recorded as
  unreviewed.

The approved new locations are the external erratum, the Task 11–16
retrospective index, the coverage ledger, a dated correction note in
`impl/README.md` and `reports/tasks/LCL-CLOSE-02_RESULT.md`.

### Entry state

Root `/mnt/F/LCL`, branch `main`, HEAD
`aebb2f70f09179d06688c36c96f79107411f54f2` ("LCL repair task1"), which is the
owner's commit of LCL-CLOSE-01's worktree. Upstream is recorded as
`origin/main`, zero ahead and zero behind, with no fetch. The worktree was
clean and nothing was stashed. Its content identity, computed read-only from
HEAD, the tracked diff and every untracked file, is
`2717dd4b48a5391ed930aafd7c9ad1c7e99988c72b2e8ccf28f5939551d7d8a2`.

LCL-CLOSE-01's final gates recorded a worktree of 26 implementation files plus
this report at HEAD `7f13aec`. The commit contains exactly those files plus
`LCL-CLOSE-01_RESULT.md`. Every committed source file was last modified by
18:33:35, before FINAL-tests started at 18:33:47. Only the two report files
changed afterwards, and no code reads `reports/`. That is timestamp evidence,
so Phase 0 reran the entry gates on the clean commit. The regenerated
production report is byte-identical to FINAL-m8-report (log SHA-256
`aec4ee69ed1822ab…`).

**Correction to the LCL-CLOSE-01 record.** Its final test gates were reported
as "0 ignored". The FINAL-tests and FINAL-msrv-tests logs each show one ignored
test: `lcl-project/tests/manifest_input_bounds.rs::child_parses_nested_manifest`,
marked `#[ignore = "driven by its parent, which supplies the depth"]`. It is the
child half of the Q-JSON probe, and its two parent tests execute it in child
processes with `--exact ... --ignored`. No acceptance was skipped, but the
ignored count in that record was wrong. Its 1,492 passed and 0 failed stand.

The owned scratch `/mnt/F/.lcl-closure-4t-4c1cd4c659b7` (mode 700) is reused.
It gains three helpers:

- `gate2.sh`: the same recorder as `gate.sh`, plus the worktree content
  identity before and after each gate;
- `worktree_identity.py`: computes that identity without writing to Git;
- `verify_protected.py`: re-hashes the 192-path protected inventory.

The toolchains are unchanged: system `rustc`/`cargo` 1.98.1, and the verified
private Rust 1.75.0 in the residual scratch.

### Phase 0 — entry gates, no edits

| Gate | Actual exit | Result |
| --- | --- | --- |
| T2-P0-protected | 0 | 192 of 192 protected files match (canonical 176, assets 4, releases 12) |
| T2-P0-canonical-checksums | 0 | 175 matching |
| T2-P0-brand-checksums | 0 | 17 matching |
| T2-P0-tests, `cargo test --offline --locked --workspace --all-targets` (1.98.1) | 0 | 143 suites, 1,492 passed, 0 failed, 1 ignored (the child helper above) |
| T2-P0-m8, production `m8_conformance_report` | 0 | `source_conforming`: 980 obligations, 2,413 probes, 2,094 executed and passed, 66 of 66 witnesses, 0 missing source probes, 319 missing semantics probes |
| T2-P0-fmt, `cargo fmt --all -- --check -l` | 1 | **FAIL, pre-existing:** 21 files (22 paths, because `witness_cases/mod.rs` is reached twice); 11 in `lcl-conformance`, 10 in checker, parser, resolver and runtime |
| T2-P0-clippy, workspace `-D warnings` | 101 | **FAIL, pre-existing:** `clippy::type_complexity` at `lcl-conformance/tests/semantic_cases.rs:35` |
| T2-P0-clippy-keep-going | 101 | The same single lint with `--keep-going` across every target; this is the complete inherited lint inventory |

Every Phase 0 gate ran at content identity `2717dd4b…`, and none changed the
worktree.

### Findings carried into the plan

1. **Q-READ:** the lossy-decoding half is still open. `host.rs` `read()` also
   ignores the `format` parameter entirely.
2. **STORE-ROLE-01, source-traced:**
   `axis_contract/implementation_profile/required_roles_by_operation` names
   `storage` for `core.memory_write` and `core.state_update`, and both rows'
   `invocation_resolution` begins "Resolve the authorized MEMORY [or STATE]
   storage profile". `data::store` never selects that role, no storage profile
   exists anywhere, and existing tests expect both rows to succeed.
3. **B4:**
   - 317 of the 319 missing semantic probes have populations only in tests,
     which the production report does not consume: `semantic_cases` (191
     groups), `operation_cases` (117) and `result_cases` (9).
     `diagnostic_policy` and `failure_lifecycle` have no population.
   - Sub-run membership is not pinned.
   - The report states "every registered implementation profile installed",
     but the analysis, reporting, generation, conversion, package and storage
     roles are not installed.
4. **DOC-01:** every `MANIFEST.json` component count matches its registry
   recount. `04_GRAMMAR/13_EXACT_FIELD_SIGNATURES.txt:11` says 335 fields and 67
   distinct value-kind expressions; the registry holds 334 field uses and 68
   distinct `value_kind` strings.
5. **HISTORY-01:** no LCL-TASK-0011 to LCL-TASK-0016 result report exists in any
   reachable commit.
6. **COVERAGE-01:** no earlier coverage ledger exists on this machine.
7. `impl/README.md:1055` still states that the gate claims
   `semantics_conforming`.

Next: Phase 1a, Q-READ.

### Phase 1a — Q-READ reopened and closed under decision D1

**Q-READ — the lossy-decoding half is FIXED_VERIFIED.** Its bound half was
already FIXED_VERIFIED in LCL-CLOSE-01. A representation defect of the same
item, established at entry, is also FIXED_VERIFIED: `host.rs` `read()`
ignored the `format` parameter entirely.

| # | File | Change | Verification |
| --- | --- | --- | --- |
| 1 | `lcl-stdlib/tests/external_operations.rs` | Under D1, the EXPECTED_REPRODUCTION `expected_reproduction_qread_a_non_utf8_read_substitutes_silently` is replaced by acceptance tests and controls (listed below) | T2-P1a-qread-red, exit 101, EXPECTED red: exactly the three planned tests failed |
| 2 | `lcl-stdlib/src/host.rs` | `read()` refuses any format other than absent, MISSING or `format.plain_text` before the target is read, and decodes exactly (below) | T2-P1a-qread-green, exit 0: the same 7 tests, assertions unchanged |
| 3 | `lcl-stdlib/tests/external_operations.rs` | Two `to_vec()` calls in the new tests removed; `with_file` takes `impl AsRef<[u8]>` | T2-P1a-stdlib-clippy, exit 101 (`clippy::unnecessary_to_owned`, those two calls) → T2-P1a-stdlib-clippy-2, exit 0 |

The replacement tests in change 1:

- **Acceptance:** non-UTF-8 content is refused with `error.host.constraint`,
  `status.blocked`, phase `pre_effect`, effect state `none`, and no value bound.
- **Acceptance:** a format this host does not implement is refused.
- **Acceptance:** a host-boundary matrix covers every spelling a `format`
  value can arrive in.
- **Controls:** exact UTF-8 content, and `format.plain_text`.

In the red run, the non-UTF-8 read and the `format.json` read each returned
content with no error, and the boundary matrix saw `Identifier("format.json")`
answered with plain text. Both controls and both existing range tests passed.

In change 2, bytes that are not valid UTF-8 now produce
`CapabilityOutcome::Unavailable`. The runtime records that as
`error.host.constraint` before effects.

| Gate | Actual exit | Result |
| --- | --- | --- |
| T2-P1a-stdlib-tests-2 | 0 | Every `lcl-stdlib` target: 10 suites, 115 passed, 0 failed, 0 ignored. Supersedes T2-P1a-stdlib-tests, which consumed the test file before the lint fix |
| T2-P1a-workspace-tests | 0 | 143 suites, 1,496 passed (1,492, less the retired reproduction, plus 5 new tests), 0 failed, 1 ignored (the Q-JSON child helper). Content identity `f37be90c…`, unchanged during the gate |

The reproduction recorded a baseline; it was not an acceptance test. Its
replacement asserts the behavior the owner chose, and that assertion was red
before the repair. Nothing was weakened.

**Behavior change.** A document that reads a non-UTF-8 file, or requests a
format other than plain text, now stops with `error.host.constraint`
(`status.blocked`). Previously it silently received substituted text as
successful content. The conformance read-range witnesses read UTF-8 fixtures
and request no format, and the `decision_witnesses` suite passed inside the
workspace gate.

Rust 1.75.0 has not yet been run over this change; that belongs to the Phase 1
closing gates.

Next: Phase 1b, STORE-ROLE-01.

### Phase 1b — STORE-ROLE-01 reproduced and closed under amendment D2

**STORE-ROLE-01 — REPRODUCED, then FIXED_VERIFIED.**

*The rule.* `axis_contract/implementation_profile/required_roles_by_operation`
names the `storage` role for every `core.memory_write` and every
`core.state_update` invocation. The two rows' resolutions begin "Resolve the
authorized MEMORY storage profile" and "Resolve the authorized STATE storage
profile". `06_STANDARD_LIBRARY/10`: "A missing, ambiguous, incomplete, or
out-of-bounds required profile role emits error.operation.precondition before
effects."

*The defect.* `data::store` never selected that role, and no storage profile
existed anywhere.

*The reproduction.* With no profile installed, both rows wrote their store and
reported success. Gate T2-P1b-store-role-red, exit 101, EXPECTED red: both new
acceptance tests saw `execution_errors` `[]` where
`["error.operation.precondition"]` is required. The other 15 tests passed.

| # | File | Change | Verification |
| --- | --- | --- | --- |
| 1 | `lcl-stdlib/tests/data_operations.rs` | Two acceptance tests (below) | T2-P1b-store-role-red, exit 101 (expected) |
| 2 | `lcl-stdlib/src/profiles.rs` | `STORE_IMPLEMENTATION` (`lcl.stdlib.store`) and `store_profiles()` (below) | T2-P1b-profiles-check, exit 0 |
| 3 | `lcl-stdlib/src/lib.rs` | Re-exports `store_profiles` | T2-P1b-export-check, exit 0 |
| 4 | `lcl-protocol/src/host.rs` | `surface()` always installs the store profiles beside the in-language verifier (below) | T2-P1b-protocol-check, exit 0 |
| 5 | `lcl-conformance/src/runner.rs` | A formatting-only rustfmt step first, then the runner installs the store profiles (below) | T2-P1b-runner-format-tests, exit 0 (conformance lib 16, runner_seam 13); T2-P1b-runner-store-tests, exit 0 |
| 6 | `lcl-stdlib/examples/m7_report.rs` | `all_profiles()` installs the store profiles | T2-P1b-m7-report-before, exit 0 |
| 7 | `lcl-stdlib/tests/data_operations.rs` | Existing store tests install the profiles; a new state control (below) | T2-P1b-store-tests-before-enforcement, exit 101: the same two expected failures |
| 8 | `lcl-stdlib/src/data.rs` | `store()` selects the `storage` role after the declaration-kind check and before the internal-store grant gate | T2-P1b-store-role-green, exit 101: see the oracle correction |
| 9 | `lcl-stdlib/tests/data_operations.rs` | Test-oracle correction (below) | T2-P1b-store-role-green-2, exit 0, 18 passed |

Details of the changes:

- **Change 1.** Memory and state writes with no profile installed must fail with
  `error.operation.precondition`, `status.failed`, phase `pre_effect`, effect
  state `none`, and no observed effect.
- **Change 2.** One `storage` profile per row. `core.memory_write` serves MEMORY
  targets with the `memory` effect; `core.state_update` serves STATE targets with
  the `state` effect. Both are deterministic. The engine's own stores reach no
  host resource, so neither declares a dependency, narrowing the rows' `host`
  maximum.
- **Change 4.** Every run starts from `Grants::internal()`, which always permits
  the engine's own stores, so installing their profiles is truthful. Shipped CLI
  and workspace behavior is unchanged.
- **Change 5.** The rustfmt step is token-equivalent apart from trailing commas
  and match-arm or closure braces.
- **Change 7.** The store-semantics tests and the ungranted-store test now install
  the profiles, so each still exercises the property it names. A new
  `core.state_update` control succeeds with the profile installed. The
  wrong-declaration-kind test deliberately stays profile-free, as an ordering
  control.

**Test-oracle correction (change 9).** My new helper `assert_refused_before_effects`
asserted that a refused store write carries no `changed` field.

- *What failed.* Both acceptance tests passed every refusal assertion: the error,
  the status, the phase, the effect state and the empty observed effects. They
  failed only on that assertion (`data_operations.rs:403`, left
  `Some(Boolean(false))`, right `None`).
- *Authority.* `built_in_groups_and_results_v0.1.0.json#/result_schemas/result.operation`
  registers `changed` with cardinality `exactly_one`, and states "changed remains
  present after failure" and "changed is FALSE when no requested target state
  changed". The runtime inserts exactly that for pre-effect failures.
- *Correction.* Assert `Some(FALSE)`.
- *Independent control.* `data_operations.rs:215` already asserts `changed` FALSE
  for a no-change operation. Before enforcement, the same documents succeeded with
  `changed` TRUE, so the corrected assertion still discriminates.
- The failed gate is retained.

| Gate | Actual exit | Result |
| --- | --- | --- |
| T2-P1b-stdlib-tests | 0 | Every `lcl-stdlib` target: 10 suites, 118 passed, 0 failed, 0 ignored |
| T2-P1b-clippy-stdlib-protocol | 0 | `lcl-stdlib` and `lcl-protocol`, all targets, `-D warnings` |
| T2-P1b-clippy-conformance-lib | 0 | `lcl-conformance` library and examples; its test target keeps the inherited lint, which is B4's |
| T2-P1b-m7-report-after | 0 | Byte-identical to T2-P1b-m7-report-before (SHA-256 `d20ffb5f…`); memory and state rows still `status.succeeded completed` |
| T2-P1-workspace-tests | 0 | 143 suites, 1,499 passed, 0 failed, 1 ignored |

### An owner commit during the Phase 1 closing gate

T2-P1-workspace-tests started at content identity `9bc48989…`: HEAD `aebb2f7`
plus tracked diff SHA-256
`67a39f5e8ad90e7cfdd628736dfa42b57fab30d45ccff6b29ae1cf9bc37321cd`.

While it ran, the owner committed `298d1e2b01a0dcd4dbecb5bf401143948b1cf716`
("LCL repair task2", 22:17:38 +02:00, parent `aebb2f7`).

- **Content.** `git diff aebb2f7 298d1e2b --binary` has the identical SHA-256, so
  the commit contains exactly the bytes the gate consumed.
- **The flag.** The record's `worktree_changed_during_gate` is true because HEAD
  moved; the consumed content did not change.
- **Upstream.** The local `origin/main` ref equals HEAD. This session performed no
  Git write, fetch or push.
- **Tooling.** The identity helper now also records `tracked_content_sha256`, which
  does not depend on HEAD.

### Owner decision: one primary session

The memory note showed a second session on this checkout, which the owner had
asked to "start with task 3"; that session did a read-only reconciliation and
stopped. RULES R07 allows one primary session, so this session paused, wrote
`LCL_CLOSE_02_HANDOFF.txt` in the owned scratch, and asked.

The owner approved the recommendation: this session finishes Task 2, the other
session stays read-only, and Task 3 stays locked until Task 2 closes. Before work
resumed, the state was confirmed unchanged: HEAD `298d1e2b`, clean, the same
content identity, no new scratch files and no running processes.

### Phase 1 closing gates, and amendment A-T2-2

| Gate | Toolchain | Actual exit | Result |
| --- | --- | --- | --- |
| T2-P1-msrv-check | Rust 1.75.0 | 0 | Workspace, all targets, no warning. A genuine re-check of the 7 affected crates; unchanged crates were fresh from LCL-CLOSE-01's 1.75.0 builds |
| T2-P1-msrv-tests | Rust 1.75.0 | **101** | 135 of 143 suites ran before cargo stopped (below) |
| T2-P1-protected | — | 0 | 192 of 192 protected files match |

The one failure in T2-P1-msrv-tests was
`lcl-workspace/tests/concurrent_persistence.rs::overlapping_saves_preserve_whole_payloads_and_the_legacy_name`,
left `(200, 409)`, right `(200, 200)`.

**Diagnosis.** It was carried out under R04: read-only first, then labelled
characterization runs.

- *What the test does.* It releases the final body byte of two full `PUT` saves of
  one document at the same instant, and asserted that both return 200. The test
  predates UI-03.
- *What UI-03 changed.* LCL-CLOSE-01's UI-03 repair (decision D3) orders replacing
  saves by acceptance. A write overtaken by a newer, already-published save is
  refused as `DocumentError::Superseded`, which `routes.rs` returns as 409
  "... was saved again while this write was in flight". That code is commented so
  that "a client is never told a write succeeded when the newer content is what is
  on disk".
- *Conclusion.* Under a true overlap, exactly one 409 is the specified outcome. The
  old `(200, 200)` expectation held only when the saves did not overlap.
  Earlier full-suite passes, including LCL-CLOSE-01's final gates, depended on
  scheduling. LCL-CLOSE-01's three repeat runs covered only the `lcl-workspace`
  library tests, never this HTTP test.
- *Measurement.* Isolated runs of the unchanged test produced exactly one 409 in
  3 of 20 runs on Rust 1.75.0 (T2-P1-diag-ui03-msrv) and in 9 of 20 on Rust 1.98.1
  (T2-P1-diag-ui03-current).

This is not a Rust 1.75.0 incompatibility and not a product defect.

**Amendment A-T2-2 — a test-oracle correction in LCL-CLOSE-01's UI-03 area; no
product change.** Recorded under R10:

- *Authority.* The approved D3 per-path publication ordering, the documented 409
  contract, and Task 01 Phase E UI-03.
- *Original mistake.* The pre-UI-03 expectation that both overlapping saves
  succeed, measured above.
- *New oracle.* The test now accepts either two 200 replies, each digest matching
  its own payload, or exactly one 409 that names the supersession and published
  nothing, with the accepted save's whole payload on disk.
- *Discriminating control.* The corrected assertions reject two refusals, mixed or
  truncated bytes, a refused write that published, a 409 that does not name the
  supersession, any other status, the legacy `.lcl.txt` name and leftover
  temporaries.

| Gate | Toolchain | Actual exit | Result |
| --- | --- | --- | --- |
| T2-P1-ui03-oracle-current | Rust 1.98.1 | 0 | The corrected test passed 20 of 20 isolated runs |
| T2-P1-ui03-oracle-msrv | Rust 1.75.0 | 0 | 20 of 20 |
| T2-P1-msrv-tests-2 | Rust 1.75.0 | 0 | Rerun of the failed gate: 143 suites, 1,499 passed, 0 failed, 1 ignored; `overlapping_saves` ok; content identity `72adee8c…`, unchanged during the gate |
| T2-P1-workspace-tests-2 | Rust 1.98.1 | 0 | Rerun after the test-file change: 143 suites, 1,499 passed, 0 failed, 1 ignored; `overlapping_saves` ok; unchanged during the gate |

At the Rust 1.98.1 rate, 20 isolated runs with no 409 at all have a probability of
about 1e-5, so the 20 consecutive passes exercised both accepted outcomes.

**Phase 1 is closed.** Q-READ and STORE-ROLE-01 are FIXED_VERIFIED on the current
and minimum toolchains. Amendment A-T2-2 is recorded, and all 192 protected files
are unchanged.

Next: Phase 2, formatting-only edits of the 10 inherited files outside
`lcl-conformance`.

### Phase 2 — inherited formatting outside `lcl-conformance`

Each file was formatted alone with `rustfmt --edition 2021` and checked four ways
before the next one was touched:

- `rustfmt --check` is clean;
- `git status` shows no other file changing, including the `mod common;` children
  that rustfmt also walks;
- `strip_compare.py`, which compares the file at HEAD with the worktree after
  removing all whitespace, lists every non-whitespace change;
- the crate's `cargo check --all-targets` gate passes.

Every non-whitespace change is a syntactic normalization that cannot alter
behavior:

- trailing commas added or removed;
- a match-arm or closure body wrapped in braces;
- `;` added after `break` or `continue` inside a `let … else` block.

| # | File | Diff | Non-whitespace changes | Check gate (exit 0) |
| --- | --- | --- | --- | --- |
| 1 | `lcl-checker/src/expr.rs` | +80 −22 | 8 `;` after `break`, 6 `,`, 1 arm body braced | T2-P2-checker-expr-check |
| 2 | `lcl-parser/src/schema.rs` | +25 −7 | 4 `,` | T2-P2-parser-schema-check |
| 3 | `lcl-parser/tests/document_structure.rs` | +0 −1 | none (one blank line) | T2-P2-parser-docstructure-check |
| 4 | `lcl-resolver/src/graph.rs` | +92 −30 | 6 `;` after `continue`, 2 `,` added, 1 `,` removed, 2 arm bodies braced | T2-P2-resolver-graph-check |
| 5 | `lcl-resolver/tests/output_instances.rs` | +15 −4 | none | T2-P2-resolver-output-instances-check |
| 6 | `lcl-runtime/src/contracts.rs` | +7 −2 | 1 closure body braced | T2-P2-runtime-contracts-check |
| 7 | `lcl-runtime/src/eval.rs` | +5 −3 | 1 `,` | T2-P2-runtime-eval-check |
| 8 | `lcl-runtime/src/mock.rs` | +9 −3 | 1 `,` | T2-P2-runtime-mock-check |
| 9 | `lcl-runtime/src/result.rs` | +52 −17 | 5 `,`, 5 arm bodies braced | T2-P2-runtime-result-check |
| 10 | `lcl-runtime/tests/failure_handling.rs` | +152 −39 | 9 `,` added, 3 `,` removed, 3 arm bodies braced | T2-P2-runtime-failure-handling-check |

| Gate | Actual exit | Result |
| --- | --- | --- |
| T2-P2-crates-tests | 0 | Every target of `lcl-parser`, `lcl-resolver`, `lcl-checker` and `lcl-runtime`: 45 suites, 539 passed, 0 failed, 0 ignored |
| T2-P2-fmt, `cargo fmt --all -- --check -l` | 1 | Remaining inherited files, all in `lcl-conformance` (below); nothing outside that crate |
| T2-P2-clippy | 0 | The four crates, all targets, `-D warnings` |

The format gate reports 10 remaining files, reached through 11 paths because
`witness_cases/mod.rs` is also reached through the example:

- `examples/m8_conformance_report.rs`
- `src/lib.rs`
- `src/report.rs`
- `src/source_cases.rs`
- `tests/decision_witnesses.rs`
- `tests/operation_cases.rs`
- `tests/result_cases.rs`
- `tests/runner_seam.rs`
- `tests/semantic_cases.rs`
- `tests/witness_cases/mod.rs`

`src/runner.rs` was already formatted in Phase 1b. Each remaining file is
formatted at its first planned B4 edit.

Next: Phase 3, the B4 accounting instrument.

### Phase 3 — the B4 accounting instrument

Phase 3 changes only three files in `lcl-conformance`: `runner.rs`, `obligations.rs` and `report.rs`. Each step
changed one file, and that step's `lcl-conformance` library-test gate passed before the next file was touched.

No new population is connected yet, so the production claim is unchanged. What changes is what a report can prove:
it now names missing sub-runs, and every kind of record has a written, closed rule.

| # | File | Change | Gate (exit 0) |
| --- | --- | --- | --- |
| 1 | `runner.rs` | `Observed.run_labels`: one label per sub-run, rendered beside it | T2-P3-runner-labels-tests (lib 17, runner_seam 13) |
| 2 | `obligations.rs` | A private `load_mapping(spec, text, digest)`, which `load` calls with the embedded mapping; a test-only `load_mapping_text`; an optional per-row `subruns` pin and `subruns(probe)` | T2-P3-obligations-subruns-tests (lib 20) |
| 3 | `report.rs` | Formatting only. `strip_compare.py` lists 4 trailing commas and 2 closure bodies wrapped in braces | T2-P3-report-format-tests (lib 20) |
| 4 | `report.rs` | `ProbeState` and `ProbeAccount`, with exact sub-run membership | T2-P3-report-probe-accounts-tests (lib 24) |
| 5 | `report.rs` | Inputless evidence counts as failed evidence | T2-P3-report-inputless-tests (lib 25) |
| 6 | `report.rs` | Unrequired records are listed | T2-P3-report-unrequired-tests (lib 26) |
| 7 | `report.rs` | `RunIdentity`, `record_run`, excluded records and the source snapshot | T2-P3-report-run-identity-tests (lib 27) |
| 8 | `report.rs` | `render_verdict_json` | T2-P3-report-verdict-json-tests (lib 29) |
| 9 | `obligations.rs` | `digest()`: the digest of the mapping text actually loaded | T2-P3-obligations-digest-tests (lib 30) |
| 10 | `report.rs` | The run identity and the rendered report state the loaded digest, not the compiled-in constant | T2-P3-report-loaded-digest-tests (lib 30) |

Net diff against HEAD, including the formatting step: `runner.rs` +32 −1, `obligations.rs` +165 −3, `report.rs` +1180 −54.

**The closed report policy.** Every required probe and every record falls into exactly one defined case.

- **Probe states.** Each required probe is in exactly one of four states, so at each claim level
  `required = satisfied + failed + missing + invalid`:
  - `satisfied`: exactly one record, which passed and carries exactly its pinned sub-runs;
  - `failed`: exactly one record, which carries exactly its pinned sub-runs and failed;
  - `missing`: no record;
  - `invalid`: more than one record, or a pinned group whose judged runs differ from the pin.

  `missing_probes` keeps its meaning, every state except `satisfied`, so existing callers are unchanged.
- **Pinned sub-runs.** Membership must be exact. The probe is invalid, and each problem is listed by label, when:
  - a pinned label has no run;
  - a label is not pinned;
  - a run has no label, or a label has no run;
  - a label appears twice.

  Only an exact ordered group (`Expectation::Runs` with one expectation per run) carries pinned sub-runs; a record of
  any other shape carries none. A pinned run that fails its own expectation is named in `failed_subruns`. A pin must
  name exactly one grouped probe with at least one label and no empty or repeated label; the loader refuses anything
  else. Exact membership also catches a reviewed control that silently disappears.
- **Inputless evidence.** Each of these counts as failed evidence: a record with an empty source; an observation with
  no input evidence; a group in which any run lacks input evidence. The group rule is new protection, because
  `Accepts` never inspects runs.
- **Unrequired records.** A record whose ID names no required probe is listed. It satisfies no obligation and cannot
  raise a claim. If it failed, it still counts as a failed case.
- **Run identity.** A report's identity has six parts:
  - implementation version;
  - language version;
  - package identity;
  - the digest of the mapping it loaded;
  - host capabilities;
  - the implementation source snapshot, recorded at build time from `LCL_SOURCE_SNAPSHOT`, or `unrecorded`.

  `record_run` admits a record only when its identity is identical. Any other record is excluded: it is listed with
  every differing field, never counted as executed and never used as evidence. `record` remains the in-process path,
  whose identity is the report's own.
- **Structured verdict.** `render_verdict_json` emits one deterministic document. Record order changes nothing in it.
  It contains:
  - the claim, the implementation and the mapping digest;
  - for each level, the required count, the four state lists, and a `problems` entry with sub-run detail for every
    probe not satisfied;
  - record counts, failed case IDs, and the unrequired and excluded records;
  - the indexed catalogues, including witnesses not established;
  - descriptive-only entries and claim limits.

  The human-readable report states the same partition, and its claim limits name every problem.

**Synthetic negative controls against the task's minimum list.** The existing instrument tests still pass. The new
tests:

| Contract control | Test |
| --- | --- |
| Omit one required sub-run from an otherwise passing group | `omitting_one_pinned_subrun_denies_completeness`, with an exact-membership control |
| Unexpected, duplicated, unlabelled or ungrouped sub-runs | `unexpected_duplicated_unlabelled_or_ungrouped_subruns_are_invalid` |
| A failed mandatory sub-run | `a_failed_pinned_subrun_fails_its_probe_and_is_named` |
| States are disjoint and cover every required probe | `probe_states_partition_every_required_probe_exactly_once` |
| Inputless success | `inputless_success_is_failed_evidence`, with a control whose runs carry input |
| Unknown probe ID; an irrelevant success | `unrequired_records_are_listed_and_neither_fill_a_gap_nor_change_the_verdict` |
| Incompatible run identity; outputs mixed from other source revisions or host configurations | `records_from_another_run_identity_are_excluded_and_listed`, one field at a time, with an own-identity control |
| Wrong inventory digest; truncated inventory | `a_mapping_under_another_digest_is_refused`; `a_truncated_inventory_is_refused_even_under_its_own_digest` |
| Malformed pins | `pinned_subruns_are_parsed_exactly_and_malformed_pins_are_refused` |
| The report states the mapping it actually loaded | `an_inventory_states_the_digest_of_the_mapping_it_loaded` |
| The structured result reconciles independently | `verdict_json_partitions_every_required_probe_and_reconciles_independently`: the parsed JSON's four lists at each level are disjoint and together equal the inventory; `problems` equals the unsatisfied list; escaping survives a round trip |
| Duplicate one success, add an irrelevant success, reorder records | `reordering_records_changes_neither_the_verdict_nor_its_json`: forward, reversed and interleaved orders give identical claims, limits and JSON, and neither the duplicate nor the irrelevant success fills the gap |

Earlier tests already cover the remaining controls:

| Contract control | Test |
| --- | --- |
| Omitted source probe | `source_requires_every_lexical_grammar_block_and_field_probe` |
| Omitted semantic probe | `semantic_contracts_are_required_in_addition_to_all_witnesses` |
| Duplicated probe | `duplicate_required_probes_block_their_level` |
| Injected failure | `forged_passing_verdict_does_not_hide_a_failed_observation` |

All of these are accounting-unit tests on synthetic records. Negative tests on the real production path come in P5.

### A process deviation during Phase 3: the gate build environment

`gate2.sh` records `CARGO_TARGET_DIR` and `TMPDIR` but does not set them. Every earlier Task 2 gate passed both
explicitly. After a context compaction in this session, 11 gates ran without them:

- the 10 cargo gates from T2-P3-obligations-subruns-tests through T2-P3-conformance-tests;
- T2-P3-fmt, which ran only `rustfmt`.

Their records state this truthfully (`None`). The deviation came to light because cargo printed relative
`target/debug/deps` paths.

- **Effect.** Cargo wrote 1,947 build-cache files under the pre-existing, untracked `/mnt/F/LCL/impl/target/debug`
  (dated 2026-09-11), including the example binaries `m8_conformance_report` and `m0_report`.
- **Not affected:**
  - tracked files and protected files;
  - `/tmp`, where the session user has no new entry;
  - any installed copy (no `lcl` on `PATH`).
- **Validity.** The test outcomes stand, because each record binds the worktree content that gate consumed.
- **Left in place.** That directory is the owner's own cache, and this session's files cannot be separated from it
  with certainty. Whether to remove them is the owner's decision.
- **Remediation.** `gate2.sh` now refuses to run, with exit 67 and before any evidence is written, unless both
  variables point inside the owned scratch. Three refusal probes (variables unset, `TMPDIR` missing, the repository
  target) wrote no evidence. T2-P3-gate-guard-control shows the admitted case, and the Phase 3 closing gates below
  were rerun in the owned environment.

### Phase 3 closing gates

| Gate | Toolchain | Actual exit | Result |
| --- | --- | --- | --- |
| T2-P3-conformance-tests-2 | Rust 1.98.1 | 0 | Every `lcl-conformance` target, built in the owned `target-current`: lib 30, decision_witnesses 7, descriptive_index 9, operation_cases 1, result_cases 1, runner_seam 13, semantic_cases 3, doctests 0 |
| T2-P3-msrv-conformance-tests | Rust 1.75.0 | 0 | The same 8 targets and counts, built in the owned `target-msrv` |
| T2-P3-fmt-2 | — | 0 | `runner.rs`, `obligations.rs` and `report.rs` are rustfmt-clean |
| T2-P3-clippy | Rust 1.98.1 | 0 | `lcl-conformance` library and examples, `-D warnings`. The test target keeps the inherited `semantic_cases.rs` lint, which P4a removes |
| T2-P3-m8 | Rust 1.98.1 | 0 | The production report, below |

**The production report after Phase 3** (T2-P3-m8):

- **Claim:** `CLAIM: source_conforming`, unchanged since Phase 0.
- **Source level:** 2,011 required, 2,011 satisfied.
- **Semantics level:** 402 required, 83 satisfied, 0 failed, 319 missing, 0 invalid.
- **Records:** 2,094 executed, all passed. Every one names a required probe, so none is unrequired and none is
  excluded.
- **Witnesses:** 66 of 66 established.
- **Source snapshot:** `unrecorded`. The final production run sets `LCL_SOURCE_SNAPSHOT`.
- **Claim limits:** 319, all `missing`. They are the 319 contract rows, and no population is connected to them yet.

Workspace-wide tests last ran at T2-P1-workspace-tests-2 and T2-P1-msrv-tests-2, before Phases 2 and 3. The Task 2
final gates run them again.

Next: P4a. The executed semantic, operation, result and witness populations and their fixtures move into the
library, a production entry point is added, and grouped cases label their runs.

### P4a — the executed populations become library code behind one production entry

A usage limit interrupted the session from 07:32 to 11:54 on 2026-09-14. On resume, before any further edit, the
state was reconfirmed read-only: the same 15 modified tracked files, the one new untracked `src/fixtures.rs`, and no
build process running.

P4a moves the four executed populations and their shared fixtures from `tests/` into the `lcl-conformance` library.
It adds one production entry point that runs all of them against the pinned inventory, and it makes grouped records
label their runs. It adds no case, changes no oracle and needs no dependency change: every crate the populations use
was already a normal dependency.

**Moves.** One file changed at a time, and each change was gated before the next file was touched. Every moved file
is proven equal to its HEAD original plus only the planned edits: `rustfmt(HEAD original + planned edits)` is
byte-identical to the new library file. The result-schema and witness files were generated that way by script, not
retyped.

| Population | New library file | Planned edits beyond rustfmt | Test file afterwards | Gates (exit 0) |
| --- | --- | --- | --- | --- |
| Fixtures | `src/fixtures.rs` | `lcl_conformance` → `crate` import; the test-only `#![allow(dead_code)]` dropped | `tests/common/mod.rs`: `pub use lcl_conformance::fixtures::*;` | T2-P4a-fixtures-module-tests, T2-P4a-fixtures-module-clippy, T2-P4a-common-reexport-tests |
| Semantic: types, operators, functions, statuses, error contracts | `src/semantic_cases.rs` | `crate` import without `Verdict`; `crate::fixtures` paths; a `TypeRow` alias, which removes the inherited clippy `type_complexity` lint; the one test that uses private helpers became a unit test | Two population tests, bodies unchanged | T2-P4a-semantic-module-tests (lib 31), T2-P4a-semantic-module-clippy, T2-P4a-semantic-thin-tests |
| Operations: binding, errors and effects for 39 operations | `src/operation_cases.rs` | `crate` import without `Verdict`; its test removed | One population test, body unchanged | T2-P4a-operation-module-tests, T2-P4a-operation-module-clippy, T2-P4a-operation-thin-tests |
| Result schemas (9) | `src/result_cases.rs` | `crate` import without `Verdict`; `crate::fixtures::task_document`; its test removed | One population test, body unchanged | T2-P4a-result-module-tests, T2-P4a-result-module-clippy, T2-P4a-result-thin-tests |
| Decision witnesses (66) | `src/witness_cases.rs` | Its six `lcl_conformance::` paths written as `crate::` | `tests/witness_cases/mod.rs`: `pub use lcl_conformance::witness_cases::*;` | T2-P4a-witness-module-tests, T2-P4a-witness-module-clippy, T2-P4a-witness-reexport-tests, T2-P4a-witness-reexport-clippy |

`src/lib.rs` first received a formatting-only step, passed through stdin so that rustfmt could not reach its
unformatted child `source_cases.rs`. `strip_compare` found the stripped texts identical (T2-P4a-lib-format-tests).
After that, `lib.rs` gained one `pub mod` line per module.

**The production entry.** `src/production.rs` exposes `production::report(spec)`. Against the complete verified
inventory it executes:

- every canonical source case, with `lexical` or `grammar` coverage as before;
- every decision-witness probe, with descriptive and not-implemented entries recorded as the witness gate records them;
- every semantic, operation and result-schema group.

Two lookups that were lenient now fail closed:

- a witness absent from the canonical catalogue is an error, where the example silently used an empty contract;
- a semantic group whose family has no reviewed coverage mapping is an error, never a guess.

`examples/m8_conformance_report.rs` is now a thin front end: it prints the text report by default, the structured
verdict with `--json`, and exits 2 with a usage message on any other argument (T2-P4a-m8-usage).

**Production-path tests.** `tests/production_report.rs` runs the real report and uses real records only. It checks:

- the report claims exactly what the evidence supports:
  - no failed, unrequired or excluded record, and every witness established;
  - every source probe satisfied;
  - every semantic probe satisfied except the rows of the two families with no population, `diagnostic_policy` and
    `failure_lifecycle`, which are missing;
- the JSON verdict reconciles with the pinned inventory;
- two runs produce identical output;
- every grouped record labels each run exactly once, and the number of grouped records equals the populated contract
  rows derived from the inventory (317).

**Run labels, reproduced before repair.** The three group builders now record each sub-run's identifier as its label.
Their gates are T2-P4a-semantic-labels-tests and -clippy, T2-P4a-operation-labels-tests and -clippy, and
T2-P4a-result-labels-tests and -clippy. The label test was written before the one known collision was repaired:

| Gate | Actual exit | Result |
| --- | --- | --- |
| T2-P4a-label-uniqueness-red | 101 | EXPECTED: exactly one violation, `semantic/result_schemas/result.value: 27 runs, 27 labels, repeated {"complete"}`, where a record-validation run and a host-boundary run shared a label. The other 3 production tests passed |
| T2-P4a-label-uniqueness-green | 0 | After the five host-boundary runs were relabelled `boundary/…`: every target, 68 tests |

Only the labels changed; the runs' sources, expectations and observations did not.

**Formatting.** The last three inherited unformatted files each received a formatting-only step, verified with
`strip_compare` and gated:

| File | Non-whitespace changes | Gate (exit 0) |
| --- | --- | --- |
| `src/source_cases.rs` | 30 trailing commas; 7 match-arm or closure bodies wrapped in braces | T2-P4a-source-cases-format-tests |
| `tests/decision_witnesses.rs` | None (whitespace only) | T2-P4a-decision-witnesses-format-tests |
| `tests/runner_seam.rs` | 8 trailing commas added, 1 moved; 1 closure body wrapped in braces | T2-P4a-runner-seam-format-tests |

`cargo fmt -p lcl-conformance --check` is now clean.

**A failed closing gate.** T2-P4a-close-clippy was the first clippy run over `--all-targets`. It failed with two
`type_complexity` errors in the `report.rs` unit tests written in Phase 3; every earlier clippy gate had linted only
`--lib --examples`. The diagnostic run T2-P4a-close-clippy-keep-going confirmed that those two errors are the only
lints in any target. The fix names the two test-local types `SubrunCase` and `IdentityAlteration` and changes no
behavior.

### P4a closing gates

| Gate | Toolchain | Actual exit | Result |
| --- | --- | --- | --- |
| T2-P4a-close-tests | Rust 1.98.1 | 0 | Every target: lib 31, decision_witnesses 7, descriptive_index 9, operation_cases 1, production_report 4, result_cases 1, runner_seam 13, semantic_cases 2, doctests 0 (68) |
| T2-P4a-close-msrv-tests | Rust 1.75.0 | 0 | The same 9 targets and 68 tests |
| T2-P4a-close-fmt | — | 0 | `cargo fmt -p lcl-conformance --check` |
| T2-P4a-close-clippy | Rust 1.98.1 | **101** | Two `type_complexity` errors in the `report.rs` unit tests (see above) |
| T2-P4a-close-clippy-keep-going | Rust 1.98.1 | 101 | Diagnostic: those two are the only lints in any target |
| T2-P4a-close-clippy-2 | Rust 1.98.1 | 0 | After the fix: every target, `-D warnings` |
| T2-P4a-close-lib-tests-2 | Rust 1.98.1 | 0 | lib 31 after the fix |
| T2-P4a-close-msrv-lib-tests-2 | Rust 1.75.0 | 0 | lib 31 after the fix |
| T2-P4a-close-m8, T2-P4a-close-m8-json | Rust 1.98.1 | 0 | The production report, below |

**The production report after P4a** (T2-P4a-close-m8 and T2-P4a-close-m8-json):

- **Claim:** `source_conforming`, unchanged.
- **Source level:** 2,011 required, 2,011 satisfied.
- **Semantics level:** 402 required, 400 satisfied, 0 failed, 2 missing, 0 invalid.
  - The two missing probes are `semantic/diagnostic_policy/core.error_selection` and
    `semantic/failure_lifecycle/core.failure_lifecycle`, the two families with no population yet.
- **Records:** 2,411 executed and all passed; none unrequired and none excluded; 66 of 66 witnesses established.
- **Labels:** every one of the 2,752 rendered sub-runs carries its label.
- **Independent reconciliation:** a Python check of the JSON verdict, sharing no code with the report, passed:
  - the digest and package identity match the mapping file;
  - at each level the four state lists are disjoint and their union is exactly that level's inventory;
  - `required` equals the total of the four lists;
  - `problems` names exactly the unsatisfied probes.

The 400 semantic probes count as satisfied only because the current mapping pins no sub-run membership. P4b answers
whether each group executes every clause its canonical requirement names.

Next: P4b, mapping revision r2. It pins the required sub-runs of every semantic contract row, derived from that row's
canonical requirement text.

### P4b — mapping revision r2 pins the required sub-runs of every semantic contract row

Revision r1 pinned probe IDs only. A group therefore counted as satisfied whatever part of its canonical requirement
it ran. Revision r2 keeps every r1 probe ID and adds, for each of the 319 semantic contract rows, the exact sub-run
labels that its requirement names (decision D4). Clauses that no run exercises yet are pinned too, so the report
names them as missing instead of never seeing them.

**Derivation.** The scratch generator `review/r2/generate_r2.py` reads four inputs: the canonical registries, the
conformance catalogue (`09_CONFORMANCE/CASES/core_conformance_cases_v0.1.0.json`), r1, and the run labels recorded at
T2-P4a-close-m8. It records every pin in `review/r2/r2_pins.tsv` with its kind, its canonical authority (the catalogue
case ID) and a justification, then emits the mapping. The four pin kinds are:

- `existing` (2,437): a label an executed run already carries. It is kept only where it names a canonical clause, and
  checked against a registry derivation where one exists.
- `registry` (264): a clause derived mechanically from a registry fact, such as a registered successor, a parameter
  default, an integer bound or a registered error.
- `relabelled` (315): an existing positional run renamed to the clause it executes (step 1 below).
- `requirement` (701): a clause named by the row's requirement text or registered contract that no run exercises yet.

Before the switch, two derivation rules were corrected. Both were found by comparing the generator's output with the
executed labels:

- The exclusive axis values `none` and `declared_state_only` are not concrete effects or dependencies, so they are
  never pinned as forbidden. This removed 45 wrong labels.
- Within one row, a registry-indexed `precondition/<i>` or `postcondition/<i>` pin replaces a hand label that
  restated the same condition.

| Family | Rows | Pinned sub-runs | Carried by an existing run | Of which relabelled | No run yet |
| --- | --- | --- | --- | --- | --- |
| type_valid | 21 | 64 | 35 | 35 | 29 |
| type_invalid | 21 | 68 | 21 | 0 | 47 |
| operator_valid | 19 | 228 | 181 | 181 | 47 |
| operator_invalid | 19 | 26 | 19 | 0 | 7 |
| function_valid | 11 | 121 | 99 | 99 | 22 |
| function_invalid | 11 | 23 | 14 | 0 | 9 |
| operation_binding | 39 | 652 | 525 | 0 | 127 |
| operation_effects | 39 | 1,188 | 969 | 0 | 219 |
| operation_errors | 39 | 516 | 214 | 0 | 302 |
| status_transition | 12 | 299 | 299 | 0 | 0 |
| error_contract | 77 | 221 | 181 | 0 | 40 |
| result_schemas | 9 | 257 | 195 | 0 | 62 |
| diagnostic_policy | 1 | 24 | 0 | 0 | 24 |
| failure_lifecycle | 1 | 30 | 0 | 0 | 30 |
| **Total** | **319** | **3,717** | **2,752** | **315** | **965** |

The mapping file `src/obligations_v0.1.0_r2.json` is 446,927 bytes, SHA-256
`386c14994f032a57143ef731ea7ac55db3dfffde52b54f0d38e39e7b5e126b20`, and byte-identical to the generator output. It
adds `"revision": "r2"` and a `subruns` list to every semantic row. Source rows and all probe IDs are unchanged.

**Steps.** Each step changed one file and was gated before the next.

| # | File | Change | Gates (exit 0) |
| --- | --- | --- | --- |
| 1 | `src/semantic_cases.rs` | The type, function and operator runs carry inline clause labels instead of positional ones: 315 labels in 51 groups. Only labels changed | T2-P4b-relabel-tests (68), -clippy, -m8 |
| 2 | `src/obligations.rs` | Test seams `with_row_subruns` and `embedded_mapping_with_subruns`. The obligation tests no longer quote r1 row text, and the truncation test cuts at a marker | T2-P4b-obligation-seam-tests (lib 32), -clippy |
| 3 | `src/report.rs` | The instrument tests build complete synthetic populations through `satisfying()`, which gives a pinned probe a group carrying exactly its pins. Assertions unchanged | T2-P4b-report-seam-tests (lib 32), -clippy |
| 4 | `tests/production_report.rs` | A revision-agnostic invariant (below) replaces "every populated probe is satisfied" | T2-P4b-production-invariant-tests (4), -clippy |
| 5 | `src/obligations_v0.1.0_r2.json`, then `src/obligations.rs` | The generated mapping added, unreferenced; then `MAPPING` and `MAPPING_DIGEST` switched to it | T2-P4b-r2-switch-tests (69), -clippy, -m8, -m8-json |
| 6 | `src/obligations.rs` | Canonical cross-checks for the registry-derivable pins (below) | T2-P4b-cross-checks-tests (70), -clippy |
| 7 | `src/obligations_v0.1.0.json` | r1 removed, after confirming it was byte-identical to HEAD and to the digest HEAD pinned (`17fb6df8…fbed`) | T2-P4b-remove-r1-tests (70), T2-P4b-remove-r1-m8 |

- **Step 1 evidence.** `review/r2/verify_relabel.py` compares T2-P4a-close-m8 with T2-P4b-relabel-m8. In each of the
  51 planned groups:
  - the new labels equal the plan, in order;
  - expectations are unchanged;
  - run sources are unchanged apart from their `SUBCASE` label headers;
  - observations are unchanged apart from run labels.

  Every other record of the 2,411 is byte-identical. The comparison passed.
- **Expectation totals.** The recorded expected outcomes of T2-P4b-r2-switch-tests, T2-P4b-cross-checks-tests and
  T2-P4b-remove-r1-tests each state a test total one lower than the actual count, because they omit the library test
  added in step 2. Every target passed. The per-target counts in the logs are authoritative: 69 at the switch, then
  70.

**The production invariant (step 4).** Under r2, many populated probes are honestly invalid. Each semantic probe must
therefore be exactly one of the following:

- satisfied, in a populated family;
- missing, only in one of the two unpopulated families;
- invalid only because pinned sub-runs have no run yet: one record, and no unexpected, duplicated or failed sub-run.

A failed probe is never accepted. The invariant held under r1 before the switch and under r2 after it.

**Canonical cross-checks (step 6).** The loader now refuses a mapping, even one presented under its own digest, that:

- leaves any semantic contract row without pins;
- pins a status row with anything other than its registered successor set. That set is every registered status plus
  `status.invented`, each for non-root and root use; `status.skipped` has no root runs;
- drops an error row's `registry-contract` pin, or the component that mirrors its registered stage (lexer, parser,
  resolver, checker, preflight, runtime or completion);
- drops a registry-derived pin of an operation row:
  - exact binding;
  - the six parameter-default component runs per parameter, and the default binding where the registry gives a
    default;
  - required parameters and targets;
  - both sides of each integer range bound;
  - the memory-target and state-target prohibitions;
  - the 15 determinism selection runs;
  - for each required role: the missing, complete and ambiguous profile runs; the four incomplete-field runs; each
    forbidden effect or dependency outside the operation's maximum; and, for deterministic operations,
    nondeterministic-under-fixed;
- pins an `error/<id>` that the operation does not register;
- pins a `precondition/<i>` or `postcondition/<i>` beyond the registered conditions.

`registry_derived_pins_cannot_be_dropped_invented_or_left_out` alters six rows, one mapping per row. Each altered
mapping is refused with its specific reason. Every other test loads the unaltered r2, which passes all the checks.

**The production report under r2** (T2-P4b-r2-m8 and -m8-json; T2-P4b-remove-r1-m8 is identical apart from build
lines):

- **Claim:** `source_conforming`, unchanged.
- **Source level:** 2,011 required, 2,011 satisfied.
- **Semantics level:** 402 required = 197 satisfied + 0 failed + 2 missing + 203 invalid.
- **Records:** 2,411 executed, all passed; none unrequired and none excluded; 66 of 66 witnesses established.
- **Invalid probes:** each of the 203 is invalid only because pinned sub-runs have no run. None has an unexpected,
  duplicated or failed sub-run.
- **Independent reconciliation:** a Python check of the JSON verdict, sharing no code with the report, passed.

**The exact remaining list.** 965 pinned sub-runs, across 205 rows, have no run. The report's `problems` entry names
every one, and scratch `review/r2/remaining_after_switch.tsv` lists them by row and label.

| Family | Rows | Sub-runs without a run |
| --- | --- | --- |
| type_valid | 13 | 29 |
| type_invalid | 21 | 47 |
| operator_valid | 19 | 47 |
| operator_invalid | 2 | 7 |
| function_valid | 11 | 22 |
| function_invalid | 6 | 9 |
| operation_binding | 34 | 127 |
| operation_effects | 39 | 219 |
| operation_errors | 39 | 302 |
| error_contract | 10 | 40 |
| result_schemas | 9 | 62 |
| diagnostic_policy | 1 | 24 |
| failure_lifecycle | 1 | 30 |
| **Total** | **205** | **965** |

Next: P4c. The missing sub-runs are implemented family by family, types first, with each family gated. A faithful
sub-run that exposes an engine defect stops the task for an owner amendment.

### P4c, types — first execution, diagnosis, and a stop for an owner amendment

A usage limit interrupted the session again before this step. On resume, the worktree identity matched the handoff
(`7eb363bc…`) and no build process was running.

**Step.** `src/semantic_cases.rs` gained the 76 pinned type sub-runs that had no run: 29 valid forms in 13 rows and 47
rejections in 21 rows. Every oracle is taken from the canonical contracts:

- `types_v0.1.0.json`: the string-literal, pattern, temporal, numeric and SET-iteration profiles;
- the constructor registry in `operators_and_functions_v0.1.0.json`;
- `02_LEXICAL/02`, `/07` and `/08`; `03_TYPES_AND_VALUES/04`; `04_GRAMMAR/10` and `/12`; `06_STANDARD_LIBRARY/07`.

Where the contract asks for more than a round trip, the valid forms check it directly:

- exact decoding without Unicode normalization;
- unbounded exact arithmetic;
- the first iteration of a two-member SET, which fixes the whole iteration order;
- GLOB and REGEX matching, with negative controls.

Rejections expect the identifier canon pins. The two `ITEM`-block runs pin only the `grammar_or_schema` stage,
because canon names no single identifier for them.

**Gate.** T2-P4c-types-tests exited **101**: the production claim test found 8 failed probes. The diagnostic runs
T2-P4c-types-m8-diagnostic and T2-P4c-types-m8-json-diagnostic (both exit 0) show semantics 402 = 223 satisfied + 8
failed + 2 missing + 169 invalid. 26 of the 34 type rows the step touched are now satisfied.

**Diagnosis, read-only, before any change.**

| Row | Sub-run | Observed | Classification |
| --- | --- | --- | --- |
| type_valid/STRING | form/escaped-control-scalar | `error.source.tab` | **Authoring defect.** The input holds a raw TAB instead of the escape `\u0009`. The inputs of form/unicode-escape and form/surrogate-pair-escape, and one operand of form/no-unicode-normalization, were also written as decoded characters. Those runs pass but do not exercise escape decoding |
| type_invalid/OBJECT | malformed (duplicate key) | `error.field.duplicate` | **Oracle mistake.** The oracle expected `error.object.schema`. Object data keys are "OBJECT fields" (`04_GRAMMAR/12`); `error.field.duplicate` is the `grammar_or_schema` identifier for a repeated field; the earliest failing stage is selected (`06_STANDARD_LIBRARY/07`). The canonical example `11_DUPLICATE_FIELD` also expects `error.field.duplicate` |
| type_invalid/LIST[T], type_invalid/SET[T] | item-block | primary `error.block.field`, reached grammar | **Conformance-runner defect.** For a source that fails lexing or parsing, `runner.rs` records the stage as the lowercased Debug name (`grammarorschema`), not the registry spelling. `RejectsAtStage` therefore cannot match on that path |
| type_valid/REGEX | form/flag-i, form/flag-m, form/flag-s, form/flags-canonical-ims, form/ascii-only-case-folding | VERIFY FALSE | **Engine defect.** A flagged REGEX declared as DATA reads UNKNOWN: `lcl-semantics` `constructor_value` has no two-argument REGEX overload. Separately, `REGEX("a", "") == REGEX("a")` is FALSE, although omitted flags equal empty flags (`types_v0.1.0.json#/material_identity_contract/REGEX`). Inline flagged patterns match correctly |
| type_invalid/REFERENCE[T] | malformed, `REF("constant.original")` | accepted | **Engine defect.** `REFERENCE_CALL = "REF", "(", IDENTIFIER, ")"` (`04_GRAMMAR/10`). A string argument produces no diagnostic, even when the value is read |
| type_invalid/PATH | workspace-escape | accepted | **Engine defect.** "Escape produces error.value.out_of_range" (`03_TYPES_AND_VALUES/04`, `05_SEMANTICS/02`). A WORKSPACE PATH declared as DATA is accepted even when read. The runtime's own escape check is textual and is not reached on this path |
| type_invalid/MEASURE | unregistered-unit, `unit.furlong` | `error.operator.operand` | **Engine defect, subject to owner confirmation.** The MEASURE constructor registers `operator.operand` and `reference.unresolved`. Identifiers under reserved namespaces resolve only to the core registry, and unknown unit identifiers fail (`06_STANDARD_LIBRARY/09`). The checker reports `operator.operand` |

**Decomposition evidence.** Small scratch documents, built from the conformance runner's own task template, ran through
the real `lcl run`. The CLI was built in the owned target (T2-P4c-types-cli-build), ran with its working directory
inside scratch, and contains no file writes. Results:

- TRUE, as canon requires: inline `REGEX(…)` matching with `i`, `m`, `s` and `ims`; ASCII class folding; the
  ASCII-only folding negative; the no-flag negatives.
- Wrong: equality with a DATA-declared `REGEX("abc", "i")` is FALSE, and matching against it is UNKNOWN;
  `REGEX("a", "") == REGEX("a")` is FALSE; `REF("constant.original")` is accepted when read;
  `PATH(REF(workspace.case), "../outside.txt")` is accepted when read.

The worktree identity was `ed1679aa…` before and after every P4c gate and after the experiments. The only change since
P4b is `src/semantic_cases.rs`.

**Stop.** Four defects are in engine crates outside `lcl-conformance`, and one is in the conformance runner. Under the
pack rules, an engine defect exposed by a faithful sub-run pauses the task for an owner amendment. The failing sub-runs
stay in place as the reproduction. Nothing has been repaired yet, including the authoring defect and the oracle
mistake.

### P4c, types — amendment A-T2-3, repairs and closing gates

**Owner decision.** On 2026-09-14 the owner approved a standing narrow amendment, **A-T2-3**. Each defect that a faithful
canonical sub-run exposes is repaired under these rules:

- reproduced red first;
- minimal, one file per step;
- gated and reported.

Work still stops for any canonical change, public-interface change, new dependency or unclear canon.

**Steps.** Each step changed one file and was gated before the next. Every expected-red gate recorded the exact failing
set in advance, and every observed set matched it.

| # | File | Change | Gates |
| --- | --- | --- | --- |
| 1 | `lcl-conformance/src/semantic_cases.rs` | Authoring defect and oracle mistake. The four escape inputs are now LCL escapes, generated from `chr(92)` and checked byte by byte. The duplicate-key oracle is `error.field.duplicate` | T2-P4c-types-authoring-tests (101, expected: exactly the six reproduction rows fail; STRING and OBJECT pass) |
| 2 | `lcl-conformance/tests/runner_seam.rs` | Runner stage spelling reproduced: the canonical invalid example `11_DUPLICATE_FIELD` must report stage `grammar_or_schema` | T2-P4c-D1-red (101, expected: only the new test fails; `grammarorschema` observed) |
| 3 | `lcl-conformance/src/runner.rs` | A source skipped before resolution records `Stage::as_registry_str()` | T2-P4c-D1-green (runner_seam 14); T2-P4c-D1-conformance (101, expected: the LIST[T] and SET[T] rows pass); T2-P4c-D1-clippy |
| 4 | `lcl-parser/src/expr.rs` | A `REF` call with anything other than exactly one identifier is `error.grammar.invalid`. `REFERENCE_CALL` requires one `IDENTIFIER`, and `REF` is not a `CALLABLE` | T2-P4c-D3-parser-tests; -parser-clippy; T2-P4c-D3-conformance (101, expected: the REFERENCE[T] row passes) |
| 5 | `lcl-conformance/src/semantic_cases.rs` | Oracle strengthening (below) | T2-P4c-oracle-guard-tests (101, expected: exposes exactly `type_valid/PATH`); T2-P4c-oracle-guard-m8-json |
| 6 | `lcl-checker/tests/constructors_and_operations.rs` | Oracle correction for unregistered units (below), with a wrong-operand-family control | T2-P4c-D5-red (101, expected: only that test fails, observing `["error.operator.operand"]`) |
| 7 | `lcl-checker/src/expr.rs` | An unregistered unit identifier is an earlier-stage `error.reference.unresolved` | T2-P4c-D5-checker-tests; -checker-clippy; T2-P4c-D5-conformance (101, expected: the MEASURE row passes) |
| 8 | `lcl-semantics/src/value.rs` | One shared REGEX value constructor. Empty flags store the pattern alone, because omitted flags equal empty flags | T2-P4c-D2a-semantics-tests; -semantics-clippy |
| 9 | `lcl-semantics/src/eval.rs` | The two-argument REGEX constructor folds through the shared constructor, so a flagged pattern declared as DATA binds | T2-P4c-D2b-semantics-tests; -semantics-clippy; T2-P4c-D2b-conformance (101, expected); T2-P4c-D2b-m8-json (only form/flags-canonical-ims still fails) |
| 10 | `lcl-runtime/src/eval.rs` | The runtime builds REGEX values through the same constructor and splits them with the shared separator | T2-P4c-D2c-runtime-tests; -runtime-clippy; T2-P4c-D2c-conformance (101, expected: the REGEX row passes) |
| 11 | `lcl-semantics/src/eval.rs` | `PATH(REF(workspace), "relative")` folds to the runtime's spelling when the resolved target is contained. An escape binds no value | T2-P4c-D4a-semantics-tests; -semantics-clippy; T2-P4c-D4a-conformance (101, expected: only the escape run fails) |
| 12 | `lcl-semantics/src/scope.rs` | Preflight reports `error.value.out_of_range` for every WORKSPACE-form PATH whose resolved target leaves its root, or whose relative string is absolute | T2-P4c-D4b-semantics-tests; -semantics-clippy; T2-P4c-D4b-conformance (**0**, 71 tests) |

**Step 5, oracle strengthening.** A scratch experiment showed that a WORKSPACE-form PATH declared as DATA read UNKNOWN:
`REF(data.ok) == UNKNOWN` was TRUE, and comparing it with the constructed literal gave FALSE. The round trip
`REF(data.actual) == REF(data.expected)` therefore passed with both sides UNKNOWN. Every round-trip form now also
requires its DATA value to be neither UNKNOWN nor MISSING, and the WORKSPACE form compares against constructed
literals. The guard exposed exactly that one form, so no other round-trip form had been passing on a sentinel.

**Step 6, an oracle correction to an existing test.** `a_unit_argument_must_be_registered_and_in_its_declared_category`
expected `error.operator.operand` for `MEASURE(1, unit.furlong)`. Its only citation, "Core 0.1.0 admits only units in
the closed unit registry", establishes that the value is rejected, not which identifier rejects it.

- **Contract.**
  - `operators_and_functions_v0.1.0.json#/evaluation_contract`: "Wrong operator/function arity or operand family uses
    error.operator.operand. Unknown references retain error.reference.unresolved." Every row's errors add to that
    common list, which includes `error.reference.unresolved`.
  - `06_STANDARD_LIBRARY/09`: "Identifiers under reserved namespaces resolve only to this core registry."
  - The MEASURE constructor registers `error.reference.unresolved`.
- **Demonstrated mistake.** An unregistered `unit.*` identifier is an unknown reference, not a wrong operand family.
- **Discriminating controls.**
  - A registered unit is still accepted.
  - A registered unit of the wrong category keeps `error.numeric.unit_mismatch` for DURATION.
  - `MEASURE("1", unit.pixel)` keeps `error.operator.operand`.

**Closing gates.**

| Gate | Toolchain | Actual exit | Result |
| --- | --- | --- | --- |
| T2-P4c-types-close-conformance-clippy | Rust 1.98.1 | 0 | `lcl-conformance --all-targets -D warnings` |
| T2-P4c-types-close-fmt | — | 0 | `cargo fmt --all -- --check` |
| T2-P4c-types-close-workspace-tests | Rust 1.98.1 | 0 | Every workspace target: 144 targets, 1,520 tests |
| T2-P4c-types-close-msrv-workspace-tests | Rust 1.75.0 | 0 | The same 144 targets and 1,520 tests |
| T2-P4c-types-close-m8, T2-P4c-types-close-m8-json | Rust 1.98.1 | 0 | The production report, below |

**The production report after the type families.**

- **Claim:** `source_conforming`, unchanged.
- **Source level:** 2,011 required, 2,011 satisfied.
- **Semantics level:** 402 required = 231 satisfied + 0 failed + 2 missing + 169 invalid. After P4b it was 197
  satisfied and 203 invalid.
- **Independent reconciliation of the JSON verdict: PASS.**
  - The verdict's digest equals the mapping file's digest.
  - At each level the state lists are disjoint, and their union equals the inventory.
  - `problems` names exactly the unsatisfied probes.
  - No problem carries a failed, unexpected or duplicated sub-run.
- **Remaining:** 889 pinned sub-runs in 171 rows. That is 835 sub-runs in the 169 invalid rows, plus the 54 pins of
  the two rows with no population.

| Family | Remaining sub-runs |
| --- | --- |
| operator_valid | 47 |
| operator_invalid | 7 |
| function_valid | 22 |
| function_invalid | 9 |
| operation_binding | 127 |
| operation_effects | 219 |
| operation_errors | 302 |
| error_contract | 40 |
| result_schemas | 62 |
| diagnostic_policy | 24 |
| failure_lifecycle | 30 |
| **Total** | **889** |

**Observations for later families.** None of these is repaired, because no pinned sub-run exercises it yet:

- A WORKSPACE-form PATH value is stored as its joined absolute spelling in both evaluators.
  `material_identity_contract.PATH` instead identifies it by the workspace declaration and the relative string. The
  pinned `equality/path-address-form-identity` sub-runs will exercise this.
- The runtime's own WORKSPACE escape check is textual: any `..` segment or a leading `/`. A contained `src/../a.txt`
  evaluated at runtime would therefore be refused, although canon checks the resolved target.
- A REGEX value joins non-empty flags to its pattern with a NUL. A pattern may legally contain a NUL scalar written
  as an escape, so such a pattern followed by flag letters would split ambiguously.

Next: P4c, operators and functions (85 sub-runs). After them come error_contract and result_schemas, the three
operation families, and the diagnostic_policy and failure_lifecycle populations.

### P4c, operators and functions — sub-runs, one repair, and the owner's scope stop

**Step.** `src/semantic_cases.rs` gained the 85 pinned operator and function sub-runs:

- **MISSING operands** come from statically typed selections past the end of a LIST.
- **UNKNOWN operands** come from optional typed INPUTs that the runner's invocation seam supplies as UNKNOWN.
- **Ordering and units:** exact INTEGER/DECIMAL ordering and MEASURE unit rules.
- **Diagnostics and edge cases** for division, ROUND, SUM, EMPTY, the quantifiers and EXISTS.
- **Address form:** PATH address-form identity, and GLOB matching of a WORKSPACE PATH.
- **Access:** closed-schema property and index access.

**Authoring corrections before any engine change** (T2-P4c-opfn-tests exited 101; then T2-P4c-opfn-authoring-tests):

- **Optional INPUTs.** They lacked the DEFAULT canon requires: "Exactly one VALUE or SOURCE unless optional with
  DEFAULT". The engine's `error.block.conditional_requirement` was correct. DEFAULT replaces only MISSING, so a
  supplied UNKNOWN is still read as UNKNOWN.
- **Quantifier labels.** The quantifier rows used `special/missing` where their pinned label is
  `quantifier/missing-member`.
- **EXISTS witness.** The unresolved-optional-binding run now uses an optional INPUT whose SOURCE does not resolve.
  That input reads MISSING, and `06_STANDARD_LIBRARY/04` says EXISTS is "FALSE only for MISSING, including an absent
  optional binding".

**Diagnosis of the 13 failures that remained.** Each was decomposed read-only with scratch documents run through
`lcl run`:

| Sub-runs | Observed | Cause |
| --- | --- | --- |
| `order/measure-unit-mismatch` (×4) | `error.operator.operand` | The checker's `order_compatible` did not apply `evaluation_contract/unit_constraint` |
| `==`, `!=` `equality/path-address-form-identity` | VERIFY FALSE | A WORKSPACE-form PATH equals an absolute PATH with the same joined spelling. `material_identity_contract.PATH`: "Different forms are unequal" |
| `MATCHES` `match/glob-workspace-path` | VERIFY FALSE | A WORKSPACE-form PATH value keeps no root, so GLOB matching cannot use its relative segments |
| `-` `constraint/negative-duration-result` | VERIFY FALSE | DURATION subtraction yields a negative DURATION instead of `error.value.out_of_range` |
| `/` `division/declared-bound` | accepted | FIELD MINIMUM and MAXIMUM are parsed but not enforced on object values: a literal `2.0` against `MAXIMUM: 1` is accepted. A quotient written inside object DATA also reads UNKNOWN |
| `/` `division/host-capacity`, ROUND `round/host-capacity`, MATCHES `pattern/resource-limit`, SUM `sum/unit-mismatch` | VERIFY UNKNOWN | `lcl-completion` mirrors 8 identifiers and drops every other fault raised while evaluating a VERIFY assertion. Evaluated in an ACTION target, the runtime raises each registered diagnostic correctly |

**Repair under A-T2-3.** `lcl-checker/src/expr.rs` `order_compatible` now reports `error.numeric.unit_mismatch` when
two MEASURE operands have different units. Gates: T2-P4c-R1-checker-tests, T2-P4c-R1-checker-clippy and
T2-P4c-R1-conformance. The four order rows pass.

**A repair attempt, reverted.** A change to `lcl-runtime/src/eval.rs` rejected negative DURATION subtraction.
T2-P4c-R2-runtime-tests exited **101**: `abs_keeps_a_durations_exact_unit` evaluates
`ABS(DURATION(30, unit.second) - DURATION(90, unit.second))` through a negative intermediate DURATION. The change
was reverted to its exact prior text before any other step, and T2-P4c-R2-revert-runtime-tests passed. That VERIFY
sub-run would in any case still record UNKNOWN until the completion defect above is repaired.

**Owner decision: stop expanding scope.** On 2026-09-14 the owner directed that Task 2 complete only work its
contract requires, record the remaining gaps where the exit contract permits full conformance to stay BLOCKED, and
close. The exit contract permits exactly that: "Task 02 may report reporting-correctness complete while full
conformance remains BLOCKED, but that is not permission to declare this task's full objective passed or start
final-release Task 03." The remaining sub-runs are required for B4 to pass, not for this closure.

**Known failures kept as evidence.** The 9 failing sub-runs stay in the population as failed evidence. Two tests pin
exactly the 8 failed probes: `tests/production_report.rs` also pins each probe's failing sub-run labels, and
`tests/semantic_cases.rs` pins the failing groups. A repair or a new failure therefore fails the tests until the set
is reviewed.

| Gate | Actual exit | Result |
| --- | --- | --- |
| T2-P4c-pin-production-tests | 0 | production_report, 4 tests |
| T2-P4c-pin-semantic-tests | 0 | semantic_cases, 2 tests |
| T2-P4c-pin-conformance-tests | 0 | Every `lcl-conformance` target, 71 tests |
| T2-P4c-pin-conformance-clippy | 0 | `--all-targets -D warnings` |

**Closure deliverables:**

- `reports/LCL_Core_0.1.0_ERRATUM_2026-09-13.md` (DOC-01);
- `reports/tasks/LCL-TASK-0011_TO_0016_RETROSPECTIVE_INDEX.md` (HISTORY-01);
- `reports/implementation/LCL_REVIEW_COVERAGE_LEDGER.md` and `.tsv` (COVERAGE-01);
- the dated correction note in `impl/README.md`;
- the r2 and BLOCKED update to `reports/implementation/LCL_CONFORMANCE_OBLIGATIONS.md`;
- `reports/tasks/LCL-CLOSE-02_RESULT.md`.

### LCL-CLOSE-02 final gates

| Gate | Toolchain | Actual exit | Result |
| --- | --- | --- | --- |
| T2-FINAL-workspace-tests | Rust 1.98.1 | 0 | 144 targets: 1,520 passed, 0 failed, 1 ignored (the child helper its parent tests run) |
| T2-FINAL-msrv-workspace-tests | Rust 1.75.0 | 0 | The same 144 targets: 1,520 passed, 0 failed, 1 ignored |
| T2-FINAL-fmt | — | 0 | `cargo fmt --all -- --check` |
| T2-FINAL-clippy | Rust 1.98.1 | 0 | `cargo clippy --workspace --all-targets -- -D warnings` |
| T2-FINAL-m8, T2-FINAL-m8-json | Rust 1.98.1 | 0 | The production report, below |
| T2-FINAL-canonical-checksums | — | 0 | 175 matching |
| T2-FINAL-brand-checksums | — | 0 | 17 matching |
| T2-FINAL-protected | — | 0 | 192 of 192 protected files match: canonical 176, assets 4, releases 12 |

One command was rejected by the shell before any gate started, because its summary helper had a quoting error. No
gate ran and no log was written; the gates were then run as listed.

**The final production report** (T2-FINAL-m8 and T2-FINAL-m8-json):

- **Source snapshot:** `impl-tree-sha256:04a8e853175d389398b3bf1a5d2c3b23cb33af688c52e39fee1c1daba376f5aa`, over
  the 325 files under `impl/`, recorded at build time through `LCL_SOURCE_SNAPSHOT`.
- **Mapping:** digest `386c1499…6b20`.
- **Claim:** `source_conforming`.
- **Source level:** 2,011 required, 2,011 satisfied.
- **Semantics level:** 402 required = 261 satisfied + 8 failed + 2 missing + 131 invalid.
- **Witnesses:** 66 of 66 established.
- **Independent reconciliation: PASS.**
  - The verdict's digest equals the mapping file's digest.
  - At each level the four state lists are disjoint, and their union is exactly the inventory.
  - `problems` names exactly the unsatisfied probes.
  - No problem has an unexpected or duplicated sub-run.
- **Remaining:** 813 pinned semantic sub-runs in 141 rows have no passing run.

| Kind | Sub-runs | By family |
| --- | --- | --- |
| No run | 750 | error_contract 40, operation_binding 127, operation_effects 219, operation_errors 302, result_schemas 62 |
| No population | 54 | diagnostic_policy 24, failure_lifecycle 30 |
| Failed on a pinned engine defect | 9 | function_invalid 2, operator_invalid 3, operator_valid 4 |

The exact list is the appendix of `reports/tasks/LCL-CLOSE-02_RESULT.md`.

**Recorded but not repaired:**

- **DATA division.** The semantic layer does not fold division in declared DATA, so such a value reads UNKNOWN.
- **Textual escape check.** The runtime's WORKSPACE escape check is textual.
- **REGEX separator.** A NUL joins non-empty REGEX flags to the pattern, which is ambiguous for a pattern that contains
  an escaped NUL.
- **Possible oracle question.** The earlier oracle `match/glob-absolute-path-false` expects FALSE for an absolute PATH
  matched against a GLOB. The GLOB profile's input rule says an input that cannot supply a WORKSPACE root "uses
  error.operator.operand". This is left for owner review.

**State at close.**

- **Git:** HEAD `298d1e2b01a0dcd4dbecb5bf401143948b1cf716`. Nothing is staged, and this task made no Git write.
- **Scratch temporary directories:** removed by exact path (five `lcl-install-*` and two experiment directories).
- **Phase 3 deviation:** the build-cache files it wrote under `impl/target/debug` remain for the owner.

LCL-CLOSE-02 is closed as reporting-correct with full semantic conformance BLOCKED. LCL-CLOSE-03 stays locked.

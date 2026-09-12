# LCL-RESIDUAL-REPAIR-01 — execution record

Date: 2026-09-12. Status: **STOPPED AT B3 — AMENDMENT A3 REQUIRED FOR SET
MATERIALIZATION; A1's focused checker/direction tests pass; A2 remains approved;
B1 / VR-01 repaired for the approved
Linux x86_64 process-group scope; B2 / VR-04 repaired; B3–B5 and final acceptance
remain open**.

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

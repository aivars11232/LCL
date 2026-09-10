# LCL-TASK-0018 IMPLEMENTATION RESULT

**Status: COMPLETE**

M9 — CLI, document/project/package/workspace tooling, `.lcl` integration and the
stable engine protocol.

## Repository state

| Fact | Value |
| --- | --- |
| Root | `/mnt/F/LCL` |
| Branch | `main` |
| Baseline HEAD (task start) | `a9943a4a9f09af2a7d211daade81b3d2eb969f0d` ("LCL task 17 finish") |
| HEAD at this report | `6935cbdb…` ("LCL task 18 p1") |
| Upstream | `a9943a4a9f09af2a7d211daade81b3d2eb969f0d` |
| Working tree | eleven modified files, uncommitted; nothing staged, committed, tagged or pushed by this task |

The user committed phases A through E mid-task as `LCL task 18 p1`. The phase F
work and the formatting pass over it are in the working tree awaiting the user's
Git closure.

## Predecessor evidence

LCL-TASK-0017 was verified closed from actual repository state before planning,
not inferred from numbering: 1015 workspace tests passing, `cargo fmt --all --
--check` clean, `cargo clippy --offline --workspace --all-targets -- -D warnings`
clean, canonical validator 31 PASS / 0 FAIL / 0 BLOCKED / 2 OUT_OF_SCOPE with
`release_ready: true`, `sha256sum -c SHA256SUMS.txt` exit 0, and
`cargo run -p lcl-completion --example m8_report` carrying all thirteen valid
examples to one terminal status each. Every count matched
`reports/tasks/LCL-TASK-0017_RESULT.md` exactly.

## Internal phases

All six completed.

| Phase | Subject | Gate |
| --- | --- | --- |
| A | Headless product API and protocol | green |
| B | CLI | green |
| C | Project and document loader | green |
| D | Package, version and checksum tooling | green |
| E | `.lcl` system and tool integration | green |
| F | Headless end-to-end gate | green |

Phases were implemented A, C, D, B, E, F rather than in letter order. The CLI is
a caller of both the facade and the project loader, so building it before them
would have meant writing it against interfaces that did not exist. Each phase's
own gate still passed before the next began, which is what the contract requires.

## Architecture

Three new crates, matching the names the pack's target tree reserves for this
task.

| Crate | Role |
| --- | --- |
| `lcl-protocol` | The stable engine facade and the machine-readable record |
| `lcl-project` | Project roots, the filesystem source provider, cache and lock |
| `lcl-cli` | The `lcl` binary |

**One refinement to the approved plan, which removed a dependency rather than
adding one.** The plan had `lcl-protocol` depending on `lcl-project`. It does
not: the facade takes `&dyn SourceProvider`, which is `lcl-resolver`'s trait, so
it needs no concrete provider and is filesystem-independent as well as
UI-independent. `lcl-cli` depends on both. Same three crates, same
responsibilities, one edge fewer.

**Protocol boundary: a direct Rust library plus a stable JSON record, and no
transport.** A Rust consumer calls `Engine`; any other consumer runs
`lcl --machine` and reads the same records, produced by the same code.
Committing to JSON-RPC or LSP now would freeze a transport before LCL-TASK-0019
chooses a toolkit, which is the stop condition this task names. Adding one later
is additive.

**No dependency was added.** The workspace's std-only policy holds. Argument
parsing and the JSON writer are written out, for the reason `impl/Cargo.toml`
states: the trust root carries no third-party supply-chain surface, and a CLI
that put a parsing crate in front of the binary that reads it would undo that.

## Files changed, in change order

Phases A–E (committed in `6935cbd`):

1. `impl/crates/lcl-protocol/Cargo.toml`
2. `impl/crates/lcl-protocol/src/json.rs` — the deterministic JSON writer
3. `impl/crates/lcl-protocol/src/record.rs` — the record types and their projection
4. `impl/crates/lcl-protocol/src/engine.rs` — the one staged walk
5. `impl/crates/lcl-protocol/src/lib.rs`
6. `impl/Cargo.toml` — three new workspace members
7. `impl/crates/lcl-protocol/tests/{common,json_writer,engine_stages}`
8. `impl/crates/lcl-semantics/src/lib.rs` — `Preflight::value_of`
9. `impl/crates/lcl-protocol/src/inputs.rs` and `tests/supplied_inputs.rs`
10. `impl/crates/lcl-project/{Cargo.toml,src/provider.rs,src/cache.rs,src/manifest.rs,src/lock.rs,src/lib.rs}`
11. `impl/crates/lcl-project/tests/{common,source_boundary,manifest_and_lock}`
12. `impl/crates/lcl-cli/{Cargo.toml,src/exit.rs,src/args.rs,src/render.rs,src/main.rs,src/syntax.rs}`
13. `impl/crates/lcl-cli/tests/{common,commands_and_exit_codes,machine_output,projects_and_capabilities}`
14. `impl/integration/linux/{lcl.xml,install.sh,uninstall.sh}` and `impl/integration/README.md`
15. `impl/crates/lcl-cli/tests/lcl_integration.rs`

Phase F (uncommitted):

16. `impl/crates/lcl-cli/tests/clean_environment.rs` — the end-to-end gate
17. `impl/crates/lcl-cli/src/main.rs` — machine output no longer double-spaced
18. `impl/README.md` — the M9 section, crates table, title and usage
19. formatting pass over the phase-B–F files

No file under `canonical/` was modified. No canonical repair was required, and no
canonical contradiction was found.

## Behavior implemented

**A stable facade.** `Engine::open` verifies the package through
`SpecPackage::open` — version pin, internal integrity, external trust anchor —
and loads all seven layers' contracts once. `check`, `validate`, `inspect` and
`run` share one staged walk that advances only while the earlier stage produced
no unhandled diagnostic, and each stops at a canonical boundary: step 5, step 9,
step 9 structurally, step 13. Before this crate that assembly sequence was
written out by hand in the conformance runner, in every milestone report example
and in six `tests/common/mod.rs` files; there is now one copy.

**A machine record.** Exact source identity and SHA-256 for every loaded unit,
authoritative byte spans with derived line and column beside them rather than
instead of them, registered diagnostic identifiers with the stage and
`default_status` the emitting layer resolved, the execution's invocations, events
and step count, and completion's checks, evidence, verdict, outputs, terminal
status and reason. Values are carried as the engine's own rendering; projecting
the value model into JSON types would be a second value model of the language in
the product layer.

**A CLI.** Six commands plus `package` and `syntax`, a closed five-code exit
table, human rendering and `--machine` JSON. The specification package comes from
`--spec`, then `LCL_SPEC`, then the manifest, and the command stops rather than
searching. A run grants the host nothing; each `--allow-*` flag adds exactly what
it names, and an implementation profile is installed only alongside the
capability it describes.

**A project.** An explicit root, root-relative source identity, and containment
decided on the resolved path after canonicalisation. `URI` imports resolve from a
content-addressed cache or not at all; nothing fetches. `lcl.lock` records the
package identity and every unit's digest, and `package verify` and `--locked`
report drift rather than absorbing it.

**`.lcl` integration.** A shared-mime-info package for `text/x-lcl` with a magic
rule matching the `LCL:` header, a user-scoped installer and its exact reverse,
and `lcl syntax --machine`, which emits the closed vocabulary from the loaded
lexicon rather than from a checked-in word list.

## Canonical sources implemented

- `01_FOUNDATION/03_NORMATIVE_PROCESSING_MODEL.txt` — the thirteen steps, the
  closed stage order, and "advances only while its earlier applicable stage has
  no unhandled unsuppressed diagnostic", which is the shape of the staged walk
- `05_SEMANTICS/02_SCOPE_TARGET_WORKSPACE_AND_SOURCE.txt` — "Ambient current
  directory and implied nearby files do not exist in portable LCL", and
  containment: "textual prefix alone does not establish containment"
- `07_VERSIONING_AND_EXTENSIONS/02_IMPORT_VERSION_CHECKSUM_AND_NAMESPACE.txt` —
  "SOURCE PATH resolves relative only to importing file or explicit WORKSPACE",
  and the `algorithm:lowercase_hex` checksum form
- `07_VERSIONING_AND_EXTENSIONS/05` — no ignore-unknown mode, which is why an
  unknown manifest key refuses
- `02_LEXICAL/01_CHARACTER_ENCODING_AND_SOURCE_TEXT.txt` — UTF-8 without a BOM,
  LF only, a required final LF, and "must not silently repair, normalize,
  re-indent, re-quote, or otherwise rewrite source before validation"
- `04_GRAMMAR/01_DOCUMENT_FORM_AND_TOP_LEVEL_ORDER.txt` — "Every document starts
  with LCL then SPECIFICATION", which grounds the MIME magic rule
- `05_SEMANTICS/06` — the resolution order that puts an explicit `VALUE` ahead of
  a supplied datum and a `DEFAULT` behind it
- `05_SEMANTICS/10` — producer completion and domain outcome as separate axes,
  which the exit table reproduces
- `block_schemas_v0.1.0.json#/schemas/INPUT` — "Exactly one VALUE or SOURCE
  unless optional with DEFAULT"
- `statuses_and_errors_v0.1.0.json#/errors` — registered stages and
  `default_status`, copied and never rewritten
- `operations_v0.1.0.json#/axis_contract/implementation_profile` — a profile is a
  claim an implementation exists
- `03_TYPES_AND_VALUES/08` and `formats_encodings_units_v0.1.0.json` —
  `format.lcl` as a format identifier that says nothing about file names

## Verification

| Gate | Result |
| --- | --- |
| `cargo test --offline --workspace --all-targets` | **1155 passed, 0 failed, 0 ignored**, 110 suites |
| `cargo fmt --all -- --check` | clean |
| `cargo clippy --offline --workspace --all-targets -- -D warnings` | clean |
| `validate_release.py --scope all` | 31 PASS / 0 FAIL / 0 BLOCKED / 2 OUT_OF_SCOPE, `release_ready: true` |
| `sha256sum -c SHA256SUMS.txt` | exit 0, 175 files OK |
| `lcl run` over all 13 valid canonical examples | each reached completion with exactly one terminal status |

Baseline was 1015 tests; this task adds 140. `lcl-protocol` contributes 47,
`lcl-project` 35 and `lcl-cli` 58.

Runtime evidence, not inspection: every CLI test runs the real binary as a
subprocess with `env_clear()`, so no inherited variable and no working directory
can carry a result. The capability pair actually writes a file to disk with the
grant and actually does not without it.

## Acceptance criteria

| Criterion | Result | How |
| --- | --- | --- |
| CLI and direct engine API produce equivalent diagnostics/results | PASS | executed — `machine_output.rs` compares CLI output against the facade **byte for byte** for an accepted document, four rejected ones, a completed run and a supplied input |
| Machine output includes source identity, byte spans, diagnostic IDs/stages/statuses and execution/evidence records | PASS | executed — each read back out of the emitted JSON with the trust root's strict reader |
| Project loading never includes unreferenced ambient files | PASS | executed — a project holding two extra documents loads exactly the two that were named, asserted through both the provider's own record and the CLI's unit list |
| Version/checksum/import behavior is reproducible | PASS | executed — byte-identical output across runs and across project locations; lock, verify and `--locked` detect an edited import |
| `.lcl` source is recognized without changing language semantics | PASS | executed — the shipped MIME package matches what `lcl syntax` reports, its magic rule matches all thirteen examples, and the same bytes produce the same record under four different file names |
| No language rule lives only in CLI/project glue | PASS | statically proven by construction, and executed at the seam — the one rule the tool needed was added to `lcl-semantics` as `Preflight::value_of`, and the byte-for-byte equivalence tests would fail if the CLI decided anything the engine did not |

## The one change to a closed milestone

`lcl-semantics` gains one additive public method, `Preflight::value_of`, which
evaluates an already-parsed expression to a `Value` by calling the private
`literal_value` that `DATA` and `INPUT` resolution already use at step 7. It
changes no existing path and adds no rule. Without it, turning `--input
id=<expression>` into a value would have meant a second literal reader living in
a CLI, which the "no language rule in glue" criterion forbids. Flagged here
because M5 is otherwise closed.

## Defects found and repaired

**Machine output was double-spaced.** `pretty()` ends with one line feed and
`println!` added a second, so `lcl --machine` emitted a blank final line and its
output was not byte-identical to the facade's. Found by the equivalence test,
which is the test that would notice. Repaired in
`impl/crates/lcl-cli/src/main.rs`; all seven machine emitters now use `print!`.

## Defects found and NOT repaired

None. No pre-existing defect in an earlier milestone was exposed by this work.
The four M5/M6/M7 gaps that LCL-TASK-0017 recorded remain open and remain outside
this task's scope.

## Expected results corrected, with the authority

Four test expectations of mine were wrong and the engine was right. Each is
recorded because the contract requires the authority that proves the previous
expectation wrong, not merely that the test now passes.

1. **`error.conflict.hard` is a resolution-stage identifier.** I expected
   `validation`, because preflight decides it at step 6.
   `statuses_and_errors_v0.1.0.json#/errors/error.conflict.hard` gives
   `"stage": "resolution"`, and the preflight layer copies the registered stage
   verbatim. The test now asserts both axes separately: the walk reached
   preflight, and the identifier's stage is `resolution`.
2. **A supplied value cannot override a declared `VALUE`.** `05_SEMANTICS/06`
   orders resolution "1. explicit VALUE; 2. resolved SOURCE/INPUT/STATE/MEMORY/
   CONTEXT", so my fixture could never have shown a supplied datum being read.
   And `block_schemas_v0.1.0.json#/schemas/INPUT` requires "Exactly one VALUE or
   SOURCE unless optional with DEFAULT", so deleting the `VALUE` produced a
   document the grammar stage rejects. The fixture is now an optional `INPUT`
   with a `DEFAULT`, which tests the precedence as well as the plumbing.
3. **Leading whitespace in a supplied expression is refused, not trimmed.**
   `02_LEXICAL/01` forbids silently repairing source, and a supplied expression
   is source. The test now asserts that a trailing space is admitted, because the
   fragment contract allows "optional whitespace" after the expression, and a
   leading one is refused.
4. **`core.write` does not create a missing file by default.**
   `operations_v0.1.0.json` gives `create_if_missing` a default of `false`. The
   capability fixture now declares it, and the failure it produced beforehand was
   correct behavior.

A fifth expectation was corrected in the opposite direction, and is the one worth
reading. The clean-environment gate asserted that seven canonical examples
succeed through the CLI, copied from the facade's own suite. Five do. The
difference is the host, not a defect: the facade suite installs every registered
profile against a deterministic in-memory host, while the CLI grants nothing
unless a flag says so, so two examples that reach mock fixtures report
`error.host.constraint` here instead. Rather than weaken the assertion to a
number, the test now also requires that **every** non-succeeding example fails for
a host reason — an uninstalled profile, an ungranted capability, or an unsupplied
datum — and never because the engine could not lex, parse, resolve or check it. A
regression in the language would break that second assertion even if the count
still held.

## Remaining limitations, and who owns them

- **No transport.** Deliberate, and explained above. `LCL-TASK-0019` owns the
  decision, and adding a socket or a language server later changes neither the
  records nor the facade.
- **No fetching.** A `URI` import resolves from the local cache or not at all. No
  canonical rule requires a fetcher, and the architecture contract forbids
  resolver-owned browsing. An operator vendors bytes once with `package vendor`.
- **No `.desktop` entry.** A desktop entry opens a file in an application, and
  that application is `LCL-TASK-0019`'s work.
- **No editor, workspace or debugger UI, and no release hardening.** Owned by
  `LCL-TASK-0019` and `LCL-TASK-0020` respectively.

## `git diff --stat`

Against the task baseline `a9943a4`, the complete task:

```
 39 files changed, 8503 insertions(+), 3 deletions(-)
```

Uncommitted at this report, phase F and the formatting pass:

```
 11 files changed, 399 insertions(+), 67 deletions(-)
```

## Suggested commit message

```
Add the LCL CLI, project tooling and engine protocol

Add lcl-protocol, the stable headless engine surface: one assembled engine
that carries source through every canonical stage in order and stops where
the requested command says, and one machine-readable record carrying exact
source identity, byte spans, registered diagnostic identifiers and the
execution and completion evidence a UI needs.

Add lcl-project: an explicit project root, a filesystem source provider that
can answer only for a source a document named, root-relative identity that
does not depend on where the project lives, and the content-addressed cache
and lock file that make a multi-document project reproducible.

Add lcl-cli, the `lcl` binary: check, validate, run, inspect, package and
syntax over that engine, with a closed exit-code table, human rendering and
--machine JSON. It grants the host nothing unless a flag says so.

Add Preflight::value_of to lcl-semantics so a supplied --input becomes a
value through the language's own step-7 evaluation rather than through a
second literal reader in a tool.
```

The commit would contain only this task's changes.

## Confirmations

- No background, delegated, parallel, asynchronous, worker or sub-agent was used
  at any point, in planning or implementation. Every command ran in one
  sequential session.
- Nothing was staged, committed, tagged or pushed by this task. The user
  controls Git closure, and committed phases A–E themselves mid-task.
- `canonical/` was not modified. Its validator and checksums are unchanged.
- No dependency was added; the workspace remains std only.
- No test was weakened, skipped, ignored or reclassified. Five expected results
  were corrected, each against the exact canonical authority that proves the
  previous expectation wrong, and one of those corrections strengthened the
  assertion rather than relaxing it.
- Nothing was installed on the system. `integration/linux/install.sh` was written
  and syntax-checked, never executed.

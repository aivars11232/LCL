# LCL-TASK-0019 IMPLEMENTATION RESULT

**Status: COMPLETE**

M10 — the LCL editor/workspace, live diagnostics, execution inspection and
debugger UI.

## Repository state

| Fact | Value |
| --- | --- |
| Root | `/mnt/F/LCL` |
| Branch | `main` |
| Baseline HEAD (task start) | `fe49902892a0d0573401bd2f19937f78b1658c80` ("LCL task 18 finish") |
| HEAD at this report | `fe49902892a0d0573401bd2f19937f78b1658c80`, unchanged |
| Upstream | same commit |
| Working tree | eight modified files and one new crate, uncommitted; nothing staged, committed, tagged or pushed |

## Predecessor evidence

LCL-TASK-0018 was verified closed from actual repository state before planning,
not inferred from numbering. Its result report records **Status: COMPLETE** with
all six internal phases green. Re-run independently at the start of this task:

| Gate | Result |
| --- | --- |
| `cargo test --offline --workspace --all-targets` | 110 suites, **1155 passed, 0 failed, 0 ignored** — exactly the count task 18 reported |
| `cargo fmt --all -- --check` | clean |
| `cargo clippy --offline --workspace --all-targets -- -D warnings` | clean |
| `validate_release.py --scope all` | 31 PASS / 0 FAIL / 0 BLOCKED / 2 OUT_OF_SCOPE, `release_ready: true` |
| `sha256sum -c SHA256SUMS.txt` | exit 0, 175 files |

The six-file M1 dirty state the pack expects was already committed; nothing
needed preserving.

## Owner decisions obtained before any implementation

Task 19 names an explicit stop condition: stop before implementation if UI
technology is an unresolved owner decision. Three questions were put to the
owner and answered:

1. **Std-only local web workspace.** A loopback HTTP server on `std::net`
   serving a hand-written browser frontend. **Zero new Rust dependencies.**
2. **Effect-boundary debugging, no engine change** to the closed runtime.
3. **Location `impl/crates/lcl-workspace`,** inside the existing cargo
   workspace.

No framework was chosen silently, and the workspace's stated dependency policy —
"std only … no third-party supply-chain surface" — is unchanged.

## Internal phases

All six completed, in order, each gate green before the next began.

| Phase | Subject | Gate |
| --- | --- | --- |
| A | UI technology and application contract | green |
| B | Project and editor shell | green |
| C | Live language intelligence | green |
| D | Run, check and inspection views | green |
| E | Debugging and capability UX | green |
| F | UI/CLI equivalence | green |

## Files changed, in change order

**Phase A — contracts**

1. `impl/crates/lcl-protocol/src/record.rs` — `NavigationRecord`,
   `DeclarationRecord`, `ReferenceRecord`, span/position JSON helpers
2. `impl/crates/lcl-protocol/src/engine.rs` — the `navigation()` projection
3. `impl/crates/lcl-protocol/src/lib.rs` — exports
4. `impl/crates/lcl-protocol/tests/navigation.rs` — 12 tests
5. `impl/crates/lcl-protocol/src/host.rs` — the shared `surface()` and `Granted`
6. `impl/crates/lcl-cli/src/{args.rs,main.rs}` — call the shared `surface()`
7. `impl/Cargo.toml`, `impl/crates/lcl-workspace/Cargo.toml` — one new member
8. `impl/crates/lcl-workspace/src/{http.rs,server.rs}` — transport and gates
9. `impl/crates/lcl-workspace/tests/transport.rs` — 14 tests

**Phase B — project and editor shell**

10. `impl/crates/lcl-workspace/src/document.rs` — byte-faithful read and atomic write
11. `impl/crates/lcl-workspace/src/project.rs` — `Workspace`, tree, spec location
12. `impl/crates/lcl-workspace/tests/persistence.rs` — 15 tests
13. `impl/crates/lcl-workspace/src/routes.rs` — the route table
14. `impl/crates/lcl-workspace/assets/{index.html,app.css,app.js}` — the frontend
15. `impl/crates/lcl-workspace/src/{lib.rs,main.rs}` — the library and the binary
16. `impl/crates/lcl-workspace/tests/routes.rs` — 10 tests

**Phase C — language intelligence**

17. `impl/crates/lcl-workspace/src/intelligence.rs` — token spans from the real lexer
18. `impl/crates/lcl-workspace/src/routes.rs` — `/api/tokens`, `/api/check`, `/api/inspect`
19. `impl/crates/lcl-workspace/assets/app.js` — painting, diagnostics, navigation
20. `impl/crates/lcl-workspace/tests/intelligence.rs` — 9 tests

**Phase D — run and inspection**

21. `impl/crates/lcl-workspace/src/execution.rs` — run sessions, pause gate, wrappers
22. `impl/crates/lcl-protocol/src/engine.rs` — `Engine::run_with`
23. `impl/crates/lcl-workspace/src/routes.rs` — `/api/run`, `/api/events`, `/api/answer`
24. `impl/crates/lcl-workspace/assets/app.js` — execution and completion views
25. `impl/crates/lcl-workspace/tests/execution.rs` — 6 tests

**Phase E — debugging and capability UX**

26. `impl/crates/lcl-workspace/assets/app.js` — breakpoints, consent, stepping, grants
27. `impl/crates/lcl-workspace/tests/debugging.rs` — 8 tests

**Phase F — equivalence**

28. `impl/crates/lcl-workspace/tests/equivalence.rs` — 6 tests
29. `impl/crates/lcl-workspace/src/{server.rs,main.rs}` — opt-in `--log`
30. `impl/README.md` — the M10 section, crates table and title

No file under `canonical/` was modified. No canonical repair was required, and
no canonical contradiction was found.

## Behavior implemented

**A workspace.** One project root, one engine opened once against a verified
package, a document tree, and open/create/save/reload over `.lcl` files. Saving
is atomic — a temporary file in the same directory, then a rename — so a crash
leaves the previous version rather than half of the new one.

**Byte fidelity, enforced rather than assumed.** `02_LEXICAL/01` requires UTF-8
without a BOM, LF only, a required final LF, and forbids silently repairing
source. An editor is the usual place that rule dies. This one refuses: a save
carrying a CR or a BOM is rejected with the reason and the file is left exactly
as it was. The one thing it adds is a missing final line feed, stated in the
reply rather than done quietly.

**A frontend that does not know the language.** No keyword list, no grammar, no
pattern matching LCL. The page asks for token spans, `lcl-lexer` produces them,
the page paints them. Diagnostics arrive with the registered identifier, the
stage, the `default_status` and the exact byte span the emitting layer decided.
Navigation follows the resolver's own bindings: go-to-definition is a pointer
into a declaration list, never a text search.

**Byte offsets stay normative end to end.** Spans cross the wire as byte offsets
with the engine's derived position beside them, and the page maps bytes to
screen positions through one index built per document.

**Runs, watched.** A run happens on its own thread and streams what it observed
over server-sent events: operations dispatched, permissions decided, effects
performed, then the complete report. The views render `StructureRecord`,
`ExecutionRecord` and `CompletionRecord` fields; the terminal status is shown
with the completion layer's own stated reason, and `Accepted` is labelled as
"no unhandled diagnostic through this stage", never as "it ran".

**A debugger at the honest seam.** Wrapping `Operations` and `Host`, the run
pauses before an operation is dispatched and before an effect crosses into the
host, presents the whole request — operation, target, parameters, category,
possible effects, the rule that authorized it, the exact source locus — and
waits. Neither wrapper decides anything: a denial returns the same `Refusal` the
host would, and the engine decides what that means. After a run, the recorded
invocations step forward and back with their spans highlighted.

**No control that does not work.** Single-stepping plan nodes is not offered,
and the Capabilities panel says why: the runtime executes an accepted plan in one
call, and pausing between nodes would mean changing a closed milestone.

**A guarded socket.** Loopback only, ephemeral port, a 256-bit session token per
launch required on every request, a `Host` header that must name the address
actually bound, an `Origin` that must be this same origin, `default-src 'self'`,
`nosniff`, and no CORS header at all. Ambiguous request framing is refused rather
than guessed at.

## Canonical sources implemented

- `02_LEXICAL/01_CHARACTER_ENCODING_AND_SOURCE_TEXT.txt` — UTF-8 without a BOM,
  LF only, a required final LF, and "must not silently repair, normalize,
  re-indent, re-quote, or otherwise rewrite source before validation", which is
  what the editor refuses to do
- `05_SEMANTICS/02_SCOPE_TARGET_WORKSPACE_AND_SOURCE.txt` — "Ambient current
  directory and implied nearby files do not exist in portable LCL": the file
  tree is a browser, and nothing it lists enters a program unless a document
  names it
- `05_SEMANTICS/10` — producer completion and domain outcome as separate axes,
  which is why a stage verdict is never displayed as a result
- `01_FOUNDATION/03_NORMATIVE_PROCESSING_MODEL.txt` — the closed stage order the
  `reached` field reports
- `statuses_and_errors_v0.1.0.json#/errors` — the registered identifiers,
  stages and the four `default_status` values the severity map is closed over
- `block_schemas_v0.1.0.json#/schemas/FORBID` — "Hard prohibition", and
  `#/schemas/ALLOW` — "Permission only … Never defeats FORBID by itself", which
  is the language gate consent cannot open
- `operations_v0.1.0.json#/axis_contract/implementation_profile` — a profile is
  a claim an implementation exists, so one is installed only beside the
  capability it describes
- `02_LEXICAL/03` — loop-local identifiers are not declarations, so a
  `FOR EACH` binding navigates to its header span and carries no declaration index
- `symbols_v0.1.0.json#/excluded` — "No symbolic comments", which is why there
  is no comment token and no comment styling

## Verification

| Gate | Result |
| --- | --- |
| `cargo test --offline --workspace --all-targets` | 120 suites, **1235 passed, 0 failed, 0 ignored** |
| `cargo fmt --all -- --check` | clean |
| `cargo clippy --offline --workspace --all-targets -- -D warnings` | clean |
| `validate_release.py --scope all` | 31 PASS / 0 FAIL / 0 BLOCKED / 2 OUT_OF_SCOPE, `release_ready: true` |
| `sha256sum -c SHA256SUMS.txt` | exit 0, 175 files OK |
| `git status --short canonical/` | empty |

Baseline was 1155 tests; this task adds **80**. `lcl-workspace` contributes 68
and `lcl-protocol` 12.

**Runtime evidence, not inspection.** Every workspace test binds a real socket
and drives it with a real `TcpStream` speaking real HTTP. The capability tests
actually write a file to disk with a grant and actually do not without one. The
equivalence tests run the `lcl` binary as a subprocess with `env_clear()`.

**Real target-platform evidence, Arch Linux / KDE Plasma 6 / Wayland.** The
`lcl-workspace` binary was launched over a project of all thirteen canonical
examples and driven with headless Firefox. Its request log shows the browser
carrying out the full sequence:

```
GET /  ·  GET /app.css  ·  GET /app.js  ·  GET /api/session
GET /api/documents  ·  GET /api/document  ·  POST /api/tokens  ·  POST /api/inspect
```

A screenshot confirms the page renders with its own stylesheet, the loaded
project root and the package identity `0.1.0 · authoritative · 00d648b16293`.
Firefox's `--screenshot` mode exits before the async boot finishes, so the
screenshot shows an earlier frame than the request log proves was reached; the
log is the evidence that the browser drove the engine end to end.

## Acceptance criteria

| Criterion | Result | How |
| --- | --- | --- |
| Create/open/save/reload LCL projects and `.lcl` files | PASS | executed — all thirteen canonical examples survive a save-and-reload **byte for byte**; a new project is created and reopens; a directory without a manifest still opens |
| Live diagnostics exactly match engine identifiers/stages/statuses/spans | PASS | executed — identifiers, stages, `default_status` and spans read back out of the served JSON, and the span asserted to cover the exact bytes it names |
| Reference/navigation from the resolver, not UI heuristics | PASS | executed — following a binding lands on a declaration whose `id_span` covers its own identifier; the projection is asserted to list exactly what the resolver indexed |
| Run/check behavior matches the CLI for identical inputs and capabilities | PASS | executed — **byte-for-byte** against the `lcl` binary for `check`, `validate` and `inspect` over thirteen valid examples and three rejected ones |
| Execution/state/evidence/output views reflect engine records | PASS | executed — every valid example runs to exactly one terminal status with a stated reason; views render report fields only |
| Capability consent cannot bypass language authorization | PASS | executed — a `FORBID`den write with the host grant given is refused at **preflight**, the file is never created, and no pause is ever offered |
| Cancellation/reload/restart does not corrupt project or engine state | PASS | executed — after a cancelled run every document still reads back and a second run completes; a denied write leaves a pre-existing file byte-identical |
| No second parser/type checker/evaluator in UI code | PASS | executed and statically proven — the byte-for-byte gate would fail if the UI decided anything the engine did not, and a test asserts the analysis route returns `Report::to_json` unaltered |

## Additions to closed milestones, and why

Three, all additive, all in M9's `lcl-protocol`, following the precedent task 18
set when it added `Preflight::value_of` to closed M5 rather than duplicating a
rule in a tool.

1. **`Report::navigation`.** The engine built a `Resolved` and dropped it, so no
   reference or declaration span reached a consumer. Without this, acceptance
   criterion 3 could only have been met by a text search. The projection copies
   `Declaration` and `Binding` and computes nothing. It attaches as soon as the
   resolver has run, **including when resolution rejected the document**, because
   an editor needs to navigate a broken file most of all.
2. **`surface()` moved from the CLI binary into `lcl-protocol`.** Acceptance
   criterion 4 requires the UI's run to match the CLI's for identical
   capabilities. Two copies that agree today are not a guarantee; one copy both
   call is. The CLI keeps its flag parsing and calls the shared function.
3. **`Engine::run_with`.** `run` with the operation dispatcher as a parameter,
   mirroring `Runtime::execute_with`, which has always taken one. A debugger
   observing the dispatch seam would otherwise have to reassemble the
   thirteen-step walk, which is the duplication the contract forbids.

No dependency was added. The workspace remains std only.

## Defects found and repaired

1. **A new document in a directory that did not exist yet was refused.** The
   path resolver canonicalised the parent, and a parent that does not exist
   cannot be canonicalised. Rewritten to normalise the relative path lexically,
   canonicalise the deepest ancestor that does exist, prove containment on that,
   and rebuild the rest. Two regression tests: the deep new document, and a
   symbolic link out of the project, which must still be refused after the
   rewrite.
2. **The page loaded unstyled and never started.** The browser fetches the
   stylesheet and the script itself, and those requests carried no session
   token, so both were refused with 403. Found by the first headless Firefox
   run, not by any Rust test. The served page is now stamped with its token.
   Regression test: fetch the page, extract every local URL it references, and
   fetch each one exactly as a browser would.
3. **Live analysis silently did nothing.** An empty `runAnalysis` stub left over
   from an earlier phase sat later in the same file than the real one, and
   JavaScript hoisting made the stub win. No error was raised anywhere; the page
   simply stopped making requests after the first token fetch. Found by reading
   the server's request log against the browser's actual behaviour. Regression
   test: the frontend script may not declare any function twice.

## Expected results corrected, with the authority

**An absent `ALLOW` is not a prohibition.** The bypass test was first written
against a document with its `ALLOW` block removed, on the assumption that this
would leave the write unauthorized. It does not: that document runs, and writes
the file. `block_schemas_v0.1.0.json#/schemas/ALLOW` says an `ALLOW` is
"Permission only; does not require execution", and the absence of a permission is
not a prohibition. `#/schemas/FORBID` is the prohibition — "Hard prohibition",
which an `ALLOW` "never defeats by itself". The test now uses `FORBID`, and is
stronger for it: the host grant is given, the operator would say yes, and the
effect is still refused at preflight before the host is consulted.

**Two fixtures of mine were invalid LCL, and the engine was right.** A
hand-written document used unquoted version literals and two-space indentation,
which `02_LEXICAL/01` rejects; another omitted the `EXECUTE` root that a
`kind.task` document requires. Both were replaced with documents built from real
canonical bytes, or from the shape `lcl-cli`'s own capability suite already
proves, so that the only thing wrong with a fixture is the thing under test.

**`SpecRecord.authority` is already lowercase.** A test expected
`AUTHORITATIVE`; the engine renders `authoritative`, and the UI does not restyle
what the engine decided.

No test was weakened, skipped, ignored or reclassified.

## Remaining limitations, and who owns them

- **No single-stepping of plan nodes.** Deliberate and stated in the product
  itself. It would require an observer hook in the closed M6 runtime, which the
  owner declined.
- **No `.desktop` entry or packaged application.** `LCL-TASK-0020` owns
  packaging and release closure.
- **No performance, fuzz, security or release sign-off.** `LCL-TASK-0020` owns
  all four. The transport is bounded and the gates are tested, but this task
  makes no hardening claim.
- **The headless screenshot shows an earlier frame than the request log.** A
  limitation of Firefox's `--screenshot` mode, not of the workspace.
- **The four M5/M6/M7 gaps recorded by LCL-TASK-0017 remain open** and remain
  outside this task's scope.

## `git diff --stat`

Tracked files, against the task baseline `fe49902`:

```
 impl/Cargo.lock                        |  15 +++
 impl/Cargo.toml                        |   8 ++
 impl/README.md                         |  95 ++++++++++++++-
 impl/crates/lcl-cli/src/args.rs        |  16 +--
 impl/crates/lcl-cli/src/main.rs        |  56 +--------
 impl/crates/lcl-protocol/src/engine.rs | 141 +++++++++++++++++++++--
 impl/crates/lcl-protocol/src/lib.rs    |  10 +-
 impl/crates/lcl-protocol/src/record.rs | 205 +++++++++++++++++++++++++++++++++
 8 files changed, 465 insertions(+), 81 deletions(-)
```

Plus `impl/crates/lcl-protocol/src/host.rs`,
`impl/crates/lcl-protocol/tests/navigation.rs` and the new
`impl/crates/lcl-workspace/` crate: 4,144 lines across sixteen source, asset and
manifest files, and 2,283 lines across eight test files.

## Suggested commit message

```
Build the LCL workspace and debugging UI

Add lcl-workspace, the editor, project shell, live diagnostics, execution
inspection and debugger a person actually uses. It is a loopback HTTP server
written on std::net serving a hand-written browser frontend, so the
workspace's std-only trust root is unchanged and no crate was added.

The page does not know the language. It has no keyword list, no grammar and
no pattern that matches LCL: it asks for token spans, the real lexer
produces them, and it paints them. Diagnostics carry the registered
identifier, stage, status and exact byte span the emitting layer decided,
and navigation follows the resolver's own bindings rather than searching
for text.

The debugger stops where an effect leaves the language, by wrapping the
existing Operations and Host traits. It presents the whole request and
waits, and neither wrapper decides anything: a denial returns the refusal
the host would, and the engine decides what that means. Stepping between
plan nodes is not offered, because the runtime does not support it and a
control that pretended otherwise would not work.

Add Report::navigation to lcl-protocol so an editor can follow the
resolver's declarations and bindings instead of grepping, available as soon
as resolution has run so a broken document is still navigable. Move the
CLI's private surface() into lcl-protocol so the workspace and the CLI
assemble capabilities through one function rather than two that agree.
Add Engine::run_with, mirroring Runtime::execute_with, so a debugger can
observe the dispatch seam without reassembling the staged walk.
```

The commit would contain only this task's changes.

## Confirmations

- **No background, delegated, parallel, asynchronous, worker or sub-agent was
  used** at any point, in planning or implementation. Every command ran in one
  sequential session.
- Nothing was staged, committed, tagged or pushed. The user controls Git closure.
- `canonical/` was not modified. Its validator and checksums are unchanged and
  `git status` reports it clean.
- No dependency was added; the workspace remains std only.
- No test was weakened, skipped, ignored or reclassified. Three expected results
  were corrected, each against the exact canonical authority that proves the
  previous expectation wrong.
- Nothing was installed on the system. The server was bound only to loopback,
  and every process started during verification was stopped.

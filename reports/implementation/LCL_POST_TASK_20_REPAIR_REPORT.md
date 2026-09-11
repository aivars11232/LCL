# LCL post-Task-20 repair report

12 September 2026.

The bounded repairs to the eight findings carried forward from the combined
read-only audit, the supplied logo integration recorded as B1, and the
follow-up change that makes `.lcl.txt` the default name for a new document.

`LCL_RELEASE_REPORT.md` is the Task 20 closing position and remains the record
of that task. Where this work changed something it stated, that section carries
a dated correction pointing here.

## 1. Repository state

| | |
| --- | --- |
| Baseline HEAD | `fbb177e0270b977ec06853c57cc4ec4c3841a033`, the audited commit |
| Interim commit | `a33950f05f00e993dff1c26ca827b9d1f0616de8`, made by the owner after findings F10, F3 and F11 |
| HEAD at this report | `a33950f05f00e993dff1c26ca827b9d1f0616de8` |
| Working tree | 37 modified files, 6 new paths, nothing staged |
| Canonical package | unchanged; `git status --short canonical/` is empty |

Nothing was staged, committed, pushed, tagged or published by this work. The
owner made the interim commit.

## 2. Status of every finding

| ID | Status | Established by |
| --- | --- | --- |
| F10 nested body aborts the process | **FIXED** | reproduced as `SIGABRT`, repaired across four files, regression added |
| F3 process pipe stall and late cap | **FIXED** | reproduced against the unrepaired adapter, 8 real-subprocess regressions |
| F11 installed launch and association | **FIXED** | 16 regressions driving the generated desktop entry from an isolated home |
| F13 wrong identifier for a malformed range | **FIXED** | moved to its prescribed stage, plus the row's own identifier at demand |
| F14 constant reference in a parameter | **FIXED** | reproduced, boundary repaired in preflight, 4 regressions |
| F9 declared minimum Rust version | **FIXED** | whole workspace built and tested on Rust 1.75.0 |
| F8 unsupported conformance claims | **FIXED** | claim model rebuilt; the product's claim correctly drops to `source_conforming` |
| F12 source-to-artifact provenance | **FIXED** | candidate rebuilt and its exact source reconstructed from its own provenance |
| B1 supplied logo | **INTEGRATED** | master hash verified, served, installed, and confirmed in a real browser |
| Follow-up `.lcl.txt` default | **DONE** | both endings recognised throughout, ordinary text unaffected |

No finding is BLOCKED. Two acceptance items are recorded as NOT EXECUTED in
section 9 and neither is claimed as a pass.

## 3. F10 — a nested body ended the process

**Reproduced first.** A subprocess reproducer carries a document through
`check`, `validate`, `inspect` and `run` on a thread with a stated stack
budget, and the parent reads how that process died. At 1 MiB and depth 1,000 it
died by signal 6 after `thread has overflowed its stack`. Depth 8 on the same
budget completed, which is what makes the finding about nesting rather than
about the budget.

The overflow was a chain of four paths, each found from a debugger backtrace
rather than guessed, and each repaired before the next became visible.

| Order | Path | Repair |
| --- | --- | --- |
| 1 | derived `Clone` on `Statement` → `Field` → `Body` → `Nested` | hand-written iterative `Clone` for the ten recursive syntax types |
| 2 | `lcl-checker` `object_data` recursing per nested body | explicit worklist |
| 3 | `lcl-checker` `collect_statement`/`collect_executable`/`collect_top_level` | one worklist replacing three mutually recursive walks |
| 4 | `lcl-resolver` `free_statements` | explicit worklist |

The shape follows what M2 and the expression repair already established in this
tree: depth costs heap, which fails as an allocation rather than as an abort.
No depth limit was invented, no diagnostic was invented, and no stack was
enlarged.

**Measured afterwards**, on a 1 MiB stack, to confirm no new resource blow-up.
The fixture's own size grows quadratically with depth because each level adds
four bytes of indentation per line, so cost is reported against source bytes.

| Depth | Source | Wall | Result |
| --- | --- | --- | --- |
| 1,000 | 2.0 MB | 1.9 s | completes |
| 2,000 | 8.0 MB | 5.9 s | completes |
| 4,000 | 32.0 MB | 23.1 s | completes |
| 8,000 | 128.1 MB | 91.0 s | completes |

About 0.7 seconds per megabyte throughout, so the relationship is linear in the
document rather than in the nesting. At 16,000 levels, a 512 MB document, the
engine still completed; it exceeded the suite's own 120-second per-case ceiling,
which is a fixture size limit and not a termination. The committed regression
runs depths 8, 1,000 and 2,000, well past the 512 the release report documented,
in about 8 seconds.

## 4. F3 — a process stalled on output nobody was reading

The adapter piped both streams and then waited for exit before reading either.
A pipe holds one buffer, so a program printing more than that blocked in
`write`, never exited, and was killed as a timeout. The stream cap was applied
to a vector that had already been filled, so it bounded what the caller saw
rather than what the process allocated.

Both streams are now drained on their own threads from the moment the child
starts, for the whole time it is supervised. Each retains at most
`max_stream_bytes` and keeps draining past that point without keeping the
excess, so the bound holds during collection and the child still never blocks.

**Reproduction is recorded, not assumed.** The eight new cases were run against
the unrepaired adapter: five failed, each burning its full 30-second deadline as
the false timeout the finding describes. The three that passed are the ones
testing behaviour that was already correct, which is the control.

| Case | Against the old adapter |
| --- | --- |
| past the pipe buffer on stdout | FAILED |
| past the pipe buffer on stderr | FAILED |
| both streams flooded at once | FAILED |
| the bound holds while reading | FAILED |
| twenty flooding children leave no zombie | FAILED |
| a slow program still times out | passed |
| a flooding program that then hangs still times out | passed |
| a child writing nothing is reaped | passed |

Nothing here is mocked. The cases drive real subprocesses, and one reads
`/proc` to confirm no child was left unreaped.

## 5. F11 — the installed entry did not launch

Three defects at once, none visible to a test that checked the file existed.
The workspace loads one exact specification package and never searches for one,
so a menu launch with no `LCL_SPEC` failed before serving. It opens a browser
only under `--open`, and a desktop launch has no terminal, so the URL it printed
went nowhere. And `%f` passes a document while the positional argument is a
project directory, so a file association opened the wrong thing.

The repair adds an explicit `--document` option, which resolves its own project
as the nearest ancestor holding a manifest or the file's own directory, and an
installed launcher script that the desktop entry names. The launcher carries the
installed binary, the installed package and a default project directory as
absolute paths, quotes its argument, distinguishes a menu launch from a file
association, logs to the XDG state directory and surfaces a failure through
whichever dialog tool exists.

Sixteen regressions install into a disposable home, read the desktop entry that
installation actually generated, parse its `Exec` line the way a desktop does,
and run it with a stub `xdg-open` that records the URL. That URL is then fetched
over a real socket and the served session is asserted. All sixteen fail against
the pre-repair packaging.

Default applications are not changed. The entry declares its media type; nothing
writes a `mimeapps.list`.

## 6. F13 — a substituted identifier for a malformed range

`core.read`'s `range` row says "An incompatible unit/representation or wrong
key/type uses error.operation.parameter". That identifier is registered at
`static_or_expression`, and the code emitted `error.operation.precondition`
instead, reasoning that a runtime emitting a source-stage identifier would be
relabelling a stage.

The second half of that reasoning does not follow, and the registry says so
directly. `expression_demand_resolution`'s `exclusion_rule` reads "Discovery
time alone never changes classification." An identifier discovered late keeps
the classification the registry gives it; substituting a different identifier is
the thing that actually changes it.

The repair has two halves, and the first is the larger.

**At the prescribed stage.** `lcl-checker` now validates a parameter written as
a literal object against the closed shape its row declares, reading the admitted
keys, their types and the closed unit list out of the registered constraint text
rather than from a list of operation names. A wrong key, a wrong written type
and an unregistered unit word are now decided at `static_or_expression`, which
is where `earliest_stage_rule` assigns them. Five cases assert the exact
identifier and the exact stage, and a sixth asserts that a well-formed range and
every registered unit word are still accepted.

**At demand.** What remains is the part no earlier stage can decide: whether a
unit indexes the representation the target actually held. `lcl-runtime` mirrors
`error.operation.parameter` so that case can be reported under the row's own
name, with its registered stage and status read from the registry like every
other mirrored identifier and never demand-resolved.

One existing expectation was corrected rather than weakened. The stdlib case
asserting `error.operation.precondition` encoded the defect; it now asserts the
row's identifier, its registered stage, its registered status, that no stage was
resolved for it, and that no effect was left behind. The authority for the
correction is written into the test.

## 7. F14 — a constant reference read nothing

**Reproduced with an observable discriminator.** `core.inspect` registers
`depth` as an `INTEGER` with the bound `0..100` and the default `1`, and the
bound check only sees a value that materialised as an integer. So:

| Written | Before | After |
| --- | --- | --- |
| literal `2` | accepted | accepted |
| literal `500` | `error.value.out_of_range` | `error.value.out_of_range` |
| `REF` to a constant holding `2` | accepted | accepted |
| `REF` to a constant holding `500` | **accepted** | `error.value.out_of_range` |

The fourth row is the defect, and it was silent: the reference read `MISSING`,
the bound check had no integer to judge, and the operation quietly took its
registered default.

**The boundary.** `05_SEMANTICS/12` lists what a reference reads in a value
context, and `DEFINE kind.constant` is in that list. Every stage honoured it
except one. M4 records a constant's statically known value and judges the
reference as a value context; M6 reads a declaration's value from the plan's
resolutions; and M5 resolved only `INPUT`, `DATA`, `CONTEXT`, `MEMORY` and
`STATE` into them. Preflight now resolves a readable constant too, with the
supplied-source, default and assumption steps skipped, because a constant is
exactly its declared value and none of those apply to it.

Type-family validation is unchanged and is stricter than the read: a `STRING`
constant standing where an `INTEGER` is registered is refused at the static
stage, identically to the equivalent literal. An unresolved reference is still a
resolution-stage defect.

## 8. F9 — the declared minimum, actually tested

The three uses of `Option::is_none_or`, which requires 1.82, were replaced with
equivalents available in 1.75. A scan for other post-1.75 standard-library APIs
found none.

Rust 1.75.0 was then obtained as a standalone toolchain in a session scratch
directory, installed neither globally nor into the checkout, and the gate was
run against it. It found a second, larger problem the audit had not seen:
`lcl-parser` did not compile on 1.75 at all. `reduce` and `binary` took no
`self` and named no lifetime, but sat inside `impl<'a> ExprParser<'a, '_>`,
where a `Self::` call bound them to the source lifetime anyway. Lifting both to
free functions states what was already true and builds on every version from the
declared minimum upward.

With that, on Rust 1.75.0, `--offline --locked`:

| | |
| --- | --- |
| `cargo check --workspace --all-targets` | clean |
| `cargo test --workspace --all-targets` | 132 suites, **1,350 passed, 0 failed, 0 ignored** |

The declared minimum is now a tested claim rather than a declaration. The
lockfile is compatible with it, which `--locked` establishes.

## 9. F8 — a claim that outran its evidence, and what it says now

`clean_coverage` treated a stage bucket with one passing case and no failures as
covered, and `claim` promoted nine such buckets to `semantics_conforming`. Nine
passing cases could certify conformance to the whole of LCL's semantics. The
canonical text forbids exactly that, in a sentence about absence rather than
about failure: "No semantic-conformance claim is permitted while the required
implementation or concrete executable cases are absent."

The claim now needs two things. The stage buckets say the evidence reaches the
semantic layers at all. Completeness is measured against the canonical decision
witness catalogue: every one of its 66 indexed witnesses must have a passing
executed case, and a witness that never ran, that failed, or that this build
records as descriptive because it cannot exhibit it, is absent in exactly the
sense the sentence names.

The 799-entry requirements index is deliberately not part of this. The canonical
text calls it "structural evidence only" and says "catalog entries without
concrete input and an implementation result are not executed conformance cases",
so requiring an executed case per entry would impose a gate the specification
declines to impose.

The report example also now runs the five filesystem-backed probes against the
same deterministic fixture the gate uses, instead of recording them as
descriptive entries with a note saying they ran elsewhere. That was true and
misleading at once: it made five behaviours look unexhibited and counted probes
in a column about witnesses.

**The consequence is a downgrade, and it is the honest number.**

| | Before | Now |
| --- | --- | --- |
| Executed probes | 61 | 66 |
| Probes passed | 61 | 66 |
| Probes failed | 0 | 0 |
| Witnesses established | not measured | **55 of 66** |
| Claim | `semantics_conforming` | **`source_conforming`** |

The eleven unestablished witnesses are named individually in the report and in
its claim limits: CLOSURE-004, -006, -021, -022, -023, -024, -027, -055, -058,
-059 and -060. Each already carried a recorded reason. What changed is that a
recorded reason no longer silently coexists with a claim that assumes it away.

Nothing was deleted to reach this. The reasons, the probe counts and the passing
evidence are all still in the report; only the claim they support has moved.

## 10. F12 — provenance that identifies its own source

The previous recipe recorded a commit and a count of uncommitted files, and
recorded them after writing the artifacts. A count is not an identity, so the
commit could not reconstruct what was built; and reading the working tree after
generating output measured a tree the run had already changed. It also built in
the shared target directory, where a stale binary could be picked up.

The source set is now enumerated, hashed and exported **before** anything is
built, the build runs in a directory the script owns, and the candidate is
written to its own output location that never touches the published release.

Verified end to end on the final candidate:

| Check | Result |
| --- | --- |
| Source archive checksum | OK |
| Source reconstructed from the archive, every file against the manifest | 702 of 702 OK |
| Payload tarball checksum | OK |
| Every payload file against the provenance | 200 of 200 OK |
| Rebuilt payload installs into an isolated home | yes |
| Rebuilt `lcl spec` against the bundled package | `authoritative`, expected identity |
| Rebuilt `lcl run` over a canonical example | `status.succeeded`, exit 0 |
| Rebuilt workspace launched through the generated desktop entry | serves the intended project and document |
| Published `releases/` archives after the build | byte-identical, verified by checksum |

Candidate at `releases/candidates/lcl-0.1.0-linux-x86_64-bf78a0890e5f/`, source
id `bf78a0890e5f6fa265d4c5becf00b72784d3c39c276ec9883139ffd156c6b039`, artifact
`aa99279e176554a1f2117ea4f77a6dd96ee08b360f06f565b652a24e53c56983`.

**Bit-for-bit reproducibility is not claimed**, and the provenance says so in
its own words. Rust embeds build paths and no attempt was made to normalize
them. What is claimed is the other direction: the source these bytes were built
from is recorded exactly, and that was demonstrated by reconstructing it.

## 11. B1 — the supplied mark

**The master is the supplied file, byte for byte.**

| | |
| --- | --- |
| Path | `assets/brand/lcl-logo-master.png` |
| SHA-256 | `6c930f61a0db7c7e2e1d7e5926b18a4d74dee1f921c0937f8c8e8092d3a96b3d` |
| Format | PNG, RGBA, 577 by 432, alpha 0 to 255 |
| Visible extent | 341 by 338, at (117, 46) to (458, 384) |

Every value matches the supplied specification. The hash is checked before the
generator will read it.

**Sixteen derivatives**, produced by `assets/brand/derive_brand_assets.py` using
only non-generative preparation: trimming the fully transparent outer margin,
downsampling, and padding to a square with transparency. Nothing draws,
recolours, traces, upscales or adds a background. No output is larger than the
master's visible extent, so every one is a downsample. Two runs produce
identical bytes. All seventeen files verify against
`assets/brand/BRAND_ASSETS.sha256`.

There is no vector original and none is claimed. Tracing one would invent detail
the source does not contain.

**Where it appears, with the evidence.**

| Surface | Evidence |
| --- | --- |
| Workspace header | served bytes equal the committed derivative, `image/png`, PNG header 97 by 96, colour type 6 |
| Browser tab | explicit `rel="icon"` declared with the session token; served bytes 32 by 32, colour type 6 |
| Desktop menu entry | `Icon=lcl-workspace` resolving to installed files at all seven sizes, each matching its directory's declared size |
| `.lcl` document icon | byte-identical to the application icon at the same size |
| Release payload | master, checksum manifest and all installed icons staged |

**Security is unchanged.** Both images pass the same three gates as every other
route: a request without the session token is refused 403, and a cross-origin
request is refused 403. No unauthenticated `/favicon.ico` route exists; the page
declares its icon with the token, exactly as it already stamped its stylesheet
and script. No CDN, no external host, no new dependency.

**Confirmed in a real browser**, headless so that nothing appeared on the
owner's desktop. Firefox 155 loaded a live workspace and the server's request
log recorded `GET /brand/lcl-mark.png`. The screenshot shows the mark rendered
in the header, square and undistorted, with transparency, and with every control
present and unobstructed. The favicon bytes were then fetched and decoded by the
same browser and rendered as an image. Against the same running instance,
`check` reached static checking with zero diagnostics and `run` reached
`status.succeeded`.

**The running browser's own window and taskbar identity is not changed by any of
this**, and is not claimed to be. That belongs to the browser. Changing it would
need a native wrapper, which nobody has authorised.

**Uninstall owns only what it installed.** Each icon file is copied in and
removed individually, never recursively; a foreign icon planted in the same
shared theme directory survives uninstall, and a directory this installation
filled by itself is removed. No icon cache is generated: writing one would put a
shared file into the theme that this installation could not safely take back,
and the icon theme specification resolves an icon by reading the directories.

## 12. Follow-up — `.lcl.txt` as the default name

A separate, bounded compatibility change, run after the repairs above and their
checks.

New documents are created as `name.lcl.txt`, so a document can be shared, opened
and edited anywhere plain text is. Suffixes never stack: `notes`, `notes.lcl`
and `notes.lcl.txt` all create `notes.lcl.txt`.

One rule in one place, `lcl_project::naming`, consumed by both the workspace and
the command-line tool. Both endings are recognised in the project tree, opening,
saving, checking, running, the syntax metadata and the file association.

**Nothing renames anything.** Creating is now its own route and applies the
default; saving writes exactly the name it was given, so opening a `.lcl`
document and pressing save never renames it. No file is bulk-renamed and no
import is rewritten: imports resolve exactly declared paths and were untouched.

**The ending decides nothing about meaning.** A `.lcl.txt` document is judged by
the same engine under the same contracts, and ending a name in `.txt` never
relaxes validation. Both halves are asserted: identical valid source under
either name produces an identical record, and identical invalid source is
refused identically.

**Ordinary text files stay ordinary.** Only the exact two-part ending is
claimed. No glob for `*.txt` is declared and `text/plain` is not modified.
Confirmed against the desktop's own resolver from an isolated installation:

| File | Resolves to |
| --- | --- |
| `a.lcl` | `text/x-lcl` |
| `b.lcl.txt` | `text/x-lcl` |
| `c.txt` | `text/plain` |
| `d.txt`, whose bytes begin `LCL:` | `text/plain` |
| `e.md` | `text/markdown` |

The fourth row is worth stating: the glob decides before the magic rule is
consulted, so a plain text file is not captured by its contents.

### Usage

```sh
lcl check  notes.lcl.txt        # the same engine, the same contracts
lcl run    notes.lcl.txt
lcl check  legacy.lcl           # unchanged, still fully supported
lcl syntax                      # reports both recognised endings
```

In the workspace, the new-document dialog offers `untitled.lcl.txt`. Typing
`report` creates `report.lcl.txt`; typing `report.lcl` also creates
`report.lcl.txt`, and the confirmation says which name it chose.

`.lcl.txt` is a distribution convenience. **Other applications do not understand
or execute LCL because a filename ends in `.txt`.** A text editor will open the
file; only the LCL engine judges it.

## 13. Verification

Every gate below was executed at the end of this work, on the final tree.

| Gate | Result |
| --- | --- |
| `cargo fmt --all -- --check` | clean |
| `cargo clippy --offline --workspace --all-targets -- -D warnings` | clean |
| `cargo test --offline --workspace --all-targets` | 132 suites, **1,350 passed, 0 failed, 0 ignored** |
| The same, on Rust 1.75.0, `--locked` | 132 suites, **1,350 passed, 0 failed, 0 ignored** |
| Executable conformance | 66 probes executed, 66 passed, 0 failed; **55 of 66 witnesses established**; claim `source_conforming` |
| UI and CLI equivalence | 6 passed |
| Application ladder | 6 passed |
| `validate_release.py --scope all` | 31 PASS, 0 FAIL, 0 BLOCKED, 2 OUT_OF_SCOPE, `release_ready: true` |
| `sha256sum -c SHA256SUMS.txt` | exit 0, 175 files |
| `git status --short canonical/` | empty |
| `sha256sum -c assets/brand/BRAND_ASSETS.sha256` | 17 of 17 OK |
| Candidate source reconstruction | 702 of 702 files OK |
| Candidate payload against provenance | 200 of 200 OK |
| Published `releases/` archives | byte-identical to before this work |

The opening baseline, measured before any edit, was 129 suites and 1,294 tests.
This work adds 3 suites and 56 tests and changes one existing expectation, in
section 6, with its authority written into the test.

### NOT EXECUTED

Two items are recorded as not executed. Neither is claimed as a pass.

1. **A graphical desktop launch.** The dispatch, argument and browser-launch
   seam is covered by sixteen automated regressions that run the generated
   desktop entry and fetch what it served. A person clicking the menu entry on a
   live KDE session and seeing a window was not performed, and the installed
   icon was therefore not confirmed by eye in a menu. It was confirmed to
   resolve, at seven sizes, from an isolated installation.
2. **Platforms other than Arch Linux x86_64 with Wayland.** Unchanged from the
   Task 20 position.

## 14. Remaining limitations

1. **Eleven decision witnesses stay unestablished**, named in section 9. Each
   needs test data the canonical text does not supply, most often a host
   scripted to fail. They now block the semantic claim instead of coexisting
   with it.
2. **A `ROUND` chain is quadratic in its depth.** Unchanged, measured, bounded.
3. **No model or provider adapter.** Unchanged.
4. **One verified platform.** Unchanged.
5. **The tarball is not bit-reproducible.** Unchanged, and now stated inside the
   provenance itself. Source identity is recorded and demonstrated instead.
6. **`core.sort` appears not to apply its `direction` parameter.** Noticed while
   building an F14 reproducer: a `descending` literal produced ascending order.
   It is outside this work's register, was not investigated further, and is
   recorded here so it is not lost. It is not a finding this report claims to
   have reproduced properly.

## 15. Changed files, in change order

**F10.** `impl/crates/lcl-hardening/tests/adversarial_inputs.rs`,
`impl/crates/lcl-parser/src/syntax.rs`,
`impl/crates/lcl-checker/src/declarations.rs`,
`impl/crates/lcl-checker/src/types.rs`,
`impl/crates/lcl-resolver/src/references.rs`.

**F3.** `impl/crates/lcl-capabilities/src/process.rs`,
`impl/crates/lcl-capabilities/tests/real_process.rs` (new).

**F11.** `impl/crates/lcl-workspace/src/project.rs`,
`impl/crates/lcl-workspace/src/main.rs`,
`impl/crates/lcl-workspace/src/routes.rs`,
`impl/crates/lcl-workspace/assets/app.js`,
`packaging/lcl-workspace-launch.in` (new), `packaging/lcl.desktop`,
`packaging/install.sh`, `packaging/uninstall.sh`, `packaging/build_release.sh`,
`impl/crates/lcl-hardening/tests/installed_launcher.rs` (new),
`packaging/README.md`.

**B1.** `assets/brand/lcl-logo-master.png` (new),
`assets/brand/derive_brand_assets.py` (new),
`assets/brand/BRAND_ASSETS.sha256` (new), `assets/brand/README.md` (new),
`impl/crates/lcl-workspace/assets/brand/` (new, 2 files),
`packaging/icons/` (new, 14 files), `impl/crates/lcl-workspace/src/http.rs`,
`impl/crates/lcl-workspace/src/routes.rs`,
`impl/crates/lcl-workspace/assets/index.html`,
`impl/crates/lcl-workspace/assets/app.css`,
`impl/crates/lcl-workspace/tests/common/mod.rs`,
`impl/crates/lcl-workspace/tests/routes.rs`, `packaging/lcl.desktop`,
`packaging/install.sh`, `packaging/uninstall.sh`,
`packaging/build_release.sh`.

**F13.** `impl/crates/lcl-checker/src/operation.rs`,
`impl/crates/lcl-checker/src/declarations.rs`,
`impl/crates/lcl-checker/tests/constructors_and_operations.rs`,
`impl/crates/lcl-runtime/src/diagnostic.rs`,
`impl/crates/lcl-stdlib/src/host.rs`,
`impl/crates/lcl-stdlib/tests/external_operations.rs`.

**F14.** `impl/crates/lcl-semantics/src/data.rs`,
`impl/crates/lcl-stdlib/tests/constant_references.rs` (new).

**F9.** `impl/crates/lcl-checker/tests/robustness.rs`,
`impl/crates/lcl-checker/tests/static_matrix.rs`,
`impl/crates/lcl-runtime/tests/concurrency.rs`,
`impl/crates/lcl-parser/src/expr.rs`.

**F8.** `impl/crates/lcl-conformance/src/report.rs`,
`impl/crates/lcl-conformance/examples/m8_conformance_report.rs`,
`impl/crates/lcl-conformance/tests/decision_witnesses.rs`,
`impl/crates/lcl-conformance/tests/descriptive_index.rs`.

**F12.** `packaging/build_release.sh`.

**Follow-up.** `impl/crates/lcl-project/src/naming.rs` (new),
`impl/crates/lcl-project/src/lib.rs`,
`impl/crates/lcl-workspace/src/project.rs`,
`impl/crates/lcl-workspace/src/document.rs`,
`impl/crates/lcl-workspace/src/routes.rs`,
`impl/crates/lcl-workspace/assets/app.js`,
`impl/crates/lcl-cli/src/syntax.rs`, `impl/integration/linux/lcl.xml`,
`impl/crates/lcl-cli/tests/lcl_integration.rs`,
`impl/crates/lcl-workspace/tests/persistence.rs`,
`impl/crates/lcl-workspace/tests/routes.rs`,
`impl/crates/lcl-hardening/tests/installed_launcher.rs`,
`impl/integration/README.md`, `packaging/README.md`.

## 16. Continuation handoff

| | |
| --- | --- |
| HEAD | `a33950f05f00e993dff1c26ca827b9d1f0616de8` |
| Working tree | 37 modified, 6 new paths, nothing staged |
| Canonical | unchanged and valid |
| Published release | untouched, checksums verified |
| Candidate | `releases/candidates/lcl-0.1.0-linux-x86_64-bf78a0890e5f/`, untracked |
| Completed | F3, F8, F9, F10, F11, F12, F13, F14, B1, the `.lcl.txt` follow-up |
| Pending | nothing from this register |
| Stopped phase | none |

The next required gate, if anything further changes, is the one in section 13,
run in full. Git closure is the owner's: nothing here is staged, committed or
pushed.

No background, delegated, parallel or sub-agent was used at any point.

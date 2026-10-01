# LCL — code cleanup: one format per language, big files split, dense code spelled out

Owner's request (2026-10-01): "clean the code, so that it's readable,
structurized, more human like, professional human … but it still must be
precise."

Starting HEAD: `4150021` (LCL 0.9.1 released). Final HEAD: see **Git**.

No behaviour was changed: every step moved or reformatted code, renamed local
names, or rewrote test helpers, and was verified by the suites that already
existed. The product version stays 0.9.1; nothing was released.

The work was done in two steps. The first gave each language one format and
split the largest Android and workspace files. The second, which the owner
asked for after reading the first ("Yes, proceed"), split the remote service's
two large files and the page's three longest dialogs.

## Where the code was furthest from readable

A survey before touching anything (about 185,000 lines in all):

| Area | Size | State before |
| --- | --- | --- |
| Rust (`impl/`, `remote/`, `update/`) | 161,000 lines | already in rustfmt's format; long lines are data tables; one 1,526-line file with a single 1,000-line `impl` block |
| Android Kotlin | 12,700 lines | no formatter; 447 lines wider than 120 columns (up to 239); two files over 1,000 lines; several statements per line; one-letter names |
| Workspace page (JS, CSS) | 6,900 lines with its tests | hand-formatted consistently, 31 labelled sections; no formatter |
| Shell and Python tools | 2,700 lines | a few very long command lines in the Android E2E script |

So the work went mostly to the Android app, then to the one oversized Rust
file, the page's formatting and the E2E script.

## Android (Kotlin)

- **One format.** Every `.kt` and `.kts` file is formatted by ktfmt 0.64 in
  the Kotlin language guidelines style (4 spaces, 100 columns, one argument or
  statement per line where a line would be too long, unused imports removed).
  `android/tools/format.sh` applies it and `--check` verifies it; the README's
  new "Code style" section says how. The formatter is a developer tool kept
  outside the repository (`/mnt/F/.lcl-android/tools/`), not a build
  dependency.
- **The screen file split.** `WorkspaceScreen.kt` (1,739 lines once
  formatted) is now the screen's layout (127 lines) plus `FilesPane.kt`,
  `EditorPane.kt`, `InspectorPanels.kt`, `WorkspaceDialogs.kt` and
  `SyncDialog.kt`. Pure moves: a check compared every non-import line before
  and after.
- **The controller split.** `WorkspaceController.kt` (1,690 lines) lost its
  state types to `WorkspaceUi.kt` and its sync to a class of its own,
  `ProjectSync.kt`. There the 280-line `runSync` is four named steps — make
  the folders, send each document, record the confirmations, read the disk
  again — with a small `Progress` object instead of five shared local
  variables; planning and the removal check raise one refusal type instead of
  returning failures from a dozen places. The controller (1,089 lines now)
  delegates; its public functions, the requests it sends and every message are
  unchanged. `PcRequests.kt` holds the two request helpers both use.
- **Text and names.** 73 message strings wider than 100 columns are broken
  into joined pieces at word boundaries, never inside a `${…}` expression (a
  script that asserts the pieces join back to the original). 34 one-letter
  locals and parameters outside the usual idioms (an index, a character, a
  comparator's pair) now say what they hold (`o` → `outcome`, `p` → `option`,
  `d` → `diagnostic`, `s`/`i` in the JSON reader → `text`/`at`, …), renamed by a
  scope-aware tool that leaves string text and other receivers' members alone.
  The text a new document starts with is a named constant written as the
  document looks, and a new test pins every byte of it.
- Comments were left as they are: about 10% of lines, and they say why.

No lines over 100 columns remain in the app's sources.

## Rust

- **`routes.rs` split.** The workspace's route table (1,526 lines, one
  1,000-line `impl`) is now `routes/mod.rs` (the state, the dispatch table and
  what every area shares, 440 lines) and one module per area: `explorer`,
  `documents`, `preferences`, `projects`, `runs`, `devices`. Methods moved
  whole, with their comments; those the dispatch table calls are
  `pub(super)`, the rest stay private.
- **The "Text file busy" test flake, fixed at its cause.** Tests that wrote a
  stand-in script from the test process and ran it at once failed now and then
  (four times across the last two tasks): while a process has a file open for
  writing, a child another test thread forks at that moment inherits the
  descriptor, and running the file fails until that child has exec'd. The two
  helpers concerned (`lcl-workspace` `remote::tests::fake`, the updater's
  `flow.rs` `write`) now have a child process write the script, so the test
  process never holds it open. Afterwards the workspace's remote tests ran 40
  times and the updater's flow tests 15 times without a failure, and the gate
  below passed in a single run.
- **`lcl-remote`: `session.rs` split** (second step). One connection from a
  device was one file of 1,287 lines. `session.rs` (281 lines) now holds what
  all connections share, a connection from accept to close, and the session
  loop. Beside it, in `session/`: `wire` (TLS, frames and the messages written
  into them), `admission` (`hello`), `operations` (the list of requests),
  `documents` and `runs` (what a session holds open and follows) and
  `legacy_tree` (the `tree` answer older apps ask for). Three functions were
  more than moved:
  - `project_op`, 210 lines of `match` inside a closure called on the spot, is
    a table with one line per operation. Each operation that changes what the
    session holds (`open`, `close`, `save`, `create`, `delete`, `run`,
    `follow`, `answer`) is a function of its own, and a small `Asked` value
    carries the project, its routes and the message to them.
  - `admit` (140 lines) is `admit`, `pair` and `connect`.
  - `serve` reads as its steps — accept, handshake, first frame, admit, the
    session, close — with the TLS details inside `Wire`.

  The run-event text that was written out three times is one function, and so
  is the `{"error": …}` body that was built in seven places.
- **`lcl-remote`: `pairing.rs` split** (second step). 1,182 lines, a third of
  them tests. `pairing.rs` (558) keeps the types and the flow (`create`,
  `present`, `approve`, `deny`); `pairing/state` is `pairing.json` — read,
  written and changed under the lock — `pairing/payload` the QR text with its
  test, and `pairing/tests` the flow's tests. `present` gave its first-time
  branch to `enlist`. Its three-step finalization stays in one place on
  purpose: the module's argument about a crash in the middle follows it step
  by step.
- The remote crate's public API is unchanged, and both files keep their paths
  (`session.rs` beside `session/`, the layout `lcl-conformance` already uses),
  so their history stays with them.
- Rust was already in rustfmt's format; nothing else was reformatted.

## Workspace page

`app.js`, `app.css` and the page tests are formatted by Prettier 3.6.2
(`.prettierrc.json`: `printWidth` 100), as the implementation README now
says. The stylesheet went from one rule per line to the standard one
declaration per line. `index.html` is left as written.

The script's three longest functions were dialogs written as one function
each. In the second step:

- **Settings** (`openSettings`, 248 lines) became the list of its sections,
  each a function — `appearanceSection`, `editorSection`, `filesSection`,
  `templatesSection`, next to the `updatesSection` and `androidDevices` that
  were functions already — with saving as `saveSettingsDialog`. It is 36
  lines.
- **New document** (`newDocument`, 200 lines) keeps what the dialog does —
  refill the choices, preview, create — and no longer holds what it only
  contained: its fields, the start-from choices, the blank document's text and
  the creating itself (`createDocument`). It is 114 lines.
- **Android devices** (`androidDevices`, 323 lines) keeps the state and the
  polling, which belong together. A device's row, a request's row, the pairing
  code and the small formatters are functions beside it. It is 208 lines, as a
  component of short functions over one state, none longer than 40 lines.

## Android E2E script

`android/tools/e2e.sh`: the repeated inline Python one-liners became two
helpers (`json_field`, `no_request_waiting`), over-long `check "…" command`
lines put the command on its own line, three long commands are wrapped, and a
variable named `done` was renamed; `shellcheck` reports nothing.

## Verification

Each step was verified on its final source commit, one heavy job after
another (`/mnt/F/.lcl-pretest/v05/cleanup-verify.sh`): the first on `0cd1d5d`,
the second on `dead635`. Both runs gave the same results:

- **Gate** (`gate9-run.log`): 71 of 71 commands as expected in one run — fmt,
  clippy, the workspace suite on the current and the 1.75 toolchain (1970
  passed, 0 failed, 1 ignored on each), Core checksums, validators,
  identities and anchors, `protected`, conformance and readiness, the
  `real_process` runs, remote (65) and updater (35) suites, manual manifest
  and examples, and the real Firefox smoke over the page.
- **Format checks** (`cleanup-checks.log`): `android/tools/format.sh --check`,
  `prettier --check` and `shellcheck` all clean.
- **Android**: 142 JVM unit tests pass (one new in the first step: the starter
  text). The full E2E — the app on the emulator against `lcl-remote`, in the
  second run the restructured one: all phases passed, no ANR or crash
  (`e2e-run14.log`). Instrumented `LocalProjectsUiTest` in light and dark,
  `VersionDisplayTest`, `FileDrawerTest` 3/3.

The second step changed code that decides what a device and a person are
told, so its text was compared before and after as well:

- **Remote.** Every string literal of `session` and `pairing`, with escapes
  and line continuations resolved (`rs_strings.py`). In `pairing` all are the
  same but one message that is now written once instead of twice. In `session`
  the differences are the intended ones: the run-event text and the `"error"`
  key written once, and the `run` operation naming its fields through the same
  helper as the others, which gives the same two messages.
- **Page.** Every string of `app.js`, taken from the syntax tree with the
  pieces of a `+` chain joined (`js_strings.cjs`): 1,944 are the same, six
  differ only in the name of a helper inside `${…}`, and one comparison with
  `"pending"` is written once instead of twice.
- The 65 remote tests and the 74 page cases pass. No test was changed; the
  pairing unit tests only moved to files of their own.

Two tests that read source text had to follow the changes: `equivalence.rs`
opens the route table by path (now `routes/mod.rs`), and the updater's version
check parsed one line of `build.gradle.kts` that the formatter wrapped (it now
reads the statement however it is wrapped). Both were caught by running the
suites, before the gate.

Not run: a physical phone (none attached). Canon is untouched (`protected`
passes).

## Not done

- `androidDevices` is still one function of 208 lines. What is left in it
  shares one state — the devices, the waiting requests, the code on screen,
  two timers — and handing that state around as an object would rewrite every
  line of it without making any of them clearer.
- The page's other functions over 120 lines — `newProject` (163),
  `renderCapabilities` (162), `convertDocument` (121) — were not part of
  either step.
- `Pairing::present` keeps its finalization in one function (see Rust).
- The engine crates (parser, resolver, checker, semantics, runtime,
  conformance — about 80,000 lines) were not touched: they are in the standard
  format, governed by the Core packages, and their long lines are case tables.
- `WorkspaceController.kt` is still 1,089 lines (connection, explorer,
  documents and runs); `ConnectionManager.kt` is 727.

## Tools

Kept outside the repository, under `/mnt/F/.lcl-android/tools/`: the ktfmt
0.64 jar (71 MB, checksum in `android/tools/format.sh`) and Prettier 3.6.2.
The one-off scripts used for the moves — a declaration-level splitter for
Kotlin, a method-level one for Rust, the string wrapper and the scope-aware
renamer — are in `/mnt/F/.lcl-pretest/v05/` (`split_kt.py`, `split_rs.py`,
`wrap_kt_strings.py`, `rename_kt.py`), and so are the two that compare text
before and after (`rs_strings.py`, `js_strings.cjs`).

## Git

Commits on `main` after `4150021`, pushed:

1. `51580b1` Android: one format for the Kotlin sources (ktfmt, Kotlin guidelines style)
2. `26c0972` Android: the workspace screen and controller split by responsibility; sync in its own class
3. `d527be2` Android: message strings wrapped at words, terse names spelled out, the starter document as a constant
4. `22ce38d` Workspace page: one format for the script, the stylesheet and the page tests (Prettier)
5. `66b75d4` Workspace: the route table split into a module per area
6. `c976724` Tests: stand-in programs are written by a child process, so running them is never 'Text file busy'
7. `e8c4624` Tests: the version check reads the Gradle statement however it is wrapped
8. `0cd1d5d` Android E2E script: helpers for the repeated JSON checks, one command per readable line
9. `9899b0f` this report, as it stood after the first step
10. `71e0baa` Remote: the session and pairing files split by responsibility, long functions in named steps
11. `dead635` Workspace page: the Settings, New document and Android devices dialogs in named parts
12. this report, with the second step

Build output of each verification was deleted afterwards (the repository's
ignored build folders, `/mnt/F/.lcl-pretest/v05/target`,
`/mnt/F/.lcl-closure-4t-4c1cd4c659b7/target-msrv`,
`/mnt/F/.lcl-pretest/update-target`, `/mnt/F/.lcl-pretest/remote-target`).

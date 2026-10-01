# LCL — code cleanup: one format per language, big files split, dense code spelled out

Owner's request (2026-10-01): "clean the code, so that it's readable,
structurized, more human like, professional human … but it still must be
precise."

Starting HEAD: `4150021` (LCL 0.9.1 released). Final HEAD: see **Git**.

No behaviour was changed: every step moved or reformatted code, renamed local
names, or rewrote test helpers, and was verified by the suites that already
existed. The product version stays 0.9.1; nothing was released.

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
- Rust was already in rustfmt's format; nothing else was reformatted.

## Workspace page

`app.js`, `app.css` and the page tests are formatted by Prettier 3.6.2
(`.prettierrc.json`: `printWidth` 100), as the implementation README now
says. The stylesheet went from one rule per line to the standard one
declaration per line. `index.html` is left as written. The script's long
functions were not restructured (see Not done).

## Android E2E script

`android/tools/e2e.sh`: the repeated inline Python one-liners became two
helpers (`json_field`, `no_request_waiting`), over-long `check "…" command`
lines put the command on its own line, three long commands are wrapped, and a
variable named `done` was renamed; `shellcheck` reports nothing.

## Verification

On the final source commit `0cd1d5d`, one heavy job after another
(`/mnt/F/.lcl-pretest/v05/cleanup-verify.sh`):

- **Gate** (`gate9-run.log`): 71 of 71 commands as expected in one run — fmt,
  clippy, the workspace suite on the current and the 1.75 toolchain (1970
  passed, 0 failed, 1 ignored on each), Core checksums, validators,
  identities and anchors, `protected`, conformance and readiness, the
  `real_process` runs, remote (65) and updater (35) suites, manual manifest
  and examples, and the real Firefox smoke over the reformatted page.
- **Format checks** (`cleanup-checks.log`): `android/tools/format.sh --check`,
  `prettier --check` and `shellcheck` all clean.
- **Android**: 142 JVM unit tests pass (one new: the starter text). The full
  E2E with the cleaned script and the split routes behind `lcl-remote`: all
  phases passed, no ANR or crash (`e2e-run14.log`). Instrumented
  `LocalProjectsUiTest` in light and dark, `VersionDisplayTest`,
  `FileDrawerTest` 3/3.

Two tests that read source text had to follow the changes: `equivalence.rs`
opens the route table by path (now `routes/mod.rs`), and the updater's version
check parsed one line of `build.gradle.kts` that the formatter wrapped (it now
reads the statement however it is wrapped). Both were caught by running the
suites, before the gate.

Not run: a physical phone (none attached). Canon is untouched (`protected`
passes).

## Not done

- The page script's longest functions (`androidDevices` 323 lines, itself a
  closure with nine named inner functions; `openSettings` 248; `newDocument`
  200) were left: they are organised internally, and rewriting closure-heavy
  page code is more risk than it returns in a pass that must not change
  behaviour.
- `remote/src/session.rs` (1,287 lines) and `pairing.rs` (1,182) are the next
  candidates for the same split `routes.rs` got.
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
`wrap_kt_strings.py`, `rename_kt.py`).

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
9. this report

Build output of the verification was deleted afterwards (the repository's
ignored build folders, `/mnt/F/.lcl-pretest/v05/target`,
`/mnt/F/.lcl-closure-4t-4c1cd4c659b7/target-msrv`,
`/mnt/F/.lcl-pretest/update-target`, `/mnt/F/.lcl-pretest/remote-target`).

# LCL — Lazy project explorer, Projects home, parked documents, product 0.5.1

Task: "REPLACE CURRENT FILE-TREE WALKER WITH A LAZY PROJECT EXPLORER" (owner
prompt, 2026-09-30), with the owner's additions during the session: unsaved
documents survive a project switch; the next product version is 0.5.1; the
Android versionCode is derived from the version.

Starting HEAD: `78e5e8c70de91974fe0c40383c6ed9654968ed59` (v0.5.0's source).
Final source HEAD: `eb605e9fd6d84d73dbc3d1ea7342fc9ada859230` (see the commit list;
this report is committed after it).

## Project model

- **Projects folder** — a container of projects. A desktop launch
  (`--default-project`) starts on the **Projects home**: `GET /api/projects`
  lists that folder's immediate subfolders (every folder, with a `manifest`
  flag; a New Project folder holds documents and no manifest, so folders are
  not filtered by it), shallowly. The Projects folder is never listed as a
  tree: `GET /api/tree` answers 409 on the home. The fallback Projects folder,
  when none is chosen in Settings and no launcher named one, is the folder the
  window was launched with (`Routes.launched`), so it does not move with the
  active project.
- **Active project** — `POST /api/project/open?path=` (absolute path, must be
  a folder) makes one folder the window's project; the same route serves the
  home's list and "Open project folder…". `POST /api/projects/open` returns to
  the home. New Project creates in the Projects folder and then opens the
  new project itself (`opens_project`), never the Projects folder.
- **Rootless projects** — a folder without `lcl.project.json`, opened
  explicitly, is a project of its own, as before (`Project::rootless`).

## Old tree system

Removed: the recursive walk (`Walk`, `holds_document`, `PROBE_DEPTH`,
`MAX_SCANNED`, `MAX_ENTRIES`, `MAX_DEPTH`, the folder-pruning rule),
`Workspace::documents`/`listing`, `GET /api/documents`, the page's flat
`state.entries`/`state.collapsed`, Android's `folds` and flat `tree`,
`FileTree.toggle`/`reveal`, `authoring::inside`, and the tests whose only
purpose was the recursive algorithm (`tree.rs` was rewritten).

Retained: `document::resolve` and `lcl_capabilities::contains` for
containment, the `KIND` cache (now retained per listed folder), file
read/save/create/delete, engine and project semantics.

Normal navigation never scans the whole project: the page reads the root at
start and one folder per unfold; ↻ re-reads the root and the unfolded
folders only. The tests count the folders requested (`treeRequests()` in the
page suite; `children` parents in the Android controller test) and prove it.

## New backend API

- `GET /api/tree?parent=<id>` → `{parent, entries: [{id, name, directory,
  bytes, kind}], truncated}`: the direct children of one folder (`""` = root),
  folders first then documents, each in name order; every folder, empty or
  not; only `.lcl` / `.lcl.txt` documents; dot directories never; a link only
  when its target is inside the project. `truncated` is per folder:
  `MAX_CHILDREN` = 4096, the first 4096 kept. 400 for a parent outside the
  project (`..`, absolute), 404 for a missing folder, a document, a dot
  directory, or an unreadable folder — only when that folder itself is asked
  for.
- `POST /api/tree/folder?id=<id>` → `{id, directory: true}`: one empty folder
  under a folder that exists; 409 when taken, 400 outside the project, 422
  for a dot name or a missing parent.
- Depth: no crawl decides depth; a parent is validated by `document::resolve`
  (lexical normalisation, canonicalised ancestor, containment). The old
  `MAX_DEPTH = 12` was a crawl bound and is gone; nesting is not limited.

## PC explorer

Click a file: opens exactly that document, or activates its tab if open, and
reads and unfolds the folders on the way to it (`reveal`). Click a folder:
unfolds it (reading its children then) or folds it; re-unfolding re-reads.
Empty folders show. `+` prefilled with the folder of the open document; ▤ New
folder; folder context menu (New document here…, New folder here…). ↻
re-reads root and unfolded folders, keeps tabs, unsaved text, folds and the
active document; a folder that is gone leaves with everything below it.
"Folder limited to 4096 entries." sits under the folder it is about. ⌂ shows
the Projects home. Switching projects parks the project's tabs (edits, order,
active tab, unfolded folders) under its folder and restores them on return;
the home marks a project with parked unsaved work with ●; closing the window
with unsaved work anywhere makes the browser ask (`beforeunload`).

## Android

- Protocol (`lcl.remote/1`, version unchanged, additive): `children`
  (`project`, `parent`) → `GET /api/tree?parent=`; `mkdir` (`project`,
  `folder`) → `POST /api/tree/folder`.
- Compatibility retained: `tree` stays for the apps of LCL 0.5.0 and earlier
  — the explorer flattened into their one answer (depth first, each folder's
  contents right after it, ≤12 levels, ≤4096 entries, `truncated`). A current
  app asks `children`; against a PC answering `children` with 400 (unknown
  operation) it falls back to `tree` once and takes the folder's children out
  of it. Nothing else of the old walk remains.
- App: `Explorer(folders, expanded)` per project; a folder tap asks the PC
  for that folder alone; file tap opens and unfolds the way to it; empty
  folders show; Refresh re-reads root + unfolded; **Folder** makes an empty
  folder through `mkdir`; the note `limited:<folder>` under a cut-short
  folder; switching projects starts the explorer over (project isolation).
  The drawer, tabs, active highlight, dirty marker, role labels and readiness
  are unchanged.

## Security

Containment is `document::resolve` + `lcl_capabilities::contains`, reused,
no second validator. Symlinks: listed only when their target is inside the
project; a link out is neither listed nor listable (`children("linked-dir")`
refused). `..`, absolute ids, `.git` and non-folders are refused. Secret scan
of tracked files and of the diff since 78e5e8c: none (the public update key
only).

## Core

`canonical/` untouched (the gate's `protected` check and identity/anchor
checks pass); no language semantics changed; Core 0.3.0 remains
`independent_review = pending`, `release_gate_permitted = false`.

## Product version

0.5.1 in the three Cargo manifests and locks and the Android default. Android
versionCode is derived from the version by one rule in `build.gradle.kts`
(`versionCodeOf`) and `build_update_release.sh`: MAJOR·1 000 000 +
MINOR·1 000 + PATCH, so 0.5.1 → 5001 (> 7 of v0.1.1, > 8 of v0.5.0), growing
with every version; overrides remain for tests only. Not released: no tag, no
build, no publication in this task.

## Tests

Focused, all green on the final source:

- backend `lcl-workspace`: tree 9, routes 24, projects 5 (incl. the Projects
  home over Alpha/Beta/unrelated and a rootless folder), persistence 21,
  files, authoring, localized, transport; page suite 73 cases (controlled and
  real-server modes; laziness by request counts, clicks, New folder, ↻,
  per-folder note, home, parking, close warning); real Firefox smoke 69/69
  (`/mnt/F/.lcl-pretest/v05/browser-smoke.mjs`).
- `lcl-remote`: 28 + 37 (children, mkdir, containment, legacy `tree`).
- updater: 23 + 8 + 2 + 2 (product version 0.5.1, derived code).
- Android JVM 124/0 (FileTree, controller lazy requests, refresh, create,
  mkdir, switch, legacy fallback, note, ProductVersion); instrumented on the
  emulator: FileDrawerTest 3, VersionDisplayTest 1 (LCL 0.5.1, never 5001),
  LocalDocument 2, ManualTab 1; full E2E 25 phases passed (p12 unfolds `proj`
  first).
- packaging: `test_update_release.py` 7/7; manual examples 97/0, manifest
  current.

Final gate (`/mnt/F/.lcl-pretest/v05/v05-gate.sh`, the USV gate with its own
prefix; 71 commands, each with an expected status): run once in full on
`af1366f` (`gate3-run.log`), where phases C and D passed every command (Core
0.1/0.2/0.3 checksums, validators, EBNF, identities and anchors, brand,
conformance text and readiness gate, `protected`; 12 sequential + 18
concurrent real_process runs) and three commands did not: the launcher test
(fixed in `0eb2993`), the pre-existing ETXTBSY flake of
`remote::tests::what_the_program_prints_is_what_the_route_answers` under the
1.75 toolchain, and `update-fmt` (fixed in `eb605e9`). Phases A, B and E were
then rerun on `eb605e9` (`gate4-run.log`): fmt, clippy and the workspace suite
(1970 passed, 0 failed, 1 ignored), the MSRV 1.75 check and suite (1970/0/1),
remote fmt/clippy/tests (65), update fmt/clippy/tests (35), manual manifest
and examples, and the real-browser smoke (69/69) — 15 commands, each with its
expected status. Phases C and D are unaffected by the two fix-up commits (a
test assertion and formatting).

## Documentation

Users Manual chapters 2 and 17 (explorer, ▤, ↻, ⌂, home, parking, close
warning, 0.5.1 samples), MANIFEST.json regenerated; `android/README.md`,
`android/SUPPORTED_OPERATIONS.md`, `remote/README.md` (protocol table,
compatibility); `packaging/README.md` (derived versionCode).

## Known limitations

- A page cannot offer its own Save on window close; the browser's standard
  leave/stay question is shown, and saving is manual.
- Parked documents live in the page only: reloading the page loses them
  (the browser asks first).
- Rename/move of files and folders remain outside the protocol; the tree API
  (`id` per entry, `parent` per listing) leaves room for them.
- No physical-phone run in this task (no device attached); the emulator ran
  the drawer, version and full E2E suites.
- The Android offline local projects + PC sync feature (owner's addition) is
  a separate task, not started here.

## Git

Commits on `main`, pushed; `HEAD == origin/main`:

1. `958ffa4` Workspace: a lazy project explorer and a Projects home
2. `a790123` Remote and Android: the same lazy explorer over the protocol
3. `59fccd2` Docs: the project explorer, the Projects home and the phone's tree
4. `d0fda9f` Workspace: unsaved documents survive a project switch; closing warns
5. `31764d9` Product version 0.5.1; Android versionCode follows from the version
6. `af1366f` Android E2E: unfold proj before its readiness
7. `0eb2993` Launcher test: a document opened by association is listed at its own root
8. `eb605e9` Updater test: rustfmt

## Verdict

LCL_LAZY_PROJECT_EXPLORER_READY — the lazy explorer replaces the recursive
tree on the PC, in the protocol and on the phone; every focused suite and the
governing gate pass on the final source. Product 0.5.1 is set but not built
or published; that is a separate release task.

# LCL 0.9 — Projects home cleanup, Android sync hardening, UI polish, product 0.9.0

Task: "PROJECTS HOME CLEANUP + ANDROID SYNC HARDENING + UI POLISH + PRODUCT
0.9" (owner, 2026-09-30/10-01).

STARTING HEAD: `93041b3ccee04cb766d532e9d0d756d1c08d0af8` (product source 0.5.2).
FINAL HEAD: see **Git** at the end (the report's own commit).

One correction to the task's premise, found at the state check: **v0.5.2 is
published** (GitHub release `v0.5.2`, 2026-09-30 21:18 UTC, made earlier that
day at the owner's request), so the latest published stable is v0.5.2, not
v0.5.1. Nothing about it was altered. A future v0.9.0 release takes v0.5.2 as
its predecessor manifest.

## PRODUCT

- machine version: **0.9.0** (`impl/Cargo.toml`, `remote/Cargo.toml`,
  `update/Cargo.toml` and their locks; Android `versionName` default)
- visible version: **LCL 0.9** (`Version::shown()` drops one trailing `.0`;
  Updates and About on the phone say "LCL 0.9", the PC's title bar and
  Settings the same)
- Android versionCode: **9000**, derived, never shown

## PROJECTS HOME

- **Previous bug.** `GET /api/projects` listed every non-dot subfolder of the
  Projects folder. With `/home/aivars/Documents` as the Projects folder, the
  home showed AI Agent Control Center APP, AIRO Smart Ebikes, API, Arch Dock,
  Agencies, Cline, Codex, … each tagged "no manifest".
- **New authority model.** A folder is on the home for one of two explicit
  reasons: it is a direct subfolder of the Projects folder holding
  `lcl.project.json` (it declares itself), or it is *kept*: registered in the
  product settings because New Project made it or the person opened it with
  **Keep in Projects Home**. Nothing is a project for being a folder, or for
  holding an `.lcl` file somewhere; no scan below the Projects folder's own
  entries and each candidate's manifest file (`routes.rs`, `list_projects`).
- **Registry location/schema.** The existing settings file,
  `$XDG_CONFIG_HOME/lcl/workspace-settings.json` (else
  `~/.config/lcl/workspace-settings.json`), schema version 1 unchanged, one
  new field: `"projects": ["<absolute canonical folder path>", …]`,
  deduplicated (`Settings::keep_project`), written atomically as before. The
  reader forgives: an entry that is not an absolute path is skipped and said,
  `projects` that is not a list registers none. It holds paths only — no
  project content, no language meaning; the engine never reads it, and a
  project absent from the home is exactly as valid as one on it.
- **Manifest projects** appear automatically (`manifest: true`,
  `registered: false`), cannot be "removed" (their reason is their file).
- **Rootless projects** still open by Open project folder… (`Rootless` badge
  on the home). **Keep in Projects Home** (checkbox in that dialog →
  `POST /api/project/open?path=&keep=1`) registers; **Remove from Projects
  Home** (the ⋯ menu of a kept project → `POST /api/projects/forget?path=`)
  unregisters. New Project registers what it made (`create_named_project`)
  and opens it as the active project, never the Projects folder.
- **Unrelated folder result.** On this PC (built binary, a copy of the real
  settings, the real `~/Documents` only listed): 14 direct subfolders, 0 with
  `lcl.project.json`, **0 listed** (`/mnt/F/.lcl-pretest/v05/pc-runtime.log`).
- **No user folder was deleted or moved.** `forget` rewrites the settings
  file and nothing else (`tests/projects.rs` compares Beta's tree before and
  after; the runtime check compares the rootless folder's listing before and
  after). No manifest is inserted anywhere, no directory renamed. Projects
  made by New Project *before* 0.9 hold no manifest and are not registered;
  they are re-added once with Open project folder… + Keep (the task's §11:
  reuse only trusted metadata, do not guess — there was none).

## LAZY EXPLORER

- PC: unchanged — `GET /api/tree?parent=` lists one folder's direct children
  (folders first, every folder even empty, documents `.lcl`/`.lcl.txt`, dot
  entries never), `MAX_CHILDREN` 4096 per folder with `truncated`; unfold reads
  one folder; ↻ re-reads root and unfolded folders. Runtime check: opening a
  rootless project read `['main.lcl']` at the root and nothing more; an empty
  folder made with `POST /api/tree/folder` showed at once.
- Android remote: unchanged (`children`, `mkdir`, legacy `tree` fallback).
- Android local: `LocalProjects.children` unchanged for the screen (4096 per
  folder, `truncated`). Sync no longer reads through it: `inventory()` walks
  the disk for every folder and document, bounded on its own by
  `MAX_SYNC_ITEMS` = 20 000 entries in all; beyond it the sync stops with
  everything kept.

## ANDROID OFFLINE PROJECTS

Unchanged and kept: create a local project, folders, `.lcl`/`.lcl.txt`
documents, edit, atomic save, reload, delete, the lazy explorer, app-private
persistent storage (`filesDir/local-projects`), no engine on the phone
(Check/Validate/Inspect/Run off with a reason). JVM `LocalProjectsTest` (5)
and the first two `LocalWorkspaceTest` cases prove create/edit/save/reload/
delete/restart still hold.

## SYNC SAFETY (Android)

Each point names the JVM test that reproduces the audited defect
(`android/app/src/test/java/io/lcl/workspace/LocalWorkspaceTest.kt`, 12 tests
in the class after this task):

- **Dirty-document guard.** `planSync`, `runSync`, `verifyRemovable`,
  `removeLocalProject` and `removeLocalProjectConfirmed` all refuse while the
  project has an open document with unsaved edits, listing them ("Save these
  documents before syncing:\n• main.lcl"). Nothing is auto-saved, no stale disk
  bytes are sent, no project with unsaved editor text is deleted. Test
  `unsaved_edits_block_sync_and_removal_until_they_are_saved` replays the
  exact sequence disk = A, editor = B dirty, plan made earlier, Sync-and-remove
  → refused, PC empty, project kept, B still on screen; after Save the PC gets
  B and the project may go.
- **Complete inventory.** `LocalProjects.inventory()` (folders + documents,
  disk walk, own bound). `fullySynced` and the plan use it; the explorer's
  `entries.take(MAX_CHILDREN)` is never a source of completeness.
- **>4096 test.**
  `every_document_beyond_what_the_explorer_shows_is_synced_and_nothing_goes_before_all_are_confirmed`:
  4097 documents in one folder — the explorer shows 4096 and `truncated`, the
  plan has 4097 items; with the PC not confirming the last one the result is
  partial, not complete, not removed, all 4097 still on the phone; the next
  sync finds 4096 identical, confirms the last, and only then removes. The
  same test proves the own bound: `LocalProjects(root, maxSyncItems = 3)`
  refuses the inventory of 4 documents and `fullySynced` is false.
- **Empty-folder sync.** `SyncPlan.folders` carries every local folder;
  `runSync` makes each on the PC (`mkdir`; a 409 is accepted only when
  listing it proves a folder is there) and records it. Tests: the
  whole-project case now expects `phone/Alpha/empty` made; 
  `a_project_of_empty_folders_syncs_its_structure_and_may_then_go` syncs
  `planning/` and `planning/q1/` alone, complete and removable.
- **PC revalidation before standalone removal.** `verifyRemovable` needs the
  connection and asks the PC, for every folder (`children`) and document
  (`open`, digest == the recorded PC digest), that it still holds what was
  recorded; any difference refuses removal naming the entry; without the PC
  nothing is claimed. The Remove dialog runs it and enables Remove only on
  success (`remove_verdict`). Test
  `removal_asks_the_pc_again_and_is_refused_when_it_no_longer_holds_everything`:
  file deleted on the PC → refused naming `b.lcl`; changed → refused; folder
  gone → refused; restored → allowed; PC forgotten → "Connect to the PC
  first". **Sync and remove from phone** uses that same sync's confirmations,
  only for a whole-project plan that is complete.
- **Local change during sync.** The plan keeps each document's digest; at the
  end `runSync` reads the inventory and every confirmed document again — an
  edited document, or an entry that appeared or went, makes the sync
  incomplete, unremoved, and is named in `problem`. Test
  `what_changes_on_the_phone_during_the_sync_is_not_synced_and_the_project_stays`
  (b.lcl rewritten and a folder made while a.lcl is being created).
- **Conflict destination persistence.** A record whose PC path lies in the
  same PC folder is tried first: while the PC still holds exactly what it
  confirmed there, that path is the destination, IDENTICAL or — the phone's
  text being newer — the new state `SUPERSEDED`, written by `save` with the
  PC's digest as base and no question asked. If the PC copy changed or went,
  the usual destination is judged again (a DIFFERENT conflict as before).
  Covered in `a_sync_is_planned_against_the_pc_and_writes_only_what_the_person_chose`.
- **Partial failure.** Outcomes are reported exactly (✓/✕ per entry);
  `SyncResult.partial` when some reached the PC and some did not; the dialog
  titles it "Partial sync of …" and says the phone's copy is unchanged and
  the next sync continues; no rollback on the PC is invented.
- **Awaited removal.** `removeLocalProjectConfirmed` is a `suspend` function
  that deletes, then updates the UI state, then returns; `runSync` sets
  `removed = true` only from its success. A deletion the disk refuses returns
  `removed = false`, the project stays listed, the reason is shown. Test
  `a_removal_the_disk_refuses_leaves_the_project_listed_and_is_not_claimed`
  (a non-writable subfolder; skipped as root).
- **One write per sync.** Confirmations are collected and written to
  `.sync.json` once at the end (`recordSyncedAll`) instead of one atomic
  rewrite per document — the 4097-document test exposed the O(n²) rewrite
  (and 4097 fsyncs). A crash before that write loses no data: the next sync
  asks the PC and finds the files identical.

Reused, not duplicated: `OpenDocument.dirty`, `LclNames`, the digest helper,
`FileTree.ancestors/parentOf`, the `Explorer` rows, the PC's existing
`open`/`mkdir`/`create`/`save`/`children` operations, `LocalProjects.replace`
for every write. No protocol change.

## PC VERSION REQUIREMENT FOR SYNC

Verified from source: `mkdir` and `children` were added to `remote/src/session.rs`
in `a790123` (2026-09-30); `git grep` finds them in tag `v0.5.1` and not in
`v0.5.0`. So sync needs **PC LCL 0.5.1 or newer**. `planSync` probes
`children` on the PC project's root first; a PC answering "unknown operation"
(400) gets `Syncing local projects requires LCL 0.5.1 or newer on the PC.` and
nothing is attempted (test
`a_pc_without_the_explorer_operations_is_named_as_too_old_and_nothing_is_attempted`).
`android/README.md` and manual chapter 17 say so; the earlier claim that 0.5.0
works is withdrawn.

## UI REDESIGN

- **PC** (`impl/crates/lcl-workspace/assets/app.css`, one stylesheet, no
  framework, no web font — the CSP is self-only): graphite surfaces via
  `light-dark()` tokens under `color-scheme` (both themes from one palette;
  `data-theme` narrows), one blue accent, 4–8 px radii, 120 ms transitions,
  compact IDE-density explorer rows with an inset accent bar for the open
  document, dirty ● kept, pill role badges, tabs with an underline, a
  denser title bar with the brand mark and a monospace path pill, dialogs
  with a fixed title / scrolling body / reachable actions (the viewport rule
  the page suite pins is kept: `max-width: min(560px, 100%)`), toasts with a
  colour bar plus text (never colour alone). Projects home: rows with a glyph,
  the name, `Rootless` when there is no manifest, ● for parked unsaved work,
  and a ⋯ action menu (visible on hover/focus) for kept projects; the
  Projects folder path; + New project / Open project folder… (with Keep in
  Projects Home). Every id and class the tests and smoke depend on is kept.
- **Android** (`Theme.kt`, `HomeScreen.kt`, `WorkspaceScreen.kt`): the PC's
  palette (graphite background, the same accent, outline/variant colours),
  Material 3 shapes reduced to 4–12 dp; the dashboard as labelled sections
  (**On this phone**, **On <PC name>**, **On a PC** when none is paired) with
  a rule instead of nested cards; a local project's status line under the
  project name (`local_status`: Unsaved changes / Local — not synced yet /
  Synced — the PC confirms again before removal / Changed since sync / Empty)
  read again after edits, saves, explorer changes and every sync; the sync
  dialog's titles and texts for synced / synced and removed / partial / failed,
  the summary's "N to update" and "N folders"; the Remove dialog's PC verdict.
  Settings, About, Updates, Manual and the pairing screens are touched only by
  the theme.
- **Accessibility.** Keyboard menus and focus rings kept (`focus-visible`
  rings on buttons, rows, tabs), `aria-label` on the ⋯ button and `role=menu`
  items, Escape closes menus, `prefers-reduced-motion` disables transitions,
  no state by colour alone (glyphs and words accompany it).
- **Responsive tests.** The page suite's dialog-viewport case and the real
  Firefox smoke (desktop viewport) in the gate; the phone layouts in the
  instrumented tests (narrow `lcl36` emulator) and the E2E.

## VERSION / PACKAGING

- **0.9.0 consistency**: `update/tests/product_version.rs` reads every
  manifest and the Gradle default; `VersionDisplayTest` (instrumented) expects
  versionName 0.9.0 / versionCode 9000 and the texts "Installed version:
  LCL 0.9" and "LCL 0.9", never the code.
- **9000 derivation and bounds**: `versionCodeOf` in `build.gradle.kts`
  (`matchEntire` of `^(\d+)\.(\d+)\.(\d+)$`, MINOR and PATCH ≤ 999, code in
  1..2 100 000 000) and the same rule in `build_update_release.sh`'s awk
  (anchored `$`, `$2 <= 999 && $3 <= 999`, `code <= 2100000000`). The Rust test
  mirrors the rule and asserts both scripts carry it: 0.5.999 → 5999,
  0.6.0 → 6000 (neighbours, never equal), 2100.0.0 accepted, and refused:
  0.5.1000, 0.1000.0, 2101.0.0, 0.0.0, 1.2, 1.2.3.4, v1.2.3, 1.2.3-rc1, padded
  and spaced forms. The awk was run by hand on the same cases.
- **Product/Core separation**: `build_release.sh` no longer defaults its
  Core-bundle selector to the product version (which refused 0.9.0); the
  default is the newest Core package in the captured source (0.3.0),
  `LCL_RELEASE_VERSION` still chooses. `build_update_release.sh` keeps its
  explicit `LCL_RELEASE_VERSION=0.3.0`.
- **Archive identity**: the candidate, its archive and its provenance are
  now named by the product version (`lcl-0.9.0-linux-x86_64.tar.gz`, top-level
  directory `lcl-0.9.0-linux-x86_64`, `lcl-0.9.0-PROVENANCE.txt`); the
  provenance records `core bundle: 0.3.0` and `product version: 0.9.0`.
  Safe for every published updater: `stage.rs` takes the top-level directory
  from the archive itself and requires only the `lcl-…-linux-x86_64` shape
  and safe members. `packaging/test_update_release.py` 7/7 (its snapshot's
  product version equals the Core one, so its fake build stays valid).
- `.vscode/settings.json`: the Python environment setting is gone; the Java
  build-configuration setting (Android/Gradle in the editor) stays.

## UPDATER REGRESSION

Unchanged trust architecture: one production key `lcl-update-1`, signed
manifest, previous-manifest continuity, artifact digest/size checks, Android
signer continuity, downgrade refusal, staged validation, rollback. `lcl
--version` stays one line. Tests: update crate `product_version` 2/2, the
whole update suite in the gate, packaging tests 7/7. The v0.5.1 → v0.9.0 (and
v0.5.2 → v0.9.0) updater path is proven at release time by the same
procedure the 0.5.x releases used (published predecessor manifest,
byte-identical); not run here, since publication is not part of this task.

## CORE/CANONICAL STATUS

`canonical/` untouched (the gate's `protected`, checksum, identity and anchor
checks). Core 0.3.0 remains `independent_review = pending`,
`release_gate_permitted = false`. LCL 0.9 is the product; no Core version
changed.

## SECRET SCAN

Tracked files: no `.jks`, `.keystore`, `.p12`, `.pfx`, `.pk8`, `.pem`, `.key`,
`keystore.properties`, `local.properties`, `.env`; no private-key blocks in
tracked text; the diff since 93041b3 adds no password, token or key material.
`update/trusted_keys.txt` holds the public key of `lcl-update-1` only.

## FOCUSED TESTS

- `lcl-workspace`: `projects` 5/5 (home over Alpha/Beta/Delta/unrelated/
  Photos/empty, keep, forget, disk untouched, New Project registers), settings
  unit tests 7/7 (round trip with projects, skipped relative entries, atomic
  store), page suite 74/74 in the controlled DOM double and 74/74 against the
  real server (`editor_save` floor raised 69 → 74), real Firefox smoke in the
  gate, `cargo clippy -D warnings` and `cargo fmt --check` clean. The full
  crate run passed everything except `equivalence.rs`, whose three tests need
  the `lcl` binary a whole-workspace build makes (a single-crate run does not:
  the test says so itself); the gate runs the whole workspace.
- Android JVM: `LocalWorkspaceTest` 12/12, `LocalProjectsTest` 5/5,
  `WorkspaceControllerTest`, `ProductVersionTest` — 37/37 across the runs.
- Updater: `product_version` 2/2, clippy and fmt clean; packaging
  `test_update_release.py` 7/7; `bash -n` on both scripts.
- PC runtime (built binary): 16/16 checks (see below).

## FINAL GATE

Governing gate `/mnt/F/.lcl-pretest/v05/v05-gate.sh` (71 commands, each with
an expected status), on the final source commit **`aaf3766`**:

- Full run (`/mnt/F/.lcl-pretest/v05/gate7-run.log`): **70 of 71 as
  expected** — fmt, clippy, the MSRV 1.75 check and suite (1970 passed, 0
  failed, 1 ignored), Core 0.1/0.2/0.3 checksums, validators, EBNF, identities
  and anchors, `protected`, the conformance text and readiness gate, the 12
  sequential and 18 concurrent `real_process` runs, remote fmt/clippy/tests
  (65), update fmt/clippy/tests (35), manual manifest and examples, the real
  Firefox smoke, and the selftest's two deliberate failures. One mismatch:
  `test-workspace`, 1969 passed and 1 failed —
  `remote::tests::a_failure_is_reported_in_the_program_s_own_words` in
  `lcl-workspace`, with "Text file busy (os error 26)": the known ETXTBSY race
  of the test process (see Known limitations), not a statement about the
  product.
- Rerun of that command's phase on the same commit (`gate7b-run.log`): fmt,
  clippy and `test-workspace` as expected — **1970 passed, 0 failed, 1
  ignored**.

So every one of the 71 commands has its expected status on `aaf3766`: 70 in
the full run, `test-workspace` in the rerun. The gate keeps one results file
per run, so there is no single 71-row verdict: the full run's table is in
`gate7-run.log`, the rerun's three rows in `gate7b-results-aaf3766.tsv`.

For the record: the first full run, on `56cbe0f` (`gate5-run.log`), found one
real defect of this task — the hardening test
`a_0_2_0_candidate_records_both_languages` still read the candidate by its
Core bundle's name after the §47 rename (fixed in `e578cb3`) — and one ETXTBSY
flake in the updater's `flow.rs`, green on rerun. Phases a, b and e then
passed on `3d1fe4f` (`gate6-run.log`, 15/15). The verification was interrupted
once by a shutdown of the PC and resumed the next morning.

**Android E2E** (`android/tools/e2e.sh`, the `lcl36` emulator against a real
`lcl-remote`), on `aaf3766` (`e2e-run12.log`): **all phases passed** — the
seven no-PC phases; p1 pair and work (edit, save, Check, Validate, Inspect,
an approved run); p2–p11, p13, p14 (reconnects, network loss, PC restart,
conflict, revocation, repair, scan, denied pairing, key retirement, forget,
Back); p12 (Core 0.3 roles and readiness); p15/p16 (an update found, verified,
installed in place, the pairing kept); p17 (a project made on the phone
synced into the PC's project, the PC's bytes checked on disk, the phone's copy
removed only after confirmation); and no ANR or crash of LCL in the system's
record. Two harness defects were fixed on the way, both from reusing
`E2E_DIR`: a backup file left in the run's work folder made the run's
`core.create` refuse at p1 (`e578cb3`), and device records left in the
service's trust store by earlier and interrupted runs failed "exactly one
device" after p1 (`aaf3766`: a run now removes its own config, state, data,
work folder, evidence and update release before it starts).

**Instrumented UI tests** on the emulator (`ui-*.txt`): `LocalProjectsUiTest`
1/1, `VersionDisplayTest` 1/1 (versionName 0.9.0, versionCode 9000, "LCL
0.9", never the code), `FileDrawerTest` 3/3 — run with `adb shell am
instrument`, since Gradle's connected task needs an artifact the offline cache
lacks.

## PHYSICAL PHONE RESULT

**NOT RUN.** No phone was attached (`adb devices` empty) and no release
carrying this build exists; the owner's acceptance list (install in place,
"LCL 0.9", offline create/folder/document/edit/save, reopen, reconnect, sync,
folders on the PC, conflict, dirty refusal, save and sync, remove, PC copy
remains, next start) is for the release task.

## PC RUNTIME RESULT

Run on this PC against the freshly built `lcl-workspace` with a *copy* of the
owner's settings (`XDG_CONFIG_HOME` in a scratch directory), so the real
settings file was not written (verified byte-identical afterwards), script
`/mnt/F/.lcl-pretest/v05/pc-runtime.sh`, log `pc-runtime.log`:

1. The real Projects folder `/home/aivars/Documents/` (listed only): 14 direct
   subfolders, 0 with `lcl.project.json`, **0 listed** — Arch Dock, Photos,
   canva-linux, Jaaropgraaf, Agencies and the rest are gone.
2. A scratch Projects folder: only the declared project listed at first; New
   project creates `Made` and opens it, and `Made` appears at once (registered,
   rootless); a rootless folder opened without Keep is not listed, with Keep
   it is; the tree is lazy (root children only); an empty folder is made and
   listed; Remove from Projects Home forgets it and its folder's listing is
   byte-for-byte what it was; a random folder never appeared.

Not run on the installed 0.5.2 workspace (it has no registry; the updater
path is exercised at release time).

## DISK BEFORE/AFTER

Before the gate (2026-10-01 00:04): `/mnt/F` 62 G used / 197 G free, `/`
151 G used / 35 G free; repository 4.6 G with build output; `/mnt/F/.lcl-android`
7.5 G (SDK, JDK, AVD, Gradle caches — kept).
Peak during the verification: `/mnt/F` 87 G used.

After the verification and the cleanup (2026-10-01 07:09): **`/mnt/F` 56 G
used / 202 G free**, repository 233 M. Deleted, all rebuildable build output
of this task's test and gate runs:

- in the repository (ignored by Git): `impl/target` (3.8 G), `remote/target`,
  `update/target`, `android/app/build`, `android/build`, `android/.gradle`;
- outside it: `/mnt/F/.lcl-pretest/v05/target` (9.5 G, the gate's Cargo
  target), `/mnt/F/.lcl-closure-4t-4c1cd4c659b7/target-msrv` (14 G, the gate's
  Rust 1.75 target), `/mnt/F/.lcl-pretest/remote-target` (2.6 G, the E2E's
  `lcl-remote`), `/mnt/F/.lcl-pretest/update-target` (0.4 G).

Kept, hidden folders beside the repository: `/mnt/F/.lcl-android` 7.5 G (JDK,
Android SDK, emulator image, Gradle caches, the debug keystore the app is
signed with), `/mnt/F/.lcl-pretest` 3.7 G (logs and evidence of this and
earlier tasks), `/mnt/F/.lcl-residual-repair-01-lxd8dwu8` 1.1 G (the Rust 1.75
toolchain the gate needs), `/mnt/F/.lcl-closure-4t-4c1cd4c659b7` 0.5 G,
`/mnt/F/.lcl-repair-6t` 0.2 G, `/mnt/F/.lcl-releases` 0.1 G (published
manifests, release notes), `/mnt/F/.lcl-bootstrap-20260928`,
`/mnt/F/.lcl-updater-repair-20260928`. No global Cargo or Gradle cache and no
signing material was deleted; `git clean` was not used.

## GIT

Commits on `main` after `93041b3`:

1. `2c0e763` Workspace: the Projects home lists declared and kept projects only
2. `4b2d8f6` Android: sync sends the whole project, waits for unsaved edits, and removes only what the PC confirms again
3. `2093687` UI: a graphite workspace look on the PC and the phone
4. `7ec3e70` Product version 0.9.0; bounded versionCode rule; the candidate is named by the product version
5. `56cbe0f` Docs: the Projects home, phone sync safety, versioning
6. `e578cb3` Tests: the candidate's name follows the product version; the E2E's work folder starts empty
7. `3d1fe4f` Report, first version (pushed at the owner's request when the PC was shut down mid-verification)
8. `aaf3766` Android E2E: a run starts without what an earlier run left in its work directory
9. this report

`git diff --check` clean; working tree clean; pushed, `main` == `origin/main`.
No tag and no release were made.

## KNOWN LIMITATIONS

- Projects made by New Project before 0.9 are not on the home until re-added
  once (Open project folder… → Keep in Projects Home); no metadata identified
  them, and guessing was ruled out.
- A kept project whose folder is absent (unmounted, moved) is not listed
  while absent; its registration stays until the folder returns and it is
  removed from the home, or the settings file is edited.
- A removal the disk refuses can leave part of the phone's copy (what
  `deleteRecursively` managed); everything removed was confirmed on the PC
  before, and the project stays listed until removal succeeds.
- Sync records name the PC project, not the PC; re-verification asks the
  connected PC for that project. A different PC with the same project id and
  bytes at the same paths would also pass — the bytes are then on that PC.
- Android polish stops at the theme, the dashboard, the Files pane and the
  sync/remove dialogs; Settings, About, Updates and pairing are restyled by the
  theme only.
- Physical phone acceptance not run (no device); the updater's 0.5.x → 0.9.0
  path is a release-time proof.
- `equivalence.rs` in `lcl-workspace` needs a whole-workspace build; a
  single-crate test run reports it as failed for that reason alone.
- The ETXTBSY flake is still there. Tests that write a stand-in program from
  the test process and execute it at once (`lcl-workspace` `remote::tests`,
  the updater's `flow.rs`) fail now and then with "Text file busy": another
  test thread that forks in that window carries the still-open write
  descriptor until its own exec. It cost three gate commands across this
  task's runs, each green on rerun. The fix is test-only — retry the launch on
  that one error, or have the helper write the stand-in through a child
  process — and was left out of this task's scope.

## FINAL VERDICT

**LCL_0_9_SOURCE_READY**

Every required source fix is in, and the final governing gate passes on
`aaf3766`: all 71 commands with their expected status (70 in the full run,
`test-workspace` on the rerun of its phase after an ETXTBSY flake), the full
Android E2E, and the instrumented UI tests. The product source is 0.9.0,
shown as LCL 0.9, Android versionCode 9000; a release candidate is buildable
(`packaging/build_update_release.sh`, predecessor manifest: published v0.5.2).

Not done here, by the task's own terms: publication of v0.9.0 (a separate
owner authorisation, after the PC runtime test on an installed build and the
physical-phone acceptance, which was not run). Not claimed:
`LCL_0_9_RELEASED`, `CORE_0_3_RELEASED`, `CORE_0_3_REVIEW_COMPLETE`.

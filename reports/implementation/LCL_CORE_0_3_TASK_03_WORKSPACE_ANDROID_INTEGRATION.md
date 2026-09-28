# LCL Core 0.3 — Task 03: Workspace, Android, Manual Access, Conflict Safety and Final Integration

Governing pack: `/mnt/F/LCL_Core_0.3_Task_Pack/` — `tasks/TASK_03_WORKSPACE_ANDROID_INTEGRATION.md`,
`GLOBAL_RULES.md`, `ACCEPTANCE_GATES.md` (gates 0–10), and the contracts
`ARCHITECTURE_CONTRACT.md`, `PROJECT_MODEL_CONTRACT.md`, `TEMPLATE_CONTRACT.md`,
`USERS_MANUAL_ACCESS_CONTRACT.md` and `USAGE_CONTRACT.md`.

Result: **LCL_CORE_0_3_TASK_03_BLOCKED** — only on the physical-phone smoke, which needs the owner (section 9). Every other gate is closed.

## 1. Starting state

- Branch `main` at `b3f6e56` ("LCL core 0.3 task 2 part2"), clean and equal to
  `origin/main`. Task 01 (`1b55445`, `865bef9`) and Task 02 (`75330e6`,
  `b3f6e56`) were committed by the owner, who then asked for Task 03 and for
  its gates to be closed.
- Core identities at the start: 0.1.0 `00d648b1…67ed` (176 files), 0.2.0
  `061a79c9…8b3f` (216 files), 0.3.0 `bf66e369…04aa12` (291 files). No
  canonical file was changed by this task (section 7).
- Found while surveying: the desktop workspace and `lcl-remote` never attached
  the Core 0.3.0 engine (only the CLI had `--project-spec`), so neither could
  judge a 0.3.0 document or project. That wiring came first.

## 2. What was built

No language rule was added or changed. Roles, scaffolds, Master validity,
readiness and admission remain the Core 0.3.0 engine's; the desktop page and
the Android app only ask and show.

### Part A — Desktop project UX

- **Core 0.3.0 in the workspace.** `Workspace::open_with_specs` attaches the
  0.3.0 engine; `lcl-workspace --project-spec`, then `LCL_PROJECT_SPEC`, then
  the manifest's `project_spec` (the CLI's order).
- **New Project** (⊞ in the Project panel): folder inside the workspace, Core
  version, start (Default/Canonical × Guided/Minimal, or a project Master),
  exact preview of every file with its role, origin and marked lines, then
  Create. `GET /api/project/plan` writes nothing and returns a `plan_digest`;
  `POST /api/project` recomputes the plan and refuses (409) a plan that is not
  the previewed one, and (428) a creation without a preview. Creation is
  all-or-nothing (`masters::create`).
- **New File by role** (+): Blank LCL file or Task, Description, Rules,
  Context, Data, Output, Checks, Definitions (the engine's `part_kind`
  domain, `GET /api/roles`), with the exact text previewed
  (`GET /api/scaffold`). `POST /api/document?role=…` makes the server write the
  scaffold or Master for exactly the final name; a body with a role is
  refused.
- **Tree roles.** `GET /api/documents` adds `kind`: the `SPECIFICATION KIND`
  the judging engine reads in the file — from the parse, or, for a file that
  does not parse yet (a fresh scaffold with empty slots), from the engine's
  own lexer tokens inside the top-level `SPECIFICATION` block. Never from the
  file name.
- **Readiness panel.** For an entry (declares `kind.project`) or a part of the
  entry last shown: `validate` over the entry (buffer when open, else
  `GET /api/project/status` on disk). States loading / incomplete / invalid /
  ready / running; per-file ready, invalid, missing, omitted, duplicate;
  diagnostics listed and clickable. The panel's entry is never guessed.
- **Run refusal is the engine's.** A run of a non-admitted project ends
  rejected before any invocation (test in section 6). The Run button is also
  disabled for an entry known not to be ready (secondary).
- **Validate** button added beside Check/Inspect/Run.
- **Slots in the editor.** `POST /api/slots` returns the engine's marks
  (`scaffold::check_text`) for the live buffer of a part; the gutter marks
  required and optional slots, guidance and generated IDs, and the status bar
  counts required slots still empty. This closes Task 02's deferred "marks in
  the editors" for the desktop.
- **Cross-file navigation**: go to definition from the entry's `EXECUTE`
  reference opens the part that declares it (verified in a real browser).

### Part B — Users Manual access

- **One snapshot.** Every top-level `users_manual/*.md`; version and digest in
  the new `users_manual/MANIFEST.json`, written and checked by
  `users_manual/tools/manual_manifest.py`. Digest: SHA-256 over, per file in
  byte order of name, `name NUL decimal-length NUL bytes`. Current: version
  `0.3.0`, 23 files, digest `fa189d307a342f12ede0db1ec02810b570d6ab111624f815b2e3cfa89fcd4dd3`.
- **One viewer.** `impl/crates/lcl-workspace/assets/manual/` (HTML, JS, CSS):
  table of contents, per-chapter sections, search, back/forward, code blocks,
  session position. Markdown is rendered into DOM nodes with `textContent`
  only, so no manual text becomes markup; only snapshot files are reachable,
  other link targets are shown as text.
- **Desktop.** A `?` button (`aria-label`/title "Users Manual") beside
  Settings opens `/manual/` in its own window (`window.open`, named window).
  The snapshot is compiled into the binary (`build.rs`), so it is offline;
  `/manual/snapshot` serves only embedded bytes; every manual route needs the
  session token.
- **Android.** Bottom navigation **Workspace | Manual**; a `?` button on the
  home and file screens switches to Manual. The Manual tab is a WebView of the
  same viewer, loaded from the app's assets, JavaScript bridge limited to
  snapshot/position; every navigation away is refused. Gradle copies
  `users_manual/*.md`, `MANIFEST.json` and the viewer into the APK at build
  time. Editor undo history and selection are held above both tabs; open
  documents and the connection live in the app container, so switching tabs
  loses nothing. The tab survives activity recreation.
- **Parity evidence** (section 6): Rust test (embedded = manifest = source
  files), Android JVM test (packaged assets = manifest = source; viewer files
  byte-identical to the desktop's), instrumented test on the emulator
  (installed APK assets = manifest = the digest the desktop serves), and the
  E2E host check that unzips the APK and recomputes the digest.

### Part C — Settings → Templates

Settings → **Templates…**: list with validity; per-role default (a Master, or
*Canonical scaffold* to reset); New (starts from the role's canonical
structure, `GET /api/master/starter`), Edit, Duplicate, Delete. Masters are
edited as their JSON file and saved only after the engine accepts them
(`PUT /api/master?create=1` refuses an id in use; `?replace=<id>` refuses an id
change). Deleting never touches created files (Task 02's copy semantics,
retested through HTTP).

### Part D — Android project integration (through the PC)

New remote operations (`remote/README.md`): `roles`, `scaffold`, `project`
(readiness), and `create` with `role`/`mode` (the PC writes the scaffold; the
phone sends no text and holds no scaffold). The app shows each file's
declared role in the tree, a **Readiness** dialog for entries (files and
statuses exactly as the PC engine reports; Run stays the PC's decision), and
New with a kind (Blank or a role) and Guided/Minimal. Open/edit/save
(revision-aware), Check, Validate, Inspect and Run were already there. No LCL
parsing was added in Kotlin.

### Part E — A13 identity collision (Android)

`ConnectionManager.pair` refuses a pairing code whose PC id is already paired
under a different certificate fingerprint, before any key is created or any
address contacted, and re-checks at the moment of saving. Nothing is replaced,
retired or deleted; the old pairing keeps connecting. The message
("Identity conflict: … Forget it on the PCs screen first, then pair again")
is shown on the Pair screen.

### Part F — A14 revision-safe desktop save

`PUT /api/document` now requires `base` (the digest of the revision edited):
missing → 428, nothing written; stale or deleted → 409 with `conflict: true`,
`exists` and the disk `digest`, nothing written (`Workspace::save_expecting`).
The page sends its last acknowledged digest; queued saves run in order, each
basing itself on the previous acknowledgement, so the editor never conflicts
with its own write. A conflict opens **Changed on disk**: Cancel, **Reload disk
version**, or **Keep mine…** which asks once more and then saves over exactly
the reported disk revision (or re-creates a deleted file). Never merged. The
cross-process window stated in `document::write_expecting` (the read-and-hash
instant) remains the documented boundary of A5; its comment no longer says the
desktop saves without a precondition.

### Part G — Existing Android bugs

No Android UI/state bug was reproduced in this task. The owner's earlier
report ("some bugs, or maybe I don't understand how to use it yet",
2026-09-25) has no steps yet; nothing was changed speculatively.

### Part H — Migration

Tree context menu → **Convert to multi-file project…** for a `kind.task`
document. The only decomposition that needs no choice
(`05_SEMANTICS/13`: `kind.part.task` admits every family; IMPORT, EXTENSION
and EXECUTE belong to the entry): `main.lcl` with the header (LCL `VERSION`
0.3.0, `KIND: kind.project`), one `PART`, and the IMPORT/EXTENSION/EXECUTE
blocks; `task.lcl` with the header (`ID <spec>.task`, `KIND: kind.part.task`)
and every other top-level item byte for byte. Offered only when the result is
admitted by the 0.3.0 engine (validated in memory); refused with the reason
for localized documents (the entry would mix spellings), non-task kinds,
unparsable documents or an existing target. Exact preview, `plan_digest`
binding, all-or-nothing creation; the original is never modified. Further
splitting is left to the person, as the task requires.

### Packaging

`install.sh`/`uninstall.sh` install and remove `share/LCL_Core_0.3.0` when a
payload carries it; the launcher passes it as `--project-spec`;
`build_release.sh` accepts `LCL_RELEASE_VERSION=0.3.0` (bundles 0.1.0, 0.2.0,
0.3.0, and checks `lcl version` reports all three). No candidate was built.

### Users Manual

New chapter 19 "Your first multi-file project" — the beginner flow the task
lists (New Project, Guided scaffold, Description, Rules, Task, Check, fix,
Validate, Run, manual icon), plus Android, save conflicts, conversion and
Templates; `--project-spec` in chapter 17; README row. The manual stays
documentation, not authority.

## 3. HTTP routes added (desktop workspace; the PC service calls the same in process)

| Route | Purpose |
|---|---|
| `GET /api/roles` | engine roles, labels, default Master per role |
| `GET /api/scaffold?role=&path=[&mode=&source=&master=]` | exact text + marks of a new file, written nowhere |
| `POST /api/document?id=&role=…` | create a file of a role (no body) |
| `POST /api/slots?id=` | engine marks of a buffer |
| `GET /api/project/plan?folder=…`, `POST /api/project?…&plan_digest=` | New Project preview / creation |
| `GET /api/project/status?entry=` | `validate` report of an entry on disk (readiness) |
| `GET /api/masters`, `GET/PUT/DELETE /api/master`, `GET /api/master/starter`, `PUT /api/masters/default` | Templates |
| `GET /api/convert/plan`, `POST /api/convert` | conversion preview / creation |
| `GET /manual/`, `/manual/manual.js`, `/manual/manual.css`, `/manual/snapshot` | Users Manual window |
| `PUT /api/document` | now requires `base` (A14) |

## 4. Files changed

- `2b52bc7` "LCL core 0.3 task 3 part1" (55 files): `impl/crates/lcl-workspace/`
  (`src/authoring.rs`, `src/manual.rs`, `build.rs`, `assets/manual/*` new;
  `routes.rs`, `project.rs`, `main.rs`, `masters.rs`, `http.rs`, `document.rs`,
  `lib.rs`, `Cargo.toml`, `assets/app.{js,css}`, `index.html`; tests
  `authoring.rs` new, `routes.rs`, `concurrent_persistence.rs`,
  `editor_save.{rs,cjs}`), `impl/Cargo.lock`,
  `impl/crates/lcl-hardening/tests/installed_launcher.rs`, `remote/`
  (`src/{main,projects,session}.rs`, `tests/remote.rs`, `README.md`,
  `Cargo.lock`), `android/` (`manual/ManualSnapshot.kt`, `ui/ManualScreen.kt`,
  `ManualSnapshotTest.kt`, `ManualTabTest.kt` new; `ConnectionManager.kt`,
  `WorkspaceController.kt`, `Root.kt`, `WorkspaceScreen.kt`, `HomeScreen.kt`,
  `build.gradle.kts`, JVM and instrumented tests, `tools/e2e.sh`,
  `SUPPORTED_OPERATIONS.md`), `packaging/` (install, uninstall, launcher,
  `build_release.sh`, README), `users_manual/` (chapter 19, `MANIFEST.json`,
  `tools/manual_manifest.py`, chapter 17, README).
- `4ff1be4` "LCL core 0.3 task 3 part2": this report (draft at the pause).
- `35c1a6c`: `.vscode/settings.json` (the owner's editor setting).
- Uncommitted: `android/tools/e2e.sh` (the E2E host check fix in section 6)
  and this report's final sections.

## 5. Decisions made without the owner

- Project location is a folder inside the open workspace (the server serves
  one root; opening another root is a relaunch).
- Tree roles come from the declared `SPECIFICATION KIND` (parse, or engine
  lexer tokens for a file with empty slots).
- The readiness panel follows the open entry, or the entry last shown when it
  lists the open file as a part; it never searches for an entry.
- `PUT /api/document` without `base` is refused (428), not treated as a blind
  overwrite; existing route tests now name the revision they edit.
- Conversion offers only the entry + one task part split.
- Manual rendering: one JS viewer for both frontends (WebView on Android), so
  parity is by identical bytes plus digest.

## 6. Tests and gates

### Final gate (Rust, canonical, remote, manual, desktop)

- Runner `/mnt/F/.lcl-pretest/c03/t3/c03t3-run.sh` (the Task 01/02 gate
  relabelled, phases selftest, a, c, b, d, plus a new phase e), gate script
  `c03t3-gate.sh`, logs `/mnt/F/.lcl-pretest/logs/C03T3-G-*.log`, run log
  `C03T3-G-gate-run.log` (ends `GATE-DONE`), results
  `/mnt/F/.lcl-pretest/c03/t3/c03t3-results.tsv`.
- Ran once, from the start, on the committed tree: `HEAD 35c1a6c`, working
  tree clean. **67 commands, every exit status equal to the expected one.**
  (An earlier run on 2026-09-27 was stopped at the owner's request during
  phase b; it is not counted.)

| Step | Command (abridged) | Exit | Expected |
|---|---|---|---|
| selftest | the runner's own failure detection | phase 0 | 0 |
| fmt, clippy | `cargo fmt --check`; `cargo clippy --workspace --all-targets -D warnings` | 0, 0 | 0 |
| test-workspace | `cargo test --workspace --all-targets` — 175 binaries, **1937 passed, 0 failed, 1 ignored** | 0 | 0 |
| sha-0.1.0/0.2.0/0.3.0 | package `SHA256SUMS` | 0 | 0 |
| validate-0.1.0 | `validate_release --scope all` | 0 | 0 |
| validate-0.2.0 / release-state-0.2.0 | blocked only on release metadata | 1 / 0 | 1 / 0 |
| validate-0.3.0 / release-state-0.3.0 | 32 PASS, 0 FAIL, 1 BLOCKED (`independent_review`), 2 OOS | 1 / 0 | 1 / 0 |
| language contracts, localization, source fixtures (0.2.0, 0.3.0); `validate_projects` 0.3.0 | package validators | 0 | 0 |
| ebnf, identity (0.1.0, 0.2.0, 0.3.0); anchors; brand | grammar, identities, trust anchors | 0 | 0 |
| conformance-text | 2413 required probes: 2011 source + 402 semantics, all satisfied; `CLAIM: semantics_conforming` | 0 | 0 |
| readiness-gate | `m8_conformance_gate` | 0 | 0 |
| protected | nothing under `canonical/`, `releases/`, `assets/` differs from HEAD | 0 | 0 |
| msrv-check, msrv-tests | Rust 1.75.0 — 175 binaries, **1937 passed, 0 failed, 1 ignored** | 0 | 0 |
| realproc-seq-1…12, realproc-par (3 rounds × 6) | `lcl-capabilities --test real_process`, 30 runs | 0 each | 0 |
| remote-fmt, remote-clippy | `remote/`: fmt check; clippy `--all-targets -D warnings` | 0, 0 | 0 |
| remote-tests | `remote/` tests: 4 binaries, **64 passed, 0 failed** | 0 | 0 |
| manual-manifest | `manual_manifest.py`: current, 23 files, digest `fa189d30…4dd3` | 0 | 0 |
| manual-examples | `verify_examples.py`: 97 checked, 42 fragments, 0 failures | 0 | 0 |
| browser-smoke | headless Firefox (WebDriver BiDi), `browser-smoke.{sh,mjs}`: **27 passed, 0 failed** | 0 | 0 |

Expected non-zero statuses: the selftest's two deliberate failures; the
0.2.0 and 0.3.0 `validate_release` BLOCKED items, each asserted by its
release-state step (unchanged from Tasks 01 and 02).

What the new tests cover (all inside the rows above):

- `lcl-workspace/tests/authoring.rs` (10): A14 stale save / keep mine / reload
  / sequential chain / deleted file; every role and mode creates its own
  scaffold equal to its preview, tree kinds, bad role refused; New Project
  exact-or-nothing with digest binding and containment; readiness ready vs
  incomplete and **Run of an incomplete project reaches no invocation**;
  Masters validated, copied, never linked, default priority, delete; slot
  marks follow the text; conversion admitted and original untouched; manual
  snapshot = manifest = source files, route escapes refused, token required.
- `editor_save.cjs` (62 cases, run by `editor_save.rs` against a real server):
  five A14 cases — stale save refused with the conflict dialog, Keep mine only
  after a second confirmation, Reload disk version, sequential chain, two
  saves in flight without a self-conflict.
- `installed_launcher.rs`: a 0.3.0 payload installs, launches from the menu
  with roles available and the manual served, uninstalls only its package.
- `remote/tests/remote.rs`: roles, scaffold, create by role, readiness, and a
  phone save from a revision the desktop has replaced → 409, disk unchanged.
- Browser smoke (27): manual icon visible, keyboard button, name and tooltip;
  separate window; TOC, version/digest, read-only, code blocks, back, search;
  nothing loaded from outside; bad route refused; unsaved editor text kept;
  readiness ready/incomplete; Run disabled when incomplete; tree roles;
  cross-file go to definition; conflict dialog and Reload; role preview;
  editor slot marks; New Project; Settings → Templates.

### Android

On the committed Android sources (log `C03T3-G-android.log`, `--rerun-tasks`):

| Check | Result |
|---|---|
| JVM unit tests | **90 passed, 0 failed** (A13 ×3, roles/readiness ×2, manual parity ×4 new) |
| lint | 0 errors, 1 warning (the pre-existing OldTargetApi) |
| assembleDebug | OK, 31,893,262 bytes |
| assembleRelease | OK, unsigned, 24,718,785 bytes |

### Emulator E2E (AVD `lcl36`, API 36 x86_64 — emulator, not a phone)

`android/tools/e2e.sh` with the lcl-remote built from `35c1a6c`, evidence
`/mnt/F/.lcl-pretest/c03/t3/e2e-final/`, run log `e2e-final-run.log`:
**every phase PASS (20 phase runs), 29 PC-side checks, exit 0.** New phases:

- `ManualTabTest` (no PC): Manual tab offline, digest equals the desktop's,
  survives activity recreation, `?` icon switches to it; then the PC unzips
  the APK and recomputes the manual digest (`fa189d30…4dd3`).
- `p12_core03_roles_manual_readiness_and_identity_conflict`: New Rules file by
  role holds the PC's scaffold; unsaved text survives Workspace → Manual →
  Workspace with the connection kept; tree shows "Rules"; readiness of a
  project missing a required part is "incomplete"; a QR code with the paired
  PC id and another fingerprint is refused ("Identity conflict…") with the
  pairing record and keys unchanged. PC side: the file is the scaffold, the
  unsaved text never reached it, trusted devices unchanged, no pending
  request, and the refused code's challenge was never used.

The first attempt on `35c1a6c` (`e2e-final-attempt1/`) passed every phase it
reached (through p9) and then failed a host check: the existing "codes that
paired are spent" check expected exactly four pairing codes, and p12 issues a
fifth that the phone — correctly — never sends. The check now identifies each
code's challenge by the SHA-256 the PC stores and additionally asserts that
the refused code has no request. The new check was first run against the
failed attempt's saved state (passes), then the whole E2E was rerun (above).
No app or PC code changed between the attempts.

### Physical phone — NOT RUN

No phone was attached (`adb devices` listed only the emulator, and nothing
once it stopped). None of the physical-phone items of the task is claimed.

## 7. Canonical preservation

No canonical file changed. `protected` passed, and the recomputed identities
equal the anchors and the Task 01 baseline: 0.1.0
`00d648b162939d06c44838481a67c39bc12c64bdd6d105035c24150148fe67ed` (176
files), 0.2.0 `061a79c92c76ed7bb05968adde24b016cae3b30666b8fb289c6ab968907e8b3f`
(216), 0.3.0 `bf66e36902dbef480db75772494d384847220959d76088dbe102b8ebff04aa12`
(291).

## 8. Status

| Item | Status |
|---|---|
| Core 0.3 language status | `CORE_0_3_CANDIDATE` — unchanged by this task; `validate_release` 0.3.0 BLOCKED only on `independent_review` |
| Implementation | complete; final gate 67/67 on `35c1a6c` |
| Desktop | complete; Rust, Node and real-browser tests pass; install/launch/uninstall tested |
| Android | complete on emulator (JVM 90/0, lint, debug + release build, full E2E); **physical phone not tested** |
| Manual parity | CLOSED — one snapshot (`0.3.0`, `fa189d30…4dd3`), proved equal in the binary, the APK assets, the installed emulator app and the source |
| A13 | CLOSED — refused before any key or contact; JVM 3/3, emulator E2E p12 with PC-side proof |
| A14 | CLOSED — precondition required, 409 conflict, Reload / Keep mine; Rust, Node and browser tests |
| Release-ready | **no** (`UNRELEASED_CANDIDATE`; a separate release task decides) |
| Released | **no** |

## 9. Result

**LCL_CORE_0_3_TASK_03_BLOCKED**, on one item only: the physical-phone smoke
(Gate 9, "physical-phone smoke recorded"), which cannot be done without the
owner. Everything else the task lists is implemented and its gates pass.

To close it:

1. Connect the phone (OPPO Find X3 Neo) by USB with USB debugging on.
2. Allow updating the installed PC side for the test: the `lcl-remote` user
   service in `~/.local/bin` is an earlier build without the new operations
   and without the Core 0.3.0 package, so a phone could not reach the new
   features through it.
3. The app update must be signed with the same debug key as the installed
   app (`/mnt/F/.lcl-android/android-home/debug.keystore`), as every build here
   is; the phone's pairing then survives the update.

The smoke then records: app launches; PC connects; project tree loads; a role
file opens; Manual tab opens offline; Manual ↔ Workspace keeps the editor
text; Check; Validate; Run request/result; one revision conflict. Camera, a
separate LAN and mobile data are not claimed unless actually run.

## 10. Known limitations

- Cross-process atomicity of a save remains the A5 documented boundary: a
  second process can still replace a file in the instant between hash and
  rename (MSRV 1.75 has no file locking; other editors honour none).
- Android shows slot marks only as the PC's diagnostics; the gutter marks are
  desktop only.
- Settings → Templates edits the Master's JSON directly (validated by the
  engine on save); there is no form editor.
- An entry that does not yet get through the engine's early stages — a fresh
  canonical scaffold with empty slots is one — has no project record, so
  readiness shows its diagnostics but no per-file rows until its header slots
  are filled.
- Part G: no existing Android bug was reproduced; the owner's report of
  2026-09-25 still has no steps, and nothing was changed on speculation.

## 11. Final Git state

- `main` at `35c1a6c`, equal to `origin/main` (Task 03 work committed and
  pushed at the owner's request on 2026-09-27 as `2b52bc7`, `4ff1be4`,
  `35c1a6c`).
- Uncommitted: `android/tools/e2e.sh` (host check fix) and this report.
- Nothing was built into `releases/`; no candidate was made.

## 12. Ready to commit

Yes — the two uncommitted files above, once the owner asks. Task 03 itself
closes when the physical-phone smoke is recorded.

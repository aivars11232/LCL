# LCL Core 0.3 — Task 03: Workspace, Android, Manual Access, Conflict Safety and Final Integration

Governing pack: `/mnt/F/LCL_Core_0.3_Task_Pack/` — `tasks/TASK_03_WORKSPACE_ANDROID_INTEGRATION.md`,
`GLOBAL_RULES.md`, `ACCEPTANCE_GATES.md` (gates 0–10), and the contracts
`ARCHITECTURE_CONTRACT.md`, `PROJECT_MODEL_CONTRACT.md`, `TEMPLATE_CONTRACT.md`,
`USERS_MANUAL_ACCESS_CONTRACT.md` and `USAGE_CONTRACT.md`.

Result: **NOT YET DECIDED (paused)** (section 9).

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

Source and tests (uncommitted): see the commit that carries this report and the source commit before it.

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

**Paused 2026-09-27 at the owner's request; final gates not finished.**

Run so far (`/mnt/F/.lcl-pretest/c03/t3/c03t3-run.sh`, logs `/mnt/F/.lcl-pretest/logs/C03T3-G-*`):

- Phase a — PASSED: fmt 0, clippy `-D warnings` 0, full workspace tests
  175 binaries, **1937 passed, 0 failed, 1 ignored** (pre-existing ignore;
  Task 02 had 1926 — the +11 are the new `authoring.rs` 10 and the installed
  0.3.0 launcher test).
- Phase c — PASSED: 29 commands at their expected status (Core 0.1/0.2/0.3
  SHA256SUMS, validators, EBNF, identities, anchors, conformance report,
  readiness gate, protected paths).
- Phase b (MSRV 1.75), d (real_process 12 + 18) and e (remote fmt/clippy/tests,
  manual manifest and examples, browser smoke) — NOT RUN in the final gate
  (stopped at the owner's request during phase b).

Run separately during development (not the final gate):

- `authoring` 10/10, `routes` 24/24, `concurrent_persistence` 3/3,
  `installed_launcher` 26/26; Node UI suite 62/62; remote `core03` test 1/1;
  desktop browser smoke (headless Firefox) 27/27; Android JVM 90/0; Android
  lint 0 errors (1 pre-existing OldTargetApi warning); assembleDebug,
  assembleDebugAndroidTest, assembleRelease OK.
- Emulator E2E (`/mnt/F/.lcl-pretest/c03/t3/e2e`): all phases up to and
  including the new p12 PASSED (ManualTabTest, APK manual digest check, p1,
  p12 roles/Manual/readiness/A13); the run then stopped on a host-side check
  of mine (compared `devices --json` including `last_seen`), since fixed to
  compare trust only. Must be rerun in full, on the final tree.
- Physical phone: NOT RUN — no phone attached (adb lists only the emulator).

## 7. Canonical preservation

No file under `canonical/` changed (gate phase c `protected` passed; identities equal the Task 01 anchors).

## 8. Status

| Item | Status |
|---|---|
| Core 0.3 language status | `CORE_0_3_CANDIDATE` — unchanged by this task; `validate_release` 0.3.0 still BLOCKED only on `independent_review` |
| Implementation | PENDING — work paused by the owner on 2026-09-27 before the final gates finished; see section 6. |
| Desktop | PENDING — work paused by the owner on 2026-09-27 before the final gates finished; see section 6. |
| Android | PENDING — work paused by the owner on 2026-09-27 before the final gates finished; see section 6. |
| Manual parity | implemented and tested (Rust, JVM, emulator, APK digest); final gate pending |
| A13 | CLOSED — refused before any key or contact; JVM 3/3 and emulator E2E p12 |
| A14 | CLOSED — precondition required; 409 conflict; Reload / Keep mine; Rust, Node and browser tests |
| Release-ready | **no** (UNRELEASED_CANDIDATE; a separate release task decides) |
| Released | **no** |

## 9. Result

Not decided: the final gates (MSRV, real_process, phase e, full emulator E2E) must be rerun on this tree, and the physical-phone smoke needs the owner's phone.

## 10. Known limitations

- Cross-process atomicity of a save remains the A5 documented boundary: a
  second process can still replace a file in the instant between hash and
  rename (MSRV 1.75 has no file locking; other editors honour none).
- Android shows slot marks only as the PC's diagnostics; the gutter marks are
  desktop only.
- Settings → Templates edits the Master's JSON directly (validated by the
  engine on save); there is no form editor.
- A fresh canonical project's entry does not parse until its slots are filled,
  so its readiness shows diagnostics but no per-file rows until then (the
  engine produces no project record for an entry it cannot parse).

## 11. Final Git state

Committed and pushed at the owner's request on 2026-09-27 ("pause everything now, commit and sync").

## 12. Ready to commit

Committed as work in progress at the owner's request; Task 03 is not yet closed.

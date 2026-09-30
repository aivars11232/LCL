# LCL for Android — projects on the phone, and explicit sync to a PC

Task: "ANDROID OFFLINE LOCAL PROJECTS + EXPLICIT PC SYNC" (owner, 2026-09-30),
prompted by the phone showing nothing usable while disconnected.

Starting HEAD: `eb605e9` / report `1a2fe02` (LCL 0.5.1's source). Final
source HEAD: `9212231`.

## What the phone can do without a PC

Projects on the phone live in the app's private files directory
(`filesDir/local-projects/<name>`; never the cache Android may clear), as
folders and `.lcl` / `.lcl.txt` documents exactly like a PC project.
`local/LocalProjects.kt` makes, lists, reads, writes and deletes them: the
listing is lazy and per folder with the PC's contract (folders first, every
folder even empty, dot entries never, 4096 per folder with `truncated`);
every write goes to a temporary file beside the target, is flushed, and
replaces it in one atomic rename; a save names the digest it was loaded at
and is refused with the current text when the file moved on (the PC's rule,
`LocalConflict`); paths are resolved inside the project only.

The controller (`WorkspaceController`) routes a project whose id starts with
`local:` to that storage instead of the PC: explorer, open, save, reload,
create, new folder, delete. The phone has no LCL engine: Check, Validate,
Inspect and Run are disabled for such documents and say why; typing asks
nothing. Leaving or changing the PC clears only the PC's state
(`WorkspaceUi.withoutPc`); the phone's projects, documents and unsaved edits
stay. The dashboard has an "On this phone" section (open, new project), the
project picker separates "On this phone" from "On the PC", and Back from
Settings/About returns to the workspace while a local project is in use.

## Sync to PC, explicit

`Sync…` (only while connected, only when pressed) plans first
(`planSync`): for every local document it asks the PC `open` at the
destination (`<folder>/<id>` in the chosen PC project) and records ABSENT,
IDENTICAL (same digest) or DIFFERENT (the PC's digest and text kept for the
person). Nothing is written by planning. The person decides each conflict:
keep the PC's version, replace it (a `save` with `base` = the PC's digest,
so a change since the check is refused 409), or save beside it as
`name-phone.lcl` (a `create`). Then `runSync` makes the folders (`mkdir`,
409 = already there), creates or replaces each document with the existing
routes, and marks it synced only when the PC's answer names the bytes sent
(the digest of the text, or of the text plus the final line feed the PC
adds). A lost connection stops the remaining documents ("not attempted")
and changes nothing on the phone. Sync records (`.sync.json` per project:
local digest, PC project, path, PC digest) make "synced" mean "the bytes
now are the ones a PC confirmed"; an edit unsyncs the document again.

"Remove from this phone", and "Sync and remove from phone", delete the
phone's copy only when every document is confirmed as it is now, and only
because the person asked; a partial or failed sync leaves it whole.
Reconnecting never syncs anything by itself.

Reused, not duplicated: the `OpenDocument` conflict model, `Explorer` /
`FileTree` rows, `LclNames`, the digest helper, the New document / New
folder dialogs, and the PC's `open`/`mkdir`/`create`/`save` operations — no
protocol change, no second conflict model.

## Tests

- JVM `LocalProjectsTest` (5): offline make/list/edit, atomic save and stale
  refusal, restart over the same storage with sync records, containment,
  per-folder bound.
- JVM `LocalWorkspaceTest` (5): offline project/folder/file, edit, save,
  reload, delete, engine actions refused, restart; connecting later lists
  the PC beside the phone and syncs nothing; plan states, cancel-equivalent
  (planning writes nothing), keep-PC, rename, replace, confirmation; a PC
  reporting other bytes and a connection lost mid-sync leave the phone
  unchanged and unsynced, remove refused; whole-project sync preserves
  nested folders, and removal after confirmation.
- Android JVM total 134/0. Instrumented on the emulator:
  `LocalProjectsUiTest` (dashboard → project → folder → document → edit →
  save → engine actions off → survives recreation), FileDrawerTest,
  VersionDisplayTest: 5/5.
- Full E2E with the new phase p17 (a phone project synced into the real
  `lcl-remote`'s shared project, the bytes checked on the PC's disk, the
  phone's copy removed only after the PC confirmed): all 26 phases passed
  (`/mnt/F/.lcl-pretest/v05/e2e-run9.log`), with the host's check that the
  PC's file is exactly the phone's text plus the PC's final line feed.
  Two dialog defects were found and fixed by this phase alone: the sync
  dialog vanished with the removed project, first because it was tied to the
  current project, then because the layout switches once no document is
  open; it now lives at the screen level and shows the result until Close.

Physical-phone acceptance (the owner's list in the task) was not run in
this task: no phone was attached. It needs a release carrying this build.

## Documentation

`android/README.md` (Projects on this phone; Sync; Remove),
`android/SUPPORTED_OPERATIONS.md` (three rows), Users Manual chapter 17 (one
paragraph), MANIFEST.json regenerated.

## Known limitations

- Rename/move of local files and folders are not offered (the app has no
  rename for PC projects either); Delete of a document is.
- Sync compares whole documents by digest; there is no merge, by design.
- Planning reads every document of the local project once (they are the
  phone's own, small files); the PC is asked one `open` per document.
- No PC-side change: an older PC (0.5.0) works for sync too, since only
  existing operations are used.

## Git

Commits on `main`, pushed:

1. `c2c6867` Android: projects on the phone, and an explicit sync to the PC
2. `9212231` Docs: projects on the phone and sync to the PC
3. the report itself

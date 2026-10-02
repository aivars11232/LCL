# Ivory workflow Masters as the New Project default — result

Date: 2026-10-02. Checkout: `/mnt/F/LCL`, `origin` = `aivars11232/LCL`, `main` @
`97ae6f6397ce4874630be53438cf992467f6aaa7`, clean worktree, equal to the locally
recorded `origin/main`. The supplied pack `/mnt/F/LCL_Workflow_Masters_v1`
pins the same commit.

**Verdict: `COMPLETE` — configuration only.** The owner's LCL profile now
holds the 21 role Masters and the project Master of the pack, and New Project →
Default resolves to `ivory-workflow-project`. No LCL source, test, manual,
canonical package, trust anchor or release gate was changed. The only
repository change is this report.

## 1. What existed already

LCL's Master system already provides the requested behaviour, so no code was
written:

* `impl/crates/lcl-workspace/src/masters.rs`: `Masters::project` resolves
  `Selection::Automatic` through `defaults.json["kind.project"]`; each plan
  part that names a `master` is read with `Selection::Master`, so the
  per-type New Document defaults play no part in such a project.
* `impl/crates/lcl-protocol/src/scaffold.rs`: a project Master carries the
  plan; the entry is always generated from it (`scaffold::entry`), one `PART`
  per planned file plus `EXECUTE` with a `REFERENCE` slot.
* The installed page (`/app.js` served by the installed binary) sends
  `source=default&mode=guided` for its preselected New Project option
  "Default (Guided)", `source=canonical` for Canonical, `master=<id>` for an
  explicit Master. Settings → Templates sets or clears each default with
  `PUT /api/masters/default`.

## 2. Build, Core and store that were used

| | |
|---|---|
| Installed product | `lcl 0.9.1` (`~/.local/bin`, built 2026-10-01 11:36; `lcl-workspace` sha256 `7c12c2340482a97828cbfd4aa806b63dd76085e3e92e82c6a2576055d1fafb78`) |
| Source HEAD | also `0.9.1`; four later commits touch `impl/` (format, route split, dialog split, tests). Equality was not assumed: every check ran the installed binary. |
| Core used for Master validation | `0.3.0`, identity `7c8d46931933fa28c212a84e6405ef4b3ad99cebf07831bb835deac96a701aff`, authority `authoritative`; the installed package is identical (`diff -rq`) to `canonical/LCL_Core_0.3.0/`. |
| Effective Master store | `/home/aivars/.config/lcl/masters/`. The desktop session (systemd user manager, `plasmashell`) and `lcl-remote.service` run with `HOME=/home/aivars` and no `XDG_CONFIG_HOME`; the store did not exist before this task. |
| Pack integrity | 50 manifest entries, sizes and SHA-256 all match, nothing unlisted or missing; each role Master's `text` equals its `template_project` source byte for byte; IDs, roles, narrower types and order consistent; no path of this machine inside. |

## 3. What changed, and where

Configuration in the owner's profile, written by LCL itself (`PUT /api/master?create=1`
and `PUT /api/masters/default`, i.e. the same code path as Settings → Templates →
Save, with LCL's atomic file replacement):

* created `/home/aivars/.config/lcl/masters/ivory-*.json` — 22 files,
  byte-identical to `masters/*.json` of the pack: `ivory-description`,
  `ivory-bindings`, `ivory-authority`, `ivory-read-order`, `ivory-rules`,
  `ivory-contracts`, `ivory-usage`, `ivory-resources`, `ivory-reusability`,
  `ivory-agents`, `ivory-code-hygiene`, `ivory-security`, `ivory-git-policy`,
  `ivory-definitions`, `ivory-context`, `ivory-handoff`, `ivory-data`,
  `ivory-checks`, `ivory-stop-conditions`, `ivory-output`, `ivory-task-001`
  (saved in that order), then `ivory-workflow-project`;
* created `/home/aivars/.config/lcl/masters/defaults.json` with exactly
  `{"kind.project": "ivory-workflow-project"}`. Previous project default:
  none. No other default was set: the project names its part Masters itself,
  so the standalone New Document defaults were left alone.
* reused: 0; conflicts: 0 (no `ivory-*` existed).

Unchanged: `workspace-settings.json` (hash before = after), the registered
project `~/Documents/E-bike` (names, sizes, mtimes identical), the running
`lcl-remote` (PID 639 throughout), the pack folder (manifest re-verified,
mtimes identical), and this repository.

Task evidence, backup and tooling live outside every repository in
`/mnt/F/LCL_Workflow_Masters_v1_install_evidence/`: `tools/` (the thin client
of LCL's routes, the install-once setup, the gates), `logs/` (every command's
output), `profile_backup_before/` (`workspace-settings.json` as it was and
`STATE_BEFORE.json` recording that no store existed).

## 4. The generated entry versus the pack's illustrative `main.lcl.txt`

LCL generates the entry from the plan; the pack's own `main.lcl.txt` is
reference text. Differences, all structural-preserving:

* `SPECIFICATION ID` is `specification.main` (from the entry's file name) instead of
  `specification.ivory_workflow`; `NAME` and `VERSION` are slots instead of
  `"Reusable coding workflow"` / `"1.0.0"`; there is no `DESCRIPTION` field
  and no explanatory `COMMENT` block (the project Master is `minimal`).
* `PART` IDs are `part.<file stem>` (`part.description` … `part.task_001`)
  instead of `part.ivory_<stem>`. `SOURCE` paths, `KIND`s and the order are
  identical, all 21 real files, every name `.lcl.txt`.
* `EXECUTE REFERENCE` is a slot in both.

Fill the entry's `VERSION` with `"1.0.0"` to match the copied parts, as the
pack's README says.

## 5. Gates

All ran against the installed binary in isolated profiles (`HOME` and
`XDG_CONFIG_HOME` under a scratch folder, disposable projects), except K, which
read the owner's real profile. Log: `logs/10_staging_gates.log` (96 of 96
checks passed), `logs/22_owner_profile_check.log` (10 of 10).

| Gate | Result | Evidence |
|---|---|---|
| A native validation of all 22 | PASS | `PUT /api/master?create=1` accepted each; `GET /api/masters` lists all 22 `valid`, `problem: null`, `core 0.3.0`; the Description's intentional `COMMENT CONTENT` slot is the one slot LCL marks; no other part has a slot |
| B Default → preview → create | PASS | `source=default&mode=guided`, no Master named: preview and creation give `main.lcl.txt` + the 21 parts in the supplied order, no extra nesting, nothing else in the folder. Framework-owned metadata: only the Projects-home registration in `workspace-settings.json` |
| C texts, roles, types, order | PASS | each part equals its installed Master, the preview text and the pack source; narrower types kept (`bindings`, `contracts`, `usage`, `stop_conditions`); 21 `PART` blocks in order; preview digests equal created bytes; task part from `ivory-task-001`, not from an unrelated `kind.part.task` default |
| D project edit | PASS | rule edited, saved over its revision, reopened in a new instance; stale-revision save refused (409); Master and Project B unchanged; nothing else in A changed |
| E default edit | PASS | `ivory-rules` edited through `PUT /api/master?replace=`; Project C received it; A keeps its own edit, B the original; an edit that breaks the role is refused and the Master kept |
| F persistence | PASS | after restart the default stands; Default (Guided) and Default (Minimal) both resolve to the workflow; Canonical and another project Master work explicitly and do not undo it; the default can be cleared (stays cleared across a restart — nothing re-applies it) and chosen again |
| G refusals | PASS | missing, invalid, wrong-Core (0.2.0) and type-conflicting role Masters, and a default naming a missing project Master, each refuse preview and creation (422) with no folder made and no canonical fallback; wrong-Core, invalid and duplicate-id saves refused (422/409); existing folder refused (409, untouched); stale preview refused (409), no preview refused (428) |
| H repeated setup | PASS | a repeat writes nothing, reports the customized Master instead of overwriting it, does not re-activate a cleared default; unrelated Masters, defaults and projects A/B/C intact; a differing `ivory-*` Master blocks a first install before any write; a rejection part-way (synthetic invalid pack copy) rolls 19 saved Masters back, store byte-identical, previous default kept; the real pack unchanged |
| I pack unavailable | PASS | instance run under `bwrap --tmpfs` over the pack folder (empty inside) still creates the complete project from the installed Masters; the store holds regular files naming no sample path |
| J honest incompleteness | PASS | untouched project: readiness `rejected` at the lexical stage (only `error.indentation.empty_block`), Check of the Description rejected, Run rejected, `lcl validate`/`lcl run` exit 1; Description filled alone → still rejected; entry metadata filled → still stopped by the empty `EXECUTE REFERENCE`; a reference to an unwritten task → rejected, not admitted; CLI run non-zero |
| K owner's profile | PASS | store at the documented location, 22 valid, defaults exactly `{kind.project: ivory-workflow-project}`, `/api/roles` names it as the project default, stored files byte-identical to the pack; New Project → Default previews the complete workflow, created nothing (`~/Documents/Ivory_Default_Preview_Check` does not exist); Canonical still selectable |

Observed by the way: a part file checked alone answers `error.project.placement`
at the resolution stage, by design — parts are judged inside their project.

### Commands and exit codes

| Command | Exit |
|---|---|
| `python3 tools/staging_gates.py <scratch>` | 0 (96/96) → `logs/10_staging_gates.log` |
| `python3 tools/ivory_setup.py --pack … --window … --backup …` (owner profile) | 0, `INSTALLED` → `logs/21_owner_install.log` |
| `python3 tools/owner_check.py --pack … --window …` | 0 (10/10) → `logs/22_owner_profile_check.log` |
| repeat of `ivory_setup.py` on the owner profile | 0, `ALREADY_INSTALLED`, nothing written → `logs/24_owner_repeat_setup.log` |
| `lcl version` | 0: `lcl 0.9.1`, `protocol lcl.engine/1`, languages 0.1.0, 0.2.0, 0.3.0 |

### Not run

* A visual click-through of the New Project dialog in a browser. The dialog's
  requests were verified from the installed page's own script and replayed
  verbatim against the installed server.
* Anything on a paired phone. The phone works through the PC's engine; this
  task changed no phone-side code.

## 6. Remaining slots and limits

A new project from this default is complete as an instruction set and
intentionally incomplete as a program: `description/description.lcl.txt` has
one empty `COMMENT CONTENT` (the brief), the entry has `NAME`, `VERSION` and
`EXECUTE REFERENCE` slots, and `tasks/task_001.lcl.txt` describes the workflow
without an executable `TASK`. Check/Validate/Run refuse it until real
project-specific content exists; nothing here fakes readiness. `COMMENT`
blocks stay non-operational and a runtime's output is not assumed to carry
them.

Rollback, if ever wanted: delete the 22 `ivory-*.json` files and
`defaults.json` in `/home/aivars/.config/lcl/masters/` (or delete each
template in Settings → Templates); the previous state — no store at all — is
recorded in `profile_backup_before/STATE_BEFORE.json`.

## 7. Using it

* **New project:** Project panel → **⊞ New project**, type a name, leave
  **Start from** on *Default (Guided)*, check the 22-file preview, **Create**.
  Fill `description/description.lcl.txt`'s last `CONTENT` with the brief and
  the entry's `NAME` and `VERSION` (`"1.0.0"`). Canonical and explicit Masters
  remain in the same list.
* **Editing a project's rules:** open the file in that project and edit; it is
  the project's own copy. Other projects and the Masters never change.
* **Changing the default for future projects:** Settings → **Templates…**:
  **Edit** an `Ivory - …` template (checked on save; existing projects do not
  change), or set **Project default** to another Master or to *Canonical
  scaffold*. The choice is read from `defaults.json` on every use and is not
  re-applied by anything at startup.

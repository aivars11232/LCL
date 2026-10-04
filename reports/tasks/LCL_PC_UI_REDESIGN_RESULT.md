# TASK-001: PC UI redesign

Source: `/mnt/F/LCL_UI_Redesign_LCL_Task_Pack_v3`, version 3; all 28 task-pack
files read. Nine PC references inspected before source changes; all 18 PC and
Android inputs copied to locally excluded `UI_References/`. Source baseline:
`fa1592b955639c4839f50861f9e89c5110064144` (product 1.0.0).

The supported desktop interface now uses the references' navy and cyan visual
identity, a reusable navigation rail, a full-width project home with a quiet
planet curve, project cards, a two-column New Project preview, and settings
cards. Updates and Android pairing are reachable directly from the navigation.
The offline manual uses the same palette. System/light/dark theme choices,
existing editor settings, lazy file exploration, project containment, unsaved
work, engine commands, updater trust, and pairing approval remain in use.

## Reference mapping

| PC image | Supported implementation and boundary |
| --- | --- |
| 1 Home | Real project container, project cards, New Project, Open Folder and Manual. Mock recent activity and downloadable Core/template catalogs are omitted. |
| 2 Search | Visual identity informs shared surfaces. Global search/command backend is absent; no enabled mock search added. |
| 3 Workspace | Existing lazy explorer, editor, tabs, status, diagnostics and inspection views. No fictional terminal. |
| 4 Source Control | Visual reference only; no Git backend or mock commit controls added to the app. |
| 5 Run / Diagnostics | Real Check, Validate, Inspect, Run and existing execution/debugging views. Mock build targets and stages omitted. |
| 6 Manual | Existing packaged offline documentation, search, table of contents and history; documentation content preserved. |
| 7 Updates / Devices | Real signed updater and lcl-remote pairing/revocation, with existing explicit approval. Mock ADB terminal, battery, screenshots and deployment omitted. |
| 8 Settings | Existing theme, font size, line numbers, file ending, Projects Folder, Masters, Updates and Android devices in reusable cards. Unsupported settings omitted. |
| 9 New Project | Existing canonical Guided/Minimal and valid project Masters, exact file previews and verified creation. No screenshot-only application templates added. |

## Native components and reuse

The desktop serves HTML/CSS/JavaScript in the owner's browser, as documented by
`packaging/README.md`; KWin owns the browser window, while these changes affect
its page. KDE's native window management APIs were researched before editing
([KWin API](https://develop.kde.org/docs/plasma/kwin/api/)). No compositor
workaround or replacement desktop framework is needed for this page redesign.
Android's existing Compose theme/components and native adaptive layout support
were also identified for TASK-002
([Android adaptive layouts](https://developer.android.com/develop/ui/compose/build-adaptive-apps)).

Only the primary agent wrote source. A read-only helper inventoried signing,
release tooling and existing verification, then reviewed the UI diff. No remote,
updater, canonical, Android implementation, or published release files changed.

## Verification

- Focused production editor suite: pass (real HTTP and at least 74 controlled DOM cases).
- Focused Firefox smoke: 80 passed, 0 failed. Includes 1920x1080, 1600x900,
  1366x768, 1280x520, and saved 480px sidebar at 800x420; Settings scrolling,
  keyboard access, explorer operations, New Project preview, navigation, manual
  and unsaved-document behavior.
- JavaScript syntax, Prettier JS/CSS and `git diff --check`: pass.
- Governing PC gate: 71/71 commands returned their expected statuses in one run.
  Current Rust 1.99 and MSRV 1.75 suites each: 1,970 passed, 0 failed, 1 ignored.
  All 2,413 required conformance probes passed; canonical checksums/identities and
  protected paths passed. The existing unreleased-Core metadata refusals retain
  their expected status; this does not release a Core package.
  Process repetitions: all 30 passed. Remote: 65 passed; updater: 35 passed.
  Manual: manifest verified, 97 examples checked, 42 documented fragments skipped,
  0 failures. Final Firefox: 80 passed, 0 failed.

The existing `/mnt/F/.lcl-pretest/v05/v05-gate.sh` and Firefox BiDi harness were
reused in `/mnt/F/.lcl-ui-redesign-20261004/`. Historical evidence was preserved.
Gate paths point to current build outputs and fresh evidence; all assertions
remain, including the 18 process repetitions, now grouped into nine pairs to
bound resource use. The browser harness adds redesign-specific checks and
captures actual rendered screenshots. Its initial preview timeout was a fixture
name collision with the project created earlier in that same smoke; choosing a
fresh preview name resolved it without weakening product assertions.

Fresh logs, gate result statuses, harness copies and rendered screenshots:
`/mnt/F/.lcl-ui-redesign-20261004/evidence/`. The screenshot inputs are local
references, never application assets or staged source.

## Delivery state

PC verification is complete. Installation and Git delivery follow this verified
source checkpoint. Installed
version before this task: 0.9.1. Product version in source: 1.0.0, not yet
published. Existing update and Android signer continuity was located; retained
for the eventual combined release.

TASK-002 and Android release remain pending owner review of TASK-001. The pack
requires explicit PC approval and CONTINUE ANDROID before Android implementation.
The user's commit, sync, installation and release authorization is retained;
no renewed authorization for those already-requested operations is needed.

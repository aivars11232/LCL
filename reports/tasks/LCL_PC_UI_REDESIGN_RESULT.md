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

Implementation commit: `1cadb6bbd1fc1973dc6d1e7cc2f2d04ecd78010c`, followed by
this report-only delivery completion. The owner authorized commit and main sync;
fresh remote parity and clean-tree evidence is recorded in
`/mnt/F/.lcl-ui-redesign-20261004/evidence/git-delivery.txt` at delivery.

Installed PC version updated from 0.9.1 to 1.0.0 using the existing packaged
installer. A clean, exact source snapshot of the implementation commit produced
`/mnt/F/.lcl-ui-redesign-20261004/pc-package/`; both archive checksums passed.
All four installed binaries (CLI, workspace, updater and previously installed
remote service) match the packaged binaries byte for byte. Installer and digest
evidence is retained. Existing user sessions were preserved and show the new UI
after reopening LCL. This is a local PC installation; no GitHub release or
Android APK has been published for 1.0.0. Existing update signing material and
Android signer continuity were verified for the eventual combined release.

Cleanup removed the owned MSRV build, disposable test fixtures and extracted
installation payload: 19,108,052,992 allocated bytes (19.11 GB / 17.80 GiB).
The package builder removed its private scratch directory on success. Shared
build caches remain available for Android/release reuse; logs, harnesses, actual
rendered screenshots, package and excluded input references are retained.
No task-owned workspace fixture, headless Firefox or package-build process
remains. Two KDialog dumps from deliberate missing-file launcher fixtures
(PIDs 1190666 and 1241889) remain under `/var/lib/systemd/coredump`, totaling
1,802,240 allocated bytes. Exact command lines and cleanup attempts are recorded
in `launcher-fixture-coredumps.txt` and `cleanup.tsv`; removal needs a sudo
password unavailable to this session. These are LCL fixture entries. The gate's
expected launcher refusal assertions passed; no LCL application crash was found.

TASK-002 and Android release remain pending owner review of TASK-001. The pack
requires explicit PC approval and CONTINUE ANDROID before Android implementation.
The user's commit, sync, installation and release authorization is retained;
no renewed authorization for those already-requested operations is needed.

# TASK-002: Android UI redesign

Owner approved the PC result and explicitly instructed completion of both tasks,
Git sync, Android publication and the final PC update on 2026-10-04. All nine
Android references were inspected before Android source edits. Reference inputs
remain locally excluded and are not application assets.

Native Kotlin/Compose remains in use. Shared theme values now match the approved
PC navy/cyan palette, including readable syntax and status colors. Reusable
cards, controls and selected navigation surfaces carry that identity throughout
all supported screens. Home gains an atmospheric introduction drawn natively,
project and paired-PC cards, and wrapping action groups. Home, Workspace, Manual
and Settings are accessible from the bottom bar. Settings uses Appearance and
Editor cards; Updates groups the real installed version and check state. New
local projects show their actual private-storage location and empty-folder
starting state. Dialog text scrolls on compact screens. Existing tablet split
panes and phone file drawers remain in use.

## Reference mapping

| Android image | Supported implementation |
| --- | --- |
| 1 Home | Shared brand, introduction, real local projects and paired PCs, project creation, connection and manual actions. No mock account, global search, activity feed or Core download catalog. |
| 2 Workspace | Existing native editor, documents, syntax colors, diagnostics, inspection and PC engine actions; preserved undo and unsaved work. |
| 3 Files | Existing lazy tree, exact file opening, active-file highlight, empty folders, New File, New Folder and Refresh with shared controls and surfaces. |
| 4 New Project | Actual local empty-folder creation and existing PC-approved scaffolds; no fictional application templates or arbitrary engine versions. |
| 5 Run / Diagnostics | Existing real PC Check, Validate, Inspect, Run, diagnostics and permission approval, with shared status colors. No simulated build pipeline. |
| 6 Updates / PC Connection | Actual verified update states, product version and existing pairing/reconnection/revocation. No invented ADB or cloud operations. |
| 7 Offline Projects / Sync | Persistent local projects and explicit conflict-safe sync, PC confirmation before removal, and existing conflict choices. No automatic deletion or simulated queue. |
| 8 Settings | Existing theme, font size and line-number preferences inside reusable cards; no enabled unsupported controls. |
| 9 Manual | Existing packaged offline manual uses the PC's shared viewer and updated palette; documentation and navigation remain unchanged. |

## Native components and reuse

Android's official [adaptive layout guidance](https://developer.android.com/develop/ui/compose/layouts/adaptive/get-started-with-adaptive-apps)
and [native Compose cards](https://developer.android.com/develop/ui/compose/components/card)
were researched before editing. Existing Material controls, scrolling, native
drawing and the app's window-constrained workspace layout were sufficient;
no framework replacement or dependency was added. The primary agent was the
sole writer. A read-only helper reviewed navigation, state retention, test
interactions and release/signing continuity.

The existing offline-project test now verifies unsaved text across Manual,
Settings, Home and reopening the same project. Existing E2E interactions scroll
to Home/Settings controls made taller by cards; all assertions are preserved.

## Verification

Focused navigation JVM tests and the extended real-emulator offline-project
flow passed. Kotlin formatting and diff checks passed. The final Android build
passed all 142 JVM tests, Android lint (zero errors; the existing target-API
warning remains), debug APK, instrumented-test APK and signed release assembly.
The signed APK certificate matches the authenticated 0.9.1 release and the
previous Downloads APK.

The full E2E initially stopped at Sync Check after the emulator switched its
default network from virtual Wi-Fi to virtual mobile data. Android's system
trace and the screenshot show reconnecting at that exact check; the app safely
refused to sync. Using one virtual network fixed the fixture: the affected phase
passed with unchanged assertions and the actual synced file was verified on the
PC. The failed run and trace are retained. The fresh final E2E passed all 25 phases and 36 PC checks, including in-place
installation with pairing and keys retained. Android recorded no LCL ANR or
crash. Normal product-version checks, file-drawer tests and the extended local
project flow passed in light mode (5 tests), dark mode (1 test), and a
1600x1200, 160dpi tablet layout (1 test). Actual screenshots were captured and
inspected. The owned emulator has been stopped and disposable private runtime
fixtures removed; logs and screenshots are retained.

No physical phone is connected: PHYSICAL_PHONE_UI_TEST_PENDING. No physical
phone app was uninstalled or cleared. Tests use only the owned read-only AVD
instance, emulator-5580, and private PC runtime fixtures.

Fresh evidence is under `/mnt/F/.lcl-ui-redesign-20261004/evidence/` and the
separate E2E output directories. Final release provenance records the exact
source commit, signer and authenticated predecessor. Publication and final PC
installation are recorded after the verified combined release is produced.

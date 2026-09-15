# LCL-CLOSE-03 — Exact-source release verification — task result

## Identity and authorization

- Package: LCL-CLOSURE-4T v1.1, task **LCL-CLOSE-03**, phases A to F
- Report date: 2026-09-15, Europe/Riga (system local)
- Repository root and branch: `/mnt/F/LCL`, `main`.
  - Entry: HEAD `eb86f7785c4b79ef2801cdb7b48c23b7defebba7`, clean, level with `origin/main`.
  - Mid-task, on 2026-09-14, the owner committed the Task 3 files and the candidate as `c1c6ddf` ("LCL task 3 part 1"), level with `origin/main`.
  - This report and the dated continuation in `reports/implementation/LCL_RESIDUAL_REPAIR_REPORT.md` are the only changes after `c1c6ddf`.
- This task made no Git write: no staging, commit, push, tag or release.
- Approvals, given by the owner on 2026-09-14 in reply to the presented plan:
  - **Scope:** accept LCL-CLOSE-02's bounded scope. Full semantic conformance stays BLOCKED, so this task's verdict can only be qualified.
  - **Build directories:** script-owned directories only. `LCL_BUILD_DIR` and `LCL_KEEP_BUILD` are removed. `LCL_RELEASE_OUT` must not exist. `SOURCE_INVENTORY.tsv` replaces `SOURCE_MANIFEST.sha256`.
  - **Provenance:** from a verified snapshot, with no commit forced and uncommitted entries recorded honestly.
  - **Graphical acceptance:** headless Firefox over WebDriver BiDi, driven by a standard-library Python script in owned scratch, plus a manual desktop-menu check by the owner.
  - Earlier approvals were retained.
- Source identity: frozen source id `b4506c8aa3d4d5464a8e31b1251582bd9ad18cbefddd40c09463dddb0a167292`, 735 files.
- Toolchains actually selected:
  - current: `rustc 1.98.1 (48a229cea 2026-09-01)` and `cargo 1.98.1`, at `/usr/bin`;
  - minimum: `rustc 1.75.0 (82e1608df 2023-12-21)`, `cargo 1.75.0` and `rustdoc 1.75.0`, from `/mnt/F/.lcl-residual-repair-01-lxd8dwu8/toolchains/1.75.0/bin`, selected by `PATH` and `RUSTC`;
  - also Python 3.14.7, Node v26.8.2 and Firefox 155.0.1.
- Evidence: `/mnt/F/.lcl-closure-4t-4c1cd4c659b7/logs/`.
  - Every gate has `.log`, `.exit` and `.record.json`, holding argv, cwd, relevant environment, toolchain paths and versions, timestamps, real exit, log digest, and the worktree content identity before and after.
  - Screenshots and preserved artifacts are in `logs/T3-E-evidence/`.
- Next task: **LCL-FEATURE-04 stays locked.** See Closure.

## Summary status

Task: **BLOCKED on one owner action**: the manual desktop-menu launch check, with its procedure below. Every executable gate passed.

- **Implementation correctness:** the packaging repairs are verified by red-to-green regressions, real builds and a Git-free rebuild. Installed-candidate acceptance found no product defect.
- **Conformance established:** **`source_conforming`**, unchanged. Semantic conformance stays **BLOCKED** under the owner-accepted scope of LCL-CLOSE-02:
  - 402 semantic probes: 261 satisfied, 8 failed, 2 missing, 131 invalid;
  - 813 sub-runs remaining, including 9 failing on engine defects.

  This task added no missing or failed ID. The exact list is in `reports/tasks/LCL-CLOSE-02_RESULT.md`.
- **Canonical identity and erratum:** `00d648b162939d06c44838481a67c39bc12c64bdd6d105035c24150148fe67ed`, unchanged. `reports/LCL_Core_0.1.0_ERRATUM_2026-09-13.md` remains in force and was not changed.
- **Review scope:** the files this task changed, plus the read-only consumers named below. No other subsystem was opened.
- **Release candidate state:** `releases/candidates/lcl-0.1.0-linux-x86_64-b4506c8aa3d4/` is **BUILT_NOT_FULLY_VERIFIED** until the owner's menu check.
- **Publication:** NOT PERFORMED by this task.

## Finding dispositions

| ID | Current applicability | Baseline evidence class | Actual reproduction / control | Disposition | Gate | Remaining condition |
|---|---|---|---|---|---|---|
| B5 | applies | explicit_open_work_plus_source_traced_gaps | Seven regressions on disposable trees failed on the old script: it compiled and copied the live tree, accepted a failed `git ls-files` with a 2-file source set, had no inventory or reconstruction, and accepted unsafe archive members. The repaired script then built the real candidate from a verified snapshot; the Git-free rebuild reproduced the same source id and identical payload members. | **FIXED_VERIFIED** | T3-A1 (101, expected) → T3-A3 (0); T3-D-build-candidate, T3-D-reconstruct, T3-D-rebuild-gitfree (0) | none beyond GATE-01's owner check |
| GATE-01 | applies | not_freshly_executed | Every integrated, minimum-toolchain, conformance, canonical, brand and preservation gate ran on the frozen source. Installed-candidate CLI, launcher, HTTP, editor, browser and uninstall acceptance ran. | **OPEN**: all executable gates PASS; the real desktop-menu launch is NOT_EXECUTED | T3-C-\*, T3-E\*, T3-F-\* (0) | the owner's manual procedure below |
| BUILD-SAFETY | applies | source_traced_review_item | Old script: `LCL_BUILD_DIR` honored (the directory is removed with `rm -rf` after a build); output created with `mkdir -p` over existing directories; a failed compile still published a candidate. Repaired script: removed options refused; existing, dangling-link, parentless or in-source outputs refused; failures keep evidence and publish nothing; only its own `mktemp` directory is removed. | **FIXED_VERIFIED** | T3-A1 (101, expected) → T3-A3 (0) | none |

No finding was closed as NOT_REPRODUCED, and none was dropped.

## Changes, in actual order

| # | File | Reason | Meaningful change | Verification |
|---|---|---|---|---|
| 1 | `impl/crates/lcl-hardening/tests/release_build.rs` **(new)** | B5, BUILD-SAFETY | Seven cases run the real script on disposable trees with stub `cargo`/`git`/`rustc` and a failable `gzip` pass-through (listed below) | T3-A1-release-build-red: exit 101, EXPECTED red, 0 passed, 7 failed, each for its target defect |
| 2 | `packaging/build_release.sh` | B5 | Snapshot build (listed below) | T3-A2-release-build-snapshot: exit 101, the planned partial state, 5 passed, 2 failed (ownership, repaired in #3) |
| 3 | `packaging/build_release.sh` | BUILD-SAFETY | Owned directories and visible failure (listed below) | T3-A3-release-build-ownership: exit 0, 7 passed; T3-A-fmt-hardening, T3-A-clippy-hardening: 0 |
| 4 | `packaging/README.md` | B5 | "Building this yourself": snapshot build, inventory, output rule, kept evidence, Git-free rebuild | documentation; no test reads it; it is a payload member bound by the candidate manifests |
| 5 | `releases/candidates/lcl-0.1.0-linux-x86_64-b4506c8aa3d4/` **(generated)** | B5, GATE-01 | the new candidate, 7 files | T3-D-\* |
| 6 | this report; dated continuation in the residual report | R12 | report only, written after the freeze and not a build input | — |

The seven cases in change 1:

1. a live-tree change after capture;
2. export, reconstruction and rebuild with no Git metadata;
3. a failed, empty, incomplete or unreadable source set;
4. a changed, incomplete or unsafe export;
5. unsafe archive members: absolute, traversal, duplicate and link;
6. compile, copy and archive failure;
7. removed options and unusable outputs.

The snapshot build in change 2:

- the source listing is written to a file and its status checked on its own;
- `SOURCE_INVENTORY.tsv` records kind, executable class, size, SHA-256 and path;
- the source archive has no recursion or owner names, and its members are checked before extraction;
- the verified snapshot is the only compiler and payload input;
- `--reconstruct` makes a source export from an archive;
- the payload archive is verified by unpacking it again;
- provenance records the origin honestly.

Owned directories and visible failure in change 3:

- `LCL_BUILD_DIR` and `LCL_KEEP_BUILD` are refused;
- an output must not exist, its parent must, and inside the source tree only `releases/` is allowed;
- artifacts are published by an atomic `mkdir` at the end, and each copy is verified;
- the run's own `mktemp` directory is removed after success and kept, with its path printed, after failure.

No product code (engine, workspace, CLI, canonical package) was changed.

### Test-oracle corrections

These are corrections to my own acceptance harnesses in scratch, not to product tests. The original failures are kept, and no product assertion was weakened.

1. **T3-E1-install-cli**, exit 1, 55 PASS and 1 FAIL.
   - **Original check:** `lcl check plain.txt` must be refused.
   - **Authority:** `impl/crates/lcl-cli/tests/lcl_integration.rs::the_extension_changes_no_verdict` ("The extension decides nothing") requires exit 0 for `main.lcl`, `main.txt`, `main` and `main.LCL`. `packaging/README.md` says "The ending decides nothing about meaning". Its "Ordinary `.txt` files are not LCL documents" concerns recognition by the project tree, opening and the media type.
   - **Independent controls, both PASS:** the installed media type claims `*.lcl` and `*.lcl.txt` and not `*.txt` (E1); the installed workspace lists `notes.lcl.txt` and not `ordinary.txt` beside it (E2).
2. **T3-E4-browser**, exit 1, 16 PASS and 3 FAIL: the unsaved indicator.
   - **Original check:** the project tree's dot.
   - **Authority:** the editor's `input` handler (`app.js` 1399–1409) renders the editor and tabs, not the tree. The live indicator is `#doc-state`, which `render()` sets from `dirty(doc)` (`app.js` 146, 451).
   - **Control in T3-E4-browser-2, PASS:** the status bar reads saved, then modified after typing, then saved after saving.
3. **T3-E4-browser**: the saved bytes, 2 of the 3 failures.
   - **Original check:** the typed text without a final line feed.
   - **Evidence:** the on-disk file held the typed text plus a final LINE FEED, preserved as `logs/T3-E-evidence/browser-1/untitled.lcl.txt.after-save`.
   - **Authority:** `02_LEXICAL/01` "Every non-empty source ends with one LINE FEED"; `impl/crates/lcl-workspace/src/document.rs` ("The one thing it does add is a missing final line feed"); `app.js` `save()` applies `final_line_feed_added` and announces it.
   - **Discriminating control in T3-E4-browser-2, PASS:** a second edit that already ends with a LINE FEED, saved with Ctrl+S, reaches disk byte for byte with nothing added.

## Verification records

Every gate below has the V04 fields in its `.record.json`. No gate changed the worktree content identity while it ran. Expected reproductions are labelled and are not counted as acceptance.

| Gate | Exit | Result |
|---|---|---|
| T3-A1-release-build-red | 101 | EXPECTED red: 0 passed, 7 failed |
| T3-A2-release-build-snapshot | 101 | planned partial state: 5 passed, 2 failed |
| T3-A3-release-build-ownership | 0 | 7 passed |
| T3-A-fmt-hardening, T3-A-clippy-hardening | 0 | pass |
| T3-B-freeze-inventory | 0 | source id `b4506c8a…`, 735 files |
| T3-B-protected | 0 | 192 of 192 |
| T3-C-fmt, T3-C-clippy (`-D warnings`, all targets) | 0 | pass |
| T3-C-workspace-tests (1.98.1) | 0 | 145 suites, 1,527 passed, 0 failed, 1 ignored (the Q-JSON child helper, driven by its parent) |
| T3-C-msrv-check, T3-C-msrv-tests (1.75.0) | 0 | check pass; 145 suites, 1,527 passed, 0 failed, 1 ignored |
| T3-C-m8, T3-C-m8-json | 0 | superseded: `source_snapshot` printed `unrecorded`, because `report.rs` reads `option_env!("LCL_SOURCE_SNAPSHOT")` |
| T3-C-m8-bound, T3-C-m8-json-bound | 0 | `LCL_SOURCE_SNAPSHOT=source-inventory-sha256:b4506c8a…`. The JSON differs from T2-FINAL-m8-json in exactly that field. Obligations 980, probes 2,413; source 2,011 of 2,011; semantics 402 = 261 satisfied + 8 failed + 2 missing + 131 invalid; witnesses 66 of 66; no unrequired or excluded records |
| T3-C-canonical-validator | 0 | 31 PASS, 2 OUT_OF_SCOPE, 0 FAIL |
| T3-C-canonical-checksums | 0 | 175 OK |
| T3-C-brand-checksums | 0 | 17 OK |
| T3-C-protected, T3-C-freeze-recheck | 0 | 192 of 192; inventory unchanged |
| T3-D-build-candidate | 0 | candidate `…-b4506c8aa3d4`; its `SOURCE_INVENTORY.tsv` is byte-identical to the freeze; checksums verified |
| T3-D-protected, T3-D-freeze-recheck | 0 | 192 of 192; inventory unchanged |
| T3-D-reconstruct | 0 | script taken from the source archive; `--reconstruct` source `b4506c8a…` with no `.git` |
| T3-D-rebuild-gitfree | 0 | same source id; origin recorded as a source export without git metadata; the export was not written to |
| T3-E1-install-cli | 1 | 55 PASS, 1 FAIL (oracle correction 1) |
| T3-E2-launch-http | 0 | 35 PASS |
| T3-E3-editor-save-installed | 0 | the existing real-HTTP editor-save acceptance against the installed workspace (the test printed sha256 `de821412…`): 1 passed |
| T3-E4-browser | 1 | 16 PASS, 3 FAIL (oracle corrections 2 and 3) |
| T3-E4-browser-2 | 0 | 24 PASS |
| T3-E5-uninstall-reinstall | 0 | 11 PASS |
| T3-R-freeze-recheck, T3-R-protected | 0 | after the owner's commit `c1c6ddf`: inventory unchanged; 192 of 192 |
| T3-F-freeze-recheck, T3-F-protected | 0 | after every Phase E gate: inventory unchanged; 192 of 192 |

The workspace test gates executed the release-build regressions, the application ladder, workspace, protocol and CLI equivalence, real HTTP transport, process supervision and deep-input regressions, the decision witnesses, and every LCL-CLOSE-01 and LCL-CLOSE-02 regression. No focused target was rerun separately.

## Contract and preservation review

- **Canonical:** all 192 protected files matched at T3-B, T3-C, T3-D, T3-R and T3-F; the validator and checksums passed; the identity is unchanged.
- **Historical releases and candidates:** the protected set holds 12 `releases/` files, including candidate `bf78a0890e5f`. All untouched; the new candidate is a new directory.
- **Brand:** 17 checksums OK. The installed icons (14) and the served `/brand/lcl-icon-32.png` and `/brand/lcl-mark.png` match `BRAND_ASSETS.sha256`. The payload carries the logo master `6c930f61…` unchanged.
- **Existing behaviour retained:** all workspace suites pass on both toolchains. Launcher, `.lcl` and `.lcl.txt` handling, capability refusal and editor-save behaviour were confirmed on the installed candidate.
- **Scope:** changes were limited to `packaging/`, one new hardening test, the generated candidate and reports.
- **No unauthorized action:** no Git write, network use, global installation or system setting change.
- **The owner's real installation, desktop, session bus and browser profile were never touched:**
  - every install ran with `env -i` in a disposable HOME and XDG tree;
  - `xdg-open` and the dialog tools were recording stubs;
  - Firefox ran headless with its own profile, no session bus, no display and loopback-only networking.
- **Owned resources:**
  - no process remains; every launched process group was verified empty;
  - the disposable `accept/` (installed candidate, 132 MB) and `rebuild/` (Git-free export, 19 MB) were removed by exact verified path, after their evidence was copied to `logs/T3-E-evidence/`;
  - scratch `tmp/` is empty.

## Source and artifact binding

- **Frozen source identity:** `b4506c8aa3d4d5464a8e31b1251582bd9ad18cbefddd40c09463dddb0a167292`. It is the SHA-256 of the candidate's `SOURCE_INVENTORY.tsv`, which covers 735 files. It was identical at the freeze, after the Phase C gates, after the build, after the owner's commit, and after acceptance.
- **Origin and VCS relationship:**
  - built from a checkout at `eb86f77` with three uncommitted entries: `packaging/README.md`, `packaging/build_release.sh` and `impl/crates/lcl-hardening/tests/release_build.rs`;
  - these are listed in the provenance, with the tracked diff in `SOURCE_CHANGES.patch`;
  - the owner then committed exactly those bytes as `c1c6ddf`;
  - the candidate is **not** labelled as built from a clean commit.
- **Accepted inputs:** `git ls-files --cached --others --exclude-standard`, without `releases/` (outputs) and ignored files such as `impl/target/`.
- **Source archive:** `lcl-0.1.0-linux-x86_64-source.tar.gz`, sha256 `441e3fc369b0c0e1bb224e09b3541c5001d95838e96316e43f681d652ded6b3d`.
- **Payload archive:** `lcl-0.1.0-linux-x86_64.tar.gz`, sha256 `3a5049f4065e652acf0de5880bf7ec8002567d60ea4709e6d6c8bf7e3104299c`. It has 200 members, each recorded in `lcl-0.1.0-PROVENANCE.txt` with executable class, size and SHA-256. It was verified after extraction by the script and again by E1.
- **Installed binaries exercised:**
  - `lcl`: `a466903a629948296bcec1fa9e82fef7401ec0e5487b9fc4e2f772e0326806fa`;
  - `lcl-workspace`: `de8214123887517994bcb8f2c2818a7e70fcaaf1d09af23acea3a5feddde335c`;
  - bundled canonical identity: `00d648b1…`.
- **Git-free reconstruction and rebuild:**
  - the documented steps were taken on the candidate: the script extracted from the source archive, then `--reconstruct`, then a build in the export with `LCL_RELEASE_OUT`;
  - the rebuilt source id equals the candidate's;
  - all 200 payload members, including both binaries, are byte-identical;
  - only the archive containers differ: rebuilt payload `481951e29e4e9d5a804df5b832e052e7d0035096fea22591a1087372202f93c2`, rebuilt source `c631eb44f33763a45ea26b0de6088e4ef2b2ab26f8c149292a4838ef43b0a34b`.
- **Candidate-level evidence:**
  - **Install and CLI (E1):**
    - archive and member checks;
    - installer writes only in its home;
    - installed binaries, specification (176 files), icons, desktop entry, media type and no default application;
    - `version` and `spec` identity;
    - with no specification, exit 4;
    - all 13 VALID examples check;
    - all 21 INVALID examples with exact error, terminal status and exit code;
    - `.lcl.txt` in a path with a space.
  - **Launcher and HTTP (E2):**
    - menu, `.lcl` and `.lcl.txt` launches through the generated `Exec`;
    - a missing document fails with status 4, a dialog and `launch.log`;
    - session root and installed specification;
    - listing, brand assets, token and Host refusal;
    - the grant pair: denied with `error.operation.precondition` and no effect, granted writing exactly `written by lcl`, a grant elsewhere with no effect, and validate with no effect.
  - **Editor save (E3):** the production editor-save logic over real HTTP.
  - **Real browser (E4-2):**
    - load, identity, mark and favicon;
    - New and Create;
    - typing, status bar, Save button with the required final LINE FEED and its announcement;
    - Ctrl+S byte for byte;
    - full reload;
    - Check shows `error.type.mismatch`;
    - Run shows `status.succeeded`.

    Screenshots `browser-2/02-saved.png` and `04-run.png` were inspected. They show the rendered editor, the diagnostics panel, and the Completion view with `status.succeeded`, `VERIFY verify.value TRUE` and output `8` published.
  - **Uninstall and reinstall (E5):**
    - exactly the 19 installed files and the specification package removed;
    - 12 operator and unrelated files byte for byte, including documents, another product's icons, desktop entry, MIME package and `text/plain` default;
    - reinstall restores the payload binaries.
- **Declared limitations:**
  - bit-for-bit reproducibility of the archives is not claimed;
  - headless Firefox proves the rendered page, but not the KDE menu or a windowed browser.
- **Why BUILT_NOT_FULLY_VERIFIED:** the real desktop-menu launch has not been observed. There is no isolated graphical session on this machine (no Xvfb and no nested compositor), and it was not run in the owner's session.

## Owner's manual desktop-menu acceptance procedure

This installs into your real `~/.local`. It replaces any earlier LCL installation there, while `~/.local/share/lcl/workspace` and its documents are kept. Every step is yours to run.

1. Check the candidate:
   `cd /mnt/F/LCL/releases/candidates/lcl-0.1.0-linux-x86_64-b4506c8aa3d4 && sha256sum -c lcl-0.1.0-linux-x86_64.sha256`.
   Expected: `lcl-0.1.0-linux-x86_64.tar.gz: OK`.
2. Unpack and install:
   `mkdir ~/lcl-candidate-check && tar -xzf lcl-0.1.0-linux-x86_64.tar.gz -C ~/lcl-candidate-check && ~/lcl-candidate-check/lcl-0.1.0-linux-x86_64/install.sh`.
3. Confirm the installed binaries with `sha256sum ~/.local/bin/lcl ~/.local/bin/lcl-workspace`. Expected: `a466903a…06fa` and `de821412…335c`.
4. From the KDE application menu (Development), start **LCL Workspace** with no document. Expected:
   - the menu entry shows the LCL icon;
   - your browser opens the workspace;
   - the header shows the LCL mark and `0.1.0 · authoritative · 00d648b16293`;
   - the project path ends in `.local/share/lcl/workspace`.
5. Copy `~/.local/share/lcl/LCL_Core_0.1.0/08_EXAMPLES/VALID/01_MINIMAL_TASK.lcl` to a folder whose name contains a space, as `minimal.lcl.txt`. In the file manager, right-click it, choose **Open With**, then **LCL Workspace**. Expected:
   - the workspace opens that folder with `minimal.lcl.txt`;
   - **Run** shows `status.succeeded` with output `8`.
6. Expected: an ordinary `.txt` file still opens with its usual application, because installation changes no default.
7. If anything fails, note the last lines of `~/.local/state/lcl/launch.log`.
8. Stop the workspace: find it with `pgrep -a lcl-workspace`, then `kill <that pid>`.
9. Optionally remove it with `~/lcl-candidate-check/lcl-0.1.0-linux-x86_64/uninstall.sh` (documents are kept), then remove `~/lcl-candidate-check`.

Report back: whether the menu launch, icon, document open and Run each worked, and any `launch.log` error lines.

## Observations (not claimed as defects)

- `lcl-hardening/tests/packaging_smoke.rs` installs the historical `releases/` tarball, not a new candidate, and leaves its five `lcl-install-*` homes under `TMPDIR`. They were removed by exact path. This behaviour existed before this task.
- The production conformance report binds `source_snapshot` at compile time. A build without `LCL_SOURCE_SNAPSHOT` states `unrecorded`, which is honest but unbound.
- The project tree's unsaved dot refreshes when the tree renders, not on each keystroke. The status bar is the live indicator.
- A private `dbus-run-session` bus activated `xdg-desktop-portal`, whose failing backend kept Firefox's remote agent from starting. The acceptance harness runs Firefox with no session bus.

## Closure and handoff

Remaining before LCL-CLOSE-03 is COMPLETE:

- the owner's manual desktop-menu check above.

Owner decisions still open:

- whether to accept this candidate as the frozen LCL 0.1 baseline for LCL-FEATURE-04, under the accepted bounded conformance scope;
- the LCL-CLOSE-02 decisions:
  - a conformance-completion task for the 813 sub-runs and 9 engine defects;
  - the `match/glob-absolute-path-false` oracle question;
  - the `impl/target/debug` deviation files.

Next exact action: the owner runs the procedure and reports the result.

- **On PASS:** add a dated addendum here and mark LCL-CLOSE-03 COMPLETE within the accepted bounded scope. LCL-FEATURE-04 then begins with its own read-only Phase A plan and approval.
- **On FAIL:** reproduce against this candidate before any change. Any product change invalidates the freeze, the candidate and every Phase C to E gate.

Latest reusable successful gates: T3-C-\*, T3-D-\* and T3-E\*, all at source id `b4506c8a…`.

Plain-text handoff: `/mnt/F/.lcl-closure-4t-4c1cd4c659b7/LCL_CLOSE_03_HANDOFF.txt`.

No probability estimate and no universal bug-free guarantee is offered. "No known blocker within the verified scope" is not proof of the absence of every possible defect.

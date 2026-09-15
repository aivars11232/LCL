# LCL-REPAIR-01 — Result

## Identity

- Task: LCL-REPAIR-01 — Packaging and Desktop Integration (LCL Six-Task Repair Pack 1.0; pack manifest verified)
- Repository: /mnt/F/LCL
- Branch: main
- Entry HEAD: 7ed84ba0636342faafd348542e6907a3173ca31e (`LCL task 4 final`), equal to the pack baseline
- Exit HEAD: 7ed84ba0636342faafd348542e6907a3173ca31e
- Entry worktree: clean
- Exit worktree: the 6 modified tracked files listed below, plus this report. No other tracked or untracked change.
- Git writes performed: **NO**
- Background/sub/parallel agents used: **NO**

## Result

**Status:** PASS WITH OUT-OF-SCOPE FINDINGS

## Scoped findings

| Finding | Reproduction | Repair | Verification | Disposition |
|---|---|---|---|---|
| B-01: launcher assignments are not shell-safe | Script level: the real install.sh ran against a stub payload with HOME=`h it's "q" $x \`y\` b\tz & 50% ;`, and the launcher got `bin=` as raw shell text. Test RED: `an_installation_under_a_home_whose_path_holds_spaces_launches` failed ("launcher never opened a URL") and `install_paths_reach_the_workspace_exactly` failed | install.sh now substitutes each launcher placeholder as one single-quoted shell word (`shell_quote`). The template keeps `bin=@BIN@`; only its comment changed | Both tests GREEN. The second compares the exact NUL-separated argv the installed workspace receives, for a menu launch and a file launch, with a 0.2.0 payload so `@LOCALIZED_SPEC@` is covered | FIXED_VERIFIED |
| B-02: installer substitution is not exact | `awk -v` turned the path text `\t` into a TAB (4 launcher lines), and dropped the backslashes the desktop quoting had added (Exec showed `"q"` unescaped). The existing single-level Exec escaping (`\"`) is also rejected by GLib itself | The value now reaches awk through `ENVIRON`, not `-v`. `desktop_quote` doubles every backslash for the Desktop Entry string layer and writes `%` as `%%` | `install_paths_reach_the_workspace_exactly` GREEN: Exec parses to exactly the launcher path, and argv is exact. The test's Exec parser now follows the spec: string escapes first (anything invalid panics, as GLib refuses it), then quoting, then `%%`. Manual GLib cross-check: `gio launch` on the installed entry (hostile path without `%`) delivered the exact argv | FIXED_VERIFIED |
| B-03: uninstaller traverses shared hicolor | Stub-payload uninstall removed the unrelated empty `scalable/apps`, `48x48/places` and `22x22/apps`. Test RED: `uninstall_leaves_every_theme_directory_it_did_not_fill` ("uninstall removed scalable/apps") | `find "$icons" -depth -type d -exec rmdir` was replaced. Only the context and size directories that held a removed LCL icon, then `hicolor` and `icons`, are offered to `rmdir`; never `-r` | P3 GREEN. The existing `uninstall_takes_only_this_products_icons`, `an_empty_theme_directory_is_removed_but_the_shared_root_is_never_recursive` and `uninstall_removes_the_launcher_and_reinstall_restores_it` are GREEN | FIXED_VERIFIED (boundary: see OOS-4) |
| B-04: MIME magic assumes `LCL:` | GLib resolver (`gio info`, scratch MIME database): extensionless `@locale lv-LV` source resolved to text/plain. Test RED: `a_localized_document_is_recognised_by_its_first_bytes` ("compiled magic does not match `@locale `") | lcl.xml gains a second, alternative `<match type="string" value="@locale " offset="0"/>`, grounded in Core 0.2.0 `02_LEXICAL/13` ("@locale, one SPACE, one locale tag and LINE FEED at byte offset zero"). All directive-less 0.2 fixtures, auto-detected ones included, still begin with `LCL:`. Comment and impl/integration/README.md corrected; no language syntax changed | P4 GREEN. The compiled magic holds both values, and via gio: canonical_en, explicit_lv and auto_lv give text/x-lcl, while `notes.txt` (directive content) and prose give text/plain. Existing MIME tests are GREEN, and `lcl_integration::the_magic_rule_matches_every_canonical_example` is GREEN | FIXED_VERIFIED |

## Files changed

| File | Why | Minimal change |
|---|---|---|
| packaging/install.sh | B-01, B-02 | `substitute` reads the value from ENVIRON; new `shell_quote`; `desktop_quote` gets the string-level escaping and `%%`; placeholders substituted with quoted words |
| packaging/lcl-workspace-launch.in | B-01 | Comment only: the placeholders now receive single-quoted words |
| packaging/uninstall.sh | B-03 | Directory removal limited to the parents of removed LCL icons, plus the theme and icons roots |
| impl/integration/linux/lcl.xml | B-04 | One alternative magic match; the comment is corrected |
| impl/integration/README.md | B-04 | The magic-rule description is corrected |
| impl/crates/lcl-hardening/tests/installed_launcher.rs | Regression tests | P1–P4 tests; `run_installer` split out of `install`; `exec_argv` follows the Desktop Entry spec. `stub_opener` now quotes its redirect: a necessary, bounded harness fix, because under a home with spaces it wrote the URL into a file named `…-home` (preserved as `R1-P1-first-green-attempt-stray-stub-output.txt`) |

## Evidence

Toolchain: cargo 1.98.1 (Arch rust 1:1.98.1-1).
Environment: `CARGO_TARGET_DIR=/mnt/F/.lcl-closure-4t-4c1cd4c659b7/target-current` (the reused Task 4 cache) and `TMPDIR=/mnt/F/.lcl-repair-6t/tmp`.
Logs: `/mnt/F/.lcl-repair-6t/logs/`.

| Gate | Command/Test | Exit | Result |
|---|---|---:|---|
| R1-warm-build | `cargo build --workspace --bins && cargo test -p lcl-hardening --test installed_launcher --no-run` | 0 | built |
| R1-P-RED | 4 new tests + `the_installed_desktop_entry_runs_a_launcher_that_exists` | 101 | EXPECTED-FAIL: 4 failed for the reproduced reasons; control 1 passed |
| R1-P12-GREEN | P1, P2 after install.sh | 101 | P2 ok; P1 failed on the harness `stub_opener` quoting (diagnosed above) |
| R1-P1-GREEN | P1 after the harness fix | 0 | 1 passed |
| R1-P3-GREEN | P3 + 3 existing uninstall/icon tests | 0 | 4 passed |
| R1-P4-GREEN | P4 + 3 existing MIME tests | 0 | 4 passed |
| sh -n | install.sh, uninstall.sh, lcl-workspace-launch.in | 0 | pass |
| XML | python3 minidom parse of lcl.xml | 0 | well-formed; matches `LCL:`, `@locale ` |
| R1-INT-installed_launcher | `cargo test -p lcl-hardening --test installed_launcher` | 0 | 20 passed, 0 failed |
| R1-INT-lcl_integration | `cargo test -p lcl-cli --test lcl_integration` | 0 | 5 passed, 0 failed |
| R1-INT-fmt | `cargo fmt --all --check` | 0 | pass |
| R1-INT-clippy | `cargo clippy -p lcl-hardening --all-targets -- -D warnings` | 0 | pass |
| GLib cross-check (manual, scratch) | real install.sh on a stub payload, then `gio launch` on the installed desktop entry | 0 | exact argv for a path with `'` `"` `$` backtick `\t` `&` `;` `*` and spaces; with `%`, GLib cannot load the entry (OOS-1) |

Not run, by design:
- `release_build.rs` stubs every packaging file, so its outcome does not depend on their content.
- `packaging_smoke.rs` installs the existing committed candidate tarball, not these scripts.
- Broad workspace gates belong to REPAIR-06.

## Protected-material check

- Core 0.1.0: `git diff HEAD` empty with no untracked files; `SHA256SUMS.txt` verifies.
- Core 0.2.0: `git diff HEAD` empty with no untracked files; `SHA256SUMS.txt` verifies. Release state is unchanged (UNRELEASED_CANDIDATE).
- Existing candidates: `releases/` unchanged against HEAD; nothing rebuilt.
- Historical evidence: `reports/` unchanged apart from this new file; `assets/` unchanged.

## Out-of-scope findings

- **OOS-1:** GLib cannot launch a desktop entry whose program path contains `%`.
  - The spec-correct `%%` fails GLib's load-time program lookup, and a single `%` is mangled into another path.
  - install.sh writes the spec-correct form.
  - Whether to refuse installation under such paths is an owner decision.
  - Affects packaging/install.sh.
- **OOS-2:** Messages print paths with `echo` in install.sh, uninstall.sh and the launcher's `report`.
  - Under a shell whose `echo` interprets backslashes (for example dash), printed messages could differ from the path. Generated files are not affected.
  - Here `/bin/sh` is bash.
- **OOS-3:** An installation path containing a LINE FEED is neither supported nor refused.
  - `desktop_quote` does not encode it, so the desktop entry would break silently.
  - Not tested.
- **OOS-4:** Uninstall has no record of which directories the installer created.
  - An empty context directory that existed before installation at a path where LCL also placed an icon (for example an empty `48x48/apps`) is still removed once LCL's icon leaves it.
  - Every other theme directory is preserved.
- **OOS-5:** The existing candidates still carry the pre-repair scripts.
  - Affected: `releases/candidates/lcl-0.1.0-linux-x86_64-b4506c8aa3d4` and `lcl-0.2.0-linux-x86_64-a0006c38fb79`.
  - They are protected and were not rebuilt. A fresh candidate belongs to a later release step.

## Remaining blockers

- None.

## Owner actions

- O-02: a manual desktop-menu launch, which should now be done with a build that includes these scripts.
- O-03: review and commit decisions for the 6 modified files and this report.
- Decide OOS-1, OOS-3 and OOS-4 if they matter.

## Quota/efficiency notes

- Reused:
  - the existing `installed_launcher.rs` harness (Home, stage_payload, launch, installed_icons);
  - the Task 4 compile cache;
  - canonical 0.2.0 localization fixtures as MIME inputs.
- No new helper layers. The integration gate ran once. Broad workspace gates are deferred to REPAIR-06.

## Handoff

The task passed its acceptance contract. LCL-REPAIR-02 is unlocked.

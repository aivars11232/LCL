# LCL for Android — the desktop workspace's look on the phone

Owner's request (2026-10-01, after updating the phone to 0.9): "How come
that UI didn't change a bit for Android after 0.9 updates? … I needed UI
changes for Android same as for PC."

Starting HEAD: `390aa66` (= tag `v0.9.0`). Final HEAD: see **Git**.

## Why 0.9 looked the same on the phone

LCL 0.9 rewrote the PC's stylesheet, but on Android it changed only a few
colour values by a shade, turned the dashboard's cards into labelled
sections and added one status line. The components the app is built from
were not restyled: every button was still Material's full pill (Material 3
buttons do not read the theme's small corner shapes), and the bottom bar, the
document chips and every selected state used Material's default lavender,
because the theme never set those colour roles. The 0.9 release notes said "a
new look on the PC and the phone"; for the phone that was not true.

## What changed

- **`ui/Theme.kt`** — the PC stylesheet's palette, token for token, for both
  themes (`LclColors`: page, ink, raised, sunken, hover, the two hairlines,
  dim and faint ink, the accent and its soft tint, the states, the syntax
  colours), and a Material colour scheme built from it that sets *every* role
  (containers, secondary and tertiary, inverse, outline, error containers, no
  elevation tint), so nothing falls back to Material's defaults. Shapes 6–12
  dp. A tighter type scale (15/14/12.5 sp body, no wide tracking).
- **`ui/LclComponents.kt`** (new) — the components every screen uses in place
  of Material's of the same names: `Button` (accent, squared), `OutlinedButton`
  (raised, hairline border — the PC's ordinary button), `TextButton`,
  `FilterChip`, `Card`, `Switch`; and the PC's own pieces: `LclTab` (a tab
  marked by a bar of the accent), `IconAction` (the small marks ⌂ ↻ + ▤, with
  what they do said to accessibility services), `SectionLabel`, `listRow()`,
  `hairline()`. The screens' Material imports of those names were removed, so
  all 73 buttons, the chips and the cards changed with no call-site edits.
- **`ui/Root.kt`** — the connection banner is the PC's status strip (sunken,
  hairline, coloured dot and word, dim detail); the bottom bar is a tab strip
  (Workspace / Manual, accent bar on the one shown) instead of Material's
  navigation bar with its pill; messages show as the PC's toast (raised,
  bordered), above the inspector's tabs.
- **`ui/WorkspaceScreen.kt`** — document tabs and the inspector's Diagnostics
  / Structure / Run are underlined tab strips on a sunken band; the status
  line ("Saved · 6:51") is a sunken monospace strip, amber while unsaved; the
  Files pane's head follows the PC's sidebar: ⌂, the project picker as a
  bordered control, the path in dim monospace, a "Files" label with ↻ + ▤,
  and for a project on the phone its sync status with Sync… and Remove beside
  it; the open file's row has the accent's soft tint and a bar at its edge;
  file roles are pill badges (the project entry in the accent).
- **`ui/HomeScreen.kt`, `ui/SettingsAbout.kt`** — bordered raised rows for
  the phone's projects, the PC's section labels.

Nothing but presentation changed: no controller, protocol, storage or sync
code. Every test tag is kept; four header buttons changed from words to the
PC's marks (Home, Refresh, New, Folder → ⌂ ↻ + ▤) and are announced by their
names to accessibility services.

## Tests

- Android JVM: 141 passed, 0 failed.
- Full E2E on the emulator against a real `lcl-remote`, twice on the final
  UI: in dark mode (`e2e-run13.log`) and, on the final tree, in light mode
  (`e2e-run14.log`): all phases passed both times, no ANR or crash of LCL.
- Instrumented: `LocalProjectsUiTest` in light and dark (now also taking
  screenshots of the phone-only screens), `FileDrawerTest` 3/3,
  `VersionDisplayTest` 1/1 on the normal build.
- One test repair: E2E `p12` pressed New before the PC's project was listed
  (the button is disabled until then, so the press did nothing — a race the
  test always had); it now waits for the project's files and asserts the
  button is enabled.
- Screenshots (evidence, outside the repository):
  `/mnt/F/.lcl-pretest/v05/ui-shots/` — `e2e-dark/` and `e2e-light/` (53 each,
  every connected screen), `dark/` and `light/` (the no-PC screens).

Not run: the Rust gate (no Rust, page or packaging file changed; the manual's
one changed line is covered by the regenerated `MANIFEST.json`), and a
physical phone (none attached).

## Documentation

`android/README.md` (a **Look** paragraph; the Files head's marks), Users
Manual chapter 19 (the phone's New button is **+**), `MANIFEST.json`
regenerated.

## Known limitations

- The toast floats over the bottom of the editor or the inspector's content
  for the seconds it shows.
- Seen on the emulator only (1080×2400, light and dark); the wide two-pane
  layout was not photographed.
- It reaches a phone only with a release: the published v0.9.0 does not
  contain it. The product version is still 0.9.0 in this source.

## Git

1. Android: the desktop workspace's look on the phone
2. Docs: the phone's look and its Files head
3. this report

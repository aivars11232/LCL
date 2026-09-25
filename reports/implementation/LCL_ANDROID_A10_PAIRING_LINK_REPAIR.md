# LCL for Android — A10 pairing-link repair

Date: 2026-09-25. Verdict: **ANDROID_A10_PAIRING_LINK_REPAIR_COMPLETE** for
finding A10 below. A1–A9 were not reopened. This is not a release-readiness
claim.

Nothing in this report was committed or pushed.

## 1. Starting state

- Repository `/mnt/F/LCL`, branch `main`.
- HEAD `3ea51528ceddc51e545102bc45bee1df95106274` ("Report: Android remote
  post-audit repair (A1-A9)"), equal to `origin/main` (checked with
  `git ls-remote origin refs/heads/main`). History under audit: MVP
  `028dac665fd53ed06fd58be38d4cf5d5df894ea8`, A1–A9 repair
  `66921afc50e8d16804e1f73ba21b76d2410742a3`, report `3ea5152`.
- Working tree clean at start (`git status --porcelain -uall` empty), no stash.
- Toolchains: rustc/cargo 1.98.1; JDK 21, Android SDK (build-tools 37.0.0)
  under `/mnt/F/.lcl-android`; emulator AVD `lcl36` (Android 16, API 36,
  x86_64), headless.
- Work files, all outside the repository: `/mnt/F/.lcl-pretest/a10/` (logs,
  reproduction evidence, one-test drivers, E2E work directories);
  `lcl-remote` build directory `/mnt/F/.lcl-pretest/remote-target`.

## 2. A10 reproduction (on the unfixed source)

Every step used one well-formed link for a PC that does not exist, so no real
pairing code was exposed:
`lclpair://pair?v=1&pc=0123456789abcdef0123456789abcdef&n=Some%20PC&fp=<64×a>&a=10.0.2.2:9&c=<base64url of 32 bytes 0x07>&e=4102444800`.

1. **Source.** `android/app/src/main/AndroidManifest.xml` at HEAD, lines
   35–41: on `.MainActivity` (`android:exported="true"`), an intent filter
   `VIEW` + `DEFAULT` + `BROWSABLE` with `<data android:scheme="lclpair"
   android:host="pair" />`.
2. **Packaged manifest.** `aapt2 dump xmltree` of the debug APK built from
   HEAD (app sha256 `5dee5e52…613d`): `MainActivity` exported, with that
   filter.
3. **System resolution.** `cmd package query-activities --brief -a
   android.intent.action.VIEW -c android.intent.category.BROWSABLE -d '<link>'`
   → `1 activities found: io.lcl.workspace/.MainActivity`.
4. **Dispatch from another process.** From `adb shell` (uid 2000, standing
   in for a camera or scanner app): `am start -W -a android.intent.action.VIEW
   -c android.intent.category.BROWSABLE -d '<link>'` → `Status: ok`, cold start
   of `io.lcl.workspace/.MainActivity`. A UI dump shows the Pair screen's text
   field holding the whole link, **`c=BwcH…` (the one-time code) included**,
   and the preview "Pair with Some PC?" with the fingerprint.
5. **Explicit route.** `am start -n io.lcl.workspace/.MainActivity -a
   android.intent.action.VIEW -d '<link>'` → the same filled form. `Root.kt`
   put any `lclpair:` link into the Pair screen however the intent arrived;
   the manifest filter only decided whom the system offered links to.
6. **The code.** `c` is 32 random bytes (`remote/src/pairing.rs:146`), stored
   by the PC only as SHA-256 (`:149`), valid 300 s by default (`:38`),
   consumed once under a file lock (`:167–182`); the app requires exactly 32
   bytes (`PairingLink.kt:62–66`).
7. **New tests, red on the unfixed source** (§7): `ManifestTest` 1 of 2 red,
   `IncomingIntentsTest` 2 of 4 red; the green ones are guards that must hold
   before and after.

## 3. Attack and trust boundary

- **What the PC trusts.** A device that completes TLS 1.3 against the PC's
  pinned certificate, proves it holds the key of the certificate it presents,
  and sends `hello` intent `pair` with an unused, unexpired code becomes a
  trusted device until revoked. The PC cannot tell which app on the phone
  presented the code: until used, the code is a bearer credential. Whoever
  presents it first, with any new key, is paired.
- **The link's route before the repair.** (a) LCL's own scanner: ZXing's
  `CaptureActivity` runs inside the app (not exported, no intent filters) and
  hands the text back to LCL as an activity result, so the code never leaves
  the app. (b) Any other QR reader: a camera or scanner app that finds a URI
  dispatches `ACTION_VIEW` + `BROWSABLE`. Android does not verify or reserve
  custom schemes. Any installed app can declare a filter for `lclpair://pair`
  and be offered the link, alone or in a chooser. By declaring the filter and
  documenting "scan with the phone's camera app, which opens LCL", LCL made (b)
  the advertised path, which put the credential where another app can read it
  and pair first, without LCL's confirmation screen.
- **Inbound too.** Through the filter, or by naming the exported
  `MainActivity`, any app, web page or message could put a pairing link
  (e.g. to an attacker's PC) into LCL's Pair form. Trust still needed the
  person's Pair press, but the surface was the same.
- **After the repair.** LCL declares no filter for `lclpair` and none that is
  `BROWSABLE`, and ignores a pairing link sent to it by name. Only the app's
  own scanner or the person's paste fills the form, and trust still needs
  Pair. The only QR flow is: LCL → Pair a PC → Scan QR code → the scanner
  (inside LCL) returns the contents → preview with PC name and fingerprint →
  Pair → TLS → the one-time code is consumed → trusted.
- **Beyond the app's reach:** a person who scans the QR with another app
  anyway, while some other app claims `lclpair://` (§13).

## 4. Root cause

Both parts of the defect treated the pairing credential as an inter-app link:

1. `AndroidManifest.xml` declared an exported, `BROWSABLE` `VIEW` filter for
   `lclpair://pair` on `MainActivity`. The link carries the one-time
   enrollment code, and a custom scheme is not owned, so this invited camera
   apps to send the credential through the system resolver, where any app
   with the same filter can receive it.
2. `ui/Root.kt` (`data.scheme == PairingLink.SCHEME -> screen =
   Screen.Pair(data.toString())`, then `PairScreen(initialLink = …)`) copied
   any incoming `lclpair:` link into the Pair form. This was reachable even
   without the filter, because `MainActivity` must stay exported for the
   launcher.

## 5. Files changed

Production (Android):

- `android/app/src/main/AndroidManifest.xml`: pairing filter removed; a
  comment says why.
- `android/app/src/main/java/io/lcl/workspace/ui/Root.kt`: `lclpair` branch
  removed; `Screen.Pair` carries no link; unused import removed.
- `android/app/src/main/java/io/lcl/workspace/ui/PairScreen.kt`:
  `initialLink` parameter removed; comment.
- `android/app/src/main/java/io/lcl/workspace/ui/HomeScreen.kt`: the
  unpaired hint names **Pair a PC → Scan QR code**, in this app.
- `android/app/src/main/java/io/lcl/workspace/MainActivity.kt`: comment only.

Tests and test tooling:

- `android/app/src/test/java/io/lcl/workspace/ManifestTest.kt`: new.
- `android/app/src/androidTest/java/io/lcl/workspace/IncomingIntentsTest.kt`:
  new.
- `android/app/src/androidTest/java/io/lcl/workspace/RemoteEndToEndTest.kt`:
  p1 pairs by paste; comment in p8.
- `android/tools/e2e.sh`: four new phases; comment.

Documentation:

- `android/README.md`, `android/SUPPORTED_OPERATIONS.md`.
- This report (new).

Reviewed and **not** changed: `remote/README.md` (it describes the link format
and the protocol; nothing in it says or implies camera-app pairing), `README.md`,
`users_manual/02_Installing_and_Running.md`, `users_manual/17_Tools_Reference.md`
(they say only that the PC shows a QR code). The PC's own instructions already
name the in-app scanner: `remote/src/main.rs:209` ("Scan this with LCL on your
Android device: Pair a PC → Scan QR code.") and
`impl/crates/lcl-workspace/assets/app.js:1152`. Nothing under `remote/`,
`impl/`, `canonical/`, `releases/` or `assets/brand/` changed.

## 6. Exact repair

- **Manifest.** The seven-line `lclpair` filter is gone. The launcher filter
  and the `.lcl` / `.lcl.txt` document filter are untouched, byte for byte.
- **Root.kt.** The incoming-intent handler keeps only the document branch,
  with its two lines unchanged: `ACTION_VIEW` with a `.lcl` / `.lcl.txt` name
  or a `content:` URI opens the local document screen. An `lclpair:` link
  matches neither and is dropped. `Screen.Pair` is now a `data object`, so no
  route into the Pair screen can carry a link.
- **PairScreen.kt.** The form starts empty. The scanner callback, the paste
  field, the preview and `pair()` (called only by the Pair button) are
  unchanged.
- **Kept on purpose.** `PairingLink.kt` is unchanged, so the `lclpair://`
  serialization stays the QR and paste format, and a pasted link parses
  exactly as before (`PairingLinkTest` unchanged and green). The pairing
  protocol, TLS, pinning, the code's entropy, single use, expiry and hashing,
  device keys, run ownership, effect approval, sharing and CAS are all
  untouched.
- **Tried and reverted.** A longer instruction sentence on the Pair screen
  moved the Pair button below the visible area on the test screen (§8). It was
  removed, and the Pair screen's text is exactly as before.

## 7. Tests added or changed

- **`ManifestTest`** (JVM, new, 2 tests) reads `src/main/AndroidManifest.xml`.
  - `no_other_app_can_hand_the_app_a_pairing_link`: no filter anywhere
    declares scheme `lclpair`, and none has category `BROWSABLE`. Red before
    ("a filter takes pairing links: [lclpair]"), green after.
  - `the_launcher_and_lcl_documents_still_open_the_app`: `MainActivity` keeps
    `MAIN`/`LAUNCHER`, plus exactly one document filter (`VIEW`, `DEFAULT`,
    schemes `file` and `content`, exactly the four `.lcl` / `.lcl.txt` path
    patterns). A guard: green before and after.
- **`IncomingIntentsTest`** (instrumented, new, 4 tests; runs against the
  installed app, no PC needed).
  - `no_activity_of_the_app_takes_a_pairing_link`: PackageManager resolution,
    restricted to the app, with and without `BROWSABLE`, finds nothing; an
    implicit `BROWSABLE` start throws `ActivityNotFoundException`. Red before:
    `expected:<[]> but was:<[io.lcl.workspace.MainActivity]>`.
  - `a_pairing_link_sent_to_the_app_by_name_fills_nothing_in`: an explicit
    intent to `MainActivity` carries the link. For 3 s, no `pair_link` or
    `pair_preview` appears, no Keystore pairing key or PC record is created.
    A `.lcl` document then sent the same way opens with its exact text, which
    proves this route delivers intents. Red before: "a link from another app
    filled the Pair screen in".
  - `lcl_documents_from_other_apps_still_open_the_app_and_plain_txt_does_not`:
    these resolve to exactly `MainActivity`:
    `content://com.android.externalstorage.documents/document/primary%3ADownload%2F{todo.lcl,notes.lcl.txt}`
    and `file:///storage/emulated/0/Download/{…}`, typed
    `application/octet-stream` / `text/plain`. `notes.txt` resolves to
    nothing. A guard.
  - `an_lcl_or_lcl_txt_file_linked_from_another_app_opens_exactly`: creates a
    `.lcl` and a `.lcl.txt` file in Download through MediaStore, then sends an
    implicit `VIEW` (`file://`, restricted to the app's package) that the
    system resolves through the document filter. The document screen shows
    exactly the file's UTF-8 text, non-ASCII included (the A6 strict decoding
    path), and the files are deleted afterwards. Requires API 30 or later
    (assumption). A guard.
- **`RemoteEndToEndTest.p1_pair_and_work`** (changed). It paired through the
  external intent that A10 removes; that intent no longer resolves (§10), so
  the old step would throw `ActivityNotFoundException`. It now pastes the link
  on the Pair screen, the path that remains. It keeps every earlier assertion
  (preview, fingerprint, no key before Pair, Pair → Connected, then all the
  work) and adds two: no PC recorded and no connection before Pair. The
  removed path is now asserted the other way round, in `IncomingIntentsTest`.
  Unused `Uri` import removed. The p8 comment no longer says "or outside
  link"; its code is unchanged.
- **`tools/e2e.sh`** runs the four `IncomingIntentsTest` tests as phases on
  the freshly cleared app, before pairing.

The four required checks:

| Required | Covered by |
|---|---|
| Test 1: manifest / external surface | `ManifestTest`; `IncomingIntentsTest` #1, #2; packaged-manifest inspection (§10) |
| Test 2: built-in scanner | p8, unchanged (§9) |
| Test 3: paste | p1 (preview, no trust before Pair, Pair succeeds); p7 (a used pasted code is refused, a new one pairs); p0 |
| Test 4: `.lcl` / `.lcl.txt` opening | `ManifestTest` #2; `IncomingIntentsTest` #2 (document half), #3, #4; `LocalDocumentScreenTest` × 2 (A6), unchanged |

## 8. Test commands and results

On the final tree unless marked as a red run.

| Command | Exit | Result |
|---|---|---|
| `android`: `./gradlew --offline testDebugUnitTest` | 0 | 61 passed, 0 failed (59 before + 2 new) |
| `android`: `./gradlew --offline lint` | 0 | 1 warning, pre-existing (`OldTargetApi`, as in the A1–A9 report); none in changed files |
| `android`: `./gradlew --offline assembleDebug` | 0 | built |
| `android`: `./gradlew --offline assembleRelease` | 0 | built, unsigned (no signing identity in the repository, by design); sha256 `8ef5858c1fd3d2ce716589ff55c76c949de01953f123d2fb8e53eeb6e826b4df` |
| `remote`: `cargo fmt --check` | 0 | clean |
| `remote`: `cargo clippy --locked --all-targets -- -D warnings` | 0 | no warnings |
| `remote`: `cargo test --locked` | 0 | 43 passed, 0 failed (17 unit + 26 end-to-end) |
| `android/tools/e2e.sh` (emulator `lcl36`, real `lcl-remote serve`; `LCL_REMOTE=/mnt/F/.lcl-pretest/remote-target/debug/lcl-remote`) | 0 | **15/15** instrumented phases `OK (1 test)`; every PC-side check passed; no ANR or crash recorded for LCL; 4 min 3 s |

The 15 E2E phases: `LocalDocumentScreenTest` × 2, `IncomingIntentsTest` × 4
(new), p1 pair by paste and work, p2 restart, p2 after reboot, p3 network
loss, p4 PC service restart, p5 PC edits and conflict, p6 revoked, p7 re-pair
and forget, p8 scanner.

`remote` stayed green with no source change there. That includes the A1–A9
tests `a_device_cannot_switch_off_the_pause_before_an_effect` (A1),
`only_the_device_that_started_a_run_can_follow_or_answer_it` (A2),
`a_project_stops_being_shared_at_once_without_a_restart` (A4), and
`a_first_message_larger_than_a_hello_is_refused_at_once`,
`malformed_or_unsupported_hellos_are_refused_and_closed`,
`silent_slow_and_broken_peers_are_closed_by_the_hello_deadline`,
`every_refused_connection_gives_its_slot_back` and
`unauthenticated_peers_cannot_crowd_out_the_rest` (A7), plus the pairing tests
`an_expired_or_reused_code_is_refused` and
`the_wrong_pc_is_refused_before_anything_is_said`. On Android, the A3 scanner
phase (p8), the A6 phases (`LocalDocumentScreenTest`, `LocalTextTest`) and the
A9 `KeyProtectionTest` / About check (p1, p2) passed.

**Red runs** (unfixed HEAD plus the new tests):

- `ManifestTest`: 2 tests, 1 failure.
- `IncomingIntentsTest`: #1 and #2 FAIL; the guards #3 and #4 PASS.
- After the repair, all four passed singly and again in the E2E run.

**A failure found and fixed during verification.** The first full E2E run
(`/mnt/F/.lcl-pretest/a10/e2e-full`) passed its first six phases, then failed
p1 with "connection is Connected still not satisfied after 60000 ms".

- **What was observed.** The screenshot taken just before the press shows the
  Pair button below the visible area. At that time the change included a
  longer instruction sentence on the Pair screen (two more lines), which
  pushed the button off the emulator's 1080×2400 screen. The test's press on
  it reached nothing: no pairing attempt appears in the app's log, the PC
  listed 0 devices, and the code was unconsumed.
- **Proof.** `RemoteEndToEndTest#p0_pair_only` (paste, then Pair) failed the
  same way with the sentence (0 devices on the PC). It passed with the
  original text restored (1 device).
- **Resolution.** The sentence was removed. The second full run, on the final
  tree (`/mnt/F/.lcl-pretest/a10/e2e-final`), is the one in the table.

Not run:

- **HawkScan (DAST)**: NOT_RUN. No HTTP surface changed, `HAWK_API_KEY` is not
  set, and a scan would use an external service.
- **The `impl/` gate**: not rerun, since no file outside `android/` (and this
  report) changed. The E2E run drives the PC engine through `lcl-remote`, and
  p2 checks that it reports the Core 0.1 and 0.2 specification identities.

## 9. The built-in scanner still requires Pair

p8 is unchanged and passed on the final tree. The scan runs through the app's
own **Scan QR code**, with `CaptureActivity` answered by the instrumentation
(no camera). It fills the text field with exactly the link and shows the
preview with the fingerprint. Three seconds later there are the same Keystore
aliases, the same PC record and the same connection label, and no pairing
problem is shown. Only Pair connects and adds one key. The PC then lists 3 devices,
the newest unrevoked, and every pairing code is consumed. The scanner code in
`PairScreen.kt` (`rememberLauncherForActivityResult(ScanContract())`, which
only sets `link`) is unchanged.

## 10. External `lclpair://` dispatch is no longer exported

- The packaged release manifest (`aapt2 dump xmltree` of
  `app-release-unsigned.apk`) contains no `lclpair` and no `BROWSABLE`
  entries. `MainActivity` has exactly two filters: the launcher and the
  document filter. ZXing's `CaptureActivity` has none and is not exported.
- On the emulator, with the fixed app:
  - `cmd package query-activities` for the link, with and without
    `BROWSABLE` → `No activities found`.
  - `am start` with the implicit `BROWSABLE` intent → `Error: Activity not
    started, unable to resolve Intent`.
  - `am start -n io.lcl.workspace/.MainActivity` with the link → the app
    opens on Home; the UI dump shows no link and no code.
- `ManifestTest` and `IncomingIntentsTest` #1 and #2 pass; they failed before
  the repair.

## 11. Document opening is intact

- The document filter is unchanged in the source and in the packaged manifest
  (`file`/`content`, `*/*`, `.*\.lcl`, `.*\..*\.lcl`, `.*\.lcl\.txt`,
  `.*\..*\.lcl\.txt`).
- `ManifestTest` #2 and `IncomingIntentsTest` #3 pass: `.lcl` and `.lcl.txt`
  resolve to `MainActivity`, a plain `.txt` does not.
- `IncomingIntentsTest` #4 passes: a real Download file resolved by the
  system opens with its exact text. So does the document half of #2.
- The A6 strict UTF-8 decoding is unchanged: `LocalDocumentScreenTest` × 2
  and `LocalTextTest` pass.

## 12. Protected paths

`git diff --name-only -- canonical/ releases/ assets/brand/` printed nothing,
and `git status --porcelain -uall -- canonical releases assets/brand` printed
nothing. Nothing under `canonical/`, `releases/` or `assets/brand/` changed.
No language grammar, semantics, registry or conformance mapping was touched.

Artifacts: nothing is staged. The only untracked files are the two new test
sources and this report. Build outputs (`android/app/build/`,
`android/build/`, `android/.gradle/`, `android/.kotlin/`, `impl/target/`) and
`android/local.properties` (pre-existing, `sdk.dir` only) are gitignored and
were not added. No APK, AAB, keystore, private key, log or credential is in
the tree. All logs, APK evidence and E2E work directories are under
`/mnt/F/.lcl-pretest/a10/`. `lcl-remote` builds went to
`/mnt/F/.lcl-pretest/remote-target`.

## 13. Remaining limitations

- **A QR scanned with another app is read by that app.** LCL no longer
  claims `lclpair://`, but it cannot stop another installed app from claiming
  it. If the person scans the PC's QR with a camera or scanner app that opens
  links, that app can pass the whole link, code included, to such an app,
  which can pair first. What guards against it:
  - the in-app hint and the docs send people to LCL's own scanner;
  - the code works once and lasts 5 minutes by default;
  - the PC lists every paired device and can revoke any of them;
  - the real phone's Pair then fails with "already used", and Troubleshooting
    says to check the device list.

  Closing this completely would need a QR payload that other apps do not
  treat as a link, or another channel. That is a pairing-format change,
  outside this v0.1 task.
- A pasted link passes through the clipboard, where the keyboard or a
  clipboard history may see it. Paste is documented as the advanced
  alternative, "treat the link like the QR code".
- `MainActivity` stays exported (it is the launcher). Any app can start LCL or
  send it a document; a pairing link sent that way is ignored (tested).
- The E2E presses Pair without scrolling to it. On a smaller screen or a
  larger font size, a longer Pair screen could push the button off-screen the
  way §8 shows. This is pre-existing test fragility, not changed here.
- Physical camera: NOT_RUN. Physical phone: NOT_RUN. Separate LAN, Internet,
  mobile data: NOT_RUN. What a real camera app does with an `lclpair://` QR
  after this change was not observed. All device tests ran on one emulator
  (API 36). `IncomingIntentsTest` #4 is skipped below API 30.

## 14. Final Git status

```
 M android/README.md
 M android/SUPPORTED_OPERATIONS.md
 M android/app/src/androidTest/java/io/lcl/workspace/RemoteEndToEndTest.kt
 M android/app/src/main/AndroidManifest.xml
 M android/app/src/main/java/io/lcl/workspace/MainActivity.kt
 M android/app/src/main/java/io/lcl/workspace/ui/HomeScreen.kt
 M android/app/src/main/java/io/lcl/workspace/ui/PairScreen.kt
 M android/app/src/main/java/io/lcl/workspace/ui/Root.kt
 M android/tools/e2e.sh
?? android/app/src/androidTest/java/io/lcl/workspace/IncomingIntentsTest.kt
?? android/app/src/test/java/io/lcl/workspace/ManifestTest.kt
?? reports/implementation/LCL_ANDROID_A10_PAIRING_LINK_REPAIR.md
```

Branch `main`, HEAD `3ea51528ceddc51e545102bc45bee1df95106274`, nothing
staged.

## 15. Commit and push

- Committed: NO
- Pushed: NO

Afterwards, on 2026-09-25, the owner asked for the work to be committed and
synced. The changes listed in §5 are commit
`e3e197c37c8538d5661530695b1fcde3ac2eca2b`, on `main` directly after
`3ea5152`, and this report is the commit after it. §14 and the two lines
above describe the tree as it was when the report was written, before those
commits.

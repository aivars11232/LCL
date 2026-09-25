# LCL for Android

A native Android app that works on your PC's LCL from your phone or tablet:
browse the PC's projects, edit documents, and Check, Validate, Inspect and Run
them with the PC's own engine. Every effect a run wants waits for you to allow
it.

```
   PC                                         phone
   ─────────────────────────────              ───────────────────────────
   projects and files on disk                 project and file browser
   the LCL engine (lcl-workspace routes)      editor, line numbers, colours
   Check · Validate · Inspect · Run    ◄───►  diagnostics, structure
   host permissions, run execution            run events, effect approvals
   lcl-remote: identity, trusted devices      Android Keystore: device key
        TLS 1.3, both ends pinned, protocol lcl.remote/1
```

The PC is the authority. The phone never parses, checks or runs LCL itself —
there is no second LCL implementation in Kotlin. What a document means, whether
it is valid and what a run may do is decided by the same engine, routes and
rules as the desktop LCL Workspace and the `lcl` command. The phone asks, shows
the answers, and keeps its copies honest about which revision they came from.

What the app supports, operation by operation, is in
[SUPPORTED_OPERATIONS.md](SUPPORTED_OPERATIONS.md). The PC side, its commands
and the protocol are in [../remote/README.md](../remote/README.md).

## Contents

- [What you need](#what-you-need)
- [Building](#building)
- [Installing on a phone](#installing-on-a-phone)
- [Release signing and updates](#release-signing-and-updates)
- [Setting up the PC](#setting-up-the-pc)
- [Pairing](#pairing)
- [Working on a document](#working-on-a-document)
- [Connections: pairing is not a connection](#connections-pairing-is-not-a-connection)
- [Disconnect, Forget PC, Revoke device](#disconnect-forget-pc-revoke-device)
- [Networks: what works where](#networks-what-works-where)
- [Security model](#security-model)
- [Testing](#testing)
- [Known limitations](#known-limitations)
- [Troubleshooting](#troubleshooting)

## What you need

- **Phone or tablet:** Android 10 (API 29) or newer. No Google account, Play
  Store, root, custom ROM, ADB or developer mode is needed to use the app.
- **PC:** Linux with LCL installed (`packaging/install.sh`) and `lcl-remote`
  installed and running ([Setting up the PC](#setting-up-the-pc)).
- **To build the app:** JDK 17 or newer (21 is what it was built and tested
  with), the Android SDK with platform 37 and build tools, and network access
  for Gradle's first dependency download. The Gradle wrapper downloads Gradle
  9.8.0 and checks its published SHA-256 (`gradle-wrapper.properties`).

## Building

From `android/`, with `ANDROID_HOME` (or `local.properties` with `sdk.dir=`)
pointing at the SDK:

```sh
./gradlew assembleDebug            # app/build/outputs/apk/debug/app-debug.apk
./gradlew testDebugUnitTest        # JVM unit tests
./gradlew lint                     # Android lint; any error fails the build
./gradlew assembleRelease          # release APK, signed if a signing key is configured
```

`local.properties`, `build/`, `.gradle/` and `.kotlin/` are machine-local and
ignored by Git.

| | |
|---|---|
| Application id | `io.lcl.workspace` |
| minSdk / targetSdk / compileSdk | 29 / 36 / 37 |
| UI | Kotlin, Jetpack Compose, Material 3 — no WebView anywhere |
| Networking | `javax.net.ssl` TLS 1.3 sockets (Conscrypt), no HTTP stack |
| QR scanning | ZXing (`zxing-android-embedded`), camera permission asked on use |

## Installing on a phone

The app is an ordinary APK; install it directly.

1. Copy the APK to the phone (USB, a download, a shared folder).
2. Open it. Android asks once to allow installs from that source (Files,
   Chrome, …) — allow it for that app.
3. Install.

With a USB cable and developer mode, `adb install app-debug.apk` does the same;
that is a developer convenience, not a requirement.

**Updates.** A newer APK signed with the *same* key installs over the old one
and keeps everything: paired PCs, device keys, settings. An APK signed with a
different key is refused by Android as an update; uninstalling first would
delete the device keys, and every PC would have to be paired again. That is
why the release key matters — see the next section. A debug APK and a release
APK are signed differently, so do not switch between them on one phone.

## Release signing and updates

Nothing secret is in the repository. The build reads the release key from
Gradle properties (for example `~/.gradle/gradle.properties`, outside the
repository) or from the environment:

| Gradle property | Environment variable |
|---|---|
| `lclReleaseStoreFile` | `LCL_RELEASE_STORE_FILE` |
| `lclReleaseStorePassword` | `LCL_RELEASE_STORE_PASSWORD` |
| `lclReleaseKeyAlias` | `LCL_RELEASE_KEY_ALIAS` |
| `lclReleaseKeyPassword` | `LCL_RELEASE_KEY_PASSWORD` |

Without them, `assembleRelease` produces an unsigned
`app-release-unsigned.apk` that can be signed later with `apksigner`.

**Create the signing identity once:**

```sh
keytool -genkeypair -v -keystore ~/keys/lcl-android-release.jks \
    -alias lcl-release -keyalg RSA -keysize 4096 -validity 10000
```

Then, in `~/.gradle/gradle.properties`:

```properties
lclReleaseStoreFile=/home/you/keys/lcl-android-release.jks
lclReleaseStorePassword=…
lclReleaseKeyAlias=lcl-release
lclReleaseKeyPassword=…
```

**Keep it.** Every future update must be signed with this exact key. Keep at
least two copies of the keystore and its passwords, offline and apart (for
example an encrypted USB drive and a password manager). A lost key cannot be
recovered: phones would have to uninstall and pair again. Never commit the
keystore or its passwords; `android/.gitignore` ignores `*.jks`, `*.keystore`,
`*.p12`, `*.pk8` and `keystore.properties` as a second line of defence.

Check a signed APK before handing it out:

```sh
apksigner verify --print-certs app-release.apk
sha256sum app-release.apk
```

Publish the SHA-256 beside the APK so people can check what they install.

## Setting up the PC

Install LCL first (`packaging/install.sh`), then the PC side of the app:

```sh
remote/install.sh          # builds lcl-remote, installs ~/.local/bin/lcl-remote
                           # and a systemd user unit, which it does NOT enable
systemctl --user daemon-reload
systemctl --user enable --now lcl-remote     # run now and at every login
```

or run `lcl-remote serve` in a terminal. The service listens on TCP port
47300 and answers local discovery on UDP port 47301; a firewall on the PC must
let the phone's network reach them. It serves nothing to a device that has not
paired. See [../remote/README.md](../remote/README.md).

## Pairing

Pairing is done once per phone and PC.

1. On the PC: **LCL Workspace → Settings → Android devices → Pair Android
   device**, or `lcl-remote pair` in a terminal. A QR code appears, with the
   PC's fingerprint.
2. On the phone, open **LCL**.
3. Choose **Pair a PC**.
4. Choose **Scan QR code** and scan the PC's QR code.
5. The phone shows the PC's name and fingerprint. Check that the fingerprint
   is the one on the PC's screen.
6. Press **Pair**.

Scan with LCL's own **Scan QR code**, not with the phone's camera app or
another scanner app. The QR code holds an `lclpair://` link with a one-time
code, and the code pairs whoever uses it first; any app can claim that link
scheme, so LCL takes no pairing link from another app. A camera app, a web
page or a message cannot open LCL's pairing, and a link another app sends to
LCL is ignored.

Advanced, instead of scanning: paste the link `lcl-remote pair` prints into
the text field on **Pair a PC**. Treat the link like the QR code: it pairs
whoever uses it first, until it is used or expires.

A scanned code and a pasted link only fill the form in. No key is made,
nothing is recorded and nothing connects until you press Pair.

The QR code carries: the link format version, the PC's id and name, the
SHA-256 fingerprint of the PC's certificate, the addresses to try, a one-time
code and its expiry. It carries no private key and no reusable password. The
code works once and for five minutes (`lcl-remote pair --minutes N`, 1–60);
the PC stores only its SHA-256 and marks it used the moment a device pairs.

During pairing the phone makes a new ECDSA P-256 key inside Android Keystore,
proves it holds it in the TLS handshake, and the PC records the device by the
fingerprint of that certificate. From then on the phone connects with that key;
the QR code is never needed again.

Whether that key is hardware-backed depends on the phone. Android Keystore
may keep it in a StrongBox secure element, in a trusted execution environment
(TEE), or in software; in every case it is not exportable and the app holds
only a handle. The app requires none of the hardware kinds, so phones without
them are supported, and **About → This device's key** shows what Android
reports for the key. The emulator the app is tested on reports a software
key; no physical phone has been checked yet.

## Working on a document

- **Projects and files.** The PC's shared projects (its default workspace, plus
  any added with `lcl-remote projects add`), their `.lcl` and `.lcl.txt`
  documents, and a project picker. Other files, `.txt` included, are not LCL
  and are not listed. `lcl-remote projects add` and `remove` take effect for a
  running service at once: a removed project is gone from the list and every
  request for it is refused from the next one on, and a run the phone started
  there is stopped and reported ended.
- **Editor.** Monospace text that is always visible; line numbers that start
  at 1, follow every inserted or deleted line at once, scroll with the text and
  can be hidden; no wrapping, with horizontal scrolling for long lines;
  selection, copy, cut and paste; Undo and Redo; the cursor's line and column;
  Indent and a hardware Tab key insert four spaces, never a tab character.
- **Colours** come from the PC's lexer for the exact text on screen. Typing is
  shown at once; colours for the new text follow when the PC answers, and until
  then the text is drawn plain. Highlighting never hides text.
- **Diagnostics** come from the PC's engine, underlined in the text and listed
  with their stage, status and position; tapping one moves the cursor there.
  **Structure** shows the imports, declarations and execution plan Inspect
  derived.
- **Save** replaces exactly the revision this copy was loaded from. If the file
  changed on the PC meanwhile, nothing is written: the app shows the PC's
  version and asks — **Use PC version** or **Keep mine**. Keep mine makes your
  text an edit of the PC's newer revision, and the next Save writes it, because
  you chose so. Two paired devices saving from the same revision cannot both
  land (tested). The exact boundary: the PC sees every change made to the file
  before it compares, whoever made it; it cannot lock out another program on
  the PC — the desktop workspace, whose saves carry no precondition, or any
  editor — that replaces the file in the instant between that comparison and
  the save's atomic rename. That window is the time to read and hash the file,
  not the time the phone had it open.
- **Changes made on the PC** arrive while a document is open. A copy with no
  unsaved edits is refreshed; one with unsaved edits is stopped with the choice
  above.
- **New document.** A name without an ending gets the PC's default (`.lcl`,
  or `.lcl.txt` if the PC is set so): `test` → `test.lcl`; `test.lcl` and
  `test.lcl.txt` are kept as typed.
- **Run** starts the document on the PC. You grant host permissions for this
  run (read and write paths, programs, network hosts, inputs); the document's
  own authorizations still apply. The run pauses before every effect — the PC
  enforces that for every remote run, and no device can turn it off — and shows
  what the engine says it is — operation, target, parameters, category,
  effects, where in the document, and what authorized it — and waits for
  **Allow**, **Deny** or **Stop run**. Pairing never approves anything by
  itself. Events and the final status and outputs are shown as they come. If
  the connection drops, the run keeps going on the PC and is followed again
  from the first event the phone missed. Only the device that started a run
  can follow it or answer its pauses; another paired device is refused.
- **Settings** (this device only): theme System / Dark / Light, font size,
  line numbers.
- **About**: app version, remote protocol version, the connected PC and its
  fingerprint, this device's fingerprint for that PC, the PC's service and
  engine protocol versions, and the LCL Core 0.1 and Core 0.2 packages with
  their identity digests — as the PC's engine reports them — and the phone's
  Android version and ABIs.
- **Opening a file.** A `.lcl` or `.lcl.txt` file opened from another app is
  shown read-only and can be checked or inspected on the connected PC, as if it
  were a document of the current project. Its bytes must be valid UTF-8 (at
  most 4 MB): a file that is not is refused with the offset of the first bad
  byte, and is neither shown nor sent to the PC — never repaired with
  replacement characters.

## Connections: pairing is not a connection

**Pairing** is long-lived trust: the phone keeps the PC's fingerprint and its
own key; the PC keeps the phone's certificate fingerprint. **A connection** is
a TLS socket, which comes and goes.

None of these touch the pairing, and none of them need a QR code afterwards:
closing the app, swiping it away, the app's process being killed, restarting
the phone, restarting the PC or its service, losing Wi-Fi or mobile data,
changing networks, a router restart, a new IP address, a timeout.

While the app is running it keeps the connection up by itself:

- It connects on start to the PC it was last working with.
- If the connection ends, it retries after 1, 2, 4, 8, 15 and then every
  30 seconds; **Reconnect** retries at once.
- With no network it waits (**Offline**) and connects as soon as there is one.
- When the phone moves to another network (Wi-Fi to mobile data, one Wi-Fi to
  another), it reconnects at once instead of waiting for the old socket to fail.
- It tries, in order: the address that worked last, every address it has used
  for this PC, and then asks the local network (UDP discovery) where the PC is
  now. Whatever answers must still present the pinned certificate.
- A dead connection is noticed within about 30 seconds (a ping every 20 s).

After a reconnect the app re-reads the PC's projects and settings, compares
every open document with the PC's current revision (refreshing clean copies,
stopping at conflicts), and follows any run that was going.

**In the background.** There is no foreground service and no permanent
notification. While Android keeps the app's process alive the connection
stays up; when Android stops the process, the next time the app opens it
reconnects at once. A foreground service would only keep a socket open for an
editor nobody is looking at; trust does not need one. Offline, documents stay
on screen, read-only, until the PC is back.

## Disconnect, Forget PC, Revoke device

| | Where | Connection | Trust | Afterwards |
|---|---|---|---|---|
| **Disconnect** | phone, PCs screen | closed now | kept | **Reconnect**, or the next app start, connects again — no QR code |
| **Forget PC** | phone, PCs screen | closed | this phone deletes the PC's record and its own key for that PC; if connected, it also asks the PC to revoke it | only a new QR code pairs them again |
| **Revoke device** | PC: Settings → Android devices, or `lcl-remote revoke ID` | ended within a second | the PC refuses that device from now on | the phone shows *Not trusted*; only a new QR code pairs it again |

Revoking one device leaves every other device alone. Trust is never recreated
silently: a revoked device does not retry, and a device the PC no longer knows
is told so instead of re-pairing.

## Networks: what works where

What works and was tested:

- **Same network.** Phone and PC on the same LAN: pairing, reconnection,
  discovery after an address change. Tested on an Android 16 emulator against
  a real `lcl-remote` on the PC.

What works by design but was **not** tested:

- **Any route the phone can reach the PC by.** Identity is the certificate, not
  the address, so the phone connects over whatever path reaches the PC's port:
  a VPN or overlay network (WireGuard, Tailscale, ZeroTier), or a forwarded
  port on the PC's router. Add the address the phone should use, then pair:

  ```sh
  lcl-remote address add my-pc.example.net:47300   # goes into new QR codes
  ```

  Only port 47300/TCP needs to be reachable. A connection through a VPN or a
  forwarded port is still end-to-end TLS with both ends pinned.

What is **not implemented**:

- **NAT traversal and a relay.** There is no hole punching and no relay
  server. A phone on mobile data cannot reach a PC behind a home router unless
  one of the routes above exists. A relay would fit this design without
  weakening it: the TLS session runs end to end, so a relay would only move
  bytes and could neither read documents nor act as a device (see
  [../remote/README.md](../remote/README.md#a-relay-later)). It is future work.
- Discovery is local-network broadcast only.

## Security model

- **Two identities, both keys.** The PC has an ECDSA P-256 key and a
  self-signed certificate (`~/.config/lcl/remote/identity.key`, `0600`, in a
  `0700` directory). Each phone has one key *per PC*, generated in Android
  Keystore, non-exportable; the app only holds a handle. Whether the phone
  keeps it in hardware (StrongBox or TEE) or in software is the phone's
  choice, reported in About; hardware is not required. Neither private key
  ever leaves its device or appears in a QR code, a log or a file the app
  writes.
- **Pinned both ways.** TLS 1.3 only. The phone accepts exactly the PC
  certificate whose SHA-256 the QR code carried — no certificate authority,
  host name or IP address is trusted. The PC accepts a device only by the
  SHA-256 of the certificate the device proved it holds the key for. A
  different machine at the PC's address is refused before the phone says
  anything to it.
- **What is stored where.** Phone, SharedPreferences: the PC's id, name,
  fingerprint, addresses, this device's id and the Keystore alias — nothing
  secret. Phone, Keystore: the device keys. App backup and device transfer are
  off for all of it: a restored copy would hold records whose keys stayed on
  the old phone. PC: the device list (`devices.json`, `0600`) with each
  device's name, certificate fingerprint, pairing date, last connection,
  protocol and revocation.
- **One-time bootstrap.** A pairing code is 32 random bytes, stored by the PC
  only as a hash, good once and for minutes, consumed under a file lock. A
  replayed or expired code pairs nothing.
- **The code goes only into LCL's own form.** An unused code, with a new key,
  is all the PC asks of a device that pairs. So the app takes no pairing link
  from another app: no intent filter handles `lclpair://` and none is
  BROWSABLE, and a link sent to the app's activity by name is ignored. Only
  the app's own scanner, or the person pasting a link, fills the Pair form in.
- **Fail closed.** A malformed first message, an unsupported protocol version,
  an unknown or revoked device, a device naming another device's id, an
  unreadable trust store: one error, then the connection closes. Before a
  device is authenticated it has 10 seconds to finish TLS and say `hello`, and
  its first message may be at most 8 KiB; a longer one, a frame that is not
  UTF-8 or silence closes the connection. After, frames may be up to 16 MiB.
  At most 32 connections are served at once, of which at most 8 — and at most
  4 from any one address — may be unauthenticated, so peers that connect and
  say nothing cannot crowd out paired devices' sessions. Every connection's
  place is given back however it ends.
- **No new powers.** A device can use a fixed list of operations. None runs a
  command, reads an arbitrary path, or reaches anything outside the shared
  projects. Document and engine operations are the workspace's own routes, so
  a device is refused exactly what the workspace would refuse, and a run is
  authorized and permitted exactly as a run from the workspace.
- **Approvals.** Every remote run pauses before every effect and waits for an
  answer over the authenticated session. The PC sets this for each remote run
  and ignores a request not to; nothing is approved because a device is
  paired.
- **Runs belong to their device.** The PC records which device (by id and
  certificate fingerprint) started each remote run. Only that device can
  follow the run or answer its pauses — continue, deny or cancel; any other
  paired device is refused, whatever run id it names.
- **Sharing is live.** What is shared is read from `remote.json` on every
  request, so removing a project refuses it at once, without restarting the
  service.

## Testing

- **JVM unit tests** (`./gradlew testDebugUnitTest`): the pairing link, frames,
  pinning, UTF-8/UTF-16 offsets and spans that move with edits, undo, the
  document revision model, stores and settings, the connection manager
  (pairing, restart without QR, backoff, offline, network change, a moved PC,
  an impostor at the old address, Disconnect, Forget, revocation, several PCs)
  and the workspace controller (answers applied only to the revision they
  describe, save conflicts, PC edits, runs, approvals, reconnect), and the
  manifest (no filter takes `lclpair://` links and none is BROWSABLE; the
  launcher and the `.lcl` / `.lcl.txt` document filter are kept). The
  connection tests use a real certificate and key, made by the JDK's `keytool`
  when the tests run.
- **End to end on a device** (`tools/e2e.sh`): the app on an emulator (or a
  phone) against a real `lcl-remote serve` with its own XDG directories, real
  Android Keystore and TLS, and the real engine. It opens a local file that is
  not UTF-8 (refused) and one that is (shown exactly); checks against the
  installed app that no activity takes a pairing link, that one sent to the
  app by name fills nothing in, and that `.lcl` and `.lcl.txt` files linked
  from another app still open (a plain `.txt` does not); pairs from the link
  pasted on the Pair screen, edits, saves, checks, validates, inspects and
  runs with approvals, and checks each result on the PC's disk; then restarts
  the app, reboots the phone, cuts the network, restarts the PC service, edits
  on the PC (including a real conflict), revokes, re-pairs and forgets; and
  last pairs through the app's own **Scan QR code** button, checking that the
  scanned code made no key, no record and no connection until Pair was
  pressed. The camera itself is not used: the instrumentation answers the
  scanner's camera activity with the scanned link, and everything after it is
  the app's own code. It also checks that About reports the key's protection
  as Android reports it. It fails on any ANR or crash the system records for
  the app.
- **The PC side** has its own tests (`cargo test` in `remote/`), including
  several devices, revocation and impersonation, runs that pause at every
  effect even when asked not to, runs another device cannot follow or answer,
  unsharing a project while the service runs, two devices saving from one
  revision, and peers that send oversized, broken or no first messages or
  crowd the connection slots.

## Known limitations

- No relay and no NAT traversal; see [Networks](#networks-what-works-where).
- Tested on an emulator (Android 16, x86_64). Not yet tested on a physical
  phone, with a physical camera scanning a QR code, on a phone on a separate
  LAN, or over the Internet or mobile data.
- A pairing QR code scanned with another app is read by that app. LCL takes
  no `lclpair://` links, but another installed app can claim the scheme, and
  a camera or scanner app that opens links can then pass it the whole link,
  one-time code included; whoever uses an unused code first pairs. What
  guards against it is how the code is used: scan only with **Scan QR code**
  in LCL; a code works once and expires within minutes (5 by default); and
  the PC lists every paired device, to revoke one you do not know.
- A save's precondition sees every change made to the file before the PC
  compares, and two devices cannot both save over one revision; but another
  program on the PC that replaces the file in the instant between the
  comparison and the rename is not locked out (see **Save** above).
- One active PC connection at a time (any number of paired PCs).
- Remote documents are read-only while offline; there is no offline editing
  queue.
- The desktop debugger's breakpoints and stepping, find and replace, and
  go-to-definition are not in the app; see
  [SUPPORTED_OPERATIONS.md](SUPPORTED_OPERATIONS.md).
- Deleting and renaming documents are not in the app yet (the protocol can
  delete).
- No background connection when Android stops the process (by design; see
  above).
- `lcl-remote` is not yet part of the LCL release payload; it is built and
  installed from source with `remote/install.sh`.

## Troubleshooting

| Symptom | Likely cause |
|---|---|
| *Could not reach the PC at …* | The service is not running (`lcl-remote status`), the firewall blocks TCP 47300, or the phone is on another network with no route to the PC. |
| *The computer at … is not the PC this device paired with* | Another machine answers at that address, or the PC's identity was reset (`uninstall.sh --purge`). Pair again only if you know why. |
| *Not trusted — This PC revoked this device* | Revoked on the PC. Pair again with a new QR code. |
| *This pairing code was already used* / *has expired* | Show a new QR code. If a code you did not use was already used, look at the PC's device list and revoke any device you do not know. |
| *Offline — no network* | The phone has no network at all; it connects as soon as it has one. |
| A document shows *changed on the PC* | Someone saved it on the PC while you had unsaved edits; choose Use PC version or Keep mine. |

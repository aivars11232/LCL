# lcl-remote — the PC side of LCL for Android

`lcl-remote` lets paired Android devices work on this PC's LCL: its projects,
its documents and its engine. It pairs devices by one-time QR code and your
approval on this PC, keeps the list of devices it trusts, and serves each one
an encrypted, authenticated session over the protocol `lcl.remote/1`. The app is described in
[../android/README.md](../android/README.md).

It is not the desktop workspace's local server exposed to the network. That
server stays on `127.0.0.1` with its launch token. `lcl-remote` is a separate
program with its own identity, its own trust store and a fixed list of
operations; inside, it calls the workspace's routes in process, so a device
gets exactly the engine, rules and refusals the desktop gets.

## Contents

- [Installing and running](#installing-and-running)
- [Commands](#commands)
- [Files](#files)
- [Pairing and trust](#pairing-and-trust)
- [The protocol, lcl.remote/1](#the-protocol-lclremote1)
- [Security](#security)
- [Networks](#networks)
- [Building and testing](#building-and-testing)

## Installing and running

Install LCL first (`packaging/install.sh`), then:

```sh
remote/install.sh      # builds and installs ~/.local/bin/lcl-remote and the
                       # systemd user unit ~/.config/systemd/user/lcl-remote.service
                       # (installed, not enabled)
```

Run it at every login:

```sh
systemctl --user daemon-reload
systemctl --user enable --now lcl-remote
```

or in a terminal: `lcl-remote serve`. It needs the specification packages the
LCL installer put in `~/.local/share/lcl` (or `--spec`, `--localized-spec`,
`LCL_SPEC`, `LCL_LOCALIZED_SPEC`).

It listens on `0.0.0.0:47300` (TCP) for devices and answers discovery on UDP
port 47301. A firewall must let the phone's network reach those ports.

`remote/uninstall.sh` removes the program and the unit and keeps this PC's
identity and paired devices; `remote/uninstall.sh --purge` deletes those too,
which unpairs every device for good.

`lcl-remote` is not part of the LCL release payload yet; `remote/install.sh`
builds it from this source tree. Building needs Rust 1.89 or newer.

## Commands

```
lcl-remote serve [--spec PATH] [--localized-spec PATH] [--listen ADDR] [--port PORT] [--no-discovery]
lcl-remote pair [--address HOST:PORT]... [--minutes N] [--json]
lcl-remote pending [--json]
lcl-remote approve REQUEST-ID
lcl-remote deny REQUEST-ID
lcl-remote devices [--json]
lcl-remote revoke DEVICE-ID
lcl-remote rename DEVICE-ID NAME
lcl-remote projects [--json] | projects add PATH | projects remove PATH
lcl-remote address add HOST:PORT | address remove HOST:PORT
lcl-remote identity [--json] | identity rename NAME
lcl-remote status [--json]
```

- `pair` prints a QR code in the terminal, the PC's fingerprint grouped the way
  the phone shows it, and the pairing text to paste instead. `--json` gives
  the pairing text (`payload`), the QR code as SVG, the expiry, addresses and
  fingerprint. The code pairs at most one device and is good for 5 minutes
  (`--minutes`, 1–60). Addresses default to this PC's own plus any added with
  `address add`. **Scanning the QR code does not trust the phone**: it only
  lets the phone ask (see `pending`).
- `pending` lists the pairing requests waiting for you: request id, the name
  the device gave, its full certificate fingerprint, its **verification
  code**, when it asked and when it expires. `--json` gives the same. It never
  shows the pairing code.
- `approve` trusts the device that made one request. Approve only the request
  whose verification code is the one your phone shows. The phone finishes
  pairing by itself within a few seconds. One code approves at most one
  request; approving another for the same code is refused.
- `deny` refuses one request; that device can never use it again. The code
  stays usable by your own phone.
- `devices` lists every device: name, id, fingerprint, when it paired, when it
  was last connected, whether it is online now, and whether it is revoked.
- `revoke` ends one device's trust. Its live connection is closed within a
  second and it cannot connect again; only a new QR code pairs it again. Other
  devices are unaffected.
- `projects` are the folders devices can open: the desktop workspace's default
  folder, plus any added. `projects add` and `projects remove` take effect for
  a running service at once — it reads `remote.json` on every request, with no
  restart. A removed project disappears from the list and every request for it
  is refused from then on; its open routes are closed and the runs devices
  started in it are cancelled, each device being told its run was stopped. A
  `remote.json` the running service cannot read shares nothing but the default
  workspace.

The same pairing, pending requests (**Approve…** then **Approve device**, or
**Deny**), device list and revocation are in **LCL Workspace → Settings →
Android devices**, which runs this program.

## Files

Everything is under the user's own XDG directories, files `0600` in `0700`
directories:

| File | What |
|---|---|
| `~/.config/lcl/remote/identity.key` | this PC's private key (ECDSA P-256, PKCS#8) |
| `~/.config/lcl/remote/identity.crt` | its self-signed certificate; the SHA-256 of these bytes is the PC's fingerprint |
| `~/.config/lcl/remote/identity.json` | the PC's id and name |
| `~/.config/lcl/remote/devices.json` | trusted devices: id, name, certificate fingerprint, paired, last seen, protocol, revoked |
| `~/.config/lcl/remote/remote.json` | settings: listen address, ports, extra projects, public addresses |
| `~/.local/state/lcl/remote/pairing.json` | pairing challenges — only the SHA-256 of each code, its expiry, the request approved for it and who used it — and pairing requests: request id, code, device name, certificate fingerprint, verification code, status (`pending`, `approved`, `denied`, `finalized`, `superseded`) |
| `~/.local/state/lcl/remote/status.json` | the running service's port and live sessions, for `status` and `devices` |

A trust store that cannot be read trusts nobody.

## Pairing and trust

A pairing code lets a device **ask**; only the person at this PC can make it
trusted.

1. `pair` makes a challenge: 32 random bytes, stored only as a SHA-256, with
   an expiry. The QR code carries plain pairing text, not a link:
   `LCLPAIR|v=2&pc=<id>&n=<name>&fp=<certificate SHA-256>&a=<host:port>…&c=<code>&e=<expiry>`
   — no URI scheme, no private key, no reusable secret, no approval.
2. The phone connects to an address from the code and checks, inside the TLS
   handshake, that the PC's certificate has exactly that fingerprint.
3. The phone makes its own key in Android Keystore and presents a certificate
   for it; TLS proves it holds the key.
4. The phone sends `hello` with intent `pair`, `pairing_version` 2 and the
   code. The PC checks the code and records a **pending request** bound to
   that challenge and to that certificate's fingerprint — nothing more: no
   device record, no `welcome`, no session. It answers `pairing_pending` with
   the request id and the verification code, and closes the connection. The
   code is not used up, so someone else who read the QR code and asked first
   cannot lock the real phone out. Asking again with the same key is the same
   request.
5. The phone shows the verification code; `lcl-remote pending` (or the
   workspace) shows the same one. The person approves the matching request.
6. The phone asks again every two seconds with the same key and code. Once its
   request is approved, the PC records the device by its certificate's
   fingerprint, spends the code, supersedes every other request for it, and
   answers `paired` and `welcome`: the session starts.
7. From then on the phone connects with intent `connect` and its key. The code
   is never used again.

The verification code is the first 12 hex digits of
`SHA-256("lcl-pair-v2" 0x00 <PC fingerprint> 0x00 hex(SHA-256(<code>)) 0x00 <device fingerprint>)`,
shown as `abcd-ef12-3456`. It is for a person to compare and is not secret;
approval itself is bound to the request's full certificate fingerprint and
to its challenge, never to a request id, a name or an address.

At most 8 requests wait per code and 32 on this PC; one more is refused
(`pairing_busy`). Requests expire with their code and are pruned; a denied
request stays denied while its code lives.

Finishing is ordered so that a crash cannot let another device in: the
approved request is first given the id its device record will have (written to
`pairing.json`), then the record is made in `devices.json` under that id (or
found there, if the first try got that far), and only then are the request
and the code marked done. Whatever the crash left, only the approved
certificate can finish it, and no other request can be approved meanwhile.
The pairing lock is always taken before the device registry's, never the
reverse.

Devices paired before approval existed keep their records and reconnect with
`connect` as before; nothing about an existing pairing changes.

Addresses are only where to look. Neither side identifies the other by IP
address, network or host name.

## The protocol, lcl.remote/1

**Transport.** TCP, then TLS 1.3 only (no earlier version, no session tickets,
no early data), ALPN `lcl.remote/1`. Both ends present certificates; each
accepts the other only by pinned SHA-256. Inside, every message is one frame:
a 4-byte big-endian length and that many bytes of UTF-8 JSON, at most 16 MiB
once the device is authenticated. Before that, the one frame it may send,
`hello`, is at most 8 KiB; a longer length closes the connection before
anything more is read.

**Hello.** The device's first message, within 10 seconds of connecting (TLS
included):

```json
{"type":"hello","protocol":"lcl.remote/1","intent":"pair","pairing_version":2,"code":"…","name":"Pixel 9"}
{"type":"hello","protocol":"lcl.remote/1","intent":"connect","device":"ce7f8408067ea40f"}
```

To a pairing `hello` whose request is not approved, the PC answers once and
closes the connection:

```json
{"type":"pairing_pending","request":"8c1f2e3d4a5b6c7d","verification":"abcd-ef12-3456","expires":1790000300,"pc":{…}}
```

Once the request is approved, and to a `connect`, the PC answers `paired`
(after pairing) and then `welcome`:

```json
{"type":"paired","device":{"id":"…","name":"…"},"pc":{"id":"…","name":"…","fingerprint":"…"}}
{"type":"welcome","protocol":"lcl.remote/1","pc":{…},"device":{…}}
```

or one error, after which it closes the connection:

```json
{"type":"error","code":"revoked","message":"this PC revoked this device; pair it again with a new QR code"}
```

| Error code | Meaning |
|---|---|
| `malformed` | not JSON, not a `hello`, or missing fields |
| `unsupported_protocol` | another protocol version |
| `pairing_upgrade_required` | a pairing `hello` without `pairing_version` 2: the older flow, which trusted whoever used a code first |
| `unsupported_pairing_version` | a pairing `hello` naming a later pairing version |
| `pairing_refused` | the code is unknown, spent, approved for another device, or expired |
| `pairing_denied` | this device's request was denied on the PC |
| `pairing_busy` | too many requests are waiting for a decision |
| `not_paired` | no paired device has this certificate |
| `revoked` | this device was revoked |
| `identity_mismatch` | the device named a device id that is not its own |
| `unavailable` | the trust store could not be read or written |

**Requests and responses.** After `welcome`, the device sends requests and the
PC answers each with the same `id`. `status` has HTTP's meaning, as the
workspace's routes do; `body` is the route's JSON.

```json
{"type":"request","id":7,"op":"save","project":"…","document":"a.lcl","base":"<sha-256 it was loaded at>","text":"…"}
{"type":"response","id":7,"status":200,"body":{"id":"a.lcl","digest":"…","bytes":123,"final_line_feed_added":false}}
```

| Operation | Fields | What the PC does |
|---|---|---|
| `ping` | | `{"pong": <time>}` |
| `about` | | the service version, engine protocol, and the Core 0.1 / Core 0.2 packages with their identity digests, from the engine |
| `projects` | | the shared projects |
| `unpair` | | revokes the device making the request — never another |
| `session` | `project` | `GET /api/session` |
| `tree` | `project` | `GET /api/documents` |
| `settings` | `project` | `GET /api/settings` |
| `open` | `project`, `document` | `GET /api/document`; the document is then watched for changes |
| `close` | `project`, `document` | stops watching it |
| `save` | `project`, `document`, `base`, `text` | writes only if the file on disk still has digest `base`; otherwise `409` with the PC's current `text` and `digest`, and nothing written |
| `create` | `project`, `name`, `text` | `POST /api/document` |
| `delete` | `project`, `document`, `digest` | `DELETE /api/document`, only if unchanged |
| `tokens`, `check`, `validate`, `inspect` | `project`, `document`, `text` | the matching workspace route, over the text sent |
| `run` | `project`, `document`, `text`, `grants` {read, write, program, host}, `inputs`, `break_operations` | `POST /api/run` with a pause before every effect, always — a `break_effects` field is ignored; the run's events follow as events |
| `follow` | `project`, `run`, `from` | resumes a run's events from index `from` after a reconnect; `403` unless this device started the run |
| `answer` | `project`, `run`, `sequence`, `answer` (`continue`, `deny`, `cancel`) | `POST /api/answer`; `403` unless this device started the run |

A message that is not a well-formed request closes the connection. An
operation outside this list is answered `400`. A project that is not shared is
`404`. Paths are resolved by the workspace, inside the project; anything
outside it is refused.

**Events** the PC sends on its own:

```json
{"type":"event","event":"run","project":"…","run":"run-1","name":"paused","data":{…}}
{"type":"event","event":"run","project":"…","run":"run-1","name":"end","data":{}}
{"type":"event","event":"document_changed","project":"…","document":"a.lcl","digest":"…"}
{"type":"event","event":"revoked"}
```

Run events are the run's own event log — `operation`, `permission`, `effect`,
`paused`, `resumed`, `report`, `failed` — in order, then `end`. When the
run's project stops being shared, the device instead gets `failed` with
`"error": "this project is no longer shared by this PC, so the run was
stopped"`, then `end`.
Their count is what `follow`'s `from` refers to; `end` is not counted.
`document_changed` carries `"digest": null` when the file was deleted.

**Liveness.** The device pings every 20 seconds. The PC closes a connection
silent for 90 seconds, and rechecks every second that its device is still
trusted.

**Versioning.** The protocol version is not the LCL language version and not
the engine protocol. A future `lcl.remote/2` will be refused by a `/1` PC with
`unsupported_protocol`, and the reverse, rather than half-understood. The
pairing text has its own version (`v=2`), and so does the pairing flow a
`hello` names (`pairing_version` 2). Version 1 — the `lclpair://pair?v=1`
link, and a pairing `hello` without `pairing_version` — is refused for new
pairing.

## Security

- Nothing is served before `welcome`. There is no operation that runs a
  command, reads a path a device names, or reaches outside the shared
  projects. The desktop's local server is not exposed.
- A run from a device is the workspace's run: the document's authority, the
  capability rules and the host grants all apply. On top of that, the PC makes
  every remote run pause before every effect; a device cannot ask it not to.
  Effects wait for the answer over the authenticated session; pairing never
  approves anything.
- A run belongs to the device that started it: the PC records its device id
  and certificate fingerprint with the project and run. Only that device can
  `follow` it or `answer` its pauses (`continue`, `deny`, `cancel`); another
  paired device gets `403`, however it learned the run id.
- Pairing: a code alone trusts nobody. Whoever holds it can make a pending
  request, bound to the certificate it proved; only approval on this PC
  trusts that exact certificate, and the code then trusts no one else. A
  stolen code can make noise (up to 8 waiting requests) but not a trusted
  device.
- Replay: a spent code pairs nothing more; the TLS session protects every
  later message against replay and tampering.
- Resource limits: 32 connections at a time, of which at most 8 — and at most
  4 from any one address — may be unauthenticated (still in TLS or before
  `hello`); 10 seconds from connecting to finish TLS and say hello; an 8 KiB
  first message; 16 MiB per frame after that; 90 seconds of silence. A
  connection over a limit is closed at once. Every connection's slot is given
  back when its thread ends, however it ends. There is no rate limiting beyond
  these: peers that keep reconnecting can occupy the unauthenticated slots and
  delay new connections, but not disturb authenticated sessions. A device
  waiting for approval holds no connection: each ask is answered and closed.
- Saving (`save`) replaces a file only if it still has the digest the device
  started from, compared and renamed in one critical section of this service,
  so two devices cannot both save over one revision. A change any program made
  before the comparison is seen; another program — the desktop workspace, whose
  own saves carry no precondition, or an editor — that replaces the file in the
  instant between the comparison and the rename is not locked out, and is
  replaced.
- The PC's private key and the trust store are readable by the user only.
  Nothing secret is logged.

## Networks

Pairing and reconnecting work wherever the phone can open a TCP connection to
this PC's port: the same LAN (tested), or — untested — a VPN or overlay
network, or a port forwarded on the router. Add such an address with
`lcl-remote address add HOST:PORT` so new QR codes carry it.

Local discovery (UDP 47301) answers `LCL-DISCOVER 1 <pc-id>` with
`LCL-HERE 1 <pc-id> <port>`: a hint of where to try, never trust. Whoever
answers must still present the pinned certificate.

There is no NAT traversal and no relay.

### A relay, later

A relay would fit without weakening anything. The device would open its TLS
session to the PC *through* the relay, which only moves bytes. It would see
ciphertext, hold no key, and could neither read a document nor act as a
device, because both ends pin each other's certificates. Such a relay is
future work; nothing in this program depends on one.

## Building and testing

A Cargo workspace of its own, so the TLS and QR crates it needs (`rustls` on
`ring`, `qrcode`) never enter the LCL engine's dependency-free build:

```sh
cd remote
cargo build --release --locked
cargo test                                  # unit tests and end-to-end tests
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

The end-to-end tests in `tests/remote.rs` run a real service on a loopback
port with real TLS: pairing only through approval on the PC (a valid code
alone trusting nobody, a stolen code used first, approval bound to the exact
certificate, denial leaving the code to the real phone, one code never
approving two devices, expiry, refusal of the older pairing flow, bounded and
deduplicated requests, a device paired before approval existed reconnecting,
`pending`, `approve` and `deny` run as commands), expired and reused codes,
the wrong PC, malformed
and unsupported hellos, unpaired, revoked and impersonating devices, `unpair`,
several devices, path traversal, a link out of the project and unknown
operations, saves over a stale revision and two devices saving from one
revision, edits on the PC, a project shared and unshared while the service
runs, Check / Validate / Inspect, and runs that pause before every effect even
when asked not to, are denied, cannot get an effect the PC did not permit, are
followed after the connection drops, and cannot be followed or answered by
another device; and peers that send an oversized, partial, non-UTF-8 or no
first message, or crowd the unauthenticated slots.

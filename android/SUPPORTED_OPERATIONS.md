# What LCL for Android can do

Every operation below is carried out by the PC; the phone asks over
`lcl.remote/1` and shows the answer. "Remote operation" is the protocol
operation the app sends (see [../remote/README.md](../remote/README.md)), and
"PC route" is the LCL Workspace route the PC runs for it, in process — the same
code the desktop workspace uses.

✅ supported (implemented; what is tested, and on what, is in
[README.md](README.md#testing)) · ⏳ not in the app yet · — not applicable

## Connection and trust

| Operation | App | Remote operation | Notes |
|---|---|---|---|
| Pair with a PC by QR code | ✅ | `hello` intent `pair` | The app's own scanner (**Pair a PC → Scan QR code**), or a link pasted on that screen. Each only fills the form in; nothing is trusted, and nothing connects, until Pair is pressed. By design, no pairing link is taken from another app — the camera app, a browser, a message: the link's one-time code pairs whoever uses it first. |
| Reconnect without a QR code | ✅ | `hello` intent `connect` | On every app start, after network loss or change, after a PC restart. |
| Several paired PCs | ✅ | — | One active connection at a time; switch on the PCs screen. |
| Disconnect | ✅ | (closes the socket) | Pairing kept. |
| Forget PC | ✅ | `unpair` when connected | Deletes this phone's key and record for the PC; the PC is asked to revoke the device. |
| Revoke device | — | `revoked` event | Done on the PC; the phone shows *Not trusted*. |
| Keep-alive | ✅ | `ping` | Every 20 s. |
| PC identity and versions | ✅ | `about` | Shown in About. |

## Projects and documents

| Operation | App | Remote operation | PC route |
|---|---|---|---|
| List shared projects | ✅ | `projects` | the service's project list, read from `remote.json` on every request |
| Switch project | ✅ | — | |
| Project tree (`.lcl` and `.lcl.txt` only) | ✅ | `tree` | `GET /api/documents` |
| PC's default file ending | ✅ | `settings` | `GET /api/settings` |
| Open a document | ✅ | `open` | `GET /api/document` |
| Close a document | ✅ | `close` | — (stops change notifications) |
| Save over the revision it came from | ✅ | `save` with `base` | `Workspace::save_expecting`: compare and atomic rename in one critical section of the PC service; see **Save** in [README.md](README.md) for the exact boundary |
| Conflict: use the PC's version | ✅ | `open` | `GET /api/document` |
| Conflict: keep mine | ✅ | then `save` | as Save |
| Reload | ✅ | `open` | `GET /api/document` |
| Changes made on the PC | ✅ | `document_changed` event | the service watches open documents |
| New document (`test` → `test.lcl`, explicit endings kept) | ✅ | `create` | `POST /api/document` |
| Delete a document | ⏳ | `delete` with `digest` | `DELETE /api/document` — in the protocol, not yet in the app's UI |
| Rename, move, new folder | ⏳ | — | not in the protocol yet |
| Open a `.lcl` / `.lcl.txt` file from another app | ✅ | `check` / `inspect` | read-only view, judged on the PC; strict UTF-8, at most 4 MB — anything else is refused and nothing is sent |

## Editing

| Feature | App | Notes |
|---|---|---|
| Visible text, independent of highlighting | ✅ | Text is never hidden or delayed by analysis. |
| Line numbers | ✅ | From 1, updated on every edit, scroll-aligned, can be hidden. |
| Selection, copy, cut, paste | ✅ | The platform text field's own. |
| Undo, redo | ✅ | Typing runs merged into one step. |
| Cursor line and column | ✅ | |
| Horizontal scrolling, no wrapping | ✅ | |
| Monospace font, font size | ✅ | Font size 11–24 sp, in Settings. |
| Indentation with spaces only | ✅ | Indent button and hardware Tab insert four spaces. |
| Highlighting from the PC's lexer | ✅ | Remote operation `tokens` → `POST /api/tokens`, applied only to the exact revision it describes. |
| Dirty / saved state | ✅ | In the tab and above the editor. |
| Find and replace | ⏳ | |
| Go to definition, references | ⏳ | |

## Analysis

| Operation | App | Remote operation | PC route |
|---|---|---|---|
| Check | ✅ | `check` | `POST /api/check` |
| Validate | ✅ | `validate` | `POST /api/validate` |
| Inspect (structure: imports, declarations, execution plan) | ✅ | `inspect` | `POST /api/inspect` |
| Diagnostics in the text and in a list, jump to position | ✅ | from the reports above | |
| Analysis while typing | ✅ | `tokens`, `inspect` | after a 0.4 s pause, for the text on screen |

## Running

| Operation | App | Remote operation | PC route |
|---|---|---|---|
| Run the document on screen | ✅ | `run` | `POST /api/run` |
| Host grants for the run: read, write, programs, hosts, inputs | ✅ | `run` `grants`, `inputs` | as the workspace's run dialog |
| Pause before every effect | ✅ (always on) | `run` | forced by the PC for every remote run; a `break_effects` field is ignored |
| Pause before every operation | ⏳ | `run` `break_operations` | in the protocol, not in the app |
| Effect approval: Allow / Deny / Stop run | ✅ | `answer` | `POST /api/answer`, only from the device that started the run |
| Run events, final status, outputs | ✅ | `run` events | the run's event log |
| Follow a run again after a reconnect | ✅ | `follow` with `from` | the run's event log, from the first event missed; only the device that started the run |
| Breakpoints and stepping (desktop debugger) | ⏳ | — | |

## This device

| Feature | App |
|---|---|
| Theme System / Dark / Light | ✅ |
| Font size | ✅ |
| Line numbers on / off | ✅ |
| About: app, protocol, PC, fingerprints, this device's key protection (StrongBox, TEE or software, as Android reports it), PC service and engine protocol, LCL Core 0.1 and 0.2 identities, Android version and ABIs | ✅ |

## Never available to a device

These are not operations the PC offers, whatever a device sends:

- running a shell command or any program outside an LCL run the PC permits;
- reading or writing a path outside the shared projects, except through a run
  whose document authorizes it and whose host grants and effect approvals
  allow it;
- changing the PC's settings, its shared projects, its trusted devices (other
  than revoking itself) or its identity;
- following, approving, denying or cancelling a run another device started;
- a run whose effects happen without a pause the device answered;
- anything before the device has proved it holds a paired key.

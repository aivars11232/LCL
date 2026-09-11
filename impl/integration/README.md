# `.lcl` toolchain and desktop integration

What a system needs in order to recognize an LCL document, and what an editor
needs in order to colour one. Both are product conventions. Neither changes any
language rule, and neither is installed by building the workspace.

## The boundary this directory stays inside

LCL Core 0.1.0 registers `format.lcl` as "A document conforming to an exact LCL
version" and says nothing about file names, extensions or media types. A
document's meaning comes from its bytes and the version it declares. So:

- the `.lcl` and `.lcl.txt` endings are conventions, and the toolchain never
  uses either to decide anything — `lcl check` reads whatever path it is given,
  whatever it is called;
- `text/x-lcl` is this product's media type, chosen because no registered one
  exists;
- syntax metadata is for highlighting, not for validation. An editor that
  coloured a keyword correctly has learned nothing about whether the document
  compiles. For that it runs `lcl check --machine` and reads the diagnostics.

## Desktop registration (Linux)

`linux/lcl.xml` is a shared-mime-info package defining `text/x-lcl`, with globs
for `*.lcl` and `*.lcl.txt` and a magic rule matching the first four bytes.

`*.lcl.txt` is the ending a newly created document is given, so a document can
be shared and edited anywhere plain text is. It is recognised **in addition to**
`*.lcl`, never instead of it, and it changes no language rule: both are read,
checked and run by the same engine under the same contracts, and ending a name
in `.txt` never relaxes validation.

Ordinary text files are unaffected. Only the exact two-part ending is claimed,
no glob for `*.txt` is declared, and `text/plain` is not modified. Where the two
overlap the specification resolves by the longer pattern, so `notes.lcl.txt` is
an LCL document and `notes.txt` is not — including a `notes.txt` whose bytes
happen to begin with `LCL:`, because the glob decides before the magic rule is
consulted. The magic rule is
grounded rather than guessed: `04_GRAMMAR/01` requires that "Every document
starts with LCL then SPECIFICATION" and `02_LEXICAL/01` requires UTF-8 with no
byte-order mark, so every conforming document begins with exactly `LCL:`.

Install it for your own user:

```sh
cd integration/linux
./install.sh
```

It writes one file into `${XDG_DATA_HOME:-$HOME/.local/share}/mime/packages` and
refreshes that database. Nothing system-wide, no elevation, nothing outside the
two paths it prints. `./uninstall.sh` reverses it exactly.

Verify:

```sh
xdg-mime query filetype some-document.lcl       # text/x-lcl
xdg-mime query filetype some-document.lcl.txt   # text/x-lcl
xdg-mime query filetype ordinary-notes.txt      # text/plain
```

On Arch Linux, `update-mime-database` comes from `shared-mime-info`.

A `.desktop` entry lives in `packaging/` rather than here, and is installed by
`packaging/install.sh` together with the binaries it launches. This directory
registers the media type on its own, for a machine that wants the type without
the application.

## Syntax metadata

```sh
lcl syntax --machine
```

Emits the closed vocabulary as JSON: reserved words, callables, block names,
type words, literal words, adopted symbols, excluded lexemes, plus the source
rules an editor needs in order to avoid writing a file the lexer will refuse —
UTF-8 without a BOM, line feed only, a required final line feed, four-space
indentation, no tabs, no trailing space.

Every list is read from the canonical registries through the loaded lexicon,
which refuses to load unless the package is authoritative. That is the point of
generating it rather than checking one in: the workspace's first design rule is
"No transcription. No registry table is written into Rust source", and a
checked-in syntax file would be exactly that transcription, drifting the first
time a registry moved. A test asserts that the metadata's counts equal the
lexicon's own.

To generate an editor's syntax file, run the command and transform its output.
Do not hand-maintain the word lists.

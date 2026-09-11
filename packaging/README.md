# LCL, packaged

Two binaries, the specification package they load, a desktop entry and a media
type. Everything installs under your own home directory.

## Install

```sh
./install.sh
```

It needs no elevation, writes nothing system-wide, and prints every path it
touches. `./uninstall.sh` reverses it exactly.

## The one thing to know

Every command needs a specification package, and **nothing searches for one**.
`05_SEMANTICS/02` puts it plainly: "Ambient current directory and implied
nearby files do not exist in portable LCL." So the package travels with the
binaries and you name it, once:

```sh
export LCL_SPEC=~/.local/share/lcl/LCL_Core_0.1.0
```

or per command with `--spec`, or per project in `lcl.project.json`.

## Use

```sh
lcl help                      # commands, options and exit codes
lcl version                   # tool, engine protocol and language versions
lcl spec                      # the package identity and whether it is authoritative
lcl check   src/main.lcl      # canonical steps 1 to 5
lcl validate src/main.lcl     # steps 1 to 9, before any effect
lcl run     src/main.lcl      # steps 1 to 13, one terminal status
lcl run --allow-write /tmp/out src/main.lcl    # the same, with one capability
lcl check --machine src/main.lcl               # the JSON a tool consumes
lcl-workspace my-project --open                # the editor and debugger
```

A run is granted nothing unless a flag says so. A document that asks for the
world against a tool that was granted nothing gets a refusal, not a surprise.

## Exit codes

| Code | Meaning |
| --- | --- |
| 0 | the requested work completed, and for `run` the terminal status was `status.succeeded` |
| 1 | the document was rejected by a diagnostic |
| 2 | the document ran to completion with a non-success terminal status |
| 3 | the command line was not usable |
| 4 | the environment was not usable: no package, an unreadable document, a project that would not open |

`1` and `2` are deliberately different. A program that failed its own `VERIFY`
ran correctly and did not succeed, which `05_SEMANTICS/10` treats as an
ordinary outcome rather than an error.

## Building this yourself

```sh
packaging/build_release.sh
```

Builds `--release --offline --locked`, stages the payload, writes the tarball,
its checksum, and a provenance record naming the source commit, the toolchain,
the package identity and the checksum of every file in the payload.

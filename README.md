# LCL

This repository holds the LCL language specification, its implementation, and
the evidence that ties the two together. This note says which trees define the
language and which are history.

## Language authority

Only the packages under `canonical/` define LCL:

| Package | Status | Identity digest |
|---|---|---|
| `canonical/LCL_Core_0.1.0/` | Active Core 0.1.0 authority, protected and never modified | `00d648b162939d06c44838481a67c39bc12c64bdd6d105035c24150148fe67ed` |
| `canonical/LCL_Core_0.2.0/` | Unreleased localization candidate, bare specification complete: the owner accepted its independent review on 2026-09-25 (reviewed identity `00daee8de1919c4945ef04ff65edb22164bd8046a493be08a87d5fa3b4c3e604`), `release_gate_permitted` true; not released, no archive | `061a79c92c76ed7bb05968adde24b016cae3b30666b8fb289c6ab968907e8b3f` |

Inside each package, `00_RELEASE/02_NORMATIVE_AUTHORITY_ORDER.txt` decides how
its artifacts rank. Nothing outside a package root is a normative input to the
language. The implementation verifies each package against the external trust
anchor compiled into `impl/crates/lcl-spec/src/anchor.rs` before it judges a
document.

## Historical material at the repository root

These files and trees are kept unchanged as lineage evidence. They are **not**
language authority, and nothing in the implementation reads them:

- `LCL_Core_0_1_Final/` is the extracted pre-repair base. It is not the
  canonical 0.1.0 package: `canonical/LCL_Core_0.1.0/` was repaired from it and
  differs from it. `canonical/LCL_Core_0.1.0/00_RELEASE/00_CANONICAL_SOURCE_AND_PROVENANCE.txt`
  records the lineage.
- `LCL_Core_0_1_Final_SHA256.txt`, `LCL_Core_0_1_Final_VALIDATION.json` and
  `LCL_Core_0_1_Final_VALIDATION(1).txt` describe 126-file ZIP and TAR candidate
  archives whose bytes are not present. They validate neither tree.
- `LC language structure tree.txt` and
  `LC_Language_Word_and_Symbol_Audit_v0.1.txt` are supporting research and
  source material.

## Everything else

- `impl/` is the Rust implementation. It consumes `canonical/` and never writes
  to it.
- `apps/` holds example LCL projects, and `packaging/` holds the installer and
  release scripts.
- `android/` is LCL for Android, a native app that works on a paired PC's
  projects through that PC's engine, and `remote/` is `lcl-remote`, the PC
  service it pairs with. See `android/README.md`.
- `releases/` holds built artifacts. Candidates under `releases/candidates/` are
  evidence of the source state recorded in each one's provenance, not of the
  current tree.
- `reports/` holds task and validation reports.

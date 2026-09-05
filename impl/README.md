# LCL implementation — milestones M0 (foundation) and M1 (lexer)

This directory is a **consumer** of the canonical specification at
`../canonical/LCL_Core_0.1.0`. It is not part of the release, is not listed in
`MANIFEST.json` or `SHA256SUMS.txt`, and never writes to `canonical/`.

Building this does not change the release's status. The `complete_example_parse_matrix`
and `semantic_case_execution` gates remain `OUT_OF_SCOPE` for LCL Core 0.1.0;
implementation conformance is separate evidence, and M1 produces **lexical**
evidence only.

## Crates

| Crate | Milestone | Role |
| --- | --- | --- |
| `lcl-spec` | M0 | Specification authority loader. Verifies package integrity against `MANIFEST.json` and `SHA256SUMS.txt`, checks an **external trust anchor**, pins the formal version, loads the 12 closed registries and 2 catalogs as data. |
| `lcl-diagnostics` | M0 | Diagnostics skeleton. The 7 normative stages, 12 statuses and 77 errors, loaded and closure-checked. |
| `lcl-conformance` | M0 | Conformance skeleton. Indexes the 799 descriptive requirements and 66 decision witnesses. Executes nothing. |
| `lcl-lexer` | M1 | Deterministic, non-executing lexer. Source bytes in; tokens with exact byte spans or stable-ordered registered lexical diagnostics out. |

## Trust boundary (M0.1)

`MANIFEST.json` and `SHA256SUMS.txt` live inside the package they describe, so
verifying against them proves only **internal self-consistency**. Anyone who
alters a payload file and regenerates both records produces a package that
verifies perfectly and is not the approved release.

`lcl-spec::APPROVED_PACKAGE` closes that gap. It is an immutable anchor
compiled into this crate — outside the package, where the package's own
metadata cannot reach it — holding the expected **identity digest**:

```text
SHA-256( "LCL-PACKAGE-IDENTITY-V1\n"
         || for each file, in ascending package-root-relative path order:
            "<sha256 hex>  <path>\n" )
```

All 176 files are covered, `MANIFEST.json`, `VALIDATION_REPORT.txt` and
`SHA256SUMS.txt` included: nothing is self-excluded, because the digest is not
stored in the package.

Approved identity for `canonical/LCL_Core_0.1.0`:
`00d648b162939d06c44838481a67c39bc12c64bdd6d105035c24150148fe67ed` (176 files).

| Entry point | Gates | Result |
| --- | --- | --- |
| `SpecPackage::open` | version pin + internal integrity + trust anchor | `Authority::Authoritative` |
| `SpecPackage::open_with_anchor` | same, against a supplied anchor | `Authority::Authoritative` |
| `SpecPackage::open_unverified` | none enforced; integrity computed and reported | `Authority::Unverified` — **never normative input** |

Internal verification is preserved, not replaced, and still runs first.
Approving a different package is a source change to `anchor.rs`, subject to
review; `mint_anchor` computes a candidate digest but never writes one.

## Design rules

1. **No transcription.** No registry table is written into Rust source. Every
   keyword, symbol, error, status and requirement is read from the canonical
   registries at load time. The two named exceptions are the `Stage` enum (M0)
   and the `LexicalError` enum (M1); both are *validated against* the registry
   at load rather than trusted, and a registry that disagrees refuses to load.
2. **Fail closed.** Any hash mismatch, size mismatch, missing file, unrecorded
   file, count disagreement, version mismatch, or unverified package refuses
   the load. `Lexicon::load` accepts only an `Authoritative` package.
3. **No third-party dependencies.** The trust root carries no supply-chain
   surface; SHA-256 and the JSON reader are implemented in `lcl-spec`. The
   lexer is `std` only as well.
4. **No false claims.** `lcl-conformance` has no `Pass` state and no `run()`.
   `lcl-lexer`'s `Outcome::Tokenized` means "no lexical diagnostic", never
   "document accepted".
5. **Authority is explicit.** `Authority` is a two-state enum, not a boolean
   buried in a struct, so an unverified package cannot be quietly mistaken for
   the approved release.

## M1 — `lcl-lexer`

### API

```rust
let spec    = lcl_spec::SpecPackage::open("../canonical/LCL_Core_0.1.0")?; // Authoritative or error
let lexicon = lcl_lexer::Lexicon::load(&spec)?;                             // vocabulary from the registries
let lexer   = lcl_lexer::Lexer::new(&lexicon);
let lexed   = lexer.lex(bytes);                                             // total: never panics

lexed.tokens()          // &[Token]        — kind, exact byte Span, decoded string value, case fold
lexed.diagnostics()     // &[Diagnostic]   — in the registry's stable_order
lexed.primary()         // first diagnostic in stable_order (primary_rule)
lexed.outcome()         // Outcome::Tokenized | Outcome::Rejected
lexed.terminal_status() // registered default_status of the primary, e.g. "status.invalid"
lexed.lexeme(&token)    // the token's exact source bytes
```

`TokenKind` is exactly the EBNF's terminals: `RESERVED_WORD`,
`SIMPLE_IDENTIFIER`, `QUALIFIED_IDENTIFIER`, `INTEGER_LITERAL`,
`DECIMAL_LITERAL`, `STRING`, `MULTILINE_STRING`, adopted `SYMBOL`, `SPACE`
(one per space, as the grammar counts them), `NEWLINE`, `BLANK_LINE`, `INDENT`,
`DEDENT`, `EOF`. Byte offsets are zero-based against the caller's own buffer
(BOM included), per `diagnostic_selection.location_rule`; line/column on a
`Diagnostic` is derived presentation only.

**Invariant:** a lexeme yields either one token or diagnostics, never both. A
token never overlaps a diagnostic's bytes. Zero-width diagnostics (absent final
LINE FEED, empty block) name a position and withdraw nothing.

### Rules implemented, by normative source

| Source | Rules |
| --- | --- |
| `02_LEXICAL/01` | UTF-8 only (`error.encoding.invalid`); BOM (`error.source.bom`); LF the only terminator (`error.newline.invalid` for CR); TAB, DEL, C0/C1 controls prohibited **as raw source, inside strings too** (`error.source.tab`, `error.source.control_character`); non-ASCII only inside strings (`error.source.non_ascii_outside_string`); no trailing SPACE (`error.source.trailing_space`); required final LF at the EOF offset (`error.source.final_line_feed`). |
| `02_LEXICAL/02` | 4-space levels (`error.indentation.width`); at most +1 level (`error.indentation.jump`); +1 only after a block-opening `:` or MULTILINE_COLLECTION `[` (`error.indentation.invalid`); dedent closes every deeper block; empty blocks (`error.indentation.empty_block`, at the first following non-blank line or EOF); BLANK_LINE emits no structure and is emitted **after** the DEDENTs the grammar puts before it; longest reserved word / symbol; unregistered uppercase run is one token (`error.keyword.unknown`); maximal lowercase identifiers with dot-joined segments; `REF(a).b` yields `.` then an identifier. |
| `02_LEXICAL/02` (case) | `error.keyword.case` for mixed-case spellings of registered words, and for lowercase spellings in the two lexically decidable syntax-required positions: a key at indentation level 0 (object data cannot occur there) and a registered callable immediately before `(`. Lowercase spellings elsewhere remain identifiers, with `Token::case_folds_to` recording the fold for the parser. |
| `02_LEXICAL/03` | `[a-z][a-z0-9_]*` and qualified forms; anything else `error.identifier.invalid`. |
| `02_LEXICAL/04` | All 141 reserved words, from `keywords_v0.1.0.json`, cross-checked in tests against the EBNF's `RESERVED_WORD` and `CALLABLE`. |
| `02_LEXICAL/07` | `0\|[1-9][0-9]*`, `(0\|[1-9][0-9]*)\.[0-9]+`; sign is a separate token; anything else `error.literal.invalid`. Strings decode exactly; all six escapes; `\uHHHH` with the normative surrogate-pair formula; unpaired/invalid/unsupported escapes `error.literal.escape`; unclosed at LF/EOF `error.literal.unclosed`. Multiline strings: opening `"""` + LF, content prefix of declaration indentation + 4 (stripped exactly, extras preserved), blank content line = one LF, closer alone at declaration indentation, no structural tokens inside; misaligned/mis-prefixed forms `error.literal.invalid`. |
| `02_LEXICAL/08`, `/12` | Bracket/comma data forms lex as adopted symbols; no comment syntax (`#`, `//`, `/* */` are excluded symbols); COMMENT is an ordinary block. |
| `02_LEXICAL/09`, `/10` + `symbols_v0.1.0.json` | 21 adopted symbols; 23 excluded exact lexemes reported whole (`error.symbol.invalid`) under the registry's longest-lexeme `selection_rule`; the `xml_tag` notation pattern; bare `\` outside a string `error.lexical.malformed_token`; `@` (in neither inventory) `error.lexical.unknown_symbol`; `(`/`[` pairing (`error.delimiter.mismatch`, `error.delimiter.unclosed`). |
| `diagnostic_selection` | Supersession (transitive, same locus and cause), duplicate suppression on `duplicate_key`, `stable_order` (offset, specificity rank, identifier), `primary_rule`; ranks and edges read from the registry. |

### Diagnostics

All **23** registered `stage: lexical` identifiers are emitted, and `Lexicon::load`
fails if the registry's lexical set ever differs from the enum. Every
diagnostic carries the registry's `meaning`, `default_status` and
`specificity_rank` verbatim.

Recovery is chosen so one defect stays one diagnostic (a width error adopts the
licensed level rather than also reporting an empty block; an unclosed string
ends at its LF; a misaligned multiline closer ends the literal) while
independent defects are all reported, per `multiplicity_rule`.

### Explicitly deferred to the parser (M2)

* `error.keyword.case` in the two positions that need context the lexer does not
  have — field keys inside blocks (object data vs. schema key) and required
  connectors/operators/type positions. The lexer hands over `case_folds_to`.
* Constructor value-domain checks registered as `error.literal.invalid` (for
  example REGEX flag order in `08_EXAMPLES/INVALID/17_REGEX_FLAGS`). Those
  tokens are well-formed; the rule belongs to the constructor layer.

### Proof

* `09_CONFORMANCE/SOURCE_FIXTURES`: all 15 primaries equal `expected_results.json`,
  with exact diagnostic lists and spans pinned.
* `08_EXAMPLES/VALID`: all 13 tokenize with no diagnostic.
* `08_EXAMPLES/INVALID`: the 7 token-formation cases produce the pinned primary
  and `status.invalid`; the other 14 lex clean, as `earliest_stage_rule` requires
  for a later-stage expectation.
* 52 rule-by-rule tests; 9 authority tests (registry ↔ EBNF cross-checks,
  unverified package refused); 9 totality tests (every byte, every byte pair,
  seeded random corpora, all 176 canonical files, every prefix/suffix and
  single-byte mutation of every fixture, deep nesting, 100k-byte runs).

## Not yet implemented

No parser, AST, resolver, type checker, evaluator, capability kernel, runtime,
CLI or UI. M2 has not been started. The diagnostic *selection* algorithm beyond
one source-validation run (`expression_demand_resolution`, producer paths,
iteration and retry indexes) remains data, not code.

## Use

```bash
cargo test --offline                                          # 134 tests (51 M0 + 83 M1)
cargo run --offline -p lcl-lexer --example m1_report          # lexer report over all canonical inputs
cargo run --offline -p lcl-conformance --example m0_report    # foundation report
cargo run --offline -p lcl-spec --example mint_anchor         # compute a package identity
cargo clippy --offline --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

All are read-only with respect to `canonical/`. Integrity tests that need to
mutate a package operate on a throwaway copy under `target/test-tmp/`.

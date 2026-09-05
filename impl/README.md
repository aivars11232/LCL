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
| `lcl-lexer` | M1 | Deterministic, non-executing lexer. Source bytes in; tokens with exact byte spans or stable-ordered registered lexical diagnostics out, including every contextual `error.keyword.case` position and the closed literal profiles of constructor arguments. |

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

### Bounded token context

Several lexical rules of `02_LEXICAL/02` need context. The scanner keeps
exactly that context itself — it parses nothing into a tree and resolves no
name:

* **one context per indentation level** — top level / conditional or loop body
  / nested schema (*structural*), a registered block by name, lowercase-key
  *object data*, MULTILINE_COLLECTION members, or *indeterminate* (see below).
  A `KEY:` opener whose `(enclosing block, KEY)` signature in
  `field_signatures_v0.1.0.json` is `value_or_object_expression` opens object
  data; a lowercase key inside object data opens nested object data; a
  registered block name opens that block; everything else is structural;
* **the lexeme classes of the current line**, so "after a complete operand and
  a SPACE" is known;
* **the line's key** and the open `LIST[`/`SET[`/`OBJECT[`/`REFERENCE[` and
  `REF(` brackets, so type positions are known;
* **how the previous line ended**: a block-opening `:` or `[` that reached the
  LINE FEED across spaces only (`Opener`), no opener (`Plain`), or an opener
  followed by a byte already rejected as raw source (`Indeterminate`). An
  indeterminate line end neither demands nor forbids a child block, so a TAB,
  CARRIAGE RETURN or control character after a colon raises only its own
  raw-source diagnostic and never a fabricated `error.indentation.empty_block`
  or `error.indentation.invalid`. Trailing spaces after an opener keep the
  structural reading (they are already `error.source.trailing_space`).

### Rules implemented, by normative source

| Source | Rules |
| --- | --- |
| `02_LEXICAL/01` | UTF-8 only (`error.encoding.invalid`); BOM (`error.source.bom`); LF the only terminator (`error.newline.invalid` for CR); TAB, DEL, C0/C1 controls prohibited **as raw source, inside strings too** (`error.source.tab`, `error.source.control_character`); non-ASCII only inside strings (`error.source.non_ascii_outside_string`); no trailing SPACE (`error.source.trailing_space`); required final LF at the EOF offset (`error.source.final_line_feed`). |
| `02_LEXICAL/02` | 4-space levels (`error.indentation.width`); at most +1 level (`error.indentation.jump`); +1 only after a block-opening `:` or MULTILINE_COLLECTION `[` (`error.indentation.invalid`); dedent closes every deeper block; empty blocks (`error.indentation.empty_block`, at the first following non-blank line or EOF); BLANK_LINE emits no structure and is emitted **after** the DEDENTs the grammar puts before it; longest reserved word / symbol; unregistered uppercase run is one token (`error.keyword.unknown`); maximal lowercase identifiers with dot-joined segments; `REF(a).b` yields `.` then an identifier. |
| `02_LEXICAL/02` (case) | `error.keyword.case` for a mixed-case spelling of a registered word anywhere, and for a lowercase spelling in **every** syntax-required position: a block or field key outside object data (the head of a structural line, where the grammar admits only a reserved word — `id:`, `else:`, `for`, `if`); a required connector or operator (after a complete operand and a SPACE — `TRUE and FALSE`, `IF (…) then:`, `FOR EACH x in …`); a registered callable immediately before `(`; a built-in type position (the inline value of a `type_expression` / `type_or_format_base` field such as `TYPE:` or `BASE:`, and a type argument inside `LIST[`…`]`, outside `REF(…)`). Everywhere else a lowercase spelling remains a legal identifier — object-data keys such as `status:`, enum members, collection members, `FIELD.NAME` — with `Token::case_folds_to` recording the near-miss as information. |
| `02_LEXICAL/03` | `[a-z][a-z0-9_]*` and qualified forms; anything else `error.identifier.invalid`. |
| `02_LEXICAL/04` | All 141 reserved words, from `keywords_v0.1.0.json`, cross-checked in tests against the EBNF's `RESERVED_WORD`, `CALLABLE` and `BLOCK_WORD`. |
| `02_LEXICAL/07` | `0\|[1-9][0-9]*`, `(0\|[1-9][0-9]*)\.[0-9]+`; sign is a separate token; anything else `error.literal.invalid`. Strings decode exactly; all six escapes; `\uHHHH` with the normative surrogate-pair formula; unpaired/invalid/unsupported escapes `error.literal.escape`; unclosed at LF/EOF `error.literal.unclosed`. Multiline strings: opening `"""` + LF, content prefix of declaration indentation + 4 (stripped exactly, extras preserved), blank content line = one LF, closer alone at declaration indentation, no structural tokens inside; misaligned/mis-prefixed forms `error.literal.invalid`. |
| `03_TYPES_AND_VALUES/04`, `/07`; `types_v0.1.0.json#/pattern_profiles`, `#/temporal_literal_contract`; `operators_and_functions_v0.1.0.json#/constructors` | Literal `STRING` arguments of the constructors whose closed profile names `error.literal.invalid`: **REGEX** flags (only the registry's `i`, `m`, `s`; each at most once; canonical `ims` order; omitted = empty) and the frozen REGEX grammar (escapes, classes, ranges, groups, quantifiers, assertions); **GLOB** (segments, `**` only as a whole segment, escapes, classes); **DATE**, **TIME**, **DATETIME** (the exact temporal profile, leap years, no leap second, no `-00:00`); **URI** (RFC 3986 absolute-URI with a scheme, no fragment, per-component character sets and percent-encoding). Only a call whose arity is a registered all-`STRING` overload and whose arguments are all `STRING` tokens is judged; a `REF` or expression argument is execution-stage material and a wrong arity is `error.operator.operand`. `PATH` has no closed literal profile and is not judged. |
| `02_LEXICAL/08`, `/12` | Bracket/comma data forms lex as adopted symbols; no comment syntax (`#`, `//`, `/* */` are excluded symbols); COMMENT is an ordinary block. |
| `02_LEXICAL/09`, `/10` + `symbols_v0.1.0.json` | 21 adopted symbols; 23 excluded exact lexemes reported whole (`error.symbol.invalid`) under the registry's longest-lexeme `selection_rule`; the `xml_tag` notation pattern; bare `\` outside a string `error.lexical.malformed_token`; `@` (in neither inventory) `error.lexical.unknown_symbol`; `(`/`[` pairing (`error.delimiter.mismatch`, `error.delimiter.unclosed`). |
| `diagnostic_selection` | Supersession (transitive, same locus and cause), duplicate suppression on `duplicate_key`, `stable_order` (offset, specificity rank, identifier), `primary_rule`; ranks and edges read from the registry. |

### Diagnostics

All **23** registered `stage: lexical` identifiers are emitted, and `Lexicon::load`
fails if the registry's lexical set ever differs from the enum. Every
diagnostic carries the registry's `meaning`, `default_status` and
`specificity_rank` verbatim. Nothing the registry stages as lexical is
deferred to a later milestone.

Recovery is chosen so one defect stays one diagnostic (a width error adopts the
licensed level rather than also reporting an empty block; an unclosed string
ends at its LF; a misaligned multiline closer ends the literal; a corrupted
opener line is indeterminate) while independent defects are all reported, per
`multiplicity_rule`.

### Proof

* `09_CONFORMANCE/SOURCE_FIXTURES`: all 15 primaries equal `expected_results.json`,
  with exact diagnostic lists and spans pinned.
* `08_EXAMPLES/VALID`: all 13 tokenize with no diagnostic.
* `08_EXAMPLES/INVALID`: all 8 lexical-stage expectations — including
  `17_REGEX_FLAGS` on its `"mi"` literal — produce the pinned primary and
  `status.invalid`, with no filename or diagnostic exemption; the other 13
  lex clean, as `earliest_stage_rule` requires for a later-stage expectation.
* 56 rule-by-rule tests (every `error.keyword.case` position with exact
  spans, plus negatives for object-data keys and identifiers that spell
  keywords; the indeterminate line-end regressions with complete diagnostic
  lists); 15 constructor-literal tests; 15 authority tests (registry ↔ EBNF
  cross-checks, independent re-derivation of the object-data and type-position
  signatures, the REGEX grammar and flag contract, unverified package refused);
  9 totality tests (every byte, every byte pair, seeded random corpora, all 176
  canonical files, every prefix/suffix and single-byte mutation of every
  fixture, deep nesting, 100k-byte runs).

## Not yet implemented

No parser, AST, resolver, type checker, evaluator, capability kernel, runtime,
CLI or UI. M2 has not been started. Value-domain checks on dynamically supplied
constructor arguments, and the diagnostic *selection* algorithm beyond one
source-validation run (`expression_demand_resolution`, producer paths,
iteration and retry indexes), remain data, not code.

## Use

```bash
cargo test --offline --workspace --all-targets                # 159 tests (51 M0 + 108 M1)
cargo run --offline -p lcl-lexer --example m1_report          # lexer report over all canonical inputs
cargo run --offline -p lcl-conformance --example m0_report    # foundation report
cargo run --offline -p lcl-spec --example mint_anchor         # compute a package identity
cargo clippy --offline --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

All are read-only with respect to `canonical/`. Integrity tests that need to
mutate a package operate on a throwaway copy under `target/test-tmp/`.

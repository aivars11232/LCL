# LCL implementation — milestones M0 (foundation), M1 (lexer), M2 (parser), M3 (resolver), M4 (checker), M5 (semantic preflight), M6 (runtime), M7 (capabilities and standard library), M8 (completion and executable conformance), M9 (CLI, projects and the engine protocol), M10 (the workspace, inspector and debugger) and M11 (hardening, the application ladder and release)

This directory is a **consumer** of the canonical specification at
`../canonical/LCL_Core_0.1.0`. It is not part of the release, is not listed in
`MANIFEST.json` or `SHA256SUMS.txt`, and never writes to `canonical/`.

Building this does not change the release's status. The `complete_example_parse_matrix`
and `semantic_case_execution` gates remain `OUT_OF_SCOPE` for LCL Core 0.1.0, because the
bare-language package ships no engine. Both classifications are correct and unchanged.
What this workspace supplies is the executed implementation evidence they defer to:
`lcl-parser`'s `tests/parse_matrix.rs` for the first, and `lcl-conformance`'s
`tests/decision_witnesses.rs` for the second. Implementation conformance is separate
evidence and is never a claim about the release.

## Crates

| Crate | Milestone | Role |
| --- | --- | --- |
| `lcl-spec` | M0 | Specification authority loader. Verifies package integrity against `MANIFEST.json` and `SHA256SUMS.txt`, checks an **external trust anchor**, pins the formal version, loads the 12 closed registries and 2 catalogs as data. |
| `lcl-diagnostics` | M0 | Diagnostics skeleton. The 7 normative stages, 12 statuses and 77 errors, loaded and closure-checked. |
| `lcl-conformance` | M0 + M8 | The descriptive index **and** the executable case runner. Indexes the 799 descriptive requirements and 66 decision witnesses, and separately carries concrete sources through every implemented stage, comparing observed against expected. An indexed requirement and an executed case are different types with different evidence standards, and the report counts them in separate columns with no total. |
| `lcl-lexer` | M1 | Deterministic, non-executing lexer. Source bytes in; tokens with exact byte spans or stable-ordered registered lexical diagnostics out, including every contextual `error.keyword.case` position and the closed literal profiles of constructor arguments. |
| `lcl-parser` | M2 | Deterministic, non-executing parser. M1 tokens in; a source-faithful syntax tree with exact byte spans or registered grammar-and-schema diagnostics out. |
| `lcl-resolver` | M3 | Deterministic, non-executing resolver. Parsed units plus an explicit source provider in; version, import, extension, namespace, ID and `REF` bindings and the structural candidate graph, or registered resolution diagnostics, out. |
| `lcl-checker` | M4 | Deterministic, non-executing static and type checker. A resolved program graph in; every declaration's type, every expression's static contract and the value obligations left to the demanding layer, or registered `static_or_expression` diagnostics, out. |
| `lcl-semantics` | M5 | Deterministic, no-effect semantic preflight. A statically checked program plus explicit invocation data in; an authorized, dependency-resolved, prevalidated and ordered execution plan, or registered pre-effect diagnostics, out. |
| `lcl-completion` | M8 | Canonical processing steps 11 to 13. An accepted execution in; post-execution `VERIFY` and `TEST` against what was actually observed, resolved `EVIDENCE`, a `SUCCESS`/`FAILURE` decision, exactly one terminal invocation status and the declared outputs, or registered `verification_or_completion` diagnostics, out. It performs no effect of its own. |
| `lcl-runtime` | M6 | Deterministic evaluator and runtime. An accepted execution plan plus an explicit host capability boundary in; canonical execution events, invocation results, state and diagnostics out. Every external effect leaves the language through `Host`; the runtime core performs no filesystem, process, network, provider or clock access. |
| `lcl-protocol` | M9 | The stable headless engine surface. One assembled engine that carries source through every canonical stage in order and stops where the requested command says, and one machine-readable record of what happened, with a JSON projection. It resolves nothing, checks nothing and classifies nothing: every identifier, stage, status, span and value is copied from the layer that decided it. |
| `lcl-project` | M9 | Projects and documents. An explicit project root, a filesystem source provider that can answer only for a source a document named, root-relative source identity that does not depend on where the project lives, and the content-addressed cache and lock file that make a multi-document project reproducible. |
| `lcl-cli` | M9 | The `lcl` binary. `check`, `validate`, `run`, `inspect`, `package` and `syntax` over the same engine any other consumer uses, with a closed exit-code table, human rendering and `--machine` JSON. It grants the host nothing unless a flag says so. |
| `lcl-hardening` | M11 | The release gate. A deterministic seeded generator, mutation and adversarial corpora, the cross-layer invariants every stage must hold under hostile input, the performance and repeatability measurements, the product-level security matrix, the staged application ladder and the packaging smoke test. It defines no language rule and nothing depends on it: it drives the same public surfaces a user drives. |
| `lcl-workspace` | M10 | The editor, project shell, live diagnostics, execution inspection and debugger. A loopback HTTP server on `std::net` serving a hand-written browser frontend, over the same engine the CLI uses. It holds no language rule of its own, and the page does not even highlight: token spans come from the real lexer. |

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
4. **No false claims.** `lcl-conformance`'s indexed `CaseState` still has one
   variant, `NotExecuted`, so a catalog entry can never carry a verdict; an
   executed case is a separate type that cannot be built without the exact
   source that ran and the result the engine gave. `lcl-lexer`'s
   `Outcome::Tokenized` means "no lexical diagnostic", never "document
   accepted", and `lcl-completion`'s `succeeded()` means one terminal status
   was `status.succeeded`, never that every declared check passed.
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

### M2 stack safety

Every construct in the grammar nests without bound, and canonical LCL Core
0.1.0 declares **no** maximum nesting depth for source syntax:
`04_GRAMMAR/02` and the EBNF state the shape and no bound, and the only
`maximum_depth` in the registries belongs to `contract_type_notation`, which
says in terms that "This registry notation does not extend LCL source syntax".
No registered diagnostic permits an implementation-defined nesting rejection
either — the twelve `grammar_or_schema` errors are all structural-shape
defects, and `error.pattern.resource_limit` is explicitly about `GLOB` and
`REGEX`. So the depth a document may reach is not this implementation's to cap,
and the repair could not be a limit.

The parser originally paid native stack per nesting level and died by
`SIGABRT` — not by a diagnostic — once a document exceeded it:

| Path | Old limit (2 MiB stack) | Cost per level |
| --- | --- | --- |
| `(((…)))` groups | 175 | ~13.0 KiB |
| `[[[…]]]` collections | 175 | ~13.0 KiB |
| nested call arguments | 175 | ~13.0 KiB |
| `NOT NOT …` chains | ~1,250 | ~1.7 KiB |
| nested indented blocks | ~350 | ~6 KiB |
| tree teardown (drop glue) | ~24,000 | ~85 B |

Four paths, one root cause. All four are now iterative:

* **`expr`** — the nine-function precedence cascade became one loop over an
  explicit `Frame` stack. Each frame holds what a native frame held: the
  operand and operator stacks of one expression level, its pending unary
  prefixes, and what to do when the level closes.
* **`block`** — the `block → indented_body → statements → statement →
  body_after_colon` cycle became a stack of open containers, driven by the
  `Indent` and `Dedent` tokens M1 already emits.
* **`schema`** — the post-parse walks over blocks, control forms and object
  data became a depth-first worklist. A nested body is now judged in place
  through an `Anchor` carrying its two diagnostic spans, so the synthetic
  `Block` the recursive version cloned per level is gone as well.
* **`syntax`** — `Expr`, `Statement` and `Executable` have hand-written `Drop`
  impls that dismantle a tree through a worklist. Without this the fix would
  have moved the abort from construction to teardown.

Depth now costs heap, which fails as an allocation rather than as an
unrecoverable abort. `tests/robustness.rs` proves it on a deliberately small
**256 KiB** stack — one eighth of the stack the old code died on — at 50,000
levels of groups, collections, call arguments and unary prefixes, 4,000 levels
of nested blocks, and the same depths for teardown, span exactness and
malformed input.

One consequence worth knowing: because `Expr`, `Statement` and `Executable`
implement `Drop`, they can no longer be destructured by value. Borrowing,
cloning and passing by value are unaffected, and every consumer in this
workspace already borrows.

## M3 — `lcl-resolver`

Canonical processing step 4: "Resolve exact LCL version, imports, extensions,
namespaces, IDs, and REF. Resolve structural candidate graph membership and
branch/loop templates from EXECUTE for check selection, without evaluating
dynamic conditions or starting actions."

### API

```rust
let rules    = lcl_resolver::Rules::load(&spec, &grammar)?;
let resolver = lcl_resolver::Resolver::new(&rules, &grammar, &lexicon);
let resolved = resolver.resolve(&root_unit, &provider)?;   // Err = an earlier stage failed

resolved.units()        // every loaded source unit, with its SHA-256 digest
resolved.paths()        // every unit reached by every exact import path
resolved.imports()      // each IMPORT/EXTENSION and what happened to it
resolved.declarations() // every declared identity, fully qualified
resolved.bindings()     // every REF and what it resolved to
resolved.graph()        // the structural candidate graph
resolved.diagnostics()  // registered resolution diagnostics, in stable_order
resolved.outcome()      // Resolved | Rejected
```

### The source boundary

`05_SEMANTICS/02` is categorical: "Ambient current directory and implied nearby
files do not exist in portable LCL." Every byte reaches the resolver through
`SourceProvider`, which has **one** method and no enumeration, listing, globbing,
search-path or default-unit capability — so a source the document did not name
has no interface through which to enter. The crate performs no I/O at all; the
report and the tests read files and hand bytes to an in-memory provider.

Identity is per *import path*, not per unit: "Distinct acyclic import paths do
not override one another", and nested imports "prepend each prefix in order", so
one library imported under two prefixes is loaded once and contributes two sets
of fully qualified IDs.

### Rules implemented, by normative source

| Source | Rules |
| --- | --- |
| `01_FOUNDATION/03` | Step 4 in full; stage monotonicity — a unit that failed the lexical or grammar stage has no resolution verdict, and the signature enforces it. |
| `07_VERSIONING_AND_EXTENSIONS/01`, `/05` | `LCL.VERSION` must equal the exact supported version, read from the `LCL.VERSION` block schema's own `exact_string` argument (`error.version.unsupported`). An unsupported version stops that unit resolving further rather than reinterpreting it. |
| `07_VERSIONING_AND_EXTENSIONS/02` | `IMPORT VERSION` must equal the imported `SPECIFICATION.VERSION` (`error.version.mismatch`); `sha256:<64 lowercase hex>` checksums verified against the exact loaded bytes (`error.import.checksum`); unresolvable sources (`error.import.not_found`); cycles (`error.import.cycle`); mandatory, lowercase, non-reserved namespaces (`error.namespace.invalid`); prefix ownership against other imports and against local declaration IDs (`error.id.duplicate`). |
| `07_VERSIONING_AND_EXTENSIONS/03` | An `EXTENSION` must load a `kind.extension` document that adds definitions only (`error.extension.invalid`); alias `BASE` chains resolve transitively and acyclically to one core identifier of the same domain. |
| `02_LEXICAL/03` | IDs unique within one namespace; the nine reserved built-in namespaces refused as a declaration's first segment; unqualified references resolve the local namespace only; loop-local bindings resolve inside their `FOR EACH` body and nowhere else. |
| `types_v0.1.0.json#/reference_context_contract`, `#/source_type_contract` | Every `REF` binds to exactly one declaration or loop-local binding (`error.reference.unresolved`); a reference in a slot with a registered target set must match it (`error.reference.kind`); the receiving context reaches through parentheses and reference-typed collection members but not into another operator, call, property or index access. |
| `field_signatures#/value_kind_registry` | `operation_identifier` binds to a core operation or a `DEFINE kind.operation` (`error.operation.undefined`). |
| `block_schemas#/execution_graph_contract`, `05_SEMANTICS/01` | The structural candidate graph from `EXECUTE`: TASK execution fields in source field order with reference lists expanded left to right, PHASE and SEQUENCE children in lexical order, the STEP arm, both `IF` branches and each `FOR EACH` body as templates; `TARGET`, `OUTPUT` reads, check references, `BEFORE` and `AFTER` activate nothing; structural cycles use `error.reference.cycle`. |

### Deliberately not decided here

Two registered `stage: resolution` identifiers are **not** emitted, because
deciding them needs effective authority and priority — step 6 of the processing
model, after this stage:

* `error.conflict.hard` — M5 semantic preflight;
* `error.override.invalid` — M5 semantic preflight. `OVERRIDE.WINNER` and
  `LOSER` are still bound here as `reference(rule_clause)` slots.

`error.execution.order` is registered at `stage: execution` and is likewise
decided before effects, by the M5 ordering layer. This milestone builds the node
set and child order that layer will order; it adds no ordering edges.

`lcl_resolver::DEFERRED` names both deferred identifiers with their owner, and a
test asserts no canonical input ever produces one.

### Proof

* `08_EXAMPLES/VALID`: all 13 resolve with no diagnostic, including the import
  of `02_IMPORT_LIBRARY.lcl` by `03_IMPORTING_TASK.lcl` through the provider.
* `08_EXAMPLES/INVALID`: all 21 consistent — 12 owned by an earlier stage, 3
  raised here (`error.id.duplicate`, `error.reference.unresolved`,
  `error.version.unsupported`), 1 deferred and reported as deferred, 5
  resolving cleanly as `earliest_stage_rule` requires.
* Determinism: a provider whose backing store enumerates in the opposite order
  produces a byte-identical fingerprint of units, paths, declarations,
  bindings, graph and diagnostics; repeated resolution is identical; units the
  document never names never load.
* Totality: sampled truncations and byte mutations of every valid example, plus
  adversarial shapes, resolve without panicking.

## M4 — `lcl-checker`

Canonical processing step 5: "Statically check value families, expression
names/arity/types, constructors, parameters, and schemas without demanding
deferred expression values."

### API

```rust
let contracts = lcl_checker::Contracts::load(&spec)?;
let checker   = lcl_checker::Checker::new(&contracts);
let checked   = checker.check(&resolved)?;   // Err = resolution did not succeed

checked.declaration_types()      // every declaration's exact static type
checked.annotations()            // every expression's static outcome, by exact span
checked.deferred()               // the value obligations the demanding layer owns
checked.diagnostics()            // registered static diagnostics, in stable_order
checked.earlier_stage_defects()  // registered defects whose own stage is earlier
checked.outcome()                // Checked | Rejected
```

### Type, then value

Each expression yields its static outcome and, **only when it is statically
knowable**, its value. A literal, a constructor over literals and a
`DEFINE kind.constant` built from them are statically known; a read of `INPUT`,
`OUTPUT`, `STATE`, `CONTEXT` or `MEMORY` is not, and none is guessed.

That split is what the canonical examples require in both directions. `1 / 3` is
`error.numeric.non_terminating` and `ROUND(1 / 0, 2)` is
`error.numeric.division_by_zero` *here*, at `status.invalid`, before any effect —
neither is decidable without the exact value. The same constraint over an operand
that arrives at demand is recorded as a `DemandObligation` instead, which is what
`expression_demand_resolution` describes. `INTEGER` and `DECIMAL` are unbounded,
so the arithmetic is exact and carries its own base-10 magnitude rather than a
machine number.

### Rules implemented, by normative source

| Source | Rules |
| --- | --- |
| `01_FOUNDATION/03` | Step 5 in full; stage monotonicity — a program that failed resolution has no static verdict, and the signature enforces it. A reference's declaration and static type are resolved without reading an `OUTPUT` before its producer binds it. |
| `03_TYPES_AND_VALUES/01`, `/02`, `/05` | One static type per value; no coercion outside exact INTEGER-to-DECIMAL promotion; transparent aliases resolved away; enum domains identified by their defining declaration; acyclic type resolution. |
| `03_TYPES_AND_VALUES/03`, `/10`, `types#/object_type_contract` | Structural object identity over field name, type and requiredness; closed schemas; combined `TYPE` + `SCHEMA` identity including defaults and constraints; bracket literals defaulting to `LIST` unless one exact `SET[T]` is expected; invariant member types; empty-literal context. |
| `03_TYPES_AND_VALUES/04`, `/06`, `/07`, `operators_and_functions#/constructors` | Every registered constructor overload, arity and operand family; registered units and their declared categories; `PERCENTAGE` and `BYTES` bounds; exact division, the terminating-quotient rule and `ROUND`'s single materialization; `MEASURE` unit preservation and the same-unit constraint. |
| `03_TYPES_AND_VALUES/09`, `types#/reference_context_contract` | `MISSING`, `UNKNOWN` and `NULL` placement; identity versus value contexts, including through parentheses and reference-typed collection members; declaration-property reads before any bound value; `VALIDATE`/`VERIFY` Boolean results; loop-local element types. |
| `05_SEMANTICS/12`, `operators_and_functions` | All 19 operators, 11 functions and 11 constructors from the registry: names, arity, operand families, result families, promotion, three-valued logic, quantifier arguments and postfix access. Both operands of a short-circuit and both arms of a conditional are checked. |
| `06_STANDARD_LIBRARY/10`, `operations` | All 39 operation contracts at `ACTION` and `HANDLER` invocation sites: required target with the handler-context exception, required, duplicate and unregistered named parameters — including a registered invocation field that *is* a named parameter, as `HANDLER.LIMIT` is `core.retry`'s `limit`. |
| `types#/pattern_profiles` | The closed `GLOB` and `REGEX` matching profiles, applied to statically known values under a declared `PATTERN`, with declared finite compilation and matching budgets. |

### Decided here under an earlier registered stage

A registered stage is a classification, not a schedule. Two defects need every
static type resolved before they can be seen, yet carry an identifier the
registry stages earlier. They are reported with **their own** identifier and
stage through `earlier_stage_defects()`, never as a static identifier:

* `error.reference.cycle` — a `kind.type` `BASE` chain that resolves to itself.
  M3 checks the four alias domains that resolve to a *core identifier*; a
  `kind.type` chain resolves to a declaration, so every `REF` in it binds and the
  resolution stage is clean.
* `error.literal.invalid` — a one-STRING relative `PATH` outside `IMPORT.SOURCE`
  and `EXTENSION.SOURCE`. M1 owns every other closed literal profile and
  deliberately left this one, because it "depends on the receiving field and on
  resolution".

No registered `static_or_expression` identifier is deferred by this milestone:
all twelve have a trigger this stage reaches.

### Deliberately not decided here

No authority, priority, override or conflict resolution; no runtime value
demand; no `VALIDATE` execution; no effect. A field value kind that names a
closed identifier domain — `FORMAT`, `ENCODING`, `MODE` and the like — is
checked where value kinds are checked, and `error.field.type` is the grammar
stage's identifier for it.

### Proof

* `08_EXAMPLES/VALID`: all 13 check with no static diagnostic, 751 expressions
  annotated.
* `08_EXAMPLES/INVALID`: all 21 consistent — 15 owned by an earlier stage, 5
  raised here (`error.type.mismatch` twice, `error.numeric.non_terminating`,
  `error.numeric.division_by_zero`, `error.operation.parameter`) each with its
  pinned identifier, locus and `status.invalid`, and 1 left to M5.
* Determinism: repeated and independently resolved checks produce a
  byte-identical fingerprint of annotations, types, obligations and diagnostics.
* Totality: sampled truncations of every valid example, deep collections,
  expressions and alias chains, and a 200-link type chain all return without
  panicking; a pattern shape that is catastrophic for a backtracking engine
  completes.
* 99 tests: registry authority, exact arithmetic, the two pattern profiles, the
  type model, expressions, constructors, operation contracts, references and
  special values.

## Not yet implemented

No capability kernel beyond the boundary and its deterministic mock, no
executable Core operation surface, no `VERIFY`/`TEST`/evidence/completion layer,
no CLI and no UI. M7 owns the first two, M8 the third, M9 the fourth.

### The M2 stack-safety defect, now closed

M3 recorded a known limitation here: `lcl_parser::Parser::parse` built its
syntax tree recursively, so an expression nested a few hundred levels deep
exhausted the stack instead of producing a diagnostic. That defect was repaired
under a corrective reopening of `LCL-TASK-0011`; see **M2 stack safety** below.
`lcl-resolver`'s own expression walk was already iterative and is unchanged.

## M5 — `lcl-semantics`

Canonical processing steps 6 through 9, as one no-effect pass:

> 6. Establish effective authority, priority, scope, condition contracts, and
>    conflicts; evaluate each condition only when its values are required.
> 7. Resolve input, state, memory, context, default, assumption, and
>    dependencies.
> 8. Run every selected and applicable VALIDATE check before side effects,
>    including optional checks and prerequisites.
> 9. Finalize and check ordering edges of that resolved candidate graph before
>    effects; no check reference or value read adds graph membership or edges.

### API

```rust
let contracts = lcl_semantics::Contracts::load(&spec)?;   // refuses an unverified package
let preflight = lcl_semantics::Preflight::new(&contracts);
let invocation = lcl_semantics::Invocation::new()
    .with("input.endpoint", lcl_semantics::Value::Text("...".into()));
let planned = preflight.plan(&checked, &resolved, &invocation)?; // Err = an earlier stage failed

planned.plan()                    // Option<&Plan> — None when rejected: nothing is authorized
planned.partial_plan()            // &Plan — for reports and tests, accepted or not
planned.diagnostics()             // &[Diagnostic] in the registry's stable order
planned.primary()                 // first in stable_order
planned.outcome()                 // Outcome::Planned | Outcome::Rejected
planned.terminal_status()         // registered default_status of the primary
planned.unused_invocation_data()  // supplied ids this document never declared
```

A `Plan` carries decisions, never expressions to re-derive them from: plan nodes
with their resolved mode, `REQUIRED` contract and exact `Authorization`;
ordering `Edge`s with the reason each exists; one stable topological `order`;
every `Resolution` with the step of the canonical order that produced it; every
selected `CheckResult`; the applied-assumption `EvidenceRecord`s; and every rule
clause's effective authority and priority.

### The effect boundary

This is the last layer before effects exist, so "no effect has happened yet" is
made structural rather than asserted:

* the crate depends on no capability, host or I/O interface and performs no
  filesystem, process, network or clock access;
* every external datum arrives through `Invocation`, which has no method to
  enumerate, search or default a value — the same closure `SourceProvider` puts
  around source bytes. "Ambient current directory and implied nearby files do
  not exist in portable LCL";
* every diagnostic carries `FailurePhase::PreEffect`, because no other phase is
  reachable from here.

### A mixed-stage layer

M1 to M4 each own exactly one registered stage. M5 does not, and the twelve
identifiers it mirrors carry four:

| Registered stage | Identifiers |
| --- | --- |
| `resolution` | `error.conflict.hard`, `error.override.invalid`, `error.reference.cycle` |
| `static_or_expression` | `error.value.out_of_range`, `error.value.unknown` |
| `validation` | `error.determinism.mismatch`, `error.validation.failed` |
| `execution` | `error.dependency.unsatisfied`, `error.execution.order`, `error.permission.denied`, `error.required.missing`, `error.scope.violation` |

A registered stage is a *classification*, not a schedule — the distinction M4
already made with `EarlierStageDefect`. `05_SEMANTICS/09` states the execution
group is pre-effect work ("error.dependency.unsatisfied and
error.scope.violation are pre_effect only"), and decision witness `CLOSURE-063`
states it for ordering. Every `Diagnostic` copies its stage from the registry,
so nothing downstream has to guess.

This closes M3's two named deferrals, `error.conflict.hard` and
`error.override.invalid`, and decides `error.execution.order` before effects as
M3's report said M5 would.

### Preflight demands keep their registered classification

`expression_demand_resolution` moves an eligible identifier to the execution
stage only for a demand *after* preflight, and its own context excludes this
layer: "Preflight-required expression demands retain registered
source-validation classification." So M5 never applies that map, and
`error.required.missing` and `error.value.unknown` carry `status.blocked` here
because that is their registered default, not because an override was applied.

### Rules implemented, by normative source

| Source | Rules |
| --- | --- |
| `05_SEMANTICS/03` | `REQUIRE`/`ALLOW`/`FORBID`/`PREFER`/`PRESERVE`; a reachable required `ACTION` authorizes exactly its declared operation, target and parameters; `ALLOW` never causes execution and never defeats `FORBID` alone; a preference never implies permission. |
| `05_SEMANTICS/04` | Effective authority 0..1000 with the document's own authority as a clause's default and the import ceiling applied to imported clauses; `PRIORITY` −1000..1000, defaulting to 0 and never inheriting; higher authority wins only where clauses conflict; equal hard contradictions are `error.conflict.hard` unless an exact `OVERRIDE` names winner and loser; a lower-authority winner is `error.override.invalid`. |
| `05_SEMANTICS/02` | `SCOPE` as `INCLUDE` minus `EXCLUDE`; `WORKSPACE` containment by resolved segments, never textual prefix, with a resolved escape as `error.value.out_of_range`. |
| `05_SEMANTICS/06` | The six-step resolution order; `DEFAULT` only for `MISSING`, never for `NULL` or `UNKNOWN`; `ASSUME` conditional, never overriding explicit data, always recorded as evidence. |
| `05_SEMANTICS/05` | An unbound `OUTPUT` read yields `MISSING` and never starts its producer; one producing `ACTION` per selected `OUTPUT` per candidate invocation. |
| `check_selection_contract` | Selection in the `EXECUTE` root document, targetless clauses applying to the invocation, targeted clauses matching a graph member, a referenced data/output declaration or an exact material target; prerequisites in stable topological order with a cycle as `error.reference.cycle`; absent `WHEN` meaning TRUE and FALSE leaving no result; `REQUIRED` controlling blocking, never running; no selected pre-effect check deferring itself past effects. |
| `execution_graph_contract` | Membership copied unchanged from M3; sequential lexical order contributing predecessor edges; `MODE` defaulting to `mode.sequential`; `BEFORE`/`AFTER` adding edges and never reversing a sequential one; endpoints as distinct siblings in one container and iteration context; duplicate activation paths, ordering cycles and conflicting parallel writes as `error.execution.order`. |

### Decision witnesses executed

`CLOSURE-053`, `-054`, `-055`, `-056`, `-060`, `-062`, `-063`, `-064`, `-065`
and `-066` are executed as tests rather than described.

### Deferred by M5, with its owner named

`error.determinism.mismatch` fires when "a kind.operation definition declares
DETERMINISTIC TRUE, but its fully resolved operation or **selected profile set**
is nondeterministic". Neither half is decidable before effects: a custom
operation "selects no implementation profile", and `determinism_identity` is
stated over *dependency snapshots*, so even a `model` or `human` dependency can
satisfy it and no `DEFINE` is self-contradictory on its own. The selected
profile set arrives with the capability kernel, so the identifier is deferred to
**M7** by name — the same discipline M3 used when it deferred
`error.conflict.hard` to M5. A test asserts it is never emitted here.

### What M5 does not do

No effect executes. No dynamic reachability condition is evaluated, no branch
selected, no loop iterated, no retry attempted and no handler activated — those
are M6's. No `VERIFY`, `TEST`, evidence, `SUCCESS`/`FAILURE` or terminal status
is produced; those are M8's. `Outcome::Planned` means steps 6 through 9 found no
reason to refuse, and nothing more.

The evaluator is bounded on purpose: it answers only what is decidable from
literals, resolved declarations and the registered operator, function and
three-valued logic tables. Where a value is not decidable before effects it
returns *no answer* rather than a guess, and the caller either leaves the
obligation to the demanding layer or, for a selected pre-effect check that may
not defer, fails with the registered identifier.

## M6 — `lcl-runtime`

Canonical processing step 10, and only step 10:

> 10. Evaluate dynamic reachability conditions when reached and execute only
>     reachable actions in declared order and authorization bounds.

### API

```rust
let contracts = lcl_runtime::Contracts::load(&spec)?;   // refuses an unverified package
let runtime = lcl_runtime::Runtime::new(&contracts);
let mut host = lcl_runtime::MockHost::new();            // or a real adapter, in M7
let execution = runtime.execute(&planned, &checked, &resolved, &mut host)?;

execution.invocations()     // every invocation entered, in declared execution-path order
execution.diagnostics()     // after supersession, duplicate suppression and stable_order
execution.primary()         // the first *unhandled, unsubstituted* diagnostic
execution.events()          // every raised event occurrence and what became of it
execution.bindings()        // every OUTPUT binding, per producer instance
execution.terminal_status() // the primary diagnostic's resolved default_status, or None
execution.serialize()       // canonical, order-stable text for reports and comparison
```

`execute` returns `NotPlanned` when preflight rejected the program. That is the
stage boundary made structural rather than documented: `Planned::plan` hands out
`None` for a rejected preflight, so a refused program has no plan to run and no
effect can be authorized.

### The effect boundary

Every external effect leaves the language through one trait, and it passes two
independent gates first:

> Host permission does not imply LCL authorization. LCL authorization does not
> force host permission. Both gates must pass for an effect.

`capability::request` is the only path from the runtime to a `Host`, and it
checks the plan's authorization *and* the host's permission. Neither is derived
from the other. A `Host` returns **observations** — fields, effects, and whether
it can prove it caused none — and has no field to write a status or an error
identifier into, so it cannot "smuggle hidden semantic decisions back into the
runtime". The runtime chooses the canonical identifier, the status and the
failure phase.

The runtime core has no clock. `RETRY.DELAY` is a declared quantity handed to
`Host::delay`; nothing here sleeps or reads time, which is what makes the
deterministic scheduler exact rather than approximate.

### Concurrency without threads

`05_SEMANTICS/08` grants freedom in execution order and withholds it in
observable order:

> mode.parallel permits unspecified scheduling but requires declared
> independence ... Completion semantics remain deterministic.

> Result collection and diagnostics use declared child order, never finish
> order. Independent eligible children may execute in any order.

So parallelism is an explicit interleaving over one deterministic step queue,
not OS threads. There is no scheduler timing to depend on. `Interleaving::ALL`
gives three admissible orders, and the test suite runs every canonical example
under all three and compares serialized output byte for byte.

Independence itself is not re-derived here: M5 proves it before effects and
refuses to plan a conflicting `mode.parallel` container.

An unrecovered required failure halts what is *after* it — and "after" follows
sequential edges only. `mode.parallel` "omits implicit sequential edges", so an
independent sibling is not cancelled by a failure it does not depend on;
cancelling it would make the observable result depend on which child the queue
ran first.

### The first layer allowed to reclassify a demand

Every earlier milestone was forbidden to apply
`diagnostic_selection.expression_demand_resolution`, because its own context
says "Preflight-required expression demands retain registered source-validation
classification". Step 10 is the other side of that door.

A `Diagnostic` therefore carries both stages: `registered_stage` is what the
registry declares and never changes, and `resolved_stage` is the demand-resolved
stage when the closed eligible map applied. Evidence shows the reclassification
instead of hiding it. Eligibility is read from the registry, never decided at a
call site, so the `exclusion_rule` holds structurally.

In practice this is also where the M4/M6 boundary becomes visible. M4 decides a
value-domain defect it can see statically — `1 / 0`, `MEASURE(1, unit.meter) +
MEASURE(1, unit.kilometer)`, `EMPTY(NULL)` — and M6 decides the same identifiers
only when the value arrives at a demand, which is exactly what the map's own
trigger sentences say ("a **dynamically supplied** material value", "actually
demanded **after** preflight").

### Rules implemented, by normative source

| Source | Rules |
| --- | --- |
| `05_SEMANTICS/12` | Left-to-right operand order stopping at the first diagnostic; `AND`/`OR` short circuit with the registered strong-Kleene tables; MISSING consumption except `==`, `!=` and `EXISTS`; UNKNOWN propagation; `==` sentinel, cross-type and MEASURE-unit rules; `IN`/`CONTAINS` for collections, objects and strings; zero-based `LIST` indexing yielding MISSING out of range; `OBJECT` key access; the registered total-order profile; exact division with the direct-`ROUND` half-even exception; `MATCHES` over both closed pattern profiles. |
| `types_v0.1.0.json#/pattern_profiles` | `closed_lcl_regex_0_1_0` compiled to a Thompson automaton and simulated as a state set, so "no implementation strategy ... changes Boolean acceptance" holds and an adversarial pattern is bounded rather than exponential; `closed_lcl_glob_0_1_0` with `**` whole-segment semantics; both with declared finite resource limits. |
| `05_SEMANTICS/01` | `IF` evaluated once upon reachability with only the selected arm entered; `FOR EACH` snapshotting once and iterating sequentially with per-instance identity and per-instance `OUTPUT` binding; the declared successor as the unique next reachable sibling, ascending finished sequential containers. |
| `05_SEMANTICS/05` | `OUTPUT` projection by zero, one or many `PROPERTY` occurrences against the schema's `default_property` and `projectable_fields`; MISSING and UNKNOWN never binding; each retry attempt starting unbound. |
| `05_SEMANTICS/06` | The closed event model: a diagnostic is the only thing that raises an event; the three-key total selection order; at most one activation per diagnostic; non-reentrancy; the handler-context target binding; `FALLBACK` eligibility and single substitution. |
| `05_SEMANTICS/08` | `RETRY` as a budget the declared block owns and a selected `core.retry` handler authorizes; budget then `WHEN` then safety then `DELAY`; `error.retry.exhausted` only after exactly `1 + LIMIT` actual failed attempts. |
| `05_SEMANTICS/09` | `failure_phase`, `effect_state` and `output_binding` as three independent axes, with the registry's cross-axis invariants checked rather than assumed; the registered status lifecycle driven by `allowed_next`. |

### Decision witnesses executed

`CLOSURE-007`, `-011`, `-012`, `-013`, `-014`, `-015`, `-016`, `-017`, `-022`,
`-023`, `-024`, `-027`, `-029`, `-030`, `-031`, `-032`, `-039`, `-045`, `-047`,
`-058` and `-059` are executed as tests rather than described.

### Deferred, with its owner named

`error.dependency.unsatisfied` and `error.scope.violation` are registered at the
execution stage but `05_SEMANTICS/09` states both are "pre_effect only". M5
decides them. They are mirrored here so the set can be checked against the whole
registered execution stage rather than a subset this build chose, and a test
asserts the runtime never emits either — under a permissive host and under a
hostile one.

`error.determinism.mismatch` remains deferred to **M7** by name, exactly as M5
deferred it.

### How `DURATION` is told apart from `MEASURE`

The two are different types with opposite comparison rules — `DURATION` compares
by normalized magnitude, `MEASURE` requires "the identical unit identifier" —
and the `MEASURE` constructor row admits Time-category units explicitly. M5's
`Value::Quantity` spells both the same way.

This layer resolves that without forking the value model: a `DURATION` is
normalized at construction exactly as `duration_normalization` requires, and
carries a unit identifier outside the `unit.` namespace, so it is not a
registered unit and can never collide with a `MEASURE`. Two `DURATION`s then
compare by magnitude, two `MEASURE`s keep their written units, and no `MEASURE`
is ever equal to a `DURATION`.

### What M6 does not do

No `VERIFY`, no `TEST`, no evidence collection, no `SUCCESS`/`FAILURE`
evaluation and no single terminal root status: those are canonical steps 11
through 13 and belong to M8. `Execution::terminal_status` reports only the
primary unhandled diagnostic's resolved `default_status`, and `None` means no
diagnostic fixes a status — which is not a claim of success.

The complete executable Core operation surface and its real filesystem, process,
network and provider adapters belong to M7. This milestone ships the capability
boundary itself, the four control operations that are pure execution-path
control, and a deterministic mock host whose default observations have the
registered shape of each result schema.

### Proof

```text
cargo test --offline -p lcl-runtime      # 205 tests
cargo run  --offline -p lcl-runtime --example m6_report
```

The report executes all 13 canonical valid examples, confirms all 21 invalid
examples are owned by an earlier stage, and compares every valid example's
serialized execution across all three admissible interleavings.

## M9 — `lcl-protocol`, `lcl-project` and `lcl-cli`

The first milestone whose output a person uses directly. Everything before it was
a library; this is the terminal command, the project on disk, and the record a
later workspace UI will consume.

### API

```rust
// One assembled engine, reused for any number of documents.
let engine = Engine::open("canonical/LCL_Core_0.1.0")?;

// One project: an explicit root, and a provider that cannot volunteer a file.
let project = Project::open("my-project")?;
let provider = project.provider()?;
let unit = provider.root_unit("src/main.lcl")?;

let report = engine.check(&unit, &provider);                          // steps 1-5
let report = engine.validate(&unit, &provider, &inputs);              // steps 1-9
let report = engine.inspect(&unit, &provider, &inputs);               // steps 1-9, structural
let report = engine.run(&unit, &provider, &inputs, &mut stdlib, &mut host); // steps 1-13

report.outcome;            // accepted, rejected, or refused
report.reached;            // the furthest canonical stage
report.terminal_status();  // Some(..) only for a run that completed
report.to_json().pretty(); // exactly what `--machine` prints
```

### Where a command stops

Each command ends at a canonical boundary rather than a convenient one, and
`Reached` names it in the language's own vocabulary.

| Command | Canonical steps | Effects possible |
| --- | --- | --- |
| `check` | 1 to 5 — lexical, grammar, resolution, static checking | no |
| `validate` | 1 to 9 — adds the no-effect semantic preflight | no |
| `inspect` | 1 to 9, reported as units, imports, declarations and the ordered plan | no |
| `run` | 1 to 13 — execution, verification, evidence, one terminal status | yes, through the host |

Stopping early is the point of a command, and `check` on a document with a
validation-stage defect reports success: it is a statement about steps 1 to 5,
and the rendering says so rather than letting a reader infer more.

### Three things that are deliberately not the same

**A rejected document, a completed run that did not succeed, and an unusable
request.** `05_SEMANTICS/10` keeps producer completion and domain outcome apart,
so the exit table does too. `Outcome::Refused` is the third: a caller who
mistyped `--input` has learned nothing about their document, and a report saying
"rejected" would be claiming they had.

| Code | Meaning |
| --- | --- |
| 0 | the requested work completed; a run also succeeded |
| 1 | the document was rejected by a diagnostic |
| 2 | the document ran and its terminal status was not `status.succeeded` |
| 3 | the command line or a supplied input was not usable |
| 4 | the specification, project or document could not be read |

### Nothing is discovered

`05_SEMANTICS/02` is categorical: "Ambient current directory and implied nearby
files do not exist in portable LCL." Three consequences, each tested:

- the specification package comes from `--spec`, then `LCL_SPEC`, then the
  project manifest, and if none names one the command stops rather than looking;
- the project root is the directory a caller named or the document's own
  directory, with no walk upward, because a root found by searching parents is an
  implied nearby file by another name;
- `SourceProvider` has one method and no listing, globbing, searching or
  defaulting, so a file no document names cannot enter the program.
  `FileProvider::loaded` exists so a caller can prove that afterwards, and the
  resolver cannot call it.

Containment is decided on the *resolved* path — "textual prefix alone does not
establish containment" — so a symbolic link pointing outside the project root is
refused even though its spelling is innocent.

### A run is granted nothing

`HostAdapter` starts with the engine's internal `MEMORY` and `STATE` stores and
no filesystem, no process runner and no transport. Each `--allow-*` flag adds
exactly what it names, and a profile is installed only alongside the capability
it describes, because a profile is a claim that an implementation exists.

The two-gate rule stays structural. One `--allow-write` is the whole difference
between a run that writes a file and one that reports
`error.operation.precondition` before any effect, and the document is byte for
byte the same in both. Five of the thirteen canonical examples succeed under this
closed default; the other eight do not, each for a host reason the record states
and never because the engine could not read them.

### Reproducibility, without a network

`07_VERSIONING_AND_EXTENSIONS/02` already makes `VERSION` exact and a `URI`
import's `CHECKSUM` mandatory, and the resolver already enforces both. M9 adds
what a *project* needs around that:

- source identity is root-relative, so two checkouts at different absolute paths
  produce identical identities, spans and reports;
- `lcl.lock` records the package identity and every unit's SHA-256; `package
  verify` and `--locked` report drift rather than absorbing it;
- a `URI` import is answered from a content-addressed cache or not at all.
  Nothing fetches: the architecture contract rules out resolver-owned web
  browsing, and a tool that downloaded on demand would make a document's meaning
  depend on when it was resolved. `package vendor` puts local bytes in the cache
  and prints the `CHECKSUM` the importing document must declare.

### Why there is no transport, and no editor file

A protocol crate could have shipped a socket, a JSON-RPC dispatcher or a language
server. It ships none: M10 has not chosen a UI toolkit, and a transport chosen
now would be a guess later work has to live with. What a UI needs is a stable set
of records and one way to obtain them, and both forms are here — a Rust consumer
calls `Engine`, any other consumer runs `lcl --machine` — producing the same
records from the same code. A transport can be added later without changing
either.

`integration/linux/lcl.xml` registers `text/x-lcl` for the desktop, with a magic
rule matching the `LCL:` header every conforming document begins with. There is
no `.desktop` entry, because a desktop entry opens a file in an application and
that application is M10's work. `lcl syntax --machine` emits the closed
vocabulary from the loaded lexicon rather than from a checked-in word list, so
editor metadata cannot drift from the registry.

### What M9 does not do

No editor, no workspace, no debugger UI, and no release hardening. It also adds
no language rule: the one piece of language behavior the tool needs that did not
exist — turning a supplied `--input` expression into a value — was added to
`lcl-semantics` as `Preflight::value_of`, which calls the same literal
evaluation `DATA` and `INPUT` resolution already performs at step 7. A second
literal reader in a CLI would have been a second implementation of the language.

### Proof

The equivalence claim is tested by running the CLI and the facade over the same
document and comparing the records **byte for byte**, for an accepted document, a
rejected one, a completed run and a supplied input. Comparing selected fields
would only have proved that the fields someone remembered to compare agree.

The clean-environment suite runs every command from an empty environment, with no
inherited variable and no working directory the tool may rely on, and asserts
that the same project produces byte-identical output from three different
directories and that read-only commands leave the project's file listing
unchanged.

## Use

```bash
cargo test --offline --workspace --all-targets                # every milestone, M0 through M9
cargo run --offline -p lcl-lexer --example m1_report          # lexer report over all canonical inputs
cargo run --offline -p lcl-parser --example m2_report         # parser report over all canonical inputs
cargo run --offline -p lcl-resolver --example m3_report       # resolver report over all canonical inputs
cargo run --offline -p lcl-checker --example m4_report        # static and type checker report
cargo run --offline -p lcl-semantics --example m5_report      # semantic preflight report
cargo run --offline -p lcl-conformance --example m0_report    # foundation report
cargo run --offline -p lcl-completion --example m8_report     # completion report over all canonical examples
cargo run --offline -p lcl-conformance --example m8_conformance_report  # executed conformance evidence
cargo run --offline -p lcl-spec --example mint_anchor         # compute a package identity
cargo clippy --offline --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

The M9 binary, over a document or a project:

```bash
cargo build --offline -p lcl-cli                              # builds target/debug/lcl
lcl help                                                      # commands, options and exit codes
lcl spec --spec canonical/LCL_Core_0.1.0                      # package identity and authority
lcl check   src/main.lcl                                      # canonical steps 1-5
lcl validate src/main.lcl                                     # steps 1-9, before any effect
lcl inspect src/main.lcl                                      # units, imports, declarations, plan
lcl run     src/main.lcl                                      # steps 1-13, one terminal status
lcl run --allow-write /tmp/out src/main.lcl                   # the same, with one capability
lcl check --machine src/main.lcl                              # the JSON record a UI consumes
lcl package lock                                              # record what resolution loaded
lcl package verify                                            # compare against that record
lcl syntax --machine                                          # registry-derived editor metadata
```

Every command needs a specification package, from `--spec`, `LCL_SPEC` or the
project manifest's `"spec"`. None of them searches for one.

All are read-only with respect to `canonical/`. Integrity tests that need to
mutate a package operate on a throwaway copy under `target/test-tmp/`, and the
`.lcl` desktop registration in `integration/linux/` is never installed by
building the workspace.

## M10 — `lcl-workspace`

The product a person actually uses: an editor, a project shell, live
diagnostics, execution inspection and a debugger, over the same engine the CLI
uses.

It is a loopback HTTP server serving a browser frontend, both written here,
both on `std` alone. No UI toolkit was added, and no crate was added, because
the workspace's dependency policy — "std only … no third-party supply-chain
surface" — is the same policy that made `lcl-cli` write out its own argument
parser and `lcl-protocol` its own JSON writer. The owner approved that shape
before any of it was built.

### The rule everything else follows from

> CLI and UI consume engine APIs/protocols. Neither may contain a second
> private implementation of language semantics.

So this crate does not tokenize, parse, resolve, check, plan or evaluate, and
neither does the page. **The browser does not even highlight.** A JavaScript
regular-expression highlighter is a second lexer, transcribed into a language
where it will drift the first time a registry moves, so instead the page asks
for token spans, the real `lcl-lexer` produces them, and the page paints them.
The frontend holds no keyword list, no grammar and no pattern that matches LCL.

Byte offsets stay normative the whole way out. Spans cross the wire as byte
offsets with the engine's own derived position beside them, and the page maps
bytes to screen positions through one index built per document rather than by
counting characters itself.

### Running it

```bash
cargo build --offline -p lcl-workspace
lcl-workspace my-project --spec canonical/LCL_Core_0.1.0     # prints a URL
lcl-workspace my-project --create                            # make the project first
lcl-workspace my-project --open                              # and open it in a browser
lcl-workspace my-project --log                               # one line per request
```

The server binds `127.0.0.1` on an ephemeral port and mints a session token per
launch. Three gates run before any handler sees a request: the `Host` header
must name the address actually bound, which is what refuses a rebound DNS name;
an `Origin`, when present, must be this same origin; and the token must match.
Responses carry `default-src 'self'`, `nosniff`, and no CORS header at all.

### What the debugger can and cannot do

It stops at the capability boundary — before the standard library dispatches an
operation, and before an effect crosses into the host — by wrapping the existing
`Operations` and `Host` traits. That is where an effect leaves the language, so
it is both the most useful place to stop and the one the engine already exposes
a seam for. Neither wrapper decides anything: a denial returns the same
`Refusal` the host would, and the engine decides what that means.

It does **not** single-step plan nodes, and does not pretend to. The runtime
executes an accepted plan in one call over its own deterministic queue, and
pausing between nodes would mean changing a closed milestone. After a run, the
recorded invocations step forward and back with their spans highlighted, which
is a different thing and is labelled as one.

Consent is the second gate and only the second gate. A pause is reached only for
a request the language already authorized; a `FORBID`den effect is refused at
preflight, before the host is consulted, so there is no prompt to say yes to.

### Additions to M9

Three, all additive, all in `lcl-protocol` where both consumers reach them:

- **`Report::navigation`** — the resolver's declarations and bindings, with
  spans, projected for `inspect`. Without it, go-to-definition in an editor
  would have to be a text search. It is attached as soon as resolution has run,
  including when resolution *rejected* the document, because an editor needs to
  navigate a broken file most of all.
- **`surface()`** — the function that turns granted paths, programs and hosts
  into a `Stdlib` and a `HostAdapter`. It was private in the CLI binary; the
  workspace has to behave identically for identical grants, and one copy both
  call is the only way to guarantee that.
- **`Engine::run_with`** — `run` with the operation dispatcher as a parameter,
  mirroring `Runtime::execute_with`, which has always taken one. A debugger that
  observes the dispatch seam would otherwise have to reassemble the thirteen-step
  walk itself.

### The gate

`lcl-workspace`'s equivalence suite runs the `lcl` binary as a subprocess with a
cleared environment and compares its `--machine` output against what the
workspace serves, **byte for byte**, for `check`, `validate` and `inspect` over
all thirteen valid canonical examples and over documents rejected at the
lexical, grammar and resolution stages. Comparing selected fields would prove
only that the fields someone thought of agree.

The editor-save regression target also requires **Node.js 22 or newer** on PATH.
It uses only Node built-ins; there is no npm package or production dependency.
`tests/editor_save.cjs` loads the entire production frontend with a controlled
DOM and drives its actual input, save and close-modal handlers. The Rust driver
starts an owned workspace process, loads its served JavaScript, exercises real
authenticated HTTP and checks the persisted bytes. A missing Node prerequisite
fails the target. This evidence is distinct from a visible browser/desktop check.

```bash
node crates/lcl-workspace/tests/editor_save.cjs
cargo test --offline -p lcl-workspace --test editor_save -- --nocapture
```

For release acceptance, point the same target at the exact extracted/installed
candidate and its bundled specification. Both overrides are required together;
the process receives an otherwise cleared environment, including no inherited
`LCL_SPEC`, and uses a newly reserved project under `TMPDIR`.

```bash
LCL_EDITOR_WORKSPACE_BIN="/absolute/candidate/bin/lcl-workspace" \
LCL_EDITOR_SPEC="/absolute/candidate/spec" \
cargo test --offline -p lcl-workspace --test editor_save -- --nocapture
```

The log records the exercised binary and frontend SHA-256 identities. These
checks cover failed/inactive-tab close-save, edits during save, overlapping
saves, pending work, final-line-feed acknowledgement, discarded/reopened
documents and refresh failure after successful persistence.

## M11 — hardening, the ladder, and the release

The last milestone adds no layer. It asks whether the ten below it survive a
world that is not trying to help, and it closes a release.

### What it repaired

Five defects were open when it started, each blocking a canonical decision
witness, and the executable conformance gate was withdrawing its claim because
of three of them. All five are closed, and the gate now claims
`semantics_conforming` over 61 executed cases with none failing.

Two of the five were diagnosed differently from how they were recorded. The
`SUM`-over-empty defect was not in the reduction, which was correct; the fault
it raised was being discarded by the layer that demanded it, so a registered
diagnostic was raised there or nowhere. The `core.read` range defect was two
defects, because the parameter carrying the range never reached the operation:
an object-valued `PARAMETER` body was read only in its inline form.

### What it found

A document could end the process by `SIGABRT`. Every nesting shape —
parentheses, collections, unary prefixes, binary operands, call arguments,
property and index chains — aborted between 2,000 and 16,000 levels. Canonical
LCL declares no maximum nesting depth and no registered diagnostic permits an
implementation-defined nesting rejection, so a limit was not available as a
repair, exactly as M2 had already concluded for the parser.

Two things still recursed. The static checker's expression walk now judges a
node's descendants through an explicit worklist and never recurses into a child
it did not have to; `children_of` answers, without judging anything, which
children a node's judging function will ask for and with which receiving
contract, and answers `None` for anything it cannot predict exactly, which
costs one stack frame and changes no judgement. And `Expr`'s `Clone` is now
iterative, like the `Drop` beside it, because every consumer that copies a
subtree to satisfy the borrow checker was paying derived recursion.

Every shape now survives 50,000 levels, the depth M2 proved for the parser.
Nested indented bodies are the one declared bound: 512 levels, recorded in
`reports/implementation/LCL_RELEASE_BASELINE.md` with the two paths that set it.

### The ladder

Four projects under `apps/`, run through the binary with an empty environment
and exactly the capabilities they declare: a single-document calculation, a
project with an imported rule library and one granted write, a two-phase
pipeline with an authority override and a retry handler, and the same pipeline
written as two sub-tasks composed by a parent phase. The last pair agree on
every observable the engine decides, differing only in the workspace each was
told to use.

### Running it

```bash
cargo test --offline -p lcl-hardening              # the whole gate, a few minutes
cargo test --offline -p lcl-hardening --test fuzz_stages
cargo test --offline -p lcl-hardening --test applications
../packaging/build_release.sh                      # the tarball, checksum and provenance
```

Every generated case is a pure function of a seed, so a failure is reproducible
from the number the failure message prints and no corpus is stored.

# LCL implementation — milestones M0 (foundation), M1 (lexer), M2 (parser), M3 (resolver), M4 (checker) and M5 (semantic preflight)

This directory is a **consumer** of the canonical specification at
`../canonical/LCL_Core_0.1.0`. It is not part of the release, is not listed in
`MANIFEST.json` or `SHA256SUMS.txt`, and never writes to `canonical/`.

Building this does not change the release's status. The `complete_example_parse_matrix`
and `semantic_case_execution` gates remain `OUT_OF_SCOPE` for LCL Core 0.1.0;
implementation conformance is separate evidence.

## Crates

| Crate | Milestone | Role |
| --- | --- | --- |
| `lcl-spec` | M0 | Specification authority loader. Verifies package integrity against `MANIFEST.json` and `SHA256SUMS.txt`, checks an **external trust anchor**, pins the formal version, loads the 12 closed registries and 2 catalogs as data. |
| `lcl-diagnostics` | M0 | Diagnostics skeleton. The 7 normative stages, 12 statuses and 77 errors, loaded and closure-checked. |
| `lcl-conformance` | M0 | Conformance skeleton. Indexes the 799 descriptive requirements and 66 decision witnesses. Executes nothing. |
| `lcl-lexer` | M1 | Deterministic, non-executing lexer. Source bytes in; tokens with exact byte spans or stable-ordered registered lexical diagnostics out, including every contextual `error.keyword.case` position and the closed literal profiles of constructor arguments. |
| `lcl-parser` | M2 | Deterministic, non-executing parser. M1 tokens in; a source-faithful syntax tree with exact byte spans or registered grammar-and-schema diagnostics out. |
| `lcl-resolver` | M3 | Deterministic, non-executing resolver. Parsed units plus an explicit source provider in; version, import, extension, namespace, ID and `REF` bindings and the structural candidate graph, or registered resolution diagnostics, out. |
| `lcl-checker` | M4 | Deterministic, non-executing static and type checker. A resolved program graph in; every declaration's type, every expression's static contract and the value obligations left to the demanding layer, or registered `static_or_expression` diagnostics, out. |
| `lcl-semantics` | M5 | Deterministic, no-effect semantic preflight. A statically checked program plus explicit invocation data in; an authorized, dependency-resolved, prevalidated and ordered execution plan, or registered pre-effect diagnostics, out. |

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

No semantic preflight, evaluator, capability kernel, runtime, CLI or UI. M5 has
not been started. Value-domain checks on operands that arrive only at demand are
*recorded* as obligations rather than decided, and the diagnostic *selection*
algorithm beyond one source-validation run (`expression_demand_resolution`
applied at demand, producer paths, iteration and retry indexes) remains data,
not code.

### Known limitation outside M3

`lcl_parser::Parser::parse` builds its syntax tree recursively, so an expression
nested a few hundred levels deep exhausts the stack instead of producing a
diagnostic. `lcl-resolver`'s own expression walk is iterative and adds no depth,
but it cannot run on a tree the parser could not build. This is an M2 defect and
is deliberately not repaired under the M3 task.

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

## Use

```bash
cargo test --offline --workspace --all-targets                # M0 + M1 + M2 + M3 + M4 + M5
cargo run --offline -p lcl-lexer --example m1_report          # lexer report over all canonical inputs
cargo run --offline -p lcl-parser --example m2_report         # parser report over all canonical inputs
cargo run --offline -p lcl-resolver --example m3_report       # resolver report over all canonical inputs
cargo run --offline -p lcl-checker --example m4_report        # static and type checker report
cargo run --offline -p lcl-semantics --example m5_report      # semantic preflight report
cargo run --offline -p lcl-conformance --example m0_report    # foundation report
cargo run --offline -p lcl-spec --example mint_anchor         # compute a package identity
cargo clippy --offline --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

All are read-only with respect to `canonical/`. Integrity tests that need to
mutate a package operate on a throwaway copy under `target/test-tmp/`.

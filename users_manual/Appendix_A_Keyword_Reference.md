# Appendix A. Keyword reference

LCL has exactly 141 reserved words. They are always written in uppercase, and
the list is closed: any other uppercase word is `error.keyword.unknown`, and
a reserved word in the wrong case is `error.keyword.case`.

The meanings below are copied from the specification's keyword registry,
`10_REGISTRIES/keywords_v0.1.0.json`, which `02_LEXICAL/05_KEYWORD_REFERENCE_A_TO_M.txt`
and `02_LEXICAL/06_KEYWORD_REFERENCE_N_TO_Z.txt` present as text. The chapter column shows where this
manual teaches the word; a dash means the manual only mentions it, and the
specification is the place to read more.

## By purpose

**Document header and identity:** `LCL`, `SPECIFICATION`, `VERSION`, `ID`, `NAME`, `KIND`, `DOMAIN`, `AUTHORITY`, `DESCRIPTION`

**Data:** `INPUT`, `DATA`, `OUTPUT`, `CONTEXT`, `MEMORY`, `STATE`, `VALUE`, `TYPE`, `FORMAT`, `ENCODING`, `SOURCE`, `DEFAULT`, `REQUIRED`, `SCHEMA`, `FIELD`, `ITEM`, `ITEM_TYPE`, `PROPERTY`, `CHECKSUM`, `PROVENANCE`

**Definitions:** `DEFINE`, `MEANING`, `BASE`, `TERM`, `RESULT`, `SIDE_EFFECT`, `DEPENDENCY`, `DETERMINISTIC`, `EXAMPLE`, `COMMENT`, `CONTENT`

**Work:** `TASK`, `GOAL`, `ACTION`, `OPERATION`, `TARGET`, `PARAMETER`, `EXECUTE`, `REFERENCE`

**Structure and control:** `SEQUENCE`, `PHASE`, `STEP`, `IF`, `THEN`, `ELSE`, `FOR`, `EACH`, `IN`, `MODE`, `BEFORE`, `AFTER`, `WHEN`

**Places and scope:** `WORKSPACE`, `PATH`, `SCOPE`, `INCLUDE`, `EXCLUDE`

**Rules:** `REQUIRE`, `ALLOW`, `FORBID`, `PREFER`, `PRESERVE`, `OVERRIDE`, `WINNER`, `LOSER`, `PRIORITY`

**Checks and results:** `VALIDATE`, `VERIFY`, `TEST`, `EXPECTED`, `ACTUAL`, `ASSERT`, `EVIDENCE`, `SUCCESS`, `FAILURE`, `STATUS`, `ERROR`, `ALL`, `ANY`, `NONE`

**Missing values and recovery:** `MISSING`, `UNKNOWN`, `NULL`, `ASSUME`, `HANDLER`, `EVENT`, `RETRY`, `LIMIT`, `DELAY`, `FALLBACK`

**Imports:** `IMPORT`, `EXTENSION`, `NAMESPACE`

**Types and constructors:** `STRING`, `INTEGER`, `DECIMAL`, `BOOLEAN`, `LIST`, `SET`, `OBJECT`, `ENUM`, `DATE`, `TIME`, `DATETIME`, `DURATION`, `PERCENTAGE`, `BYTES`, `MEASURE`, `UNIT`, `URI`, `GLOB`, `REGEX`, `PATTERN`, `REF`, `TRUE`, `FALSE`

**Operators and functions:** `AND`, `OR`, `NOT`, `CONTAINS`, `MATCHES`, `ABS`, `COUNT`, `SUM`, `MIN`, `MAX`, `ROUND`, `EMPTY`, `EXISTS`

**Bounds:** `MINIMUM`, `MAXIMUM`, `TOLERANCE`

## All reserved words

| Word | Category | Meaning | Chapter |
|---|---|---|---|
| `ABS` | function | Return the absolute value of one compatible numeric value. | 6 |
| `ACTION` | block/field | Declare an executable operation or reference one ACTION declaration. | 3 |
| `ACTUAL` | field | Record the value observed by a TEST. | 9 |
| `AFTER` | field/operator | Require one execution unit to occur after named predecessors. | 10 |
| `ALL` | field/function | Require every listed Boolean condition to be TRUE. | 9 |
| `ALLOW` | block | Grant permission for exact operations and targets without requiring execution. | 12 |
| `AND` | operator | Logical conjunction. | 6 |
| `ANY` | field/function | Require at least one listed Boolean condition to be TRUE. | 9 |
| `ASSERT` | field | Contain the Boolean expression evaluated by a rule, check, goal, or test. | 3 |
| `ASSUME` | block | Declare an explicit conditional temporary value for otherwise unresolved data. | 13 |
| `AUTHORITY` | field | Assign source strength as an INTEGER from 0 through 1000; higher is stronger. | 12 |
| `BASE` | field | Name the exact base type or parent definition from which a definition derives. | 5 |
| `BEFORE` | field/operator | Require one execution unit to occur before named successors. | 10 |
| `BOOLEAN` | type | The scalar type containing TRUE and FALSE. | 5 |
| `BYTES` | type/constructor | Represent or construct a non-negative exact byte count. | 5 |
| `CHECKSUM` | field | Declare an exact integrity digest in algorithm:lowercase_hex form. | 14 |
| `COMMENT` | block/field | Contain non-normative text ignored by execution semantics. | 4 |
| `CONTAINS` | operator | Test exact membership in a string, collection, or object-key set. | 6 |
| `CONTENT` | field | Contain exact text or data content. | 4 |
| `CONTEXT` | block | Activate explicit supporting non-authoritative information for a scope. | 8 |
| `COUNT` | function | Return the number of members, fields, or Unicode code points in a value. | 6 |
| `DATA` | block | Declare typed task material that is not interpreted as instructions. | 4 |
| `DATE` | type/constructor | Represent or construct an RFC 3339 full-date value. | 5 |
| `DATETIME` | type/constructor | Represent or construct an RFC 3339 date-time with an optional offset; omission means UTC. | 5 |
| `DECIMAL` | type | An exact finite base-10 decimal scalar. | 5 |
| `DEFAULT` | field/block | Supply a value only when the target is MISSING and no stronger value exists. | 8 |
| `DEFINE` | block | Create an immutable term, constant, type, operation, format, event, status, or error. | 7 |
| `DELAY` | field | Specify a bounded DURATION before a retry or follow-up action. | 13 |
| `DEPENDENCY` | block/field | Declare or reference a prerequisite that must be satisfied before execution. In a kind.operation definition it declares the closed dependency classes the operation may select, defaulting to declared_state_only when omitted. | 7 |
| `DESCRIPTION` | field | Contain non-normative human-readable explanation. | 4 |
| `DETERMINISTIC` | field | Declare the determinism allowance of the fully resolved operation or selected profile set. TRUE asserts semantically equivalent results, effects, ordering, and status for identical declared inputs, dependency snapshots, selected profile-role bindings, and implementation versions; it is verified and never overrides nondeterminism. FALSE conservatively permits either deterministic behavior or bounded nondeterminism and never relaxes another rule. | 7 |
| `DOMAIN` | field | Name the technical subject area whose extension definitions apply. | 7 |
| `DURATION` | type/constructor | Represent or construct a non-negative elapsed time. | 5 |
| `EACH` | control | Bind one loop-local identifier to each member of a finite collection. | 10 |
| `ELSE` | control | Open the branch used when the directly associated IF condition is FALSE. | 10 |
| `EMPTY` | function | Return TRUE when a string or collection contains zero members. | 6 |
| `ENCODING` | field | Declare exact character or binary encoding. | — |
| `ENUM` | type | A closed type containing exactly its declared values. | 5 |
| `ERROR` | field/block | Declare or reference an exact failure classification. | 9 |
| `EVENT` | field | Name the exact registered event that activates a handler. An event is raised only by an emitted diagnostic whose registered error declares that event. | 13 |
| `EVIDENCE` | block/field | Declare or reference observable material supporting a check or result. | 9 |
| `EXAMPLE` | block/field | Contain a non-normative illustration unless a TEST explicitly evaluates it. | — |
| `EXCLUDE` | field | Remove exact entities from a SCOPE after INCLUDE selection. | 11 |
| `EXECUTE` | block/field | Select the single root task, phase, sequence, action, or test to perform. | 3 |
| `EXISTS` | function | Return FALSE only for MISSING or an unresolved optional binding; UNKNOWN exists and returns TRUE. | 6 |
| `EXPECTED` | field | Declare the exact value or condition expected by a TEST. | 9 |
| `EXTENSION` | block | Load a versioned vocabulary extension that adds definitions only. | — |
| `FAILURE` | block/field | Declare a terminal non-success condition or reference its result. | 9 |
| `FALLBACK` | field | Name behavior used only when a primary handler operation cannot execute. It admits one HANDLER reference or one operation identifier that requires no named parameter and whose required target the handler-context binding supplies. | 13 |
| `FALSE` | literal | The false BOOLEAN literal. | 4 |
| `FIELD` | block | Declare one member of an OBJECT schema. | 5 |
| `FOR` | control | Begin the bounded FOR EACH iteration form. | 10 |
| `FORBID` | block | Prohibit exact operations and targets as a hard rule. | 12 |
| `FORMAT` | field | Declare exact representation of input, output, evidence, or content. | 8 |
| `GLOB` | type/constructor | Represent or construct a workspace-relative file-selection pattern using the closed LCL GLOB profile. | 6 |
| `GOAL` | block/field | Declare a required intended result or reference one GOAL declaration. | 3 |
| `HANDLER` | block/field | Declare or reference deterministic behavior for one named event. A HANDLER block is an invocation site whose OPERATION is invoked under the ACTION operation contract, selected by the closed event model from EVENT and any WHEN condition. | 13 |
| `ID` | field | Assign a unique lowercase namespaced identifier used for references. | 3 |
| `IF` | control | Begin a Boolean conditional. | 10 |
| `IMPORT` | block | Load one exact LCL document under a mandatory namespace. | 14 |
| `IN` | operator/control | Test membership or introduce the collection of FOR EACH. | 6 |
| `INCLUDE` | field | Select exact entities into a SCOPE before exclusions. | 11 |
| `INPUT` | block/field | Declare task input or reference one INPUT declaration. | 8 |
| `INTEGER` | type | An unbounded signed whole-number scalar. | 5 |
| `ITEM` | field | Declare one member identifier of a user-defined ENUM. | 5 |
| `ITEM_TYPE` | field | Declare the element type of a LIST or SET. | — |
| `KIND` | field | Select one defined document or declaration kind. | 3 |
| `LCL` | block | Open the mandatory language header of every document. | 3 |
| `LIMIT` | field | Set an exact non-negative bound. | 13 |
| `LIST` | type | An ordered collection that permits duplicate values and uses bracket value syntax. | 5 |
| `LOSER` | field | Reference the exact clause displaced by an OVERRIDE. | 12 |
| `MATCHES` | operator | Test an entire STRING against a REGEX or an entire PATH/STRING against a GLOB. | 6 |
| `MAX` | function | Return the greatest comparable member of a non-empty collection. | 6 |
| `MAXIMUM` | field | Declare an inclusive upper bound. | 5 |
| `MEANING` | field | State the exact normative meaning of a DEFINE declaration. | 7 |
| `MEASURE` | type/constructor | Represent or construct a numeric value paired with one registered unit, including a time-category unit. | 5 |
| `MEMORY` | block | Activate explicit retained data for a scope without importing hidden rules. | 8 |
| `MIN` | function | Return the least comparable member of a non-empty collection. | 6 |
| `MINIMUM` | field | Declare an inclusive lower bound. | 5 |
| `MISSING` | literal | Mark an absent required value or unresolved required source. | 13 |
| `MODE` | field | Select one exact behavior mode defined for the surrounding construct. | 10 |
| `NAME` | field | Assign a human-readable label that is never used for reference resolution. | 3 |
| `NAMESPACE` | field | Assign the lowercase prefix for imported or extension definitions. | 14 |
| `NONE` | field/function | Require every listed Boolean condition to be FALSE. | 9 |
| `NOT` | operator | Logical negation of the immediately following grouped or atomic expression. | 6 |
| `NULL` | literal/type | Represent an explicit known absence, distinct from MISSING and UNKNOWN. | 13 |
| `OBJECT` | type | A keyed value whose fields are declared by a schema or explicit data block. | 5 |
| `OPERATION` | field | Name the exact built-in or extension operation invoked by an ACTION or HANDLER invocation site under the same operation contract. | 8 |
| `OR` | operator | Logical disjunction. | 6 |
| `OUTPUT` | block/field | Declare a produced result or reference one OUTPUT declaration. | 8 |
| `OVERRIDE` | block | Resolve one exact conflict by naming a WINNER and LOSER. | 12 |
| `PARAMETER` | block | Declare one named typed operation input. | 8 |
| `PATH` | type/constructor/field | Represent or construct an absolute path or a contained workspace-relative path. | 5 |
| `PATTERN` | field | Declare a GLOB or REGEX constraint. | — |
| `PERCENTAGE` | type/constructor | Represent or construct a DECIMAL percentage from 0 through 100. | 5 |
| `PHASE` | block/field | Declare or reference a major ordered execution group. | 10 |
| `PREFER` | block | Declare a soft objective that cannot defeat a hard rule. | 12 |
| `PRESERVE` | block | Require a target or properties to compare equal before and after execution. | 12 |
| `PRIORITY` | field | Order equally authoritative applicable clauses from -1000 through 1000; optional omission defaults to integer 0. | 12 |
| `PROPERTY` | field | Name one exact property of a target or object. | 8 |
| `PROVENANCE` | field | Record exact origin and production method of data or evidence. | 9 |
| `REF` | constructor | Resolve a declaration or loop-local binding by exact identifier; preserve identity in a reference/address context and read one bound value in an ordinary value context. | 3 |
| `REFERENCE` | field/type | Declare a typed pointer to another declaration. | 3 |
| `REGEX` | type/constructor | Represent or construct a pattern in the frozen closed LCL REGEX 0.1.0 grammar with optional canonical i, m, and s flags. | 6 |
| `REQUIRE` | block | Declare a hard Boolean assertion or required ACTION. | 12 |
| `REQUIRED` | field | Declare whether an item is mandatory. | 8 |
| `RESULT` | field/block | Declare or reference the value produced by a goal, operation, or test. | 7 |
| `RETRY` | block | Declare at most LIMIT additional attempts of the containing ACTION after qualifying failure; attempts stop on success, a false WHEN, or a failed safety precondition. | 13 |
| `ROUND` | function | Round a DECIMAL or MEASURE to non-negative fractional digits using half-even rounding; MEASURE preserves its UNIT. | 6 |
| `SCHEMA` | field | Declare or reference the exact structure of an OBJECT value. | — |
| `SCOPE` | block/field | Declare or reference the exact set of entities to which a clause may apply. | 11 |
| `SEQUENCE` | block/field | Declare or reference an ordered collection of steps. | 10 |
| `SET` | type | An intrinsically unordered collection unique under strict equality using bracket value syntax; source or insertion order has no semantic meaning and equal duplicates collapse. Direct FOR EACH uses canonical ascending order only when actual members are mutually order-compatible; otherwise pass the SET directly to core.sort and iterate its LIST result. | 5 |
| `SIDE_EFFECT` | field | Declare whether an operation may change external state and, when it may, the exact closed effect classes it can produce. FALSE declares possible effects exactly none; an effect-class LIST declares one or more concrete classes and forbids none. | 7 |
| `SOURCE` | field | Identify exact origin of data, import, context, memory, state, or evidence. | 8 |
| `SPECIFICATION` | block | Declare mandatory identity and metadata of one LCL document. | 3 |
| `STATE` | block | Declare explicit mutable external or project data read at a defined point. | 8 |
| `STATUS` | field | Record one exact core status identifier or a same-domain extension alias that resolves to it; aliases add no core state or transition. | 9 |
| `STEP` | block | Declare one ordered execution unit inside a phase or sequence. | 10 |
| `STRING` | type | A Unicode text sequence represented by an exact string literal. | 5 |
| `SUCCESS` | block/field | Declare the exact Boolean completion contract or reference it. | 9 |
| `SUM` | function | Return the arithmetic sum of compatible numeric members. | 6 |
| `TARGET` | field | Identify the exact entity affected, inspected, produced, or constrained. | 8 |
| `TASK` | block/field | Declare an executable task or reference one TASK declaration. | 3 |
| `TERM` | kind | The DEFINE kind for one exact domain term. | 7 |
| `TEST` | block/field | Declare an executable comparison between expected and actual behavior. | 9 |
| `THEN` | control | Open the TRUE branch of the directly preceding IF. | 10 |
| `TIME` | type/constructor | Represent or construct an RFC 3339 clock time with an optional offset; omission means UTC. | 5 |
| `TOLERANCE` | field | Declare an exact absolute permitted numeric difference. | — |
| `TRUE` | literal | The true BOOLEAN literal. | 4 |
| `TYPE` | field/kind | Declare the data type of a value, member, parameter, input, or result. | 3 |
| `UNIT` | field/type | Declare one exact unit identifier associated with a MEASURE. | 5 |
| `UNKNOWN` | literal | Mark a value known to exist but not currently determinable. | 13 |
| `URI` | type/constructor | Represent or construct an RFC 3986 absolute URI with a scheme. | 5 |
| `VALIDATE` | block/field | Check syntax, types, references, dependencies, and preconditions before effects. | 9 |
| `VALUE` | field | Contain one exact scalar, collection, object, expression, or reference. | 3 |
| `VERIFY` | block/field | Check observable postconditions after execution. | 9 |
| `VERSION` | field | Declare one exact semantic version; ranges and latest are invalid. | 3 |
| `WHEN` | field | Limit applicability to one explicit Boolean expression. | 9 |
| `WINNER` | field | Reference the exact clause retained by an OVERRIDE. | 12 |
| `WORKSPACE` | block/field | Declare or reference an explicit base location for relative paths. | 11 |


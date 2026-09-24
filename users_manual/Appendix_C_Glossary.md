# Appendix C. Glossary

The number in brackets is the chapter where the term is explained.

**Action** (3, 8)
: One invocation of one operation on one target, with named parameters and
  at most one output. Declared with `ACTION`.

**Authority** (12)
: The strength of a document or rule, from 0 to 1000; 500 by default. Higher
  authority wins a conflict.

**Binding** (8, 10)
: A short local name inside an expression: `target` in `core.calculate`,
  `item` in a filter predicate, the names given in `bindings`, and a FOR EACH
  variable.

**Block** (3)
: An uppercase key followed by a colon and an indented body, such as
  `INPUT:`. A document is a list of blocks.

**Blocked** (9)
: A terminal status: the task could not proceed because something it needed
  (data, permission, a capability) was unavailable.

**Capability** (11)
: Something the host can do for a document, such as reading files or running
  a program. `lcl run` grants capabilities only through `--allow-*` options.

**Check** (9)
: A condition evaluated to TRUE or FALSE: VALIDATE before the work, VERIFY
  after it, or TEST in a test document.

**Constructor** (5)
: An uppercase function that builds a typed value and checks it, such as
  `DATE("2026-09-24")` or `MEASURE(5, unit.meter)`.

**Declaration** (3, 7)
: Anything with an `ID` that other parts of the document can refer to.

**Diagnostic** (15)
: A message from the tool reporting a problem, with a location, an error
  code and a stage.

**Document** (1, 7)
: One LCL file. Its `KIND` says whether it is a task, test, library, data or
  extension document.

**Enumeration (enum)** (5)
: A type whose values are a fixed list of names, defined with `BASE: ENUM`
  and `ITEM` fields.

**Event** (13)
: What a recoverable error raises, so that a handler can respond. There are
  five events.

**Evidence** (9)
: Declared material that supports a result, with its origin (provenance).

**Execution graph** (10)
: The plan of what runs and in which order, built from EXECUTE before
  anything runs. Shown by `lcl inspect`.

**Expression** (6)
: A side-effect-free formula that produces a value, such as
  `REF(input.a) + 1`.

**Field** (3)
: A `KEY: value` line inside a block.

**Fragment** (6, 8)
: An expression written inside a string, as in `core.calculate`'s
  `expression` parameter or a filter's `predicate`.

**Handler** (13)
: A declared response to an event, attached to a task, step, retry,
  dependency or failure clause.

**Hard rule** (12)
: A rule that must hold: REQUIRE, FORBID or PRESERVE.

**Host** (1, 11)
: The system that runs a document and provides files, programs and network
  access. When you use `lcl run`, the tool is the host.

**Identifier (ID)** (4, 7)
: A lowercase name, often dotted, such as `input.price`. IDs are unique within
  a document.

**Import** (14)
: Loading a library document under a namespace prefix.

**Interpreter** (1)
: Anything that validates an LCL document and acts on it: a deterministic
  tool like `lcl`, or an AI system.

**Invalid** (9)
: The terminal status of a document rejected before running.

**LC, LCL, LCP** (1)
: Learned Computing (AI systems); the Learned Computing Language; Learned
  Computing Programming.

**Library** (7, 14)
: A document of definitions, data and rules (`kind.library`) for others to
  import. It cannot execute anything.

**Lock file** (14)
: `lcl.lock`: the fingerprints of the specification package and every loaded
  document, used to prove nothing has changed.

**MISSING** (13)
: The special value meaning "no value exists". It cannot be stored.

**Namespace** (4, 14)
: The first part of an ID. An import's namespace prefixes every name it
  brings in. Some namespaces, such as `core` and `unit`, are reserved.

**NULL** (5, 13)
: A stored value meaning "known to be empty".

**Operation** (3, 8, Appendix B)
: A named kind of work, such as `core.calculate` or `core.create`. A document
  may declare its own with `DEFINE` and `kind.operation`.

**Output** (3, 8)
: A declared result. It has no value until exactly one action produces it.

**Override** (12)
: An exact, named exception that lets one rule win over one conflicting rule
  of no higher authority.

**Phase** (10)
: A major stage of work containing steps and sequences. Phases can be ordered
  with BEFORE and AFTER.

**Preflight** (2, 9)
: Everything that happens before any action runs: all checking up to and
  including VALIDATE. It is what `lcl validate` does.

**Priority** (12)
: A tie-breaker between conflicting rules of equal authority, from -1000 to
  1000; 0 by default.

**Project** (14)
: A folder with an `lcl.project.json` file.

**REF** (3, 7)
: `REF(id)`: refer to a declaration by its exact ID. It points at the
  declaration in reference fields and reads its value in expressions.

**Result record** (8)
: The structured result of an operation. An output receives one of its
  fields, chosen by default or with PROPERTY.

**Retry** (13)
: A bounded number of extra attempts at an action, declared in a `RETRY`
  block and used through a `core.retry` handler.

**Schema** (5)
: The exact list of fields an object must have, defined with `BASE: OBJECT`
  and `FIELD` blocks. Schemas are closed.

**Scope** (11)
: A declared set of things (included minus excluded) that a rule or data
  block applies to.

**Sequence** (10)
: Steps that run in their written order.

**Specification package** (2)
: The folder `LCL_Core_0.1.0` holding the official definition of the
  language. Every `lcl` command needs it.

**Stage** (15)
: One step of processing: lexical, grammar/schema, resolution,
  static/expression, validation, execution, verification/completion.

**Status** (9)
: The single final result of a run, such as `status.succeeded`,
  `status.failed` or `status.blocked`.

**Step** (10)
: One unit of execution holding exactly one action, sequence, phase or task.

**Task** (3)
: The block that ties together a goal, inputs, work, outputs and a success
  condition. Also a document of kind `kind.task`.

**UNKNOWN** (13)
: The special value meaning "a value exists but cannot be determined". It
  cannot be stored.

**Workspace** (11)
: A declared absolute folder that relative paths are measured from, and that
  they cannot leave.

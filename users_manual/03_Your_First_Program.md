# Chapter 3. Your first program

**In this chapter you will learn**

* the parts every executable LCL document has;
* what blocks, fields, IDs and `REF` are;
* how the parts connect, and in what order the engine uses them;
* how to change a program and predict what the tool will say.

## 3.1 The program

This is the program from Chapter 1. Save it as `double.lcl` and run it:

<!-- lcl: file=examples/01/double.lcl expect=run:succeeded -->
```lcl
LCL:
    VERSION: "0.1.0"

SPECIFICATION:
    ID: course.double
    NAME: "Double one number"
    VERSION: "1.0.0"
    KIND: kind.task

INPUT:
    ID: input.number
    TYPE: INTEGER
    VALUE: 21

OUTPUT:
    ID: output.doubled
    TYPE: INTEGER
    FORMAT: format.plain_text

GOAL:
    ID: goal.doubled
    ASSERT: REF(output.doubled) == 42

ACTION:
    ID: action.double
    OPERATION: core.calculate
    TARGET: REF(input.number)
    PARAMETER:
        NAME: expression
        TYPE: STRING
        REQUIRED: TRUE
        VALUE: "REF(input.number) * 2"
    OUTPUT: REF(output.doubled)

VERIFY:
    ID: verify.doubled
    ASSERT: REF(output.doubled) == 42

SUCCESS:
    ID: success.doubled
    ALL: [REF(verify.doubled)]

TASK:
    ID: task.double
    GOAL: REF(goal.doubled)
    INPUT: REF(input.number)
    ACTION: REF(action.double)
    OUTPUT: REF(output.doubled)
    SUCCESS: REF(success.doubled)

EXECUTE:
    REFERENCE: REF(task.double)
```

```
$ lcl run double.lcl
status.succeeded
  VERIFY verify.doubled = TRUE (required)
  SUCCESS success.doubled ALL = TRUE
  OUTPUT output.doubled published = 42
  because: SuccessSatisfied
  2 invocation(s), 0 event(s), 3 step(s)
```

## 3.2 Blocks and fields

An LCL document is a list of **blocks**. A block starts with an uppercase word
followed by a colon at the very start of a line, such as `INPUT:`. The lines
indented under it belong to it.

Inside a block, each line is a **field**: an uppercase key, a colon, one
space, and a value.

<!-- lcl: expect=fragment -->
```lcl
INPUT:
    ID: input.number
    TYPE: INTEGER
    VALUE: 21
```

Here `INPUT` is the block, and `ID`, `TYPE` and `VALUE` are its fields. Some
fields contain a block of their own. `PARAMETER` inside `ACTION` is an
example: its key is followed by a colon and a line break, and its own fields
are indented one level further.

Each kind of block has an exact list of fields it requires and fields it
allows. An unknown field, a missing required field or a field written twice
is an error.

## 3.3 The parts, one by one

### The header: `LCL`

<!-- lcl: expect=fragment -->
```lcl
LCL:
    VERSION: "0.1.0"
```

Every document starts with this. It names the exact language version the
document is written in. Only `"0.1.0"` is accepted. A range such as `"0.x"`
or a word such as `"latest"` is an error, because a document must mean the
same thing forever.

### The identity: `SPECIFICATION`

<!-- lcl: expect=fragment -->
```lcl
SPECIFICATION:
    ID: course.double
    NAME: "Double one number"
    VERSION: "1.0.0"
    KIND: kind.task
```

The second block is always `SPECIFICATION`. It says who this document is:

* `ID` is the document's identifier. Identifiers are lowercase words joined by
  dots.
* `NAME` is a label for people. It never affects meaning.
* `VERSION` is the version of *your* document, not of the language.
* `KIND` says what sort of document this is. `kind.task` means "a document
  that does something". Chapter 7 describes the other kinds.

### Data coming in: `INPUT`

`INPUT` declares a value the task works on: its `ID`, its `TYPE` and here its
`VALUE`. Chapter 8 shows how to supply an input from the command line instead.

### Data going out: `OUTPUT`

<!-- lcl: expect=fragment -->
```lcl
OUTPUT:
    ID: output.doubled
    TYPE: INTEGER
    FORMAT: format.plain_text
```

`OUTPUT` declares a result *before* it exists: its type and its format. It
has no value yet. An action will produce one.

### The intention: `GOAL`

`GOAL` states what the task is trying to achieve, here as a condition that can
be checked. It tells a reader, human or AI, what the work is *for*.

### The work: `ACTION`

<!-- lcl: expect=fragment -->
```lcl
ACTION:
    ID: action.double
    OPERATION: core.calculate
    TARGET: REF(input.number)
    PARAMETER:
        NAME: expression
        TYPE: STRING
        REQUIRED: TRUE
        VALUE: "REF(input.number) * 2"
    OUTPUT: REF(output.doubled)
```

An action performs one **operation** on one **target**. Operations are the
built-in verbs of LCL, and all of them have names starting with `core.`. You
will meet many; `core.calculate` computes a value from an expression.

* `TARGET` is what the action works on.
* Each `PARAMETER` supplies one named value to the operation. `core.calculate`
  needs a parameter called `expression`: the calculation, written as a string.
* `OUTPUT` names the output that receives the result.

### The proof: `VERIFY`

`VERIFY` is a check that runs **after** the work. If a required VERIFY is
false, the task cannot succeed.

### The finish line: `SUCCESS`

<!-- lcl: expect=fragment -->
```lcl
SUCCESS:
    ID: success.doubled
    ALL: [REF(verify.doubled)]
```

`SUCCESS` defines what "done" means. `ALL: [...]` means every listed
condition must be true. This is the only thing that decides whether the task
succeeded.

> **GOAL or SUCCESS?** The GOAL says what you intend. SUCCESS, built from
> VERIFY checks, is what gets measured. The engine decides the final status
> from SUCCESS, so always make sure your VERIFY checks actually test your
> GOAL. In this program both say `REF(output.doubled) == 42`.

### Tying it together: `TASK`

<!-- lcl: expect=fragment -->
```lcl
TASK:
    ID: task.double
    GOAL: REF(goal.doubled)
    INPUT: REF(input.number)
    ACTION: REF(action.double)
    OUTPUT: REF(output.doubled)
    SUCCESS: REF(success.doubled)
```

A `TASK` collects the pieces: its goal, inputs, actions, outputs and success
condition. It does not repeat them. It **refers** to them.

### Starting point: `EXECUTE`

`EXECUTE` names the one thing to run. A task document has exactly one.

## 3.4 IDs and `REF`

Everything that another part of the document needs to mention has an `ID`.
`REF(...)` refers to something by that ID:

* `REF(input.number)` in an expression means "the value of the input with ID
  `input.number`", which is 21.
* `REF(action.double)` in the TASK means "the action with ID
  `action.double`".

The dotted names such as `input.number` are a convention, not a rule: the
first part says what kind of thing it is. You could call the input
`number.one`, but `input.number` tells every reader what it is at a glance.
This manual uses these prefixes throughout: `input.`, `output.`, `goal.`,
`action.`, `verify.`, `success.`, `task.`.

A `REF` must match an ID exactly. There is no "closest match" and no guessing.

## 3.5 What order does it run in?

The blocks can appear in any order after `LCL` and `SPECIFICATION`, and
references can point forwards. The engine does not read top to bottom like a
recipe. It reads the whole document, resolves every reference, checks
everything, and only then starts:

1. `EXECUTE` points to `task.double`.
2. The task's `ACTION` field lists `action.double`, which runs and puts 42 into
   `output.doubled`.
3. `verify.doubled` checks the output.
4. `success.doubled` combines the checks and gives the final status.

## 3.6 Changing things

A good way to learn is to break the program on purpose and read what the tool
says. Try each change on a copy of `double.lcl`.

**Remove the `EXECUTE` block.**

```
$ lcl check double.lcl
double.lcl:50:1: error.block.required [grammar_or_schema]: a kind.task document requires exactly one EXECUTE root
```

**Misspell a reference**, for example `REF(task.dubble)` in EXECUTE:

```
double.lcl:52:20: error.reference.unresolved [resolution]: `task.dubble` does not resolve to a declaration or loop-local binding
```

**Give the input the wrong type**, for example `VALUE: "21"`, which is a
string and not an integer:

```
double.lcl:13:12: error.type.mismatch [static_or_expression]: this value is STRING, not the declared type INTEGER
```

All three are found by `lcl check`, before anything runs.

**Make the check wrong**: change `== 42` to `== 43` in the VERIFY only.
`lcl check` and `lcl validate` accept this, because the document is perfectly
well formed. `lcl run` does the work and then reports honestly that it did not
succeed:

```
$ lcl run double.lcl
double.lcl:36:9: error.verification.failed [verification_or_completion]: required VERIFY `verify.doubled` asserted FALSE
    A required post-execution VERIFY or TEST assertion is FALSE.

status.failed
  VERIFY verify.doubled = FALSE (required)
  SUCCESS success.doubled ALL = FALSE
  OUTPUT output.doubled published = 42
  ...
```

The exit code is 2: the document is valid, but the work did not succeed.

## 3.7 A template to start from

Most task documents in the first half of this course have the shape below.
Copy it whenever you start a new one, and replace the parts in angle brackets.
(The angle brackets are only placeholders here; they are not LCL syntax.)

<!-- lcl: expect=fragment -->
```lcl
LCL:
    VERSION: "0.1.0"

SPECIFICATION:
    ID: <your.document.id>
    NAME: "<What this task does>"
    VERSION: "1.0.0"
    KIND: kind.task

INPUT:
    ...

OUTPUT:
    ...

GOAL:
    ...

ACTION:
    ...

VERIFY:
    ...

SUCCESS:
    ...

TASK:
    ...

EXECUTE:
    REFERENCE: REF(<task.id>)
```

## Summary

* A document is a list of blocks, and each block has an exact set of fields.
* `LCL` comes first, then `SPECIFICATION`.
* INPUT and OUTPUT declare data, ACTION does work with an operation, VERIFY
  checks the result, SUCCESS decides the outcome, TASK ties them together, and
  EXECUTE starts it.
* `REF(id)` refers to a declaration by its exact ID.
* The engine resolves and checks the whole document before running anything.

## Exercises

1. Write `triple.lcl`, which triples the number 14 and checks that the result
   is 42.
2. Write `difference.lcl` with two inputs, 100 and 58, and one action that
   computes their difference. Check the result.
3. In `double.lcl`, move the `TASK` block to just after `SPECIFICATION`. Does
   the program still work? Why?
4. What happens if two blocks have the same ID? Try it with two INPUTs both
   called `input.number`.

Solutions: [solutions/README.md](solutions/README.md#chapter-3).

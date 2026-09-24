# Chapter 15. Reading error messages

**In this chapter you will learn**

* how to read every part of an `lcl` diagnostic;
* the seven stages a document passes through, and why only one stage's
  errors are shown at a time;
* a method for fixing a document that has several mistakes;
* the most common errors, what causes them, and how to fix them.

An LCL tool never repairs your document, and it never guesses what you
meant. In exchange, it tells you exactly what is wrong and where. Learning to
read its messages is the most useful skill in this course.

## 15.1 Anatomy of a diagnostic

```
bugs_2.lcl:47:17: error.reference.unresolved [resolution]: `action.dubble` does not resolve to a declaration or loop-local binding
    REF or an error/event/status alias BASE does not resolve to exactly one declaration, binding, or permitted canonical core identifier.

bugs_2.lcl was rejected at the resolution stage.
```

| Part | Here | Means |
|---|---|---|
| file | `bugs_2.lcl` | which document |
| line:column | `47:17` | where: line 47, character 17 |
| error code | `error.reference.unresolved` | exactly what kind of problem (one of 77 fixed codes) |
| stage | `[resolution]` | which processing stage found it |
| detail | `` `action.dubble` does not resolve ... `` | this particular case |
| indented line | `REF or an error/event/status alias ...` | the official meaning of the error code |
| last line | `rejected at the resolution stage` | how far the document got |

The error code is the most useful part. It is always the same for the same
kind of mistake, it is listed in the specification
(`06_STANDARD_LIBRARY/06_CORE_ERROR_IDENTIFIERS_PART_1.txt` and `..._PART_2.txt`),
and you can search for it.

When a stage finds several problems, all of them are listed. The first one is
the **primary** diagnostic, and the rest are indented under it:

```
two.lcl:12:5: error.keyword.case [lexical]: `Type` is a mixed-case spelling of the registered word `TYPE`
  two.lcl:17:5: error.keyword.case [lexical]: `Type` is a mixed-case spelling of the registered word `TYPE`
```

## 15.2 The stages

Every document passes through the same stages, in this order
(`01_FOUNDATION/03_NORMATIVE_PROCESSING_MODEL.txt`):

| Stage | Checks | Typical errors | Reached by |
|---|---|---|---|
| `lexical` | characters, indentation, words, literals | `error.source.tab`, `error.keyword.case`, `error.symbol.invalid`, `error.literal.invalid` | `check` |
| `grammar_or_schema` | block structure, required and allowed fields | `error.field.required`, `error.field.forbidden`, `error.block.required`, `error.grammar.invalid` | `check` |
| `resolution` | IDs, REFs, imports, namespaces, versions | `error.reference.unresolved`, `error.id.duplicate`, `error.version.unsupported` | `check` |
| `static_or_expression` | types, operators, function arguments | `error.type.mismatch`, `error.operator.operand`, `error.numeric.non_terminating` | `check` |
| `validation` | rules, permissions, VALIDATE checks | `error.validation.failed`, `error.conflict.hard`, `error.permission.denied` | `validate` |
| `execution` | running actions | `error.host.constraint`, `error.operation.precondition`, `error.required.missing` | `run` |
| `verification_or_completion` | VERIFY, TEST, SUCCESS | `error.verification.failed`, `error.success.unsatisfied` | `run` |

**A document stops at the first stage that fails.** The tool does not look
for type errors in a document whose grammar is broken, because it cannot know
what the broken part was meant to say. So when you fix one error, a
different one may appear from a later stage. That is progress, not a new
problem.

Two exceptions in the table are worth knowing. Some execution-stage errors,
such as a forbidden action, are found during `validate`, before anything
runs. And errors from the last two stages come with a status (`failed`,
`blocked`) rather than `rejected`, because by then the document was valid
and did run.

## 15.3 A debugging session

`bugs_1.lcl` is the doubling program from Chapter 3 with three mistakes.
Fix them one at a time, re-running `lcl check` each time.

**Round 1.**

<!-- lcl-run: file=examples/15/bugs_1.lcl expect=reject:error.keyword.case -->

```
$ lcl check bugs_1.lcl
bugs_1.lcl:18:5: error.keyword.case [lexical]: `Format` is a mixed-case spelling of the registered word `FORMAT`
...
bugs_1.lcl was rejected at the lexical stage.
```

Line 18, column 5 says `Format:`. Keywords are uppercase: change it to
`FORMAT:`. The result is `bugs_2.lcl`.

**Round 2.**

<!-- lcl-run: file=examples/15/bugs_2.lcl expect=reject:error.reference.unresolved -->

```
$ lcl check bugs_2.lcl
bugs_2.lcl:47:17: error.reference.unresolved [resolution]: `action.dubble` does not resolve to a declaration or loop-local binding
...
bugs_2.lcl was rejected at the resolution stage.
```

The document got further: past lexical and grammar, to resolution. Line 47
is in the TASK, which refers to `action.dubble`, but the action is called
`action.double`. Fix the spelling. The result is `bugs_3.lcl`.

**Round 3.**

<!-- lcl-run: file=examples/15/bugs_3.lcl expect=reject:error.type.mismatch -->

```
$ lcl check bugs_3.lcl
bugs_3.lcl:13:12: error.type.mismatch [static_or_expression]: this value is STRING, not the declared type INTEGER
...
bugs_3.lcl was rejected at the static_checking stage.
```

Line 13 is `VALUE: "21"`, a string where an integer was declared. Remove the
quotes.

**Done.**

<!-- lcl-run: file=examples/15/bugs_fixed.lcl expect=run:succeeded -->

```
$ lcl check bugs_fixed.lcl
bugs_fixed.lcl passed every stage through static_checking.
```

Then run `lcl validate` and `lcl run` as usual.

The three bugs were there from the start, but the tool showed them one stage
at a time. The method is always the same:

1. Read the **error code** and the **stage** first.
2. Go to the **line and column**.
3. Read the detail, and fix only that problem.
4. Run the same command again.

## 15.4 Common errors and their fixes

| Error | Usual cause | Fix |
|---|---|---|
| `error.source.tab` | a tab character, often inserted by the editor | set the editor to indent with spaces |
| `error.indentation.width` | 2 or 3 spaces of indentation | use exactly 4 spaces per level |
| `error.source.trailing_space` | a space at the end of a line | delete it; let the editor strip trailing spaces |
| `error.source.final_line_feed` | the file does not end with a line break | add one after the last line |
| `error.newline.invalid` | Windows (CRLF) line endings | save with LF line endings |
| `error.keyword.case` | `Type:`, `true`-style mistakes in keyword positions | uppercase keywords |
| `error.keyword.unknown` | a word LCL does not have, such as `MUST` or `WHILE` | see Appendix A; `MUST` is `REQUIRE`, and there is no `WHILE` |
| `error.symbol.invalid` | `=`, `%`, `'`, `//`, `&&`, ... | see the table in Chapter 4 |
| `error.literal.invalid` | `007`, `1_000`, `1.5e3`, a bad date, a misplaced `"""` | write the literal in its exact form |
| `error.field.forbidden` | a field the block does not have, often a typo | check the block's fields in Chapter 3 or the spec |
| `error.field.required` / `error.block.conditional_requirement` | a required field is missing | add it: the message names it |
| `error.field.duplicate` | a field written twice | remove one |
| `error.block.required` | a task without EXECUTE, or other missing block | add the block |
| `error.block.context` | a block not allowed in this document kind | check the kind (Chapter 7) |
| `error.reference.unresolved` | a misspelled ID, or a missing import prefix | copy the ID exactly; add `prefix.` for imports |
| `error.id.duplicate` | two declarations with one ID | rename one |
| `error.type.mismatch` | `"21"` for INTEGER, `5` for DECIMAL, `true` for BOOLEAN | write a value of the declared type |
| `error.operator.operand` | `"a" + "b"`, `NOT 1 == 2`, `TRUE AND 1` | check the operator table in Chapter 6 |
| `error.numeric.non_terminating` | `1 / 3` outside ROUND | use `ROUND(1 / 3, n)` |
| `error.operation.parameter` | a misspelled, missing or unknown parameter name | check the operation in Appendix B |
| `error.validation.failed` | a required VALIDATE was FALSE | the inputs do not meet the precondition |
| `error.permission.denied` | a FORBID applies, or a grant does not cover the target | add an OVERRIDE, or grant the right path |
| `error.host.constraint` | the tool was not granted a capability | add `--allow-read` / `--allow-write` / ... |
| `error.operation.precondition` | e.g. `core.create` on a file that already exists | use the right operation, or remove the file |
| `error.required.missing` | a value needed by an action or check does not exist | supply it, or give a DEFAULT |
| `error.verification.failed` | a required VERIFY or TEST was FALSE | the work did not produce what was required |

## 15.5 Machine-readable errors

Every command accepts `--machine`, which prints a JSON record instead of
text. It is meant for editors, scripts and other tools. Each diagnostic
includes its `id`, `stage`, `position` (`line`, `column`, `offset`),
`detail`, `meaning` and whether it is `primary`:

```
$ lcl check --machine bugs_3.lcl
{"protocol": "lcl.engine/1", "command": "check", ..., "diagnostics": [{"id": "error.type.mismatch", "stage": "static_or_expression", "position": {"line": 13, "column": 12, ...}, ...}]}
```

(The real output is longer. This line is shortened.)

The script that checks this manual,
[`tools/verify_examples.py`](tools/verify_examples.py), is built this way. It
reads the JSON to confirm each example's status and primary error.

## 15.6 Learning from the specification's own examples

The specification package includes 21 deliberately broken documents,
`08_EXAMPLES/INVALID/*.invalid.lcl`, each with a file saying which error it
must produce. They make an excellent quiz: open each one, predict the error,
and run `lcl check` or `lcl validate`. For example:

```
$ lcl check $LCL_SPEC/08_EXAMPLES/INVALID/12_FLOATING_VERSION.invalid.lcl
...: error.version.unsupported [resolution]: declared LCL version "latest" is not the supported exact version "0.1.0"
```

Some of them, such as `08_HARD_CONFLICT`, are rejected only by
`lcl validate`, because they fail at the rules stage.

## Summary

* A diagnostic gives file, line and column, error code, stage and detail.
  Read the code and stage first.
* Documents are checked in seven stages and stop at the first that fails, so
  errors appear one stage at a time.
* Fix one problem, re-run the same command, repeat.
* `--machine` gives the same information as JSON.

## Exercises

1. Take any working example from this course and introduce one mistake for
   each of the first four stages. Predict each error code before running
   `lcl check`.
2. Run `lcl check` on all 21 invalid examples in the specification package.
   Which ones pass `check` and are rejected only by `validate`? Why?
3. A friend's document fails with `error.operation.parameter` on an action
   using `core.sort` with a parameter named `order`. What is the likely fix?
4. Write a shell loop that runs `lcl check --machine` on every `.lcl` file in
   a folder and prints only the files that were rejected.

Solutions: [solutions/README.md](solutions/README.md#chapter-15).

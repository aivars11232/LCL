# Chapter 1. What is LCL?

**In this chapter you will learn**

* what problem LCL solves;
* how an LCL program differs from a program in Python or JavaScript;
* the twelve design principles that explain almost every rule you will meet;
* the basic words: LC, LCL, LCP, specification, interpreter, host.

## 1.1 The problem

Imagine asking an AI assistant:

> Please update the report with the latest numbers, fix anything that looks
> wrong, and don't break anything.

Every part of that request is vague. Which report? Which numbers count as
"latest"? What counts as "looks wrong", and is the assistant allowed to change
it? What does "break" mean, and how would anyone check? Two assistants, or the
same assistant on two different days, can read it in two different ways and
both believe they did what was asked.

LCL (the **Learned Computing Language**) replaces requests like that with a
document that leaves nothing to guess:

* the **goal**, stated as something that can be checked;
* the exact **inputs** and where they come from;
* the exact **operations** that may run, on exact **targets**;
* **rules**: what is required, what is allowed, what is forbidden;
* **checks** that run before and after the work;
* the one condition that means **success**.

An LCL document can be read by a person, checked by a tool before anything
happens, carried out by an AI or by a deterministic engine, and judged
afterwards against its own success condition.

## 1.2 How LCL differs from the languages you may know

If you have used Python or JavaScript, some things in LCL will feel unusual at
first:

| In most programming languages | In LCL |
|---|---|
| You write *how* to do something, step by step. | You declare *what* must be true, which operations are allowed, and how success is checked. Steps exist, but they are only one part of the document. |
| Variables can be changed (`x = x + 1`). | There is no assignment. Every value is fixed once it exists. New values come from operations, which write them to declared outputs. |
| Loops can run forever (`while True:`). | Every repetition is bounded. There is no `WHILE`; `FOR EACH` walks a finite list, and retries have a fixed limit. |
| A missing value is often `None` or `null`, and the language may guess. | Three different things are kept apart: `MISSING` (no value exists), `UNKNOWN` (a value exists but cannot be determined) and `NULL` (the value is known to be empty). Nothing is guessed. |
| Comments are ignored text such as `# ...`. | There is no comment symbol. A `COMMENT` is an explicit block, and prose never contains hidden instructions. |
| A program may do anything its process can do. | A document may only do what it declares, and the host running it grants nothing unless told to. |
| Errors are often discovered while running. | Most errors are found before anything happens: the tool checks spelling, structure, names, types and permissions first. |

## 1.3 The design principles

The specification lists twelve principles
(`01_FOUNDATION/02_DESIGN_PRINCIPLES.txt`). Nearly every rule in this manual
follows from one of them, so they are worth reading now and again at the end
of the course.

1. **Explicit source controls; conversational context does not.** Only what
   is written in the document counts. What was said earlier in a chat does not.
2. **Technical English remains readable but has constrained syntax.** LCL
   looks like English headings and fields, but only exact forms are allowed.
3. **One keyword has one core meaning.** `REQUIRE` always means "hard rule".
   It never also means "please".
4. **Exact identifiers replace vague references.** Instead of "the file from
   before" you write `REF(input.report)`.
5. **Hard rules and soft preferences are distinct.** `REQUIRE` and `FORBID` must
   hold. `PREFER` is a wish that can never defeat them.
6. **Permission is distinct from requirement.** `ALLOW` means "may", and never
   "must".
7. **Missing, unknown, and null are distinct.**
8. **Every repetition and retry is bounded.**
9. **Side effects, scope, and success are observable.** You can always tell
   what was changed, where, and whether the task succeeded.
10. **Domain vocabulary extends the core without changing core grammar.** You
    can add your own operations and types, but not new syntax.
11. **Unsupported syntax fails closed.** Anything the language does not define
    is an error, never "whatever the tool decides".
12. **Equal valid input under deterministic declarations has equivalent
    meaning across conforming interpreters.** Two correct tools must agree.

## 1.4 The vocabulary

These terms come from `01_FOUNDATION/01_PURPOSE_AND_TERMINOLOGY.txt`:

**LC (Learned Computing)**
: Trained computational systems, commonly called AI.

**LCL (Learned Computing Language)**
: This declarative specification language.

**LCP (Learned Computing Programming)**
: Programming at the level of goals, constraints, operations, outputs and
  verification, instead of writing the implementation by hand.

**Automated coding**
: The branch of LCP in which an LC writes or changes program code from an LCL
  specification.

**Specification**
: One versioned LCL document together with the exact documents it imports.
  In this manual, "document" and "program" both mean this.

**Interpreter**
: An AI system or a deterministic implementation that validates an LCL
  document and acts on it. The `lcl` tool you will use is a deterministic
  interpreter.

**Host**
: The product or runtime that provides files, tools, permissions and
  execution. When you run `lcl`, `lcl` is also the host. It decides what the
  document is actually allowed to touch.

## 1.5 A first look

Here is a complete LCL program. You will take it apart line by line in
[Chapter 3](03_Your_First_Program.md). For now, notice how it reads: it
declares an input, an output, a goal, one action, a check, what success means,
the task that ties them together, and what to execute.

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

That is a lot of text for "multiply 21 by 2", and on purpose. The point of LCL
is not to be short. The point is that every question someone might ask about
the task already has its answer in the document: what goes in, what comes out,
which operation runs, and how anyone can tell whether it worked.

## Summary

* LCL states goals, inputs, operations, rules, checks and success exactly, so
  that people, tools and AI systems all read a task the same way.
* There is no assignment, no unbounded loop, no hidden comment syntax and no
  guessing.
* The twelve design principles explain the rules you will meet.
* The host decides what a document can actually touch. By default, the `lcl`
  tool grants nothing.

## Exercises

1. Take the request in section 1.1 and list every question an assistant would
   have to guess the answer to.
2. For each question, say which kind of LCL declaration would answer it:
   input, output, goal, action, rule, check or success.
3. Principle 6 says permission is distinct from requirement. Describe a
   real-life situation where "you may" and "you must" lead to different
   results.

Solutions: [solutions/README.md](solutions/README.md#chapter-1).

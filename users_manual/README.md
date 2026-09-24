# LCL Users Manual

**A course in the Learned Computing Language, Core 0.1.0**

This manual teaches LCL the way a school course teaches a programming language:
one idea at a time, each with small programs you can run, a few exercises, and
worked solutions. It assumes you can use a terminal and a text editor. You do
not need to have programmed before, though it helps.

LCL is a language for telling a *learned computing* system (an AI) or a
deterministic engine exactly what to do: the goal, the inputs, the allowed
operations, what must never happen, and how to prove the work is done. Where an
ordinary instruction says "please tidy up the files carefully", an LCL document
says which files, which operation, what is forbidden, and which check must pass
afterwards.

## How to use this manual

Work through the chapters in order. Each one builds on the last.

1. Read the chapter.
2. Type in the examples yourself instead of copying them. Most of what you
   learn about LCL comes from the tool telling you exactly what you got wrong.
3. Run every example with `lcl`, and compare what you see with what the
   chapter shows.
4. Do the exercises at the end, then check [the solutions](solutions/README.md).

Every complete program in the chapters is also in [`examples/`](examples/),
exactly as printed, and every program in the solutions is in
[`solutions/`](solutions/). All of them have been checked against the real `lcl`
tool, and you can repeat that check yourself (see
[Checking the manual](#checking-the-manual)).

## Course map

| Chapter | You will learn |
|---|---|
| [1. What is LCL?](01_What_Is_LCL.md) | Why LCL exists, what makes it different, and the words it uses |
| [2. Installing and running LCL](02_Installing_and_Running.md) | Installing the tools, telling them where the specification is, the four commands |
| [3. Your first program](03_Your_First_Program.md) | The shape of a task, line by line; your first run |
| [4. How LCL text is written](04_How_LCL_Text_Is_Written.md) | Indentation, keywords, names, strings, numbers, forbidden symbols |
| [5. Values and types](05_Values_and_Types.md) | Numbers, text, true/false, lists, sets, objects, dates, measures, enums |
| [6. Expressions and operators](06_Expressions_and_Operators.md) | Arithmetic, exact division, comparison, logic, built-in functions, patterns |
| [7. Documents, declarations and references](07_Documents_Declarations_and_References.md) | Document kinds, IDs, `REF`, constants and your own types |
| [8. Inputs, outputs and actions](08_Inputs_Outputs_and_Actions.md) | Getting values in, doing work with operations, getting results out |
| [9. Checking results](09_Checking_Results.md) | Goals, VALIDATE, VERIFY, SUCCESS, FAILURE, TEST documents, statuses |
| [10. Steps, branches and loops](10_Steps_Branches_and_Loops.md) | SEQUENCE, PHASE, STEP, IF/ELSE, FOR EACH, sorting and filtering lists |
| [11. Files, workspaces and permissions](11_Files_Workspaces_and_Permissions.md) | Reading and writing files safely; what the tool is allowed to touch |
| [12. Rules, authority and conflicts](12_Rules_Authority_and_Conflicts.md) | REQUIRE, ALLOW, FORBID, PREFER, PRESERVE, AUTHORITY, PRIORITY, OVERRIDE |
| [13. Missing values, errors and recovery](13_Missing_Values_Errors_and_Recovery.md) | MISSING, UNKNOWN, NULL, DEFAULT, ASSUME, handlers and RETRY |
| [14. Projects and imports](14_Projects_and_Imports.md) | Splitting work across files, namespaces, project files, lock files |
| [15. Reading error messages](15_Reading_Error_Messages.md) | How to read a diagnostic, the processing stages, the most common mistakes |
| [16. Capstone project](16_Capstone_Project.md) | Building a complete, checked program from a plain-English request |
| [17. Tools reference](17_Tools_Reference.md) | Every `lcl` command and option, exit codes, `lcl-workspace` |
| [Appendix A. Keywords](Appendix_A_Keyword_Reference.md) | All 141 reserved words, grouped by purpose |
| [Appendix B. Operations](Appendix_B_Operation_Reference.md) | Every built-in `core.*` operation and what it needs |
| [Appendix C. Glossary](Appendix_C_Glossary.md) | The terms used in this manual |

A one-term course might cover chapters 1–10 as the core, 11–14 as the
second half, and use 15–16 as the final project.

## Conventions used in this manual

* `Code in this style` is something you type or that the tool prints.
* A block starting with `$` is a terminal command. Type what follows the `$`.
* Lines after a command show what the tool prints.
* **Tool note** boxes describe how the current reference tool behaves where it
  differs from, or does not yet cover, the language specification. The
  specification is what LCL *means*; the notes tell you what you will *see*.

## What this manual covers

This manual teaches **LCL Core 0.1.0**, the released version of the language,
and the reference tools `lcl` and `lcl-workspace` that come with the LCL
release.

LCL Core 0.2.0 adds programs written with localized keywords (for example in
Latvian or Chinese). It is still a candidate under review, so this manual does
not teach it yet.

The complete, authoritative definition of the language is the specification
package in `canonical/LCL_Core_0.1.0/` of the LCL repository. When this manual
and the specification disagree, the specification is right. The manual names
the specification file for each topic so you can read the exact rule.

## Checking the manual

The script [`tools/verify_examples.py`](tools/verify_examples.py) runs every
program in this manual through `lcl` and checks that it behaves as the text
says: accepted, rejected with the stated error, or finished with the stated
status. From the repository root:

```
$ export LCL_SPEC=$PWD/canonical/LCL_Core_0.1.0
$ python3 users_manual/tools/verify_examples.py --lcl ~/.local/bin/lcl
```

It ends with a summary line, `97 example(s) checked, 30 fragment(s) skipped,
0 failure(s)` for this edition, and exits with status 0 only when every
example behaved as described. (Fragments are the short pieces of a document
shown on their own, which cannot run by themselves.) Examples that use files
work in folders under `/tmp/lcl-manual/`, which the script creates and removes
again. If you change an example, run the script again.

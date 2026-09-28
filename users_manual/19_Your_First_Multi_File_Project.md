# Chapter 19. Your first multi-file project

This chapter walks through a whole project in the LCL Workspace on your
computer, step by step, and shows where the same things are on your phone.
It uses the file roles and starting structures from
[Chapter 18](18_Starting_Files_From_Templates.md). Multi-file projects are part
of LCL Core 0.3.0, which is still a candidate version.

## 19.1 The Users Manual is always one click away

In the Workspace, the small **?** button at the top right, next to Settings,
opens this manual in a window of its own. Your documents stay open, with
their unsaved edits, and you can keep the manual beside the editor while you
work. The manual is part of the installed Workspace, so it needs no internet
connection. Use its search box, the contents on the left, and the ← and →
buttons to move back and forward; it remembers where you were until you
close the browser.

In LCL for Android, the bottom bar has two tabs, **Workspace** and
**Manual**. The **?** button on the home and file screens switches to the
Manual tab. The manual is inside the app: it opens with no PC connected, and
switching tabs keeps your open files, your unsaved text and the connection
as they were. Both show the same manual; its version and a short digest are
shown at the top.

## 19.2 New Project

1. Click **⊞ New project** at the top of the Project panel.
2. Type a **Project name**, for example `Shop`: letters, digits, `_`, `-` and
   `.`. The project is a new folder of that name in your **Projects folder**
   (Settings, see Chapter 2), and the dialog shows its full path. A name that
   is already taken is refused and the existing folder is not touched. If the
   window shows another folder, creating the project opens the Projects
   folder in it.
3. **LCL Core version** shows 0.3.0, the version projects need.
4. **Start from**: *Default (Guided)* is right for a first project. *Canonical*
   always uses LCL's own starting structure; a project Master of your own
   appears here too (see 19.10).
5. Look at the preview: every file is listed with its role and its exact
   text. Nothing has been written yet.
6. Click **Create**. All the files are written, or none are.

A new project has one file of each kind, each in its own folder:
`description/`, `context/`, `definitions/`, `rules/`, `contracts/`,
`bindings/`, `usage/`, `stop_conditions/`, `tasks/` (`task_001`), `checks/`,
`data/` and `output/`, with the entry `main.lcl` beside them. Every name ends
with the default file type from Settings, so with `.lcl.txt` the entry is
`main.lcl.txt`. The entry lists the files as `PART`s in that order, which is
the order they are read in. The project opens on the entry, and the **Project readiness** panel under the
file list shows every file of the project and whether it is ready.

## 19.3 Fill in Description, Rules and Task

Each new file is a structure with empty fields, called *slots*. A slot is a
field with nothing after its colon, such as `NAME:`. The file cannot run
until every slot is filled in — or, for an optional one, deleted.

1. Open `description/description.lcl` and fill its slots: a `NAME` in quotes and the
   `VERSION`. Every file of one project declares the same `SPECIFICATION`
   `VERSION`, for example `"1.0.0"`.
2. Open `rules/rules.lcl` and fill its slots the same way, then write the rules the
   task must follow (see [Chapter 12](12_Rules_Authority_and_Conflicts.md)).
3. Open `tasks/task_001.lcl`, fill its header slots, and write the task itself: its
   `TASK`, and the `ACTION`s, `OUTPUT`s and checks it needs (Chapters 3 to 10).
4. Back in `main.lcl`, fill its header and the `EXECUTE` `REFERENCE`: the ID
   of the task to run, written `REF(task.something)`.
5. Fill the slots of the other files the same way, or delete a slot that is
   optional. A file the project does not need can be deleted together with
   its `PART` in `main.lcl`.

The files share one set of IDs, so `tasks/task_001.lcl` can name a rule or a value
declared in another file with a plain `REF(...)`.

## 19.4 Check, fix, Validate, Run

1. With a file open, click **Check**. Problems appear in red, in the editor
   margin and in the Diagnostics panel. Fix them one by one; Check again.
2. Open `main.lcl` and click **Validate**. Validate goes as far as a run goes
   before anything is done, for the whole project.
3. The **Project readiness** panel now says, file by file, *ready*,
   *invalid* or *missing*, and for the project as a whole *ready*,
   *incomplete* (a required file is missing) or *invalid*. Click a file to
   open it, or a problem to go to it.
4. When the project is **ready**, click **Run** with `main.lcl` open. Until
   then LCL refuses to run it, whatever the buttons show: a project runs only
   when every required file has been read and accepted.

On the phone, open the project's folder: files show their role, and the
entry has a **Readiness** button that shows the same file-by-file answer,
straight from the PC.

## 19.5 More files by role

The **+** button makes one new file. Under **Kind of file**, choose *Blank
LCL file*, or one of the twelve kinds — Description, Context, Definitions,
Rules, Contracts, Bindings, Usage, Stop Conditions, Task, Checks, Data or
Output — and under **Start from** the default, a canonical
structure or one of your Masters. The dialog shows the exact text first. To
make the new file part of the project, add a `PART` for it to `main.lcl`.

On the phone, **New** offers the same kinds, Guided or Minimal; the PC writes
the file, so both always start from exactly the same structure.

## 19.6 When a file changed somewhere else

If you save a file that was changed on disk after you opened it — on your
phone, in another window or in another program — LCL does not write it.
Instead it says **Changed on disk** and lets you choose:

* **Reload disk version** replaces your edits with what is on disk.
* **Keep mine…** writes your version over it, after you confirm once more.
* **Cancel** leaves everything as it is, so you can copy your edits first.

LCL never merges the two versions for you. The phone behaves the same way.

## 19.7 Turning a single document into a project

A single task document keeps working as it is. If you want it as a project,
right-click it in the file list and choose **Convert to multi-file
project…**. You choose a new folder and see the exact result first: an
entry `main.lcl` with the document's `IMPORT`, `EXTENSION` and `EXECUTE`
blocks, and one `task.lcl` holding everything else, unchanged. The
conversion is only offered when the resulting project would be ready, and
your original document is never changed. Splitting the task further, into
rules, data and so on, is up to you: which file a block belongs in is your
decision, not something LCL guesses.

## 19.8 Templates

**Settings → Templates…** lists your Master templates (see
[Chapter 18](18_Starting_Files_From_Templates.md)). There you can make a new
one from a role's canonical structure, edit, duplicate or delete one, and
choose each role's default — or *Canonical scaffold* to go back to LCL's own.
A template is checked when you save it; one that is not valid is not saved.
Files you already made from a template never change when you edit or delete
it.

## Summary

* **?** opens this manual: a separate window on the computer, the Manual tab
  on the phone. Both work offline and never touch your files.
* New Project shows every file before it writes any. Fill each file's slots,
  Check, fix, Validate `main.lcl`, and Run once the project is ready.
* A save over a newer version on disk is refused; you choose Reload or Keep
  mine.
* Convert to multi-file project makes a new project and leaves the original
  as it was.

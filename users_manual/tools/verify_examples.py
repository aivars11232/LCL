#!/usr/bin/env python3
"""Check every LCL example in the users manual against the real `lcl` tool.

Every fenced ```lcl block in a manual page must be preceded by an annotation
comment that says what the block is and what the tool must do with it:

    <!-- lcl: file=examples/03/hello.lcl expect=run:succeeded -->

Keys:
    file=PATH        The block must equal this file (relative to the manual
                     root) byte for byte, and the file is what gets tested.
    expect=WHAT      One of:
                       check            `lcl check` exits 0
                       validate         `lcl validate` exits 0
                       run:STATUS       `lcl run` ends in status.STATUS
                       reject:ERROR     `lcl validate` rejects the document
                                        and ERROR is the primary diagnostic
                       fragment         not a whole document; not run
    primary=ERROR    With run:STATUS, the primary diagnostic must be ERROR.
    args="..."       Extra command-line arguments for the run.
    workspace=DIR    Recreate DIR (it must be under /tmp/lcl-manual/) empty
                     before the run. Every such folder is removed again
                     when the script finishes.
    seed=PATH        Copy this directory's files (relative to the manual root)
                     into the workspace first.
    grant=r|w|rw     Pass --allow-read and/or --allow-write for the workspace.
    produces=NAME    After the run, the workspace must contain NAME.

A block without an annotation is an error, so nothing in the manual can go
untested by accident. A standalone comment

    <!-- lcl-run: file=examples/08/shop.lcl expect=run:failed args="..." -->

runs one more check on an example file without showing its text again, for
example the same program with a different command-line input. Usage:

    python3 users_manual/tools/verify_examples.py [--lcl PATH] [--spec PATH]

`--lcl` defaults to `lcl` on PATH and `--spec` to the LCL_SPEC environment
variable. The exit status is 0 only when every example behaved as annotated.
"""

import argparse
import json
import os
import re
import shlex
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

MANUAL = Path(__file__).resolve().parent.parent
WORKSPACE_ROOT = "/tmp/lcl-manual/"
workspaces_used = set()
ANNOTATION = re.compile(r"^<!--\s*lcl:\s*(.*?)\s*-->\s*$")
EXTRA_RUN = re.compile(r"^<!--\s*lcl-run:\s*(.*?)\s*-->\s*$")
FENCE_OPEN = re.compile(r"^```lcl\s*$")


def parse_annotation(text):
    fields = {}
    for token in shlex.split(text):
        if "=" in token:
            key, value = token.split("=", 1)
            fields[key] = value
        else:
            fields[token] = ""
    return fields


def blocks(page):
    """Yield (line number, annotation fields or None, block text or None).

    A standalone lcl-run comment yields its fields with no block text."""
    lines = page.read_text(encoding="utf-8").split("\n")
    i = 0
    while i < len(lines):
        extra = EXTRA_RUN.match(lines[i])
        if extra:
            yield i + 1, parse_annotation(extra.group(1)), None
        if FENCE_OPEN.match(lines[i]):
            start = i
            j = i - 1
            while j >= 0 and lines[j].strip() == "":
                j -= 1
            match = ANNOTATION.match(lines[j]) if j >= 0 else None
            fields = parse_annotation(match.group(1)) if match else None
            body = []
            i += 1
            while i < len(lines) and lines[i] != "```":
                body.append(lines[i])
                i += 1
            yield start + 1, fields, "\n".join(body) + "\n"
        i += 1


def run_tool(lcl, spec, command, document, extra):
    argv = [lcl, command, "--machine", "--spec", spec] + extra + [str(document)]
    done = subprocess.run(argv, capture_output=True, text=True)
    record = None
    if done.stdout.strip().startswith("{"):
        try:
            record = json.loads(done.stdout)
        except json.JSONDecodeError:
            record = None
    return done.returncode, record, done.stderr


def primary_id(record):
    for diagnostic in (record or {}).get("diagnostics", []):
        if diagnostic.get("primary"):
            return diagnostic.get("id")
    return None


def prepare_workspace(fields):
    workspace = fields.get("workspace")
    if not workspace:
        return None, []
    if not workspace.startswith(WORKSPACE_ROOT) or ".." in workspace:
        raise ValueError(f"workspace {workspace} is not under {WORKSPACE_ROOT}")
    path = Path(workspace)
    workspaces_used.add(workspace)
    if path.exists():
        shutil.rmtree(path)
    path.mkdir(parents=True)
    seed = fields.get("seed")
    if seed:
        for item in (MANUAL / seed).iterdir():
            shutil.copy2(item, path / item.name)
    grants = []
    grant = fields.get("grant", "")
    if "r" in grant:
        grants += ["--allow-read", workspace]
    if "w" in grant:
        grants += ["--allow-write", workspace]
    return path, grants


def check_example(lcl, spec, fields, document):
    """Return None when the example behaves as annotated, else a reason."""
    expect = fields.get("expect", "")
    extra = shlex.split(fields.get("args", ""))
    workspace, grants = prepare_workspace(fields)
    extra += grants

    if expect in ("check", "validate"):
        code, record, err = run_tool(lcl, spec, expect, document, extra)
        if code != 0:
            return f"`lcl {expect}` exited {code} (primary {primary_id(record)}) {err.strip()}"
        return None

    if expect.startswith("reject:"):
        wanted = expect.split(":", 1)[1]
        code, record, err = run_tool(lcl, spec, "validate", document, extra)
        got = primary_id(record)
        if code != 1 or got != wanted:
            return f"expected rejection by {wanted}, got exit {code} and primary {got} {err.strip()}"
        return None

    if expect.startswith("run:"):
        wanted = "status." + expect.split(":", 1)[1]
        code, record, err = run_tool(lcl, spec, "run", document, extra)
        status = ((record or {}).get("completion") or {}).get("terminal_status")
        if status != wanted:
            return f"expected {wanted}, got {status} (exit {code}, primary {primary_id(record)}) {err.strip()}"
        expected_code = 0 if wanted == "status.succeeded" else 2
        if code != expected_code:
            return f"{wanted} but exit {code}, expected {expected_code}"
        if "primary" in fields and primary_id(record) != fields["primary"]:
            return f"expected primary {fields['primary']}, got {primary_id(record)}"
        produced = fields.get("produces")
        if produced and not (workspace / produced).exists():
            return f"the run did not produce {produced} in {workspace}"
        return None

    return f"unknown expectation {expect!r}"


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("--lcl", default=shutil.which("lcl") or "lcl")
    parser.add_argument("--spec", default=os.environ.get("LCL_SPEC"))
    options = parser.parse_args()
    if not options.spec:
        print("no specification package: pass --spec or set LCL_SPEC", file=sys.stderr)
        return 3

    failures = 0
    checked = 0
    fragments = 0
    referenced = set()
    pages = sorted(p for p in MANUAL.rglob("*.md") if "tools" not in p.parts)
    with tempfile.TemporaryDirectory(prefix="lcl-manual-") as scratch:
        for page in pages:
            where = page.relative_to(MANUAL)
            for line, fields, text in blocks(page):
                label = f"{where}:{line}"
                if fields is None:
                    print(f"FAIL  {label}: ```lcl block has no <!-- lcl: ... --> annotation")
                    failures += 1
                    continue
                if fields.get("expect") == "fragment":
                    fragments += 1
                    continue
                if "file" in fields:
                    document = MANUAL / fields["file"]
                    referenced.add(document.resolve())
                    if not document.is_file():
                        print(f"FAIL  {label}: {fields['file']} does not exist")
                        failures += 1
                        continue
                    if text is not None and document.read_text(encoding="utf-8") != text:
                        print(f"FAIL  {label}: block differs from {fields['file']}")
                        failures += 1
                        continue
                elif text is None:
                    print(f"FAIL  {label}: lcl-run needs file=")
                    failures += 1
                    continue
                else:
                    document = Path(scratch) / f"block_{checked}.lcl"
                    document.write_text(text, encoding="utf-8")
                checked += 1
                problem = check_example(options.lcl, options.spec, fields, document)
                if problem:
                    print(f"FAIL  {label}: {problem}")
                    failures += 1
                else:
                    print(f"ok    {label}: {fields.get('expect')}")

    # The same recognition rule as the LCL tools: `.lcl` or `.lcl.txt`, and
    # never an ordinary `.txt` file.
    sources = [p for p in MANUAL.rglob("*") if p.is_file() and p.name.endswith((".lcl", ".lcl.txt"))]
    for document in sorted(sources):
        if document.resolve() not in referenced and "seed" not in "/".join(document.parts):
            print(f"FAIL  {document.relative_to(MANUAL)}: not shown in or checked by any page")
            failures += 1

    created = [Path(p) for p in sorted(workspaces_used)]
    for path in created:
        if path.exists():
            shutil.rmtree(path)

    print(f"\n{checked} example(s) checked, {fragments} fragment(s) skipped, {failures} failure(s)")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())

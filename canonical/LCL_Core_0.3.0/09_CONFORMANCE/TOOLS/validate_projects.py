#!/usr/bin/env python3
"""Validate the LCL 0.3.0 project contract and its project fixtures.

This is a bounded static checker for 05_SEMANTICS/13 and
10_REGISTRIES/block_schemas_v0.3.0.json#/project_contract. It is not an LCL
lexer, parser, resolver, type checker or executor. It reads top-level blocks,
single-line fields and REF identifiers well enough to evaluate the project
fixtures, whose every rejecting project plants exactly one defect, and it
fails closed on any fixture it cannot evaluate exactly. Localized units are
evaluated with validate_localization.py from this package.

It never enumerates a project directory to find parts: the in-memory file map
is consulted only for paths that a PART or IMPORT names, so file system order
cannot reach an evaluation.
"""

from __future__ import annotations

import argparse
import json
import posixpath
import re
import runpy
import sys
from pathlib import Path
from typing import Any


VERSION = "0.3.0"
STAGES = [
    "localization",
    "lexical",
    "grammar_or_schema",
    "resolution",
    "static_or_expression",
    "validation",
    "execution",
    "verification_or_completion",
]
EXECUTION_FAMILY = {"GOAL", "TASK", "DEPENDENCY", "ACTION", "PHASE", "SEQUENCE", "HANDLER"}
NON_NORMATIVE = {"COMMENT", "EXAMPLE"}
ENTRY_ONLY = {"IMPORT", "EXTENSION", "EXECUTE"}
EXECUTE_REQUIRED = {"kind.task", "kind.test", "kind.project"}
CONTRACT_KEYS = [
    "identity", "entry", "parts", "requiredness", "order", "namespace",
    "versions", "authority", "check_selection", "localization", "admission", "standalone",
]
PROJECT_ERRORS = {
    "error.project.part_duplicate", "error.project.part_kind",
    "error.project.part_missing", "error.project.placement",
}
HEADER = re.compile(r"^([A-Z][A-Z_]*):$")
FIELD = re.compile(r"^    ([A-Z][A-Z_]*):(?: (.*))?$")
REF = re.compile(r"REF\(([a-z][a-z0-9_]*(?:\.[a-z][a-z0-9_]*)*)\)")
RELATIVE_PATH = re.compile(r'^PATH\("([^"/][^"]*)"\)$')
STRING = re.compile(r'^"([^"]*)"$')


def load_json_strict(path: Path) -> Any:
    def reject_duplicates(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
        value: dict[str, Any] = {}
        for key, item in pairs:
            if key in value:
                raise ValueError(f"duplicate JSON key {key!r} in {path}")
            value[key] = item
        return value

    return json.loads(path.read_text(encoding="utf-8"), object_pairs_hook=reject_duplicates)


class Registry:
    def __init__(self, root: Path) -> None:
        registries = root / "10_REGISTRIES"
        schemas = load_json_strict(registries / f"block_schemas_v{VERSION}.json")
        groups = load_json_strict(registries / f"built_in_groups_and_results_v{VERSION}.json")["enum_groups"]
        self.contract = schemas["project_contract"]
        self.kind_blocks = {kind: set(blocks) for kind, blocks in schemas["document_kind_blocks"].items()}
        self.kind_block_lists = schemas["document_kind_blocks"]
        self.document_kinds = groups["document_kinds"]
        self.part_kinds = groups["project_part_kinds"]
        self.errors = load_json_strict(registries / f"statuses_and_errors_v{VERSION}.json")["errors"]
        self.part_signature = load_json_strict(registries / f"field_signatures_v{VERSION}.json")["blocks"]["PART"]
        self.part_schema = schemas["schemas"]["PART"]


def check_contract(registry: Registry) -> dict[str, Any]:
    violations: list[str] = []

    def expect(condition: bool, message: str) -> None:
        if not condition:
            violations.append(message)

    expect(list(registry.contract) == CONTRACT_KEYS, "project_contract keys differ from the closed contract")
    expect(all(isinstance(v, str) and v for v in registry.contract.values()), "project_contract values must be non-empty")
    expect(registry.part_kinds == [k for k in registry.document_kinds if k.startswith("kind.part.")],
           "project_part_kinds must be exactly the kind.part document kinds, in order")
    expect("kind.project" in registry.document_kinds, "kind.project is not a document kind")
    expect(set(registry.document_kinds) == set(registry.kind_blocks), "every document kind needs block legality, and only those")
    task = registry.kind_blocks["kind.task"]
    expect(registry.kind_blocks["kind.project"] == {"IMPORT", "EXTENSION", "PART", "EXECUTE"} | NON_NORMATIVE,
           "kind.project admits exactly PART, IMPORT, EXTENSION, EXECUTE, COMMENT and EXAMPLE")
    expect(registry.kind_blocks["kind.part.task"] == task - ENTRY_ONLY,
           "kind.part.task admits every kind.task block except IMPORT, EXTENSION and EXECUTE")
    narrow = [k for k in registry.part_kinds if k != "kind.part.task"]
    families = [registry.kind_blocks[k] - NON_NORMATIVE for k in narrow]
    expect(all(NON_NORMATIVE <= registry.kind_blocks[k] for k in registry.part_kinds), "every part kind admits COMMENT and EXAMPLE")
    expect(all(not (registry.kind_blocks[k] & (ENTRY_ONLY | {"PART"})) for k in registry.part_kinds),
           "no part kind admits PART, IMPORT, EXTENSION or EXECUTE")
    expect(sum(len(f) for f in families) == len(set().union(*families)), "narrow part families overlap")
    expect(set().union(*families) | EXECUTION_FAMILY == task - ENTRY_ONLY - NON_NORMATIVE,
           "the narrow families and the execution family do not partition kind.task's normative blocks")
    expect(registry.kind_blocks["kind.part.description"] == NON_NORMATIVE, "kind.part.description is non-normative only")
    for name in PROJECT_ERRORS:
        error = registry.errors.get(name)
        expect(error is not None and error["stage"] == "resolution" and error["default_status"] == "status.invalid"
               and error["recoverable_with_declared_handler"] is False and error["event"] is None,
               f"{name} is not the registered resolution-stage contract")
    expect(sorted(n for n in registry.errors if n.startswith("error.project.")) == sorted(PROJECT_ERRORS),
           "the error.project family differs from the four registered identifiers")
    fields = registry.part_signature["fields"]
    expect([n for n, f in fields.items() if f["required"]] == ["ID", "SOURCE", "KIND"], "PART requires exactly ID, SOURCE and KIND")
    expect(fields["KIND"]["value_kind"] == "qualified_identifier(part_kind)", "PART KIND is not the part_kind domain")
    expect(fields["SOURCE"]["value_kind"] == "part_source_path", "PART SOURCE is not a part-relative PATH")
    expect(fields["REQUIRED"]["default"] is True, "PART REQUIRED does not default to TRUE")
    expect(registry.part_schema["contexts"] == ["top_level"] and registry.part_schema["rules"] == registry.part_signature["conditional_requirements"],
           "PART schema and field signature disagree")
    return {"check": "project_contract_structure", "passed": not violations, "violations": violations}


class Unit:
    """One source unit as far as this bounded checker reads it."""

    def __init__(self, source: str, data: bytes) -> None:
        self.source = source
        self.data = data
        self.usable = False
        self.diagnostics: list[dict[str, Any]] = []
        self.locale = "canonical"
        self.blocks: list[dict[str, Any]] = []
        self.kind: str | None = None
        self.kind_offset = 0
        self.lcl_version: str | None = None
        self.lcl_version_offset = 0
        self.spec_version: str | None = None
        self.spec_version_offset = 0
        self.refs: list[tuple[str, int]] = []

    def fail(self, error: str, stage: str, offset: int) -> None:
        self.diagnostics.append({"error": error, "stage": stage, "source": self.source, "offset": offset})

    def block(self, name: str) -> dict[str, Any] | None:
        return next((b for b in self.blocks if b["name"] == name), None)


class Checker:
    def __init__(self, root: Path, registry: Registry) -> None:
        self.registry = registry
        tools = root / "09_CONFORMANCE" / "TOOLS"
        self.mask = runpy.run_path(str(tools / "validate_source_fixtures.py"))["outside_string_mask"]
        self.loc = runpy.run_path(str(tools / "validate_localization.py"))
        self.contract = self.loc["Contract"](root)
        fixtures = root / "09_CONFORMANCE" / "LOCALIZATION_FIXTURES" / "profiles"
        self.profiles = {}
        for path in sorted(fixtures.glob("*.json")):
            profile = self.loc["validate_profile"](self.contract, path.read_bytes())
            self.profiles[profile["locale"]] = profile

    # -- stages 1 to 3, per unit ------------------------------------------------

    def read(self, source: str, data: bytes, profiles: list[str]) -> Unit:
        unit = Unit(source, data)
        available = {locale: self.profiles[locale] for locale in profiles}
        record = self.loc["evaluate_source"](self.contract, data, available, self.mask)
        if "error" in record:
            stage = {"error.encoding.invalid": "lexical", "error.keyword.unknown": "lexical"}.get(record["error"], "localization")
            unit.fail(record["error"], stage, record["offset"])
            return unit
        text = data.decode("utf-8")
        offsets = self.loc["byte_offsets"](text)
        canonical, where = self.canonicalize(unit, text, offsets, record)
        if not self.lexical(unit, canonical, where, len(data)):
            return unit
        self.grammar(unit, canonical, where, len(data))
        return unit

    def canonicalize(self, unit: Unit, text: str, offsets: list[int], record: dict[str, Any]) -> tuple[str, list[int]]:
        if record.get("method") in (None, "canonical"):
            return text, offsets
        unit.locale = record["locale"]
        mask, _ = self.mask(text)
        skip = text.index("\n") + 1
        candidates = self.loc["words"](self.contract, text, mask, offsets, skip)
        index_of = {offset: index for index, offset in enumerate(offsets)}
        pieces, where, cursor = [], [], 0
        for (word, offset), canonical in zip(candidates, record["canonical_words"]):
            start = index_of[offset]
            pieces.append(text[cursor:start])
            where.extend(offsets[cursor:start])
            pieces.append(canonical)
            where.extend([offset] * len(canonical))
            cursor = start + len(word)
        pieces.append(text[cursor:])
        where.extend(offsets[cursor:])
        return "".join(pieces), where

    def lexical(self, unit: Unit, text: str, where: list[int], size: int) -> bool:
        mask, unclosed = self.mask(text)
        if unclosed:
            unit.fail("error.literal.unclosed", "lexical", size)
            return False
        stack: list[int] = []
        pairs = {"]": "[", ")": "("}
        for index, char in enumerate(text):
            if not mask[index]:
                continue
            if char in "([":
                stack.append(index)
            elif char in ")]":
                if not stack or text[stack[-1]] != pairs[char]:
                    unit.fail("error.delimiter.mismatch", "lexical", where[index])
                    return False
                stack.pop()
        if stack:
            unit.fail("error.delimiter.unclosed", "lexical", where[stack[0]])
            return False
        return True

    def grammar(self, unit: Unit, text: str, where: list[int], size: int) -> None:
        mask, _ = self.mask(text)
        position = 0
        current = None
        for line in text.split("\n"):
            header, field = HEADER.match(line), FIELD.match(line)
            if header and not line.startswith("@"):
                current = {"name": header.group(1), "offset": where[position], "fields": {}}
                unit.blocks.append(current)
            elif field and current is not None and field.group(1) not in current["fields"]:
                value = field.group(2)
                start = position + 4 + len(field.group(1)) + 2
                current["fields"][field.group(1)] = (value, where[start] if value is not None else where[position])
            position += len(line) + 1
        for match in REF.finditer(text):
            if mask[match.start()]:
                unit.refs.append((match.group(1), where[match.start(1)]))
        names = [b["name"] for b in unit.blocks]
        if names[:2] != ["LCL", "SPECIFICATION"]:
            unit.fail("error.grammar.invalid", "grammar_or_schema", 0)
            return
        version = unit.blocks[0]["fields"].get("VERSION")
        spec = unit.blocks[1]["fields"]
        if version is None or any(key not in spec for key in ("ID", "NAME", "VERSION", "KIND")):
            unit.fail("error.field.required", "grammar_or_schema", 0)
            return
        unit.lcl_version, unit.lcl_version_offset = STRING.match(version[0]).group(1), version[1]
        unit.spec_version, unit.spec_version_offset = STRING.match(spec["VERSION"][0]).group(1), spec["VERSION"][1]
        unit.kind, unit.kind_offset = spec["KIND"]
        if unit.kind not in self.registry.document_kinds:
            unit.fail("error.field.type", "grammar_or_schema", unit.kind_offset)
            return
        legal = self.registry.kind_blocks[unit.kind]
        for block in unit.blocks[2:]:
            if block["name"] not in legal:
                unit.fail("error.block.context", "grammar_or_schema", block["offset"])
        if unit.kind in EXECUTE_REQUIRED and names.count("EXECUTE") != 1:
            unit.fail("error.block.required", "grammar_or_schema", size)
        for block in unit.blocks:
            if block["name"] != "PART":
                continue
            fields = block["fields"]
            if any(key not in fields for key in ("ID", "SOURCE", "KIND")):
                unit.fail("error.field.required", "grammar_or_schema", block["offset"])
            elif not RELATIVE_PATH.match(fields["SOURCE"][0] or ""):
                unit.fail("error.field.type", "grammar_or_schema", fields["SOURCE"][1])
            elif fields["KIND"][0] not in self.registry.part_kinds:
                unit.fail("error.field.type", "grammar_or_schema", fields["KIND"][1])
            elif "REQUIRED" in fields and fields["REQUIRED"][0] not in ("TRUE", "FALSE"):
                unit.fail("error.field.type", "grammar_or_schema", fields["REQUIRED"][1])
        unit.usable = not unit.diagnostics

    # -- the project contract ---------------------------------------------------

    def evaluate(self, files: dict[str, bytes], entry: str, profiles: list[str]) -> dict[str, Any]:
        order: list[Unit] = []
        diagnostics: list[dict[str, Any]] = []
        parts: list[list[str]] = []

        def load(source: str) -> Unit:
            unit = self.read(source, files[source], profiles)
            order.append(unit)
            diagnostics.extend(unit.diagnostics)
            return unit

        def value(block: dict[str, Any], key: str) -> tuple[str, int]:
            return block["fields"][key]

        def resolve(origin: str, literal: str) -> str:
            path = RELATIVE_PATH.match(literal).group(1)
            return posixpath.normpath(posixpath.join(posixpath.dirname(origin), path))

        root = load(entry)
        result = {"diagnostics": diagnostics, "parts": parts, "locales": {}}
        if not root.usable:
            return self.finish(result, order)
        if root.kind in self.registry.part_kinds:
            root.fail("error.project.placement", "resolution", root.kind_offset)
            diagnostics.append(root.diagnostics[-1])
            return self.finish(result, order)
        if root.kind != "kind.project":
            raise ValueError(f"{entry}: only project fixtures are in scope")

        complete = True
        project: list[Unit] = [root]
        named: dict[str, str] = {}
        for block in [b for b in root.blocks if b["name"] == "PART"]:
            literal, literal_offset = value(block, "SOURCE")
            kind = value(block, "KIND")[0]
            required = block["fields"].get("REQUIRED", ("TRUE", 0))[0] == "TRUE"
            source = resolve(entry, literal)
            row = [RELATIVE_PATH.match(literal).group(1), kind, "required" if required else "optional"]
            if source in named:
                diagnostics.append({"error": "error.project.part_duplicate", "stage": "resolution", "source": entry, "offset": literal_offset})
                continue
            named[source] = "PART"
            if source not in files:
                if required:
                    diagnostics.append({"error": "error.project.part_missing", "stage": "resolution", "source": entry, "offset": literal_offset})
                    complete = False
                    parts.append(row + ["missing"])
                else:
                    parts.append(row + ["omitted"])
                continue
            unit = load(source)
            if not unit.usable:
                complete = False
                parts.append(row + ["invalid"])
                continue
            problem = None
            if unit.lcl_version != VERSION:
                problem = ("error.version.unsupported", unit.lcl_version_offset)
            elif unit.kind == "kind.project":
                problem = ("error.project.placement", unit.kind_offset)
            elif unit.kind != kind:
                problem = ("error.project.part_kind", unit.kind_offset)
            elif unit.spec_version != root.spec_version:
                problem = ("error.version.mismatch", unit.spec_version_offset)
            if problem:
                diagnostics.append({"error": problem[0], "stage": "resolution", "source": source, "offset": problem[1]})
                complete = False
                parts.append(row + ["invalid"])
                continue
            project.append(unit)
            parts.append(row + ["loaded"])

        namespaces: set[str] = set()

        def follow(unit: Unit, chain: list[str]) -> None:
            for block in [b for b in unit.blocks if b["name"] in ("IMPORT", "EXTENSION")]:
                literal, literal_offset = value(block, "SOURCE")
                source = resolve(unit.source, literal)
                if unit is root:
                    namespaces.add(value(block, "NAMESPACE")[0])
                if source in chain:
                    diagnostics.append({"error": "error.import.cycle", "stage": "resolution", "source": unit.source, "offset": literal_offset})
                    continue
                if source in named:
                    diagnostics.append({"error": "error.project.part_duplicate", "stage": "resolution", "source": unit.source, "offset": literal_offset})
                    continue
                if source not in files:
                    diagnostics.append({"error": "error.import.not_found", "stage": "resolution", "source": unit.source, "offset": literal_offset})
                    continue
                child = load(source)
                if not child.usable:
                    continue
                if child.kind == "kind.project" or child.kind in self.registry.part_kinds:
                    diagnostics.append({"error": "error.project.placement", "stage": "resolution", "source": source, "offset": child.kind_offset})
                    continue
                follow(child, chain + [source])

        follow(root, [entry])

        if complete:
            declared: dict[str, tuple[Unit, int]] = {}
            for unit in project:
                for block in unit.blocks:
                    if "ID" in block["fields"] and block["name"] != "LCL":
                        identifier, offset = block["fields"]["ID"]
                        if identifier in declared or identifier.split(".")[0] in namespaces:
                            diagnostics.append({"error": "error.id.duplicate", "stage": "resolution", "source": unit.source, "offset": offset})
                        else:
                            declared[identifier] = (unit, offset)
            for unit in project:
                for identifier, offset in unit.refs:
                    if identifier.split(".")[0] in namespaces:
                        continue
                    if identifier not in declared:
                        diagnostics.append({"error": "error.reference.unresolved", "stage": "resolution", "source": unit.source, "offset": offset})
            bases = {}
            for rank, unit in enumerate(project):
                for block in unit.blocks:
                    if block["name"] == "DEFINE" and "BASE" in block["fields"]:
                        match = REF.fullmatch(block["fields"]["BASE"][0] or "")
                        if match:
                            bases[block["fields"]["ID"][0]] = (match.group(1), unit.source, block["fields"]["BASE"][1], rank)
            reported: set[str] = set()
            for start in sorted(bases, key=lambda name: (bases[name][3], bases[name][2])):
                seen, current = [], start
                while current in bases and current not in seen:
                    seen.append(current)
                    current = bases[current][0]
                if current in seen and not reported & set(seen[seen.index(current):]):
                    cycle = seen[seen.index(current):]
                    first = min(cycle, key=lambda name: (bases[name][3], bases[name][2]))
                    reported.update(cycle)
                    diagnostics.append({"error": "error.reference.cycle", "stage": "resolution", "source": bases[first][1], "offset": bases[first][2]})
        return self.finish(result, order)

    def finish(self, result: dict[str, Any], order: list[Unit]) -> dict[str, Any]:
        rank = {unit.source: index for index, unit in enumerate(order)}
        result["diagnostics"] = sorted(
            {json.dumps(d, sort_keys=True): d for d in result["diagnostics"]}.values(),
            key=lambda d: (STAGES.index(d["stage"]), rank[d["source"]], d["offset"], d["error"]),
        )
        result["locales"] = {unit.source: unit.locale for unit in order}
        result["outcome"] = "reject" if result["diagnostics"] else "accept"
        return result


def fixture_files(directory: Path) -> dict[str, bytes]:
    """Load one fixture into memory. Listing happens only here, to load bytes;
    evaluation consults only the paths that PART and IMPORT declarations name."""
    return {path.relative_to(directory).as_posix(): path.read_bytes() for path in directory.rglob("*") if path.is_file()}


def compare(expected: dict[str, Any], actual: dict[str, Any]) -> bool:
    if expected["outcome"] != actual["outcome"]:
        return False
    if expected["outcome"] == "reject":
        return expected["diagnostics"] == actual["diagnostics"]
    if "parts" in expected and expected["parts"] != actual["parts"]:
        return False
    return all(actual["locales"].get(source) == locale for source, locale in expected.get("locales", {}).items())


def mutations(checker: Checker, files: dict[str, bytes]) -> list[dict[str, Any]]:
    """Negative controls: plant one defect in a copy of an accepted project and
    require the exact diagnostic, so the checker cannot pass vacuously."""

    def replaced(source: str, old: str, new: str) -> dict[str, bytes]:
        copy = dict(files)
        text = copy[source].decode("utf-8")
        assert text.count(old) == 1, (source, old)
        copy[source] = text.replace(old, new).encode("utf-8")
        return copy

    def offset(data: dict[str, bytes], source: str, needle: str, inner: str | None = None) -> int:
        start = data[source].index(needle.encode())
        return start + (needle.encode().index(inner.encode()) if inner else 0)

    cases = []
    without = {k: v for k, v in files.items() if k != "checks.lcl"}
    cases.append(("remove a required part", without, [{"error": "error.project.part_missing", "stage": "resolution", "source": "main.lcl", "offset": offset(files, "main.lcl", 'PATH("checks.lcl")')}]))
    changed = replaced("main.lcl", "SOURCE: PATH(\"rules.lcl\")\n    KIND: kind.part.rules", "SOURCE: PATH(\"rules.lcl\")\n    KIND: kind.part.context")
    cases.append(("expect a different part kind", changed, [{"error": "error.project.part_kind", "stage": "resolution", "source": "rules.lcl", "offset": offset(changed, "rules.lcl", "KIND: kind.part.rules", "kind.part.rules")}]))
    duplicated = replaced("data.lcl", "    VALUE: 4\n", "    VALUE: 4\n\nINPUT:\n    ID: output.value\n    TYPE: INTEGER\n    VALUE: 1\n")
    cases.append(("declare an ID in two parts", duplicated, [{"error": "error.id.duplicate", "stage": "resolution", "source": "output.lcl", "offset": offset(duplicated, "output.lcl", "ID: output.value", "output.value")}]))
    retargeted = replaced("checks.lcl", "REF(output.value)", "REF(output.other)")
    cases.append(("break a cross-file REF", retargeted, [{"error": "error.reference.unresolved", "stage": "resolution", "source": "checks.lcl", "offset": offset(retargeted, "checks.lcl", "REF(output.other)", "output.other")}]))
    moved = replaced("rules.lcl", "\nFORBID:", "\nACTION:\n    ID: action.extra\n    OPERATION: core.calculate\n\nFORBID:")
    cases.append(("put an execution block in a rules part", moved, [{"error": "error.block.context", "stage": "grammar_or_schema", "source": "rules.lcl", "offset": offset(moved, "rules.lcl", "\nACTION:", "ACTION")}]))
    main = files["main.lcl"].decode("utf-8")
    first, execute = main.index("\nPART:"), main.index("\nEXECUTE:")
    chunks = ["\nPART:" + chunk for chunk in main[first:execute].split("\nPART:")[1:]]
    reordered = dict(files)
    reordered["main.lcl"] = (main[:first] + "".join(reversed(chunks)) + main[execute:]).encode("utf-8")
    results = []
    for label, data, expected in cases:
        actual = checker.evaluate(data, "main.lcl", [])
        results.append({"mutation": label, "expected": expected, "actual": actual["diagnostics"], "passed": actual["diagnostics"] == expected})
    forward = checker.evaluate(files, "main.lcl", [])
    backward = checker.evaluate(reordered, "main.lcl", [])
    results.append({
        "mutation": "reverse the PART order",
        "expected": "accepted, with the part order following the PART declarations",
        "actual": {"outcome": backward["outcome"], "parts": [row[0] for row in backward["parts"]]},
        "passed": backward["outcome"] == "accept" and backward["parts"] == list(reversed(forward["parts"])),
    })
    return results


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[2])
    arguments = parser.parse_args()
    root = arguments.root.resolve()
    fixtures = root / "09_CONFORMANCE" / "PROJECT_FIXTURES"
    try:
        registry = Registry(root)
        checks = [check_contract(registry)]
        checker = Checker(root, registry)
        expected = load_json_strict(fixtures / "expected_results.json")
        present = sorted(p.name for p in fixtures.iterdir() if p.is_dir())
        if present != sorted(expected):
            raise ValueError(f"fixture inventory mismatch: {sorted(set(present) ^ set(expected))}")
        results = []
        for name, case in sorted(expected.items()):
            if set(case) != {"entry", "expected", "note"} or not case["note"]:
                raise ValueError(f"{name}: fixture record fields are not exact")
            files = fixture_files(fixtures / name)
            actual = checker.evaluate(files, case["entry"], case["expected"].get("profiles", []))
            view = {"outcome": actual["outcome"], "diagnostics": actual["diagnostics"]}
            if case["expected"]["outcome"] == "accept":
                view.update({"parts": actual["parts"], "locales": actual["locales"]})
            results.append({"fixture": name, "expected": case["expected"], "actual": view, "passed": compare(case["expected"], actual)})
        controls = mutations(checker, fixture_files(fixtures / "valid_all_roles"))
        passed = all(c["passed"] for c in checks) and all(r["passed"] for r in results) and all(m["passed"] for m in controls)
        output = {
            "tool": "validate_projects.py",
            "scope": "project_contract_structure_and_bounded_static_project_fixtures",
            "semantic_execution": "UNVERIFIED",
            "checks": checks,
            "fixtures": results,
            "mutations": controls,
            "counts": {
                "fixtures": len(results),
                "accepted": sum(1 for r in results if r["expected"]["outcome"] == "accept"),
                "rejected": sum(1 for r in results if r["expected"]["outcome"] == "reject"),
                "mutations": len(controls),
                "failed": sum(1 for group in (checks, results, controls) for item in group if not item["passed"]),
            },
            "passed": passed,
        }
        print(json.dumps(output, indent=2, sort_keys=True, ensure_ascii=False))
        return 0 if passed else 1
    except (OSError, UnicodeError, ValueError, KeyError, AttributeError, json.JSONDecodeError) as error:
        print(json.dumps({"passed": False, "error": f"{type(error).__name__}: {error}"}, indent=2, sort_keys=True))
        return 2


if __name__ == "__main__":
    sys.exit(main())

#!/usr/bin/env python3
"""Execute the concrete LCL source-fixture expectations.

This is a bounded source-hygiene and document-boundary validator. It is not an
LCL type checker, evaluator, executor, or proof that the descriptive case
catalog has been executed.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path
from typing import Any


def load_json_strict(path: Path) -> Any:
    def reject_duplicates(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
        result: dict[str, Any] = {}
        for key, value in pairs:
            if key in result:
                raise ValueError(f"duplicate JSON key {key!r} in {path}")
            result[key] = value
        return result

    return json.loads(path.read_text(encoding="utf-8"), object_pairs_hook=reject_duplicates)


def outside_string_mask(text: str) -> tuple[list[bool], bool]:
    """Return positions outside strings and whether a string is left open."""
    outside = [True] * len(text)
    index = 0
    mode = "normal"
    escaped = False
    while index < len(text):
        if mode == "normal":
            if text.startswith('"""', index):
                outside[index : index + 3] = [False, False, False]
                mode = "multiline"
                index += 3
                continue
            if text[index] == '"':
                outside[index] = False
                mode = "string"
            index += 1
            continue

        outside[index] = False
        if escaped:
            escaped = False
            index += 1
            continue
        if text[index] == "\\":
            escaped = True
            index += 1
            continue
        if mode == "string" and text[index] == '"':
            mode = "normal"
            index += 1
            continue
        if mode == "multiline" and text.startswith('"""', index):
            outside[index : index + 3] = [False, False, False]
            mode = "normal"
            index += 3
            continue
        index += 1
    return outside, mode != "normal"


def document_kind_error(text: str) -> str | None:
    lines = text.split("\n")
    top_level = [line[:-1] for line in lines if line and not line.startswith(" ") and line.endswith(":")]
    if len(top_level) < 2 or top_level[0] != "LCL" or top_level[1] != "SPECIFICATION":
        return "error.block.order"

    required_header_fields = {"ID", "NAME", "VERSION", "KIND"}
    specification_index = lines.index("SPECIFICATION:")
    next_top = next(
        (i for i in range(specification_index + 1, len(lines)) if lines[i] and not lines[i].startswith(" ")),
        len(lines),
    )
    header_fields = {
        line[4:].split(":", 1)[0]
        for line in lines[specification_index + 1 : next_top]
        if line.startswith("    ") and ":" in line
    }
    if not required_header_fields <= header_fields:
        return "error.field.required"

    kind_line = next(
        (line for line in lines[specification_index + 1 : next_top] if line.startswith("    KIND: ")),
        "",
    )
    kind = kind_line.removeprefix("    KIND: ")
    execute_count = sum(1 for item in top_level if item == "EXECUTE")
    if kind in {"kind.task", "kind.test"} and execute_count != 1:
        return "error.block.required"
    if kind in {"kind.data", "kind.library", "kind.extension"} and execute_count != 0:
        return "error.block.forbidden"
    return None


def validate_source(data: bytes) -> str:
    if data.startswith(b"\xef\xbb\xbf"):
        return "error.source.bom"
    if b"\r" in data:
        return "error.newline.invalid"
    if not data.endswith(b"\n"):
        return "error.source.final_line_feed"
    try:
        text = data.decode("utf-8")
    except UnicodeDecodeError:
        return "error.encoding.invalid"
    if "\t" in text:
        return "error.source.tab"
    if any((ord(char) < 0x20 and char != "\n") or 0x7F <= ord(char) <= 0x9F for char in text):
        return "error.source.control_character"
    if any(line.endswith(" ") for line in text.split("\n")):
        return "error.source.trailing_space"

    outside, unclosed_string = outside_string_mask(text)
    if any(ord(char) > 0x7F and outside[index] for index, char in enumerate(text)):
        return "error.source.non_ascii_outside_string"

    previous_indent = 0
    offset = 0
    for line in text.split("\n"):
        in_normal_mode = offset < len(outside) and outside[offset]
        offset += len(line) + 1
        if not line or not in_normal_mode:
            continue
        indent = len(line) - len(line.lstrip(" "))
        if indent % 4:
            return "error.indentation.width"
        if indent > previous_indent + 4:
            return "error.indentation.jump"
        previous_indent = indent

    outside_text = "".join(char if outside[index] else " " for index, char in enumerate(text))
    if any(symbol in outside_text for symbol in ("#", ";", "'", "%")):
        return "error.symbol.invalid"
    if re.search(r"(?<![!<>=])=(?!=)", outside_text):
        return "error.symbol.invalid"
    if unclosed_string:
        return "error.literal.unclosed"

    boundary_error = document_kind_error(text)
    return boundary_error or "accept"


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--root",
        type=Path,
        default=Path(__file__).resolve().parents[2],
        help="LCL package root (defaults to the package containing this tool)",
    )
    args = parser.parse_args()
    root = args.root.resolve()
    fixture_dir = root / "09_CONFORMANCE" / "SOURCE_FIXTURES"
    expected_path = fixture_dir / "expected_results.json"

    try:
        expected = load_json_strict(expected_path)
        fixture_names = sorted(path.name for path in fixture_dir.glob("*.lcl"))
        if fixture_names != sorted(expected):
            missing = sorted(set(expected) - set(fixture_names))
            unexpected = sorted(set(fixture_names) - set(expected))
            raise ValueError(f"fixture inventory mismatch: missing={missing}, unexpected={unexpected}")

        results = []
        for name in fixture_names:
            actual = validate_source((fixture_dir / name).read_bytes())
            wanted = expected[name]
            results.append(
                {
                    "fixture": name,
                    "expected": wanted,
                    "actual": actual,
                    "passed": actual == wanted,
                }
            )
        passed = all(result["passed"] for result in results)
        output = {
            "tool": "validate_source_fixtures.py",
            "scope": "lexical_source_hygiene_and_document_kind_boundary",
            "semantic_execution": "UNVERIFIED",
            "fixture_count": len(results),
            "passed_count": sum(1 for result in results if result["passed"]),
            "failed_count": sum(1 for result in results if not result["passed"]),
            "passed": passed,
            "results": results,
        }
        print(json.dumps(output, indent=2, sort_keys=True, ensure_ascii=False))
        return 0 if passed else 1
    except (OSError, UnicodeError, ValueError, json.JSONDecodeError) as exc:
        print(json.dumps({"passed": False, "error": str(exc)}, indent=2, sort_keys=True))
        return 2


if __name__ == "__main__":
    sys.exit(main())

#!/usr/bin/env python3
"""Read-only structural and integrity validation for an LCL repair candidate.

Unresolved bare-language definitions are BLOCKED. Parser, interpreter, runtime,
and executable semantic-conformance evidence are OUT_OF_SCOPE: this tool does not
implement or simulate those components and never reports their absence as PASS.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import runpy
import stat
import subprocess
import sys
from pathlib import Path
from typing import Any, Callable


SCOPES = ("filesystem", "text", "structured", "grammar", "registry", "catalog", "integrity", "release")
TEXT_SUFFIXES = {".txt", ".ebnf", ".lcl", ".json", ".py", ".md"}
INTEGRITY_FILES = {"MANIFEST.json", "VALIDATION_REPORT.txt", "SHA256SUMS.txt"}
RESULT_STATUSES = ("PASS", "FAIL", "BLOCKED", "OUT_OF_SCOPE")


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def load_json_strict(path: Path) -> Any:
    def reject_duplicates(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
        value: dict[str, Any] = {}
        for key, item in pairs:
            if key in value:
                raise ValueError(f"duplicate JSON key {key!r} in {path}")
            value[key] = item
        return value

    return json.loads(path.read_text(encoding="utf-8"), object_pairs_hook=reject_duplicates)


class Results:
    def __init__(self) -> None:
        self.checks: list[dict[str, Any]] = []

    def add(self, scope: str, check: str, status_value: str, **details: Any) -> None:
        if status_value not in RESULT_STATUSES:
            raise ValueError(f"unknown validation status: {status_value}")
        self.checks.append(
            {"scope": scope, "check": check, "status": status_value, "details": details}
        )

    def guarded(self, scope: str, check: str, operation: Callable[[], dict[str, Any]]) -> None:
        try:
            details = operation()
            self.add(scope, check, "PASS", **details)
        except (
            AssertionError,
            OSError,
            TypeError,
            UnicodeError,
            ValueError,
            json.JSONDecodeError,
        ) as error:
            self.add(scope, check, "FAIL", error=str(error))


def all_files(root: Path) -> list[Path]:
    return sorted((path for path in root.rglob("*") if path.is_file()), key=lambda p: p.relative_to(root).as_posix().encode())


def check_filesystem(root: Path, results: Results) -> None:
    def operation() -> dict[str, Any]:
        links = sorted(path.relative_to(root).as_posix() for path in root.rglob("*") if path.is_symlink())
        assert not links, f"symbolic links are forbidden: {links}"
        files = all_files(root)
        relative = [path.relative_to(root).as_posix() for path in files]
        assert len(relative) == len(set(relative)), "duplicate paths"
        folded: dict[str, list[str]] = {}
        for item in relative:
            folded.setdefault(item.casefold(), []).append(item)
        collisions = [items for items in folded.values() if len(items) > 1]
        assert not collisions, f"case-colliding paths: {collisions}"
        temporary = [
            item
            for item in relative
            if "__pycache__" in item.split("/")
            or item.endswith((".pyc", ".pyo", ".swp", ".tmp", "~"))
            or Path(item).name in {".DS_Store", "Thumbs.db"}
        ]
        assert not temporary, f"temporary/cache files: {temporary}"
        bad_modes = []
        for path in files:
            mode = stat.S_IMODE(path.stat().st_mode)
            if mode != 0o644:
                bad_modes.append((path.relative_to(root).as_posix(), oct(mode)))
        assert not bad_modes, f"non-canonical file modes: {bad_modes}"
        return {"file_count": len(files), "symlink_count": 0, "case_collision_count": 0}

    results.guarded("filesystem", "path_mode_and_cache_hygiene", operation)


def check_text(root: Path, results: Results) -> None:
    expected = load_json_strict(root / "09_CONFORMANCE/SOURCE_FIXTURES/expected_results.json")
    invalid = {
        f"09_CONFORMANCE/SOURCE_FIXTURES/{name}"
        for name, outcome in expected.items()
        if outcome != "accept"
    }

    def operation() -> dict[str, Any]:
        checked = 0
        for path in all_files(root):
            relative = path.relative_to(root).as_posix()
            if path.suffix not in TEXT_SUFFIXES or relative in invalid:
                continue
            data = path.read_bytes()
            data.decode("utf-8")
            assert b"\r" not in data, f"CR byte in {relative}"
            assert data.endswith(b"\n"), f"missing final LF in {relative}"
            assert b"\x00" not in data, f"NUL byte in {relative}"
            checked += 1
        return {"clean_text_files": checked, "intentional_invalid_fixtures_excluded": len(invalid)}

    results.guarded("text", "utf8_lf_and_control_hygiene", operation)

    command = [sys.executable, str(root / "09_CONFORMANCE/TOOLS/validate_source_fixtures.py"), "--root", str(root)]
    environment = dict(os.environ)
    environment["PYTHONDONTWRITEBYTECODE"] = "1"
    completed = subprocess.run(command, cwd=root, env=environment, text=True, capture_output=True, check=False)
    try:
        payload = json.loads(completed.stdout)
    except json.JSONDecodeError:
        payload = {"stdout": completed.stdout, "stderr": completed.stderr}
    results.add(
        "text",
        "concrete_source_fixtures",
        "PASS" if completed.returncode == 0 else "FAIL",
        exit_code=completed.returncode,
        output=payload,
    )

    def static_examples_operation() -> dict[str, Any]:
        examples = root / "08_EXAMPLES"
        valid_paths = sorted(examples.glob("VALID/*.lcl"), key=lambda path: path.name)
        invalid_paths = sorted(
            examples.glob("INVALID/*.invalid.lcl"), key=lambda path: path.name
        )
        expected_paths = sorted(
            examples.glob("INVALID/*.invalid.lcl.expected.txt"),
            key=lambda path: path.name,
        )
        expected_valid_names = {
            "01_MINIMAL_TASK.lcl",
            "02_IMPORT_LIBRARY.lcl",
            "03_IMPORTING_TASK.lcl",
            "04_AUTOMATED_CODING_TASK.lcl",
            "05_CONDITION_AND_ITERATION.lcl",
            "06_EXPLICIT_CONTEXT_MEMORY_AND_STATE.lcl",
            "07_DOMAIN_EXTENSION_OPERATION.lcl",
            "08_AUTHORITY_OVERRIDE_HANDLER_AND_RETRY.lcl",
            "09_EXPLICIT_ASSUMPTION_AND_EVIDENCE.lcl",
            "10_ACCEPTED_CORE_PROFILES.lcl",
            "11_EXACT_DIVISION_AND_ROUNDING.lcl",
            "12_SET_SORTING.lcl",
            "13_TYPES_AND_REFERENCE_VALUES.lcl",
        }
        expected_invalid_names = {
            "01_WRONG_KEYWORD_CASE.invalid.lcl",
            "02_TAB_INDENTATION.invalid.lcl",
            "03_BARE_EQUALS.invalid.lcl",
            "04_UNKNOWN_MODAL.invalid.lcl",
            "05_DUPLICATE_ID.invalid.lcl",
            "06_UNRESOLVED_REFERENCE.invalid.lcl",
            "07_TYPE_MISMATCH.invalid.lcl",
            "08_HARD_CONFLICT.invalid.lcl",
            "09_UNBOUNDED_WHILE.invalid.lcl",
            "10_CONTEXT_WITHOUT_SCOPE.invalid.lcl",
            "11_DUPLICATE_FIELD.invalid.lcl",
            "12_FLOATING_VERSION.invalid.lcl",
            "13_PERCENT_SYMBOL.invalid.lcl",
            "14_SINGLE_QUOTED_STRING.invalid.lcl",
            "15_MISSING_TASK_STRUCTURE.invalid.lcl",
            "16_ITEM_COLLECTION.invalid.lcl",
            "17_REGEX_FLAGS.invalid.lcl",
            "18_NON_TERMINATING_DIVISION.invalid.lcl",
            "19_DIVISION_BY_ZERO.invalid.lcl",
            "20_UNORDERED_SET_DIRECT_ITERATION.invalid.lcl",
            "21_SORT_STABLE_PARAMETER.invalid.lcl",
        }
        expected_expectation_names = {
            f"{name}.expected.txt" for name in expected_invalid_names
        }
        assert {path.name for path in valid_paths} == expected_valid_names, (
            "valid example inventory differs from the exact accepted set"
        )
        assert {path.name for path in invalid_paths} == expected_invalid_names, (
            "invalid example inventory differs from the exact accepted set"
        )
        assert {path.name for path in expected_paths} == expected_expectation_names, (
            "invalid expectation inventory differs from the exact accepted set"
        )
        paired = {Path(f"{path}.expected.txt") for path in invalid_paths}
        assert paired == set(expected_paths), "invalid example/expectation pairing differs"

        statuses_document = load_json_strict(
            root / "10_REGISTRIES/statuses_and_errors_v0.1.0.json"
        )
        error_registry = statuses_document["errors"]
        status_registry = statuses_document["statuses"]
        phase_resolution = statuses_document["failure_lifecycle"][
            "error_phase_resolution"
        ]
        expected_pattern = re.compile(
            r"EXPECTED_ERROR: (error\.[a-z0-9_.]+)\n"
            r"EXPECTED_TERMINAL_STATUS: (status\.[a-z_]+)\n"
            r"RULE: ([^\n]+)\n"
        )
        parsed_expectations: dict[str, dict[str, str]] = {}
        for path in expected_paths:
            match = expected_pattern.fullmatch(path.read_text(encoding="utf-8"))
            assert match is not None, f"invalid expectation shape: {path.name}"
            error_name, status_name, rule = match.groups()
            assert error_name in error_registry, f"unregistered expected error: {error_name}"
            assert status_name in status_registry, f"unregistered expected status: {status_name}"
            assert error_registry[error_name]["default_status"] == status_name, (
                f"expected status differs from {error_name} default"
            )
            stage = error_registry[error_name]["stage"]
            phases = phase_resolution["overrides"].get(
                error_name, phase_resolution["defaults_by_stage"].get(stage)
            )
            assert isinstance(phases, list) and "pre_effect" in phases, (
                f"invalid source expectation lacks pre_effect metadata: {error_name}"
            )
            parsed_expectations[path.name] = {
                "error": error_name,
                "status": status_name,
                "rule": rule,
            }

        source_module = runpy.run_path(
            str(root / "09_CONFORMANCE/TOOLS/validate_source_fixtures.py"),
            run_name="lcl_source_fixture_validator",
        )
        validate_source = source_module.get("validate_source")
        assert callable(validate_source), "bounded source validator is unavailable"
        valid_source_results = {
            path.name: validate_source(path.read_bytes()) for path in valid_paths
        }
        assert set(valid_source_results.values()) == {"accept"}, (
            f"valid example source-hygiene failure: {valid_source_results}"
        )

        focused_expected = {
            "02_TAB_INDENTATION.invalid.lcl.expected.txt": "error.source.tab",
            "03_BARE_EQUALS.invalid.lcl.expected.txt": "error.symbol.invalid",
            "10_CONTEXT_WITHOUT_SCOPE.invalid.lcl.expected.txt": "error.field.required",
            "11_DUPLICATE_FIELD.invalid.lcl.expected.txt": "error.field.duplicate",
            "13_PERCENT_SYMBOL.invalid.lcl.expected.txt": "error.symbol.invalid",
            "14_SINGLE_QUOTED_STRING.invalid.lcl.expected.txt": "error.symbol.invalid",
            "15_MISSING_TASK_STRUCTURE.invalid.lcl.expected.txt": (
                "error.block.required"
            ),
        }
        focused_hashes = {
            "02_TAB_INDENTATION.invalid.lcl": (
                "2eae884823243f6530c6edcbcd4f25a53136fbc152f046aa8a70e80d1cc0d050"
            ),
            "02_TAB_INDENTATION.invalid.lcl.expected.txt": (
                "1b72812c2596815a22894d4da13f16456faac14117ca196a798b64f3dafd8649"
            ),
            "03_BARE_EQUALS.invalid.lcl": (
                "25e81efc99e15efbf9a51d8f2f8f0e18a2c76b9b7b4e1ef80428a18650109323"
            ),
            "03_BARE_EQUALS.invalid.lcl.expected.txt": (
                "d1c146089a9cc71189e6f7036d273250b62ed0050cc2db7886035b94976df558"
            ),
            "10_CONTEXT_WITHOUT_SCOPE.invalid.lcl": (
                "96ec4f463aa52c3d780d20f2824b205f6457e5d5461e2d7c1ce479529b08f7f5"
            ),
            "10_CONTEXT_WITHOUT_SCOPE.invalid.lcl.expected.txt": (
                "ea6298878411745d0d0712c85d9845866ad5e4111431a3462d869a960aa3df97"
            ),
            "11_DUPLICATE_FIELD.invalid.lcl": (
                "c6739c6e62bc0ee54bd55fd64ff2960056582d2b3f6bd5aa95fb853915b71db1"
            ),
            "11_DUPLICATE_FIELD.invalid.lcl.expected.txt": (
                "53c75b217f9a266d3305483450f257162d8d710de0b385567b42be4d05fe6a58"
            ),
            "13_PERCENT_SYMBOL.invalid.lcl": (
                "e82501c2508be4b23399c0f73afd210abbd1e33d6e1e0e7bf0435ccb2fad1ebf"
            ),
            "13_PERCENT_SYMBOL.invalid.lcl.expected.txt": (
                "d1c146089a9cc71189e6f7036d273250b62ed0050cc2db7886035b94976df558"
            ),
            "14_SINGLE_QUOTED_STRING.invalid.lcl": (
                "f180d6e85bcb8ab66822ccd7bf3109924fa5eb4489fb8213bcc115b6738fe17d"
            ),
            "14_SINGLE_QUOTED_STRING.invalid.lcl.expected.txt": (
                "d1c146089a9cc71189e6f7036d273250b62ed0050cc2db7886035b94976df558"
            ),
            "15_MISSING_TASK_STRUCTURE.invalid.lcl": (
                "ab2be39a6975376d4759d095d23ce271ac2617abfc2815a33f159fa8dc4d7086"
            ),
            "15_MISSING_TASK_STRUCTURE.invalid.lcl.expected.txt": (
                "58f7697e48bf979f2bd7228f01f63df028d1582b7a7fbf7a8451a64bbde4c710"
            ),
        }
        for name, expected_hash in focused_hashes.items():
            path = examples / "INVALID" / name
            assert sha256(path) == expected_hash, f"focused example bytes differ: {name}"
        for name, error_name in focused_expected.items():
            assert parsed_expectations.get(name, {}).get("error") == error_name, (
                f"focused expected primary differs: {name}"
            )

        def top_level_block(source: str, header: str) -> str:
            lines = source.splitlines(keepends=True)
            starts = [index for index, line in enumerate(lines) if line == f"{header}\n"]
            assert len(starts) == 1, f"expected one top-level {header} block"
            start = starts[0]
            end = len(lines)
            for index in range(start + 1, len(lines)):
                line = lines[index]
                if line.strip() and not line[0].isspace():
                    end = index
                    break
            return "".join(lines[start:end])

        tab_source = examples / "INVALID/02_TAB_INDENTATION.invalid.lcl"
        assert validate_source(tab_source.read_bytes()) == "error.source.tab", (
            "TAB example does not select error.source.tab"
        )
        for name in (
            "03_BARE_EQUALS.invalid.lcl",
            "13_PERCENT_SYMBOL.invalid.lcl",
            "14_SINGLE_QUOTED_STRING.invalid.lcl",
        ):
            assert validate_source((examples / "INVALID" / name).read_bytes()) == (
                "error.symbol.invalid"
            ), f"excluded-symbol example does not select error.symbol.invalid: {name}"
        context_text = (
            examples / "INVALID/10_CONTEXT_WITHOUT_SCOPE.invalid.lcl"
        ).read_text(encoding="utf-8")
        context_block = top_level_block(context_text, "CONTEXT:")
        context_fields = re.findall(r"^    ([A-Z_]+):", context_block, re.MULTILINE)
        assert context_fields == ["ID", "TYPE", "VALUE"], (
            "CONTEXT example does not isolate missing SCOPE"
        )
        duplicate_text = (
            examples / "INVALID/11_DUPLICATE_FIELD.invalid.lcl"
        ).read_text(encoding="utf-8")
        specification_text = top_level_block(duplicate_text, "SPECIFICATION:")
        specification_fields = re.findall(
            r"^    ([A-Z_]+):", specification_text, re.MULTILINE
        )
        assert specification_fields.count("VERSION") == 2 and all(
            specification_fields.count(field) == 1
            for field in ("ID", "NAME", "KIND")
        ), (
            "duplicate-field example does not contain two SPECIFICATION.VERSION fields"
        )
        unresolved_text = (
            examples / "INVALID/15_MISSING_TASK_STRUCTURE.invalid.lcl"
        ).read_text(encoding="utf-8")
        declared_ids = set(
            re.findall(r"^\s+ID: ([a-z][a-z0-9_.-]*)$", unresolved_text, re.MULTILINE)
        )
        referenced_ids = set(re.findall(r"REF\(([a-z][a-z0-9_.-]*)\)", unresolved_text))
        assert not referenced_ids - declared_ids, "missing-root fixture has unresolved references"
        assert "\nEXECUTE:" not in unresolved_text and "\nTASK:" in unresolved_text, (
            "missing-task-structure example no longer isolates an absent EXECUTE root"
        )
        assert validate_source(unresolved_text.encode("utf-8")) == "error.block.required", (
            "bounded source check does not identify the missing required root"
        )

        index_text = (root / "INDEX.txt").read_text(encoding="utf-8")
        for path in valid_paths + invalid_paths + expected_paths:
            relative = path.relative_to(examples).as_posix()
            assert index_text.count(f"    {relative}\n") == 1, (
                f"INDEX.txt does not list example exactly once: {relative}"
            )
        return {
            "valid_example_count": len(valid_paths),
            "invalid_example_count": len(invalid_paths),
            "expectation_pair_count": len(expected_paths),
            "registered_expectation_count": len(parsed_expectations),
            "bounded_valid_source_count": len(valid_source_results),
            "focused_primary_diagnostic_count": len(focused_expected),
            "focused_byte_fingerprint_count": len(focused_hashes),
            "parser_execution": "OUT_OF_SCOPE",
            "semantic_execution": "OUT_OF_SCOPE",
        }

    results.guarded("text", "static_example_contracts", static_examples_operation)


def check_structured(root: Path, results: Results) -> None:
    def json_operation() -> dict[str, Any]:
        paths = sorted(root.rglob("*.json"))
        for path in paths:
            load_json_strict(path)
        return {"json_file_count": len(paths), "duplicate_key_count": 0}

    def python_operation() -> dict[str, Any]:
        paths = sorted(root.rglob("*.py"))
        for path in paths:
            compile(path.read_text(encoding="utf-8"), str(path), "exec")
        return {"python_file_count": len(paths), "bytecode_written": False}

    results.guarded("structured", "strict_json_parse", json_operation)
    results.guarded("structured", "python_source_compile", python_operation)


def check_grammar(root: Path, results: Results) -> None:
    command = [
        sys.executable,
        str(root / "09_CONFORMANCE/TOOLS/validate_ebnf.py"),
        str(root / "04_GRAMMAR/10_COMPLETE_EBNF.ebnf"),
        "--start",
        "DOCUMENT",
    ]
    environment = dict(os.environ)
    environment["PYTHONDONTWRITEBYTECODE"] = "1"
    completed = subprocess.run(command, cwd=root, env=environment, text=True, capture_output=True, check=False)
    try:
        payload = json.loads(completed.stdout)
    except json.JSONDecodeError:
        payload = {"stdout": completed.stdout, "stderr": completed.stderr}
    results.add(
        "grammar",
        "static_ebnf_graph",
        "PASS" if completed.returncode == 0 else "FAIL",
        exit_code=completed.returncode,
        output=payload,
    )
    results.add(
        "grammar",
        "complete_example_parse_matrix",
        "OUT_OF_SCOPE",
        classification="BARE_LANGUAGE_IMPLEMENTATION_ARTIFACT",
        reason="The bare language package does not include or require a parser or executable example matrix.",
        retired_package_task="original LCL-TASK-0007 (reference lexer and parser)",
    )


def value_kind_head(value: str) -> str:
    return re.split(r"[\[(]", value, maxsplit=1)[0]


VALUE_KIND_ARGUMENT_KINDS = {
    "json_string_literal",
    "closed_integer_range",
    "qualified_identifier_domain",
    "reference_target_union",
    "block_name",
}
VALUE_KIND_ACCEPTED_FORMS = {
    "STRING",
    "INTEGER",
    "QUALIFIED_IDENTIFIER",
    "single_reference",
    "reference_list",
    "nested_block",
}
VALUE_KIND_TEMPLATE_CONTRACTS = {
    "exact_string": {
        "syntax": "exact_string(JSON_STRING)",
        "argument_kind": "json_string_literal",
        "accepted_forms": ["STRING"],
    },
    "integer": {
        "syntax": "integer[MINIMUM..MAXIMUM]",
        "argument_kind": "closed_integer_range",
        "accepted_forms": ["INTEGER"],
    },
    "qualified_identifier": {
        "syntax": "qualified_identifier(DOMAIN)",
        "argument_kind": "qualified_identifier_domain",
        "accepted_forms": ["QUALIFIED_IDENTIFIER"],
    },
    "reference": {
        "syntax": "reference(TARGET[|TARGET...])",
        "argument_kind": "reference_target_union",
        "accepted_forms": ["single_reference"],
    },
    "reference_or_list": {
        "syntax": "reference_or_list(TARGET[|TARGET...])",
        "argument_kind": "reference_target_union",
        "accepted_forms": ["single_reference", "reference_list"],
    },
    "reference_or_list_or_nested": {
        "syntax": "reference_or_list_or_nested(BLOCK)",
        "argument_kind": "block_name",
        "accepted_forms": ["single_reference", "reference_list", "nested_block"],
    },
    "reference_or_nested": {
        "syntax": "reference_or_nested(BLOCK)",
        "argument_kind": "block_name",
        "accepted_forms": ["single_reference", "nested_block"],
    },
    "nested_block": {
        "syntax": "nested_block(BLOCK)",
        "argument_kind": "block_name",
        "accepted_forms": ["nested_block"],
    },
}
TASK_0002_NAMED_VALUE_KIND_DEFINITIONS = {
    "boolean": "One exact BOOLEAN value: TRUE or FALSE.",
    "boolean_or_reference_list": (
        "A boolean_expression or a possibly empty LIST containing only REF values "
        "whose targets evaluate to BOOLEAN or UNKNOWN."
    ),
    "duration": "One DURATION value.",
    "nonnegative_numeric_or_measure": (
        "An INTEGER or DECIMAL greater than or equal to zero, or a MEASURE whose "
        "numeric component is greater than or equal to zero."
    ),
    "operation_identifier": (
        "A core_operation_ids member or the identifier of a DEFINE declaration "
        "whose KIND is kind.operation."
    ),
    "operation_identifier_or_handler_reference": (
        "One REF resolving to HANDLER, or one operation_identifier that registers no "
        "required named parameter and whose required target, if any, is supplied by "
        "the handler-context binding."
    ),
    "path": "One PATH value.",
    "property_path": (
        "One property path made of one or more SIMPLE_IDENTIFIER segments separated by '.'."
    ),
    "regex_or_glob": "One REGEX or GLOB value.",
    "schema_reference_or_nested_schema": (
        "One REF resolving to a defined OBJECT type, or a local nested sequence of one "
        "or more FIELD blocks."
    ),
    "sha256_string": (
        "A STRING containing 'sha256:' followed by exactly 64 lowercase hexadecimal digits."
    ),
    "string": "One single-line STRING value.",
    "string_or_multiline_string": "One STRING or MULTILINE_STRING value.",
    "string_or_qualified_identifier": "One STRING or QUALIFIED_IDENTIFIER value.",
    "string_uri_or_evidence_reference": (
        "One STRING, URI, or REF resolving to EVIDENCE."
    ),
    "type_expression": (
        "One TYPE_EXPRESSION under types_v0.1.0.json#/source_type_contract; a "
        "defined type is REF(identifier) resolving to DEFINE kind.type, never a "
        "bare identifier."
    ),
    "type_or_format_base": (
        "For DEFINE kind.type, one TYPE_EXPRESSION; for kind.format, one "
        "qualified_identifier(format); for kind.error, kind.event, or kind.status, "
        "one qualified identifier in the corresponding domain. Defined BASE aliases "
        "resolve acyclically to a registered base in the same domain."
    ),
}
TASK_0002_QUALIFIED_IDENTIFIER_DOMAINS = {
    "definition_kind": {
        "source": "10_REGISTRIES/built_in_groups_and_results_v0.1.0.json",
        "pointer": "/enum_groups/definition_kinds",
        "selection": "array_values",
    },
    "document_kind": {
        "source": "10_REGISTRIES/built_in_groups_and_results_v0.1.0.json",
        "pointer": "/enum_groups/document_kinds",
        "selection": "array_values",
    },
    "encoding": {
        "source": "10_REGISTRIES/formats_encodings_units_v0.1.0.json",
        "pointer": "/encodings",
        "selection": "object_keys",
    },
    "error": {
        "source": "10_REGISTRIES/statuses_and_errors_v0.1.0.json",
        "pointer": "/errors",
        "selection": "object_keys",
        "defined_kind": "kind.error",
    },
    "event": {
        "source": "10_REGISTRIES/built_in_groups_and_results_v0.1.0.json",
        "pointer": "/enum_groups/events",
        "selection": "array_values",
        "defined_kind": "kind.event",
    },
    "format": {
        "source": "10_REGISTRIES/formats_encodings_units_v0.1.0.json",
        "pointer": "/formats",
        "selection": "object_keys",
        "defined_kind": "kind.format",
    },
    "mode": {
        "source": "10_REGISTRIES/built_in_groups_and_results_v0.1.0.json",
        "pointer": "/enum_groups/modes",
        "selection": "array_values",
    },
    "status": {
        "source": "10_REGISTRIES/statuses_and_errors_v0.1.0.json",
        "pointer": "/statuses",
        "selection": "object_keys",
        "defined_kind": "kind.status",
    },
    "terminal_non_success_status": {
        "source": "10_REGISTRIES/statuses_and_errors_v0.1.0.json",
        "pointer": "/statuses",
        "selection": "object_keys_where",
        "where": {"terminal": True},
        "exclude": ["status.succeeded"],
        "defined_kind": "kind.status",
        "defined_kind_filter": (
            "Resolve the alias first, then apply where and exclude to its canonical "
            "core status."
        ),
    },
}
TASK_0002_REFERENCE_DOMAINS = {
    "execution_unit": {
        "meta_type": "meta.execution_unit",
        "members": ["TASK", "PHASE", "SEQUENCE", "STEP", "ACTION", "TEST"],
    },
    "rule_clause": {
        "meta_type": "meta.rule_clause",
        "members": ["ALLOW", "FORBID", "REQUIRE", "PREFER", "PRESERVE", "OVERRIDE"],
    },
}
QUALIFIED_IDENTIFIER = re.compile(r"[a-z][a-z0-9_]*(?:\.[a-z][a-z0-9_]*)+")
INTEGER_LITERAL = r"(?:0|-?[1-9][0-9]*)"


def json_pointer(document: Any, pointer: str) -> Any:
    if pointer == "":
        return document
    if not pointer.startswith("/"):
        raise ValueError(f"JSON pointer must be empty or start with '/': {pointer!r}")
    current = document
    for raw_token in pointer[1:].split("/"):
        token = raw_token.replace("~1", "/").replace("~0", "~")
        if isinstance(current, dict):
            if token not in current:
                raise ValueError(f"JSON pointer token {token!r} is absent in {pointer!r}")
            current = current[token]
        elif isinstance(current, list) and re.fullmatch(r"0|[1-9][0-9]*", token):
            index = int(token)
            if index >= len(current):
                raise ValueError(f"JSON pointer index {index} is out of range in {pointer!r}")
            current = current[index]
        else:
            raise ValueError(f"JSON pointer cannot traverse token {token!r} in {pointer!r}")
    return current


def validate_value_kind_templates(templates: Any) -> list[str]:
    errors: list[str] = []
    if not isinstance(templates, dict):
        return ["value_kind_templates must be an object"]
    if set(templates) != set(VALUE_KIND_TEMPLATE_CONTRACTS):
        errors.append(
            "value-kind template heads do not match the closed template contract: "
            f"{sorted(set(templates) ^ set(VALUE_KIND_TEMPLATE_CONTRACTS))}"
        )
    for head, contract in sorted(templates.items()):
        if not re.fullmatch(r"[a-z][a-z0-9_]*", head):
            errors.append(f"invalid value-kind template head {head!r}")
            continue
        if not isinstance(contract, dict):
            errors.append(f"value-kind template {head} must be an object")
            continue
        required_keys = {"syntax", "argument_kind", "accepted_forms"}
        if set(contract) != required_keys:
            errors.append(
                f"value-kind template {head} keys must be exactly {sorted(required_keys)}"
            )
            continue
        if contract != VALUE_KIND_TEMPLATE_CONTRACTS.get(head):
            errors.append(f"value-kind template {head} differs from its exact contract")
        syntax = contract["syntax"]
        argument_kind = contract["argument_kind"]
        accepted_forms = contract["accepted_forms"]
        if not isinstance(syntax, str) or not syntax.startswith(head):
            errors.append(f"value-kind template {head} syntax does not start with its head")
        if argument_kind not in VALUE_KIND_ARGUMENT_KINDS:
            errors.append(
                f"value-kind template {head} has unknown argument_kind {argument_kind!r}"
            )
        if not isinstance(accepted_forms, list) or not accepted_forms:
            errors.append(f"value-kind template {head} accepted_forms must be a nonempty list")
        elif not all(isinstance(item, str) for item in accepted_forms):
            errors.append(f"value-kind template {head} accepted_forms must contain only strings")
        else:
            if len(accepted_forms) != len(set(accepted_forms)):
                errors.append(f"value-kind template {head} accepted_forms contains duplicates")
            unknown_forms = sorted(set(accepted_forms) - VALUE_KIND_ACCEPTED_FORMS)
            if unknown_forms:
                errors.append(
                    f"value-kind template {head} has unknown accepted forms {unknown_forms}"
                )
    semantic_signatures: dict[tuple[str, tuple[str, ...]], list[str]] = {}
    for head, contract in templates.items():
        if isinstance(contract, dict) and isinstance(contract.get("accepted_forms"), list):
            signature = (
                str(contract.get("argument_kind")),
                tuple(sorted(str(item) for item in contract["accepted_forms"])),
            )
            semantic_signatures.setdefault(signature, []).append(head)
    for heads in semantic_signatures.values():
        if len(heads) > 1:
            errors.append(f"duplicate value-kind template semantics: {sorted(heads)}")
    return errors


def validate_named_value_kinds(named: Any) -> list[str]:
    errors: list[str] = []
    if not isinstance(named, dict):
        return ["value_kind_registry must be an object"]
    descriptions: dict[str, list[str]] = {}
    for name, description in sorted(named.items()):
        if not re.fullmatch(r"[a-z][a-z0-9_]*", name):
            errors.append(f"invalid named value kind {name!r}")
        if not isinstance(description, str) or not description.strip():
            errors.append(f"named value kind {name} must have a nonempty definition")
        else:
            descriptions.setdefault(description, []).append(name)
    for names in descriptions.values():
        if len(names) > 1:
            errors.append(f"duplicate named value-kind definitions: {sorted(names)}")
    for name, expected in TASK_0002_NAMED_VALUE_KIND_DEFINITIONS.items():
        if named.get(name) != expected:
            errors.append(f"named value kind {name} differs from its Task-0002 contract")
    return errors


def resolve_qualified_identifier_domains(
    root: Path, contracts: Any, definition_kinds: set[str]
) -> tuple[dict[str, list[str]], list[str]]:
    resolved: dict[str, list[str]] = {}
    errors: list[str] = []
    if not isinstance(contracts, dict):
        return resolved, ["qualified_identifier_domains must be an object"]
    if contracts != TASK_0002_QUALIFIED_IDENTIFIER_DOMAINS:
        errors.append("qualified_identifier_domains differ from the exact Task-0002 contract")
    resolved_root = root.resolve()
    for domain, contract in sorted(contracts.items()):
        if not re.fullmatch(r"[a-z][a-z0-9_]*", domain):
            errors.append(f"invalid qualified-identifier domain {domain!r}")
            continue
        if not isinstance(contract, dict):
            errors.append(f"qualified-identifier domain {domain} must be an object")
            continue
        required = {"source", "pointer", "selection"}
        optional = {"where", "exclude", "defined_kind", "defined_kind_filter"}
        if not required <= set(contract) or set(contract) - required - optional:
            errors.append(
                f"qualified-identifier domain {domain} has invalid contract keys {sorted(contract)}"
            )
            continue
        source = contract["source"]
        pointer = contract["pointer"]
        selection = contract["selection"]
        defined_kind = contract.get("defined_kind")
        alias_filter = contract.get("defined_kind_filter")
        if alias_filter is not None and (
            defined_kind != "kind.status"
            or selection != "object_keys_where"
            or alias_filter != TASK_0002_QUALIFIED_IDENTIFIER_DOMAINS[
                "terminal_non_success_status"
            ]["defined_kind_filter"]
        ):
            errors.append(f"qualified-identifier domain {domain} has invalid alias filtering")
            continue
        if selection != "object_keys_where" and "where" in contract:
            errors.append(
                f"qualified-identifier domain {domain} uses where with selection {selection!r}"
            )
            continue
        if defined_kind is not None and (
            not isinstance(defined_kind, str) or defined_kind not in definition_kinds
        ):
            errors.append(
                f"qualified-identifier domain {domain} uses undefined definition kind {defined_kind!r}"
            )
            continue
        if not isinstance(source, str) or not isinstance(pointer, str):
            errors.append(f"qualified-identifier domain {domain} source/pointer must be strings")
            continue
        source_path = (root / source).resolve()
        try:
            source_path.relative_to(resolved_root)
        except ValueError:
            errors.append(f"qualified-identifier domain {domain} source escapes the release root")
            continue
        if not source_path.is_file():
            errors.append(f"qualified-identifier domain {domain} source is absent: {source}")
            continue
        try:
            node = json_pointer(load_json_strict(source_path), pointer)
        except (OSError, ValueError, json.JSONDecodeError) as error:
            errors.append(f"qualified-identifier domain {domain} cannot resolve source: {error}")
            continue

        values: list[str]
        if selection == "array_values":
            if not isinstance(node, list) or not all(isinstance(item, str) for item in node):
                errors.append(
                    f"qualified-identifier domain {domain} array_values target must be a string array"
                )
                continue
            values = list(node)
        elif selection == "object_keys":
            if not isinstance(node, dict):
                errors.append(
                    f"qualified-identifier domain {domain} object_keys target must be an object"
                )
                continue
            values = list(node)
        elif selection == "object_keys_where":
            where = contract.get("where")
            if not isinstance(node, dict) or not isinstance(where, dict) or not where:
                errors.append(
                    f"qualified-identifier domain {domain} object_keys_where requires an object and nonempty where"
                )
                continue
            values = [
                key
                for key, item in node.items()
                if isinstance(item, dict)
                and all(item.get(where_key) == expected for where_key, expected in where.items())
            ]
        else:
            errors.append(
                f"qualified-identifier domain {domain} has unknown selection {selection!r}"
            )
            continue

        exclude = contract.get("exclude", [])
        if not isinstance(exclude, list) or not all(isinstance(item, str) for item in exclude):
            errors.append(f"qualified-identifier domain {domain} exclude must be a string list")
            continue
        if len(exclude) != len(set(exclude)):
            errors.append(f"qualified-identifier domain {domain} exclude contains duplicates")
            continue
        missing_exclusions = sorted(set(exclude) - set(values))
        if missing_exclusions:
            errors.append(
                f"qualified-identifier domain {domain} excludes unselected values {missing_exclusions}"
            )
            continue
        values = [item for item in values if item not in set(exclude)]
        if not values:
            errors.append(f"qualified-identifier domain {domain} resolves to no values")
            continue
        if len(values) != len(set(values)):
            errors.append(f"qualified-identifier domain {domain} resolves duplicate values")
            continue
        invalid_values = sorted(item for item in values if not QUALIFIED_IDENTIFIER.fullmatch(item))
        if invalid_values:
            errors.append(
                f"qualified-identifier domain {domain} contains invalid identifiers {invalid_values}"
            )
            continue
        resolved[domain] = sorted(values)
    return resolved, errors


def validate_reference_domains(
    contracts: Any, meta_types: dict[str, Any], blocks: dict[str, Any]
) -> tuple[dict[str, set[str]], list[str]]:
    resolved: dict[str, set[str]] = {}
    errors: list[str] = []
    if not isinstance(contracts, dict):
        return resolved, ["reference_domains must be an object"]
    if contracts != TASK_0002_REFERENCE_DOMAINS:
        errors.append("reference_domains differ from the exact Task-0002 contract")
    for domain, contract in sorted(contracts.items()):
        if not re.fullmatch(r"[a-z][a-z0-9_]*", domain):
            errors.append(f"invalid reference domain {domain!r}")
            continue
        if not isinstance(contract, dict) or set(contract) != {"meta_type", "members"}:
            errors.append(
                f"reference domain {domain} must have exactly meta_type and members"
            )
            continue
        meta_type = contract["meta_type"]
        members = contract["members"]
        if meta_type not in meta_types:
            errors.append(f"reference domain {domain} uses undefined meta type {meta_type!r}")
        if not isinstance(members, list) or not members or not all(
            isinstance(item, str) for item in members
        ):
            errors.append(f"reference domain {domain} members must be a nonempty string list")
            continue
        if len(members) != len(set(members)):
            errors.append(f"reference domain {domain} members contains duplicates")
        undefined_members = sorted(set(members) - set(blocks))
        if undefined_members:
            errors.append(
                f"reference domain {domain} names undefined blocks {undefined_members}"
            )
        described_members = (
            set(re.findall(r"\b[A-Z][A-Z_]*\b", meta_types[meta_type]))
            if meta_type in meta_types and isinstance(meta_types[meta_type], str)
            else set()
        )
        if described_members != set(members):
            errors.append(
                f"reference domain {domain} members differ from {meta_type}: "
                f"{sorted(described_members ^ set(members))}"
            )
        if (
            meta_type in meta_types
            and not undefined_members
            and described_members == set(members)
            and len(members) == len(set(members))
        ):
            resolved[domain] = set(members)
    return resolved, errors


def resolve_value_kind(
    value: str,
    named: dict[str, Any],
    templates: dict[str, Any],
    qualified_domains: dict[str, list[str]],
    blocks: dict[str, Any],
    reference_domains: dict[str, set[str]],
) -> tuple[dict[str, Any] | None, str | None]:
    if value in named:
        return {"classification": "named", "name": value, "nested_targets": []}, None
    head = value_kind_head(value)
    contract = templates.get(head)
    if not isinstance(contract, dict):
        return None, f"unknown value-kind head {head!r}"
    argument_kind = contract.get("argument_kind")
    nested: list[str] = []
    arguments: list[str] = []

    if argument_kind == "json_string_literal":
        match = re.fullmatch(rf"{re.escape(head)}\((.*)\)", value)
        if not match:
            return None, "does not match exact-string template syntax"
        try:
            decoded = json.loads(match.group(1))
        except json.JSONDecodeError as error:
            return None, f"contains an invalid JSON string literal: {error.msg}"
        if not isinstance(decoded, str):
            return None, "exact-string argument is not a JSON string"
        arguments = [decoded]
    elif argument_kind == "closed_integer_range":
        match = re.fullmatch(
            rf"{re.escape(head)}\[({INTEGER_LITERAL})\.\.({INTEGER_LITERAL})\]", value
        )
        if not match:
            return None, "does not match closed-integer-range template syntax"
        minimum, maximum = (int(match.group(1)), int(match.group(2)))
        if minimum > maximum:
            return None, "integer range minimum exceeds maximum"
        arguments = [match.group(1), match.group(2)]
    elif argument_kind == "qualified_identifier_domain":
        match = re.fullmatch(rf"{re.escape(head)}\(([a-z][a-z0-9_]*)\)", value)
        if not match:
            return None, "does not match qualified-identifier-domain template syntax"
        domain = match.group(1)
        if domain not in qualified_domains:
            return None, f"uses undefined qualified-identifier domain {domain!r}"
        arguments = [domain]
    elif argument_kind == "reference_target_union":
        match = re.fullmatch(
            rf"{re.escape(head)}\(([A-Za-z][A-Za-z0-9_]*(?:\|[A-Za-z][A-Za-z0-9_]*)*)\)",
            value,
        )
        if not match:
            return None, "does not match reference-target-union template syntax"
        arguments = match.group(1).split("|")
        if len(arguments) != len(set(arguments)):
            return None, "reference target union contains duplicates"
        undefined = sorted(
            item for item in arguments if item not in blocks and item not in reference_domains
        )
        if undefined:
            return None, f"uses undefined reference targets {undefined}"
        expanded_targets: dict[str, set[str]] = {
            item: ({item} if item in blocks else reference_domains[item]) for item in arguments
        }
        overlaps = []
        for index, left in enumerate(arguments):
            for right in arguments[index + 1 :]:
                overlap = sorted(expanded_targets[left] & expanded_targets[right])
                if overlap:
                    overlaps.append({"left": left, "right": right, "members": overlap})
        if overlaps:
            return None, f"reference target union contains semantic overlap {overlaps}"
    elif argument_kind == "block_name":
        match = re.fullmatch(rf"{re.escape(head)}\(([A-Z][A-Z_]*)\)", value)
        if not match:
            return None, "does not match block-name template syntax"
        target = match.group(1)
        if target not in blocks:
            return None, f"uses undefined block target {target!r}"
        arguments = [target]
        if "nested_block" in contract.get("accepted_forms", []):
            nested = [target]
    else:
        return None, f"template has unsupported argument kind {argument_kind!r}"

    return {
        "classification": "template",
        "head": head,
        "arguments": arguments,
        "nested_targets": nested,
    }, None


def default_value_error(
    default: Any,
    resolution: dict[str, Any],
    qualified_domains: dict[str, list[str]],
) -> str | None:
    if default is None:
        return None
    if resolution.get("classification") == "named":
        name = resolution.get("name")
        if name == "boolean" and not isinstance(default, bool):
            return "boolean default is not a JSON boolean"
        if name in {"string", "string_or_multiline_string"} and not isinstance(
            default, str
        ):
            return "string default is not a JSON string"
        return None
    head = resolution.get("head")
    arguments = resolution.get("arguments", [])
    if head == "integer":
        if not isinstance(default, int) or isinstance(default, bool):
            return "integer-range default is not a JSON integer"
        minimum, maximum = (int(arguments[0]), int(arguments[1]))
        if not minimum <= default <= maximum:
            return f"integer-range default is outside {minimum}..{maximum}"
    elif head == "qualified_identifier":
        domain = arguments[0]
        if not isinstance(default, str) or default not in qualified_domains[domain]:
            return f"qualified-identifier default is not a core member of {domain}"
    elif head == "exact_string" and default != arguments[0]:
        return "exact-string default differs from the required string"
    return None


def block_contract_conflicts(
    blocks: dict[str, Any], field_blocks: dict[str, Any]
) -> list[dict[str, Any]]:
    conflicts: list[dict[str, Any]] = []
    for name in sorted(set(blocks) & set(field_blocks)):
        schema = blocks[name]
        signature = field_blocks[name]
        expected_schema_keys = {
            "contexts",
            "occurrence",
            "required",
            "optional",
            "repeatable",
            "rules",
        }
        expected_signature_keys = {
            "legal_parents",
            "block_occurrence",
            "fields",
            "conditional_requirements",
            "unknown_fields",
            "field_order",
        }
        if set(schema) != expected_schema_keys:
            conflicts.append({"block": name, "contract": "schema_keys"})
        if set(signature) != expected_signature_keys:
            conflicts.append({"block": name, "contract": "signature_keys"})
        for array_name in ("contexts", "required", "optional", "repeatable", "rules"):
            values = schema.get(array_name, [])
            if not isinstance(values, list):
                conflicts.append({"block": name, "contract": array_name, "error": "not_a_list"})
            elif len(values) != len(set(values)):
                conflicts.append({"block": name, "contract": array_name, "error": "duplicates"})
        for array_name in ("legal_parents", "conditional_requirements"):
            values = signature.get(array_name, [])
            if not isinstance(values, list):
                conflicts.append({"block": name, "contract": array_name, "error": "not_a_list"})
            elif len(values) != len(set(values)):
                conflicts.append({"block": name, "contract": array_name, "error": "duplicates"})
        declared = set(schema.get("required", [])) | set(schema.get("optional", []))
        signed = set(signature.get("fields", {}))
        if declared != signed:
            conflicts.append(
                {
                    "block": name,
                    "contract": "fields",
                    "schema_only": sorted(declared - signed),
                    "signature_only": sorted(signed - declared),
                }
            )
        overlap = sorted(set(schema.get("required", [])) & set(schema.get("optional", [])))
        if overlap:
            conflicts.append({"block": name, "contract": "required_optional_overlap", "fields": overlap})
        if set(schema.get("contexts", [])) != set(signature.get("legal_parents", [])):
            conflicts.append({"block": name, "contract": "contexts"})
        if schema.get("occurrence") != signature.get("block_occurrence"):
            conflicts.append({"block": name, "contract": "occurrence"})
        if set(schema.get("rules", [])) != set(signature.get("conditional_requirements", [])):
            conflicts.append({"block": name, "contract": "rules"})
        repeated = set(schema.get("repeatable", []))
        unknown_repeated = sorted(repeated - signed)
        if unknown_repeated:
            conflicts.append(
                {"block": name, "contract": "repeatable", "unknown_fields": unknown_repeated}
            )
        for field_name, field in signature.get("fields", {}).items():
            if set(field) != {
                "required",
                "minimum_occurrences",
                "maximum_occurrences",
                "value_kind",
                "default",
            }:
                conflicts.append(
                    {"block": name, "field": field_name, "contract": "field_keys"}
                )
            is_required = field_name in set(schema.get("required", []))
            if field.get("required") is not is_required:
                conflicts.append(
                    {"block": name, "field": field_name, "contract": "required_flag"}
                )
            expected_minimum = 1 if is_required else 0
            minimum = field.get("minimum_occurrences")
            if minimum != expected_minimum or isinstance(minimum, bool):
                conflicts.append(
                    {"block": name, "field": field_name, "contract": "minimum_occurrences"}
                )
            maximum = field.get("maximum_occurrences")
            if field_name in repeated:
                maximum_valid = maximum is None or (
                    isinstance(maximum, int)
                    and not isinstance(maximum, bool)
                    and maximum >= 2
                )
            else:
                maximum_valid = maximum == 1 and not isinstance(maximum, bool)
            if not maximum_valid:
                conflicts.append(
                    {
                        "block": name,
                        "field": field_name,
                        "contract": "maximum_occurrences",
                        "expected": "null_or_integer_at_least_2" if field_name in repeated else 1,
                        "actual": maximum,
                    }
                )
    return conflicts


def ebnf_rhs(grammar: str, production: str) -> str:
    match = re.search(rf"(?ms)^{re.escape(production)}\s*=\s*(.*?);", grammar)
    return match.group(1) if match else ""


def pseudo_parent_is_admitted(
    parent: str, child: str, grammar: str, schema_prose: str
) -> bool:
    document = ebnf_rhs(grammar, "DOCUMENT")
    if parent == "top_level_first":
        lcl_header = ebnf_rhs(grammar, "LCL_HEADER")
        return (
            child == "LCL"
            and "LCL_HEADER" in document
            and "SPECIFICATION_HEADER" in document
            and document.index("LCL_HEADER") < document.index("SPECIFICATION_HEADER")
            and '"LCL"' in lcl_header
        )
    if parent == "top_level_second":
        specification_header = ebnf_rhs(grammar, "SPECIFICATION_HEADER")
        return (
            child == "SPECIFICATION"
            and "LCL_HEADER" in document
            and "SPECIFICATION_HEADER" in document
            and document.index("LCL_HEADER") < document.index("SPECIFICATION_HEADER")
            and '"SPECIFICATION"' in specification_header
        )
    if parent == "top_level":
        return (
            "TOP_LEVEL_BLOCK" in document
            and "CORE_BLOCK" in ebnf_rhs(grammar, "TOP_LEVEL_BLOCK")
            and "BLOCK_WORD" in ebnf_rhs(grammar, "CORE_BLOCK")
            and f'"{child}"' in ebnf_rhs(grammar, "BLOCK_WORD")
        )
    if parent == "SCHEMA":
        return (
            child == "FIELD"
            and "NESTED_FIELD" in ebnf_rhs(grammar, "BLOCK_STATEMENT")
            and "FIELD_KEY" in ebnf_rhs(grammar, "NESTED_FIELD")
            and "NESTED_BODY" in ebnf_rhs(grammar, "NESTED_FIELD")
            and re.search(r"SCHEMA.*nested sequence of FIELD", schema_prose, re.DOTALL)
            is not None
        )
    if parent in {"IF", "FOR_EACH", "ELSE"}:
        production = {"STEP": "STEP_BLOCK", "COMMENT": "COMMENT_BLOCK"}.get(child)
        executable_statement = ebnf_rhs(grammar, "EXECUTABLE_STATEMENT")
        parent_body = {
            "IF": ebnf_rhs(grammar, "CONDITIONAL"),
            "FOR_EACH": ebnf_rhs(grammar, "FOR_EACH"),
            "ELSE": ebnf_rhs(grammar, "CONDITIONAL"),
        }[parent]
        parent_admitted = {
            "IF": '"IF"' in parent_body and "EXECUTABLE_BODY" in parent_body,
            "FOR_EACH": '"FOR"' in parent_body and "EXECUTABLE_BODY" in parent_body,
            "ELSE": '"ELSE"' in parent_body and parent_body.count("EXECUTABLE_BODY") >= 2,
        }[parent]
        return (
            production is not None
            and production in executable_statement
            and parent_admitted
        )
    return False


def collection_contract_violations(
    types: dict[str, Any],
    keywords: dict[str, Any],
    blocks: dict[str, Any],
    fields: dict[str, Any],
    symbols: dict[str, Any],
    grammar: str,
) -> list[str]:
    violations: list[str] = []
    expected_syntax = {
        "LIST": {
            "forms": ["inline_bracket", "multiline_bracket"],
            "item_field_allowed": False,
        },
        "SET": {
            "forms": ["inline_bracket", "multiline_bracket"],
            "item_field_allowed": False,
        },
        "ITEM": {"legal_use": "enum_member_declaration_only"},
    }
    if types.get("collection_value_syntax") != expected_syntax:
        violations.append("types.collection_value_syntax is not the exact bracket/enum-only contract")

    expected_item_keyword = {
        "category": "field",
        "meaning": "Declare one member identifier of a user-defined ENUM.",
        "contexts": (
            "Repeatable only in DEFINE declarations with KIND kind.type and BASE ENUM; "
            "never an independent block or collection-value form."
        ),
        "case_sensitive": True,
    }
    if keywords.get("ITEM") != expected_item_keyword:
        violations.append("ITEM keyword metadata is not enum-only")

    item_schema_owners = sorted(
        name
        for name, schema in blocks.items()
        if "ITEM" in set(schema["required"]) | set(schema["optional"])
    )
    item_signature_owners = sorted(
        name for name, block in fields["blocks"].items() if "ITEM" in block["fields"]
    )
    if item_schema_owners != ["DEFINE"] or item_signature_owners != ["DEFINE"]:
        violations.append("ITEM has a schema/signature owner other than DEFINE")

    define_schema = blocks.get("DEFINE", {})
    define_signature = fields["blocks"].get("DEFINE", {}).get("fields", {}).get("ITEM")
    expected_item_signature = {
        "required": False,
        "minimum_occurrences": 0,
        "maximum_occurrences": None,
        "value_kind": "simple_identifier",
        "default": None,
    }
    enum_rule = (
        "ITEM is legal only for KIND kind.type with BASE ENUM and occurs one or more times there."
    )
    if (
        "ITEM" not in define_schema.get("optional", [])
        or "ITEM" not in define_schema.get("repeatable", [])
        or enum_rule not in define_schema.get("rules", [])
        or define_signature != expected_item_signature
    ):
        violations.append("DEFINE.ITEM is not the exact repeatable enum-member signature")

    block_word_segment = grammar[grammar.index("BLOCK_WORD =") : grammar.index("IDENTIFIER =")]
    grammar_checks = {
        "shared inline collection production": "COLLECTION_LITERAL = \"[\"" in grammar,
        "shared multiline collection production": "MULTILINE_COLLECTION = \"[\"" in grammar,
        "no list-only production names": not re.search(r"\b(?:LIST_LITERAL|MULTILINE_LIST)\b", grammar),
        "ITEM is not a block word": '"ITEM"' not in block_word_segment,
        "ITEM has no production": not re.search(r"(?m)^ITEM\s*=", grammar),
    }
    violations.extend(name for name, passed in grammar_checks.items() if not passed)

    adopted = symbols.get("adopted", {})
    if "LIST/SET collection literal" not in adopted.get("[", ""):
        violations.append("adopted '[' description is not shared by LIST and SET")
    if "LIST/SET collection literal" not in adopted.get("]", ""):
        violations.append("adopted ']' description is not shared by LIST and SET")
    if "collection-literal members" not in adopted.get(",", ""):
        violations.append("adopted ',' description is not collection-generic")
    return violations


def accepted_parent_contract_violations(fields: dict[str, Any]) -> list[str]:
    violations: list[str] = []
    signatures = fields["blocks"]
    expected_fields = {
        ("TASK", "PHASE"): "reference_or_list(PHASE)",
        ("TASK", "SEQUENCE"): "reference_or_list(SEQUENCE)",
        ("TASK", "ACTION"): "reference_or_list(ACTION)",
        ("REQUIRE", "ACTION"): "reference_or_list(ACTION)",
        ("PREFER", "ACTION"): "reference_or_list(ACTION)",
        ("STEP", "SEQUENCE"): "reference_or_list(SEQUENCE)",
        ("STEP", "PHASE"): "reference_or_list(PHASE)",
    }
    for (block_name, field_name), expected in expected_fields.items():
        actual = signatures.get(block_name, {}).get("fields", {}).get(field_name, {}).get("value_kind")
        if actual != expected:
            violations.append(f"{block_name}.{field_name}: expected {expected}, got {actual}")

    expected_parents = {
        "PHASE": ["top_level"],
        "SEQUENCE": ["top_level", "PHASE"],
        "ACTION": ["top_level", "STEP"],
    }
    for block_name, expected in expected_parents.items():
        actual = signatures.get(block_name, {}).get("legal_parents")
        if actual != expected:
            violations.append(f"{block_name}.legal_parents: expected {expected}, got {actual}")

    preserved_nested = {
        ("PHASE", "SEQUENCE"): "reference_or_list_or_nested(SEQUENCE)",
        ("STEP", "ACTION"): "reference_or_list_or_nested(ACTION)",
    }
    for (block_name, field_name), expected in preserved_nested.items():
        actual = signatures.get(block_name, {}).get("fields", {}).get(field_name, {}).get("value_kind")
        if actual != expected:
            violations.append(f"{block_name}.{field_name}: expected preserved {expected}, got {actual}")
    return violations


def task_0002_field_contract_violations(
    fields: dict[str, Any], keywords: dict[str, Any]
) -> list[str]:
    violations: list[str] = []
    signatures = fields["blocks"]
    exact_fields = {
        ("DEFINE", "BASE"): "type_or_format_base",
        ("DEFINE", "EXAMPLE"): "reference_or_nested(EXAMPLE)",
        ("PRESERVE", "PROPERTY"): "property_path",
        ("OUTPUT", "PROPERTY"): "property_path",
        ("EXAMPLE", "CONTENT"): "value_or_object_expression",
    }
    for (block_name, field_name), expected in exact_fields.items():
        actual = (
            signatures.get(block_name, {})
            .get("fields", {})
            .get(field_name, {})
            .get("value_kind")
        )
        if actual != expected:
            violations.append(f"{block_name}.{field_name}: expected {expected}, got {actual}")

    for field_name, expected in {
        "SCHEMA": "schema_reference_or_nested_schema",
        "CHECKSUM": "sha256_string",
        "BEFORE": "reference_or_list(execution_unit)",
        "AFTER": "reference_or_list(execution_unit)",
    }.items():
        owners = [
            (block_name, block["fields"][field_name].get("value_kind"))
            for block_name, block in signatures.items()
            if field_name in block["fields"]
        ]
        if not owners:
            violations.append(f"no registered owner for {field_name}")
        for block_name, actual in owners:
            if actual != expected:
                violations.append(
                    f"{block_name}.{field_name}: expected {expected}, got {actual}"
                )
    expected_keyword_meanings = {
        "AFTER": "Require one execution unit to occur after named predecessors.",
        "BEFORE": "Require one execution unit to occur before named successors.",
    }
    for keyword, expected in expected_keyword_meanings.items():
        actual = keywords.get(keyword, {}).get("meaning")
        if actual != expected:
            violations.append(f"{keyword}.meaning: expected {expected!r}, got {actual!r}")
    return violations


def cross_registry_contract_violations(
    groups_and_results: dict[str, Any],
    formats_registry: dict[str, Any],
    statuses: dict[str, Any],
    operations: dict[str, Any],
) -> list[str]:
    violations: list[str] = []
    enum_groups = groups_and_results["enum_groups"]
    builtins = formats_registry["builtins"]
    comparisons: dict[str, tuple[list[str], list[str]]] = {
        "definition_kinds": (enum_groups["definition_kinds"], builtins["definition_kinds"]),
        "document_kinds": (enum_groups["document_kinds"], builtins["document_kinds"]),
        "events": (enum_groups["events"], builtins["events"]),
        "modes": (enum_groups["modes"], builtins["modes"]),
        "reserved_namespaces": (
            groups_and_results["reserved_namespaces"],
            builtins["reserved_namespaces"],
        ),
        "formats": (enum_groups["formats"], list(formats_registry["formats"])),
        "encodings": (enum_groups["encodings"], list(formats_registry["encodings"])),
        "units": (enum_groups["units"], list(formats_registry["units"])),
        "statuses": (enum_groups["statuses"], list(statuses["statuses"])),
        "errors": (enum_groups["errors"], list(statuses["errors"])),
        "core_operation_ids": (
            groups_and_results["core_operation_ids"],
            list(operations),
        ),
    }
    for name, (left, right) in comparisons.items():
        if len(left) != len(set(left)) or len(right) != len(set(right)):
            violations.append(f"{name} contains duplicates")
        if set(left) != set(right):
            violations.append(
                f"{name} differs across registries: {sorted(set(left) ^ set(right))}"
            )
    return violations


def priority_contract_violations(
    fields: dict[str, Any], blocks: dict[str, Any], statuses: dict[str, Any]
) -> list[str]:
    violations: list[str] = []
    expected_optional = {"GOAL", "ALLOW", "FORBID", "REQUIRE", "PREFER", "PRESERVE"}
    policy = fields.get("field_policies", {}).get("PRIORITY")
    expected_policy = {
        "optional_default": 0,
        "inheritance": "none",
        "explicit_value_precedence": "explicit_over_default",
        "mandatory_omission_error": "error.field.required",
    }
    if policy != expected_policy or type((policy or {}).get("optional_default")) is not int:
        violations.append("field_policies.PRIORITY is not the exact D-005 policy")

    priority_blocks = {
        name: block["fields"]["PRIORITY"]
        for name, block in fields["blocks"].items()
        if "PRIORITY" in block["fields"]
    }
    optional_names = {name for name, signature in priority_blocks.items() if not signature["required"]}
    if optional_names != expected_optional:
        violations.append(
            f"optional PRIORITY blocks: expected {sorted(expected_optional)}, got {sorted(optional_names)}"
        )

    optional_rule = (
        "Optional PRIORITY defaults to 0, never inherits, and an explicit declaration overrides the default."
    )
    for name, signature in priority_blocks.items():
        common_ok = (
            signature.get("value_kind") == "integer[-1000..1000]"
            and signature.get("maximum_occurrences") == 1
        )
        schema = blocks.get(name, {})
        if signature.get("required"):
            valid = (
                common_ok
                and signature.get("minimum_occurrences", 0) >= 1
                and signature.get("default") is None
                and "PRIORITY" in schema.get("required", [])
                and "PRIORITY" not in schema.get("optional", [])
            )
        else:
            valid = (
                common_ok
                and signature.get("minimum_occurrences") == 0
                and type(signature.get("default")) is int
                and signature.get("default") == 0
                and "PRIORITY" in schema.get("optional", [])
                and "PRIORITY" not in schema.get("required", [])
                and optional_rule in schema.get("rules", [])
            )
        if not valid:
            violations.append(f"{name}.PRIORITY does not satisfy its required/optional contract")

    required_error = statuses.get("errors", {}).get("error.field.required")
    if not required_error or required_error.get("default_status") != "status.invalid":
        violations.append("mandatory PRIORITY omission error.field.required is unavailable")
    return violations


def constructor_pattern_contract_violations(
    types: dict[str, Any],
    operator_functions: dict[str, Any],
    statuses: dict[str, Any],
    error_group: list[str],
    units: dict[str, str],
) -> list[str]:
    violations: list[str] = []

    def expect(condition: bool, message: str) -> None:
        if not condition:
            violations.append(message)

    constructors = operator_functions.get("constructors", {})
    expected_parameters = {
        "PATH": [["STRING"], ["REFERENCE[WORKSPACE]", "STRING"]],
        "URI": [["STRING"]],
        "GLOB": [["STRING"]],
        "REGEX": [["STRING"], ["STRING", "STRING"]],
        "DATE": [["STRING"]],
        "TIME": [["STRING"]],
        "DATETIME": [["STRING"]],
        "DURATION": [["INTEGER|DECIMAL", "qualified_identifier(unit)"]],
        "PERCENTAGE": [["INTEGER|DECIMAL"]],
        "BYTES": [["INTEGER"]],
        "MEASURE": [["INTEGER|DECIMAL", "qualified_identifier(unit)"]],
    }
    expect(set(constructors) == set(expected_parameters), "constructor name set is not exact")
    for name, parameters in expected_parameters.items():
        contract = constructors.get(name, {})
        actual = [overload.get("parameters") for overload in contract.get("overloads", [])]
        expect(actual == parameters, f"{name} constructor overloads are not exact")
        expect(contract.get("result") == name, f"{name} constructor result is not {name}")

    path = constructors.get("PATH", {})
    path_constraints = [item.get("constraint", "") for item in path.get("overloads", [])]
    expect(path.get("variadic") is False, "PATH must be explicitly non-variadic")
    expect(path.get("workspace_escape_error") == "error.value.out_of_range", "PATH escape error is wrong")
    expect(
        len(path_constraints) == 2
        and "IMPORT.SOURCE" in path_constraints[0]
        and "EXTENSION.SOURCE" in path_constraints[0]
        and "WORKSPACE root or one of its descendants" in path_constraints[1],
        "PATH constructor constraints do not enforce documented forms and containment",
    )

    uri = constructors.get("URI", {})
    expect(uri.get("profile") == "RFC 3986 absolute-URI with a scheme", "URI profile is not exact")
    expect(uri.get("relative_references_allowed") is False, "relative URI references are not rejected")
    expect(constructors.get("DATE", {}).get("profile") == "RFC 3339 full-date narrowed exactly by types_v0.1.0.json#/temporal_literal_contract", "DATE profile is not RFC 3339")
    expect(
        constructors.get("TIME", {}).get("profile") == "RFC 3339 partial-time with optional time-offset narrowed exactly by types_v0.1.0.json#/temporal_literal_contract"
        and constructors.get("TIME", {}).get("omitted_timezone") == "UTC",
        "TIME profile/default timezone is not exact",
    )
    expect(
        constructors.get("DATETIME", {}).get("profile")
        == "RFC 3339 full-date T partial-time with optional time-offset narrowed exactly by types_v0.1.0.json#/temporal_literal_contract"
        and constructors.get("DATETIME", {}).get("omitted_timezone") == "UTC",
        "DATETIME profile/default timezone is not exact",
    )
    expect(
        constructors.get("DURATION", {}).get("unit_category") == "Time"
        and constructors.get("DURATION", {}).get("minimum") == 0,
        "DURATION unit/range contract is not exact",
    )
    expect(
        set(constructors.get("DURATION", {}).get("errors", []))
        == {
            "error.operator.operand",
            "error.numeric.unit_mismatch",
            "error.value.out_of_range",
        },
        "DURATION diagnostics do not preserve the required unit-category trigger",
    )
    expect(
        constructors.get("PERCENTAGE", {}).get("minimum") == 0
        and constructors.get("PERCENTAGE", {}).get("maximum") == 100,
        "PERCENTAGE bounds are not exact",
    )
    expect(constructors.get("BYTES", {}).get("minimum") == 0, "BYTES minimum is not zero")
    expect(
        "Any registered unit" in constructors.get("MEASURE", {}).get("unit_rule", "")
        and "Time-category units" in constructors.get("MEASURE", {}).get("unit_rule", "")
        and any(category == "Time" for category in units.values()),
        "MEASURE does not accept registered Time-category units",
    )

    expected_glob = {
        "style": "closed_lcl_glob_0_1_0",
        "workspace_relative": True,
        "path_separator": "/",
        "tokens": {
            "*": "zero_or_more_non_separator_characters",
            "**": "zero_or_more_complete_path_segments",
            "?": "one_non_separator_character",
            "[...]": "one_non_separator_character_from_class",
        },
        "absolute_patterns_allowed": False,
        "parent_escape_allowed": False,
        "brace_expansion": False,
        "match_semantics": "full_workspace_relative_path",
        "resource_limit_error": "error.pattern.resource_limit",
    }
    expected_regex = {
        "syntax": "closed_lcl_regex_0_1_0",
        "allowed_flags": ["i", "m", "s"],
        "canonical_flag_order": "ims",
        "default_flags": "",
        "duplicate_flags_allowed": False,
        "unknown_flags_allowed": False,
        "stateful_flags_allowed": False,
        "unicode_text_handling": "always_enabled_independently_of_user_flags",
        "user_unicode_flag_allowed": False,
        "forbidden_features": [
            "lookbehind", "unicode_property_escapes", "lookahead", "backreferences",
            "named_groups", "inline_flags", "conditional_groups", "reluctant_quantifiers",
            "possessive_quantifiers", "unlisted_escapes",
        ],
        "match_semantics": "full_string",
        "full_match_implementation": "semantic_boundary_check_not_multiline_anchors",
        "resource_limit_error": "error.pattern.resource_limit",
    }
    profiles = types.get("pattern_profiles", {})
    glob_profile = profiles.get("GLOB", {})
    regex_profile = profiles.get("REGEX", {})
    glob_extra = {"closed", "decoded_text", "segments", "literal", "escape",
                  "character_class", "matching", "input", "examples", "invalid_pattern_error"}
    regex_extra = {"closed", "decoded_text", "grammar", "escape", "character_class",
                   "quantifiers", "flags_semantics", "matching", "invalid_pattern_error"}
    expect(set(profiles) == {"GLOB", "REGEX"}, "pattern profile names are not closed")
    for name, profile, base, extra in (
        ("GLOB", glob_profile, expected_glob, glob_extra),
        ("REGEX", regex_profile, expected_regex, regex_extra),
    ):
        expect(isinstance(profile, dict) and set(profile) == set(base) | extra,
               f"{name} profile fields are not exact")
        expect(all(profile.get(key) == value for key, value in base.items()),
               f"{name} primitive flags, tokens, or matching boundary differ")
        expect(profile.get("closed") is True and profile.get("invalid_pattern_error")
               == "error.literal.invalid", f"{name} is not closed with a precise invalid-pattern error")
        expect("decodes the LCL STRING exactly once" in profile.get("decoded_text", ""),
               f"{name} does not define its single decoding boundary")
    expect(glob_profile.get("examples") == {
        "src/**/*.py matches src/main.py": True,
        "** matches empty relative path": True,
        "* matches .hidden": True,
        "a/* matches a/b/c": False,
    }, "GLOB zero-segment, hidden-name, or separator witness differs")
    semantic_tokens = {
        "GLOB": {
            "segments": ("one or more nonempty", "whole segment", "literal . or .."),
            "character_class": ("optional ! negation", "ascending scalar range", "excluding /"),
            "matching": ("zero or more complete nonempty segments", "complete input segment sequence"),
            "input": ("empty relative path", "no root is inferred", "error.operator.operand"),
        },
        "REGEX": {
            "character_class": ("optional ^ negation", "ascending scalar range", "class escapes cannot"),
            "quantifiers": ("n <= m", "without leading zeroes", "finite-concatenation", "assertion cannot"),
            "matching": ("entire input", "m flag never relaxes", "Captures and matched substrings are not exposed"),
        },
    }
    for name, clauses in semantic_tokens.items():
        for clause, tokens in clauses.items():
            value = profiles.get(name, {}).get(clause, "")
            expect(isinstance(value, str) and all(token in value for token in tokens),
                   f"{name}.{clause} omits a required closed semantic boundary")
    escapes = regex_profile.get("escape", {})
    expect(set(escapes) == {"literal", "control", "classes", "invalid"},
           "REGEX escape categories are not closed")
    expect("Every unlisted escape" in escapes.get("invalid", "") and
           "ASCII 0-9" in escapes.get("classes", "") and
           "U+0009 through U+000D and U+0020" in escapes.get("classes", ""),
           "REGEX escape classes or rejection boundary differ")
    flags = regex_profile.get("flags_semantics", {})
    expect(set(flags) == {"i", "m", "s"} and
           "ASCII case-insensitive" in flags.get("i", "") and
           "no locale or external Unicode" in flags.get("i", "") and
           "U+000A" in flags.get("m", "") and
           "No implicit pre-final-newline match" in flags.get("m", "") and
           "U+000A, U+000D, U+2028, and U+2029" in flags.get("s", ""),
           "REGEX flag semantics are not independent of locale and host regex dialect")
    grammar = regex_profile.get("grammar", {})
    expect(set(grammar) == {"notation", "start_symbol", "productions"}
           and grammar.get("start_symbol") == "REGEX_PATTERN",
           "REGEX grammar fields or start symbol differ")
    # Reuse the existing EBNF metagrammar checker; this does not parse LCL programs
    # or execute regular expressions and writes no temporary files.
    ebnf = runpy.run_path(str(Path(__file__).with_name("validate_ebnf.py")))
    productions = ebnf["Parser"](ebnf["tokenize"]("\n".join(grammar.get("productions", [])))).parse()
    expected_names = {"REGEX_PATTERN", "ALTERNATION", "CONCATENATION", "PIECE",
                      "ASSERTION", "ATOM", "QUANTIFIER", "COUNT", "REGEX_LITERAL",
                      "ESCAPE", "CHARACTER_CLASS", "DIGIT", "NONZERO_DIGIT"}
    expect(set(productions) == expected_names, "REGEX grammar production names are not exact")
    # Pin the reviewed grammar forms directly as well as checking their graph.
    # Merely having productive named rules must not admit an added regex dialect.
    expected_regex_productions = [
        'REGEX_PATTERN = ALTERNATION ;',
        'ALTERNATION = CONCATENATION , { "|" , CONCATENATION } ;',
        'CONCATENATION = { PIECE } ;',
        'PIECE = ASSERTION | ATOM , [ QUANTIFIER ] ;',
        'ASSERTION = "^" | "$" ;',
        'ATOM = REGEX_LITERAL | ESCAPE | "." | CHARACTER_CLASS | "(" , ALTERNATION , ")" | "(?:" , ALTERNATION , ")" ;',
        'QUANTIFIER = "*" | "+" | "?" | "{" , COUNT , "}" | "{" , COUNT , "," , [ COUNT ] , "}" ;',
        'COUNT = "0" | NONZERO_DIGIT , { DIGIT } ;',
        'REGEX_LITERAL = ? one Unicode scalar other than . ^ $ | U+003F * + ( ) [ ] { } or backslash ? ;',
        'ESCAPE = ? one complete escape admitted by the escape table ? ;',
        'CHARACTER_CLASS = ? one complete class admitted by character_class ? ;',
        'DIGIT = ? one ASCII digit 0 through 9 ? ;',
        'NONZERO_DIGIT = ? one ASCII digit 1 through 9 ? ;',
    ]
    expect(grammar.get("productions") == expected_regex_productions,
           "REGEX grammar forms widen or alter the frozen language")
    references = set().union(*(ebnf["references"](node) for node in productions.values()))
    expect(references <= set(productions), "REGEX grammar contains undefined nonterminals")
    reachable = {"REGEX_PATTERN"}; nullable = set(); productive = set()
    changed = True
    while changed:
        before = (len(reachable), len(nullable), len(productive))
        for name, node in productions.items():
            if name in reachable:
                reachable.update(ebnf["references"](node) & set(productions))
            if ebnf["is_nullable"](node, nullable): nullable.add(name)
            if ebnf["is_productive"](node, productive): productive.add(name)
        changed = before != (len(reachable), len(nullable), len(productive))
    expect(reachable == set(productions) and productive == set(productions),
           "REGEX grammar has unreachable or unproductive productions")
    expect(not ebnf["find_cycles"]({name: ebnf["leading_references"](node, nullable)
                                   & set(productions) for name, node in productions.items()}),
           "REGEX grammar is left recursive")
    expect(constructors.get("GLOB", {}).get("profile") == "types.pattern_profiles.GLOB", "GLOB constructor profile link is wrong")
    expect(constructors.get("REGEX", {}).get("profile") == "types.pattern_profiles.REGEX", "REGEX constructor profile link is wrong")
    expect(constructors.get("REGEX", {}).get("second_parameter") == "canonical flags string", "REGEX flags parameter is not declared")

    matches = operator_functions.get("operators", {}).get("MATCHES", {})
    expect(matches.get("arity") == 2, "MATCHES arity is not two")
    expect(
        matches.get("operands") == ["STRING,REGEX", "PATH|STRING,GLOB"],
        "MATCHES operand families are not exact",
    )
    expect(matches.get("result") == "BOOLEAN|UNKNOWN", "MATCHES result family is not exact")
    expect(matches.get("non_match_result") == "FALSE", "MATCHES non-match result is not FALSE")
    expect(matches.get("match_semantics") == "full_string", "MATCHES is not full-string")
    expect(
        matches.get("full_match_implementation") == "semantic_boundary_check_not_multiline_anchors",
        "MATCHES full-string behavior relies on anchors or is undefined",
    )
    expect(
        set(matches.get("errors", [])) == {"error.operator.operand", "error.pattern.resource_limit"},
        "MATCHES diagnostics are not exact",
    )

    expected_resource_error = {
        "meaning": (
            "Compiling or matching a GLOB or REGEX exhausts the implementation's declared finite "
            "pattern-resource limit."
        ),
        "stage": "static_or_expression",
        "recoverable_with_declared_handler": False,
        "event": None,
        "default_status": "status.invalid",
    }
    all_errors = statuses.get("errors", {})
    expect(
        all_errors.get("error.pattern.resource_limit") == expected_resource_error,
        "error.pattern.resource_limit metadata is not exact",
    )
    expect(len(error_group) == len(set(error_group)), "built-in error group contains duplicates")
    expect(set(error_group) == set(all_errors), "built-in error group does not close over registered errors")
    expect(
        "typed-constructor overload" in all_errors.get("error.operator.operand", {}).get("meaning", ""),
        "error.operator.operand does not cover typed constructors",
    )
    return violations


def division_contract_violations(
    root: Path,
    types: dict[str, Any],
    operator_functions: dict[str, Any],
    statuses: dict[str, Any],
    operations: dict[str, Any],
    keywords: dict[str, Any],
) -> list[str]:
    violations: list[str] = []

    def expect(condition: bool, message: str) -> None:
        if not condition:
            violations.append(message)

    expected_division = {
        "arity": 2,
        "overloads": [
            {"left": "INTEGER", "right": "INTEGER", "result": "DECIMAL"},
            {"left": "INTEGER", "right": "DECIMAL", "result": "DECIMAL"},
            {"left": "DECIMAL", "right": "INTEGER", "result": "DECIMAL"},
            {"left": "DECIMAL", "right": "DECIMAL", "result": "DECIMAL"},
            {
                "left": "MEASURE",
                "right": "INTEGER",
                "result": "MEASURE",
                "numeric_component": "DECIMAL",
                "unit": "preserve_left_exact_unit",
            },
            {
                "left": "MEASURE",
                "right": "DECIMAL",
                "result": "MEASURE",
                "numeric_component": "DECIMAL",
                "unit": "preserve_left_exact_unit",
            },
            {
                "left": "MEASURE",
                "right": "MEASURE",
                "constraint": "same_exact_unit",
                "result": "DECIMAL",
                "unit": "cancel_identical_units",
            },
        ],
        "exactness": {
            "evaluation": "exact_mathematical_quotient",
            "finite_decimal_rule": (
                "After reducing the quotient to lowest terms, the denominator has no "
                "prime factors other than 2 and 5."
            ),
            "implicit_rounding": False,
            "non_terminating_error": "error.numeric.non_terminating",
        },
        "rounding_context": (
            "A direct first argument of ROUND is evaluated as an exact quotient and "
            "rounded once by that function."
        ),
        "tolerance_role": (
            "TOLERANCE is an acceptance constraint and never selects or rounds a quotient."
        ),
        "zero_denominator_error": "error.numeric.division_by_zero",
        "unsupported_operand_error": "error.operator.operand",
        "different_measure_unit_error": "error.numeric.unit_mismatch",
        "declared_bound_error": "error.value.out_of_range",
        "host_capacity_error": "error.host.constraint",
        "overflow_behavior": "No wrap, saturation, underflow-to-zero, Infinity, or NaN.",
        "errors": [
            "error.operator.operand",
            "error.numeric.division_by_zero",
            "error.numeric.non_terminating",
            "error.numeric.unit_mismatch",
            "error.value.out_of_range",
            "error.host.constraint",
        ],
        "precedence": 60,
        "associativity": "left",
    }
    division = operator_functions.get("operators", {}).get("/")
    expect(division == expected_division, "division operator contract is not exact")

    expected_round = {
        "overloads": [
            {"parameters": ["DECIMAL", "INTEGER"], "result": "DECIMAL"},
            {
                "parameters": ["MEASURE", "INTEGER"],
                "result": "MEASURE",
                "unit": "preserve_exact_unit",
            },
        ],
        "digits": "nonnegative_fractional_digits",
        "rounding": "round-half-to-even",
        "quotient_context": (
            "The first argument may be an otherwise non-terminating direct division "
            "quotient; round the exact mathematical quotient once."
        ),
        "tolerance_role": "independent_acceptance_constraint_only",
        "errors": [
            "error.operator.operand",
            "error.value.out_of_range",
            "error.numeric.division_by_zero",
            "error.numeric.unit_mismatch",
            "error.host.constraint",
        ],
    }
    round_contract = operator_functions.get("functions", {}).get("ROUND")
    expect(round_contract == expected_round, "ROUND function contract is not exact")

    expected_numeric_profile = {
        "INTEGER": {
            "domain": "unbounded_signed_whole_number",
            "overflow_values": False,
        },
        "DECIMAL": {
            "domain": "exact_finite_base_10_decimal",
            "coefficient": "unbounded_integer",
            "fractional_digits": "finite_nonnegative_count",
            "special_values": [],
        },
        "division": {
            "contract": "operators_and_functions_v0.1.0.json#/operators/~1",
            "scalar_result": "DECIMAL",
            "implicit_rounding": False,
            "overflow_behavior": "forbidden",
            "underflow_to_zero": False,
            "infinity_or_nan": False,
            "host_capacity_error": "error.host.constraint",
        },
    }
    expect(
        types.get("numeric_profile") == expected_numeric_profile,
        "types.numeric_profile is not the exact division numeric profile",
    )

    expected_errors = {
        "error.operator.operand": {
            "meaning": (
                "No registered operator, function, or typed-constructor overload accepts "
                "the operand types or arity."
            ),
            "stage": "static_or_expression",
            "recoverable_with_declared_handler": False,
            "event": None,
            "default_status": "status.invalid",
        },
        "error.numeric.division_by_zero": {
            "meaning": (
                "A division denominator is mathematical zero, including a zero MEASURE "
                "numeric component and a quotient evaluated inside ROUND."
            ),
            "stage": "static_or_expression",
            "recoverable_with_declared_handler": False,
            "event": None,
            "default_status": "status.invalid",
        },
        "error.numeric.non_terminating": {
            "meaning": (
                "An exact division quotient has no finite base-10 DECIMAL representation "
                "outside the direct first-argument context of ROUND."
            ),
            "stage": "static_or_expression",
            "recoverable_with_declared_handler": False,
            "event": None,
            "default_status": "status.invalid",
        },
        "error.numeric.unit_mismatch": {
            "meaning": (
                "A value supplies a unit outside its required unit category, or an operation "
                "requiring identical MEASURE units receives different exact unit identifiers; "
                "sharing a unit category is insufficient for exact-unit equality."
            ),
            "stage": "static_or_expression",
            "recoverable_with_declared_handler": False,
            "event": None,
            "default_status": "status.invalid",
        },
        "error.value.out_of_range": {
            "meaning": "A value violates an exact bound.",
            "stage": "static_or_expression",
            "recoverable_with_declared_handler": False,
            "event": None,
            "default_status": "status.invalid",
        },
        "error.host.constraint": {
            "meaning": "A host/provider limitation outside portable LCL prevents execution.",
            "stage": "execution",
            "recoverable_with_declared_handler": True,
            "event": "event.host_constraint",
            "default_status": "status.blocked",
        },
    }
    for error_name, expected_error in expected_errors.items():
        expect(
            statuses.get("errors", {}).get(error_name) == expected_error,
            f"{error_name} metadata is not exact",
        )

    expected_division_calculate_errors = {
        "error.operator.operand",
        "error.numeric.division_by_zero",
        "error.numeric.non_terminating",
        "error.numeric.unit_mismatch",
        "error.value.out_of_range",
        "error.host.constraint",
    }
    calculate_errors = operations.get("core.calculate", {}).get("errors", [])
    expect(
        len(calculate_errors) == len(set(calculate_errors))
        and expected_division_calculate_errors <= set(calculate_errors),
        "core.calculate does not contain the complete division error subset",
    )

    expected_round_keyword = (
        "Round a DECIMAL or MEASURE to non-negative fractional digits using half-even "
        "rounding; MEASURE preserves its UNIT."
    )
    expect(
        keywords.get("ROUND", {}).get("meaning") == expected_round_keyword,
        "ROUND keyword meaning does not expose the accepted numeric families",
    )

    prose_requirements = {
        "03_TYPES_AND_VALUES/01_TYPE_SYSTEM_RULES.txt": [
            "static result of INTEGER/DECIMAL division is DECIMAL",
            "MEASURE divided by INTEGER or DECIMAL is\nMEASURE",
            "same exact UNIT is DECIMAL",
        ],
        "03_TYPES_AND_VALUES/02_BUILT_IN_TYPE_REFERENCE.txt": [
            "Unbounded signed whole number",
            "Exact finite base-10 decimal",
            "Infinity, NaN",
        ],
        "03_TYPES_AND_VALUES/06_NUMERIC_ARITHMETIC_COMPARISON_AND_ROUNDING.txt": [
            "INTEGER/INTEGER, INTEGER/DECIMAL",
            "no prime factors other than 2 and 5",
            "direct first argument is\na division expression",
            "TOLERANCE is an absolute\nacceptance constraint",
            "Numeric divided by MEASURE",
            "error.numeric.division_by_zero",
            "error.numeric.non_terminating",
            "error.host.constraint",
        ],
        "03_TYPES_AND_VALUES/07_FORMATS_ENCODINGS_UNITS_BOUNDS_AND_PATTERNS.txt": [
            "TOLERANCE is\nabsolute and non-negative",
            "validates a permitted numeric difference",
            "selects arithmetic precision or rounds a quotient",
        ],
        "05_SEMANTICS/12_OPERATOR_FUNCTION_AND_SPECIAL_VALUE_SEMANTICS.txt": [
            "every scalar division overload has static result type\nDECIMAL",
            "direct first argument of ROUND",
            "TOLERANCE does not establish a\nrounding context",
            "no overflow, wrap, saturation, Infinity",
        ],
        "06_STANDARD_LIBRARY/04_BUILT_IN_FUNCTIONS.txt": [
            "ROUND(decimal_or_measure, nonnegative_integer_fractional_digits)",
            "rounded once from its exact mathematical quotient",
        ],
        "02_LEXICAL/06_KEYWORD_REFERENCE_N_TO_Z.txt": [expected_round_keyword],
    }
    for relative_path, required_text in prose_requirements.items():
        prose = (root / relative_path).read_text(encoding="utf-8")
        missing = [token for token in required_text if token not in prose]
        expect(not missing, f"{relative_path} is missing division text: {missing}")
        if relative_path.endswith("12_OPERATOR_FUNCTION_AND_SPECIAL_VALUE_SEMANTICS.txt"):
            expect(
                "mapping is unresolved" not in prose,
                "division result mapping is still marked unresolved in semantic prose",
            )

    catalog = load_json_strict(
        root / "09_CONFORMANCE/CASES/core_conformance_cases_v0.1.0.json"
    )
    cases = {case.get("id"): case for case in catalog.get("cases", [])}
    case_requirements = {
        "KEYWORD-VALID-0217": (
            "keyword_valid",
            "ROUND",
            "keywords_v0.1.0.json",
            ["DECIMAL or MEASURE", "half-even"],
        ),
        "TYPE-VALID-0327": (
            "type_valid",
            "INTEGER",
            "types_v0.1.0.json",
            ["unbounded", "without overflow"],
        ),
        "TYPE-VALID-0329": (
            "type_valid",
            "DECIMAL",
            "types_v0.1.0.json",
            ["exact finite base-10", "without Infinity"],
        ),
        "FUNCTION-VALID-0500": (
            "function_valid",
            "ROUND",
            "operators_and_functions_v0.1.0.json",
            ["DECIMAL and MEASURE", "direct exact division quotient"],
        ),
        "FUNCTION-INVALID-0501": (
            "function_invalid",
            "ROUND",
            "operators_and_functions_v0.1.0.json",
            ["negative fractional digits", "zero division inside ROUND"],
        ),
        "OPERATOR-VALID-0518": (
            "operator_valid",
            "/",
            "operators_and_functions_v0.1.0.json",
            ["all four INTEGER/DECIMAL pairs", "same-exact-unit MEASURE"],
        ),
        "OPERATOR-INVALID-0519": (
            "operator_invalid",
            "/",
            "operators_and_functions_v0.1.0.json",
            ["mathematical-zero denominators", "non-terminating unrounded quotients"],
        ),
        "OPERATION-ERRORS-0561": (
            "operation_errors",
            "core.calculate",
            "operations_v0.1.0.json",
            ["division operand", "host-capacity"],
        ),
        "ERROR-CONTRACT-0721": (
            "error_contract",
            "error.numeric.division_by_zero",
            "statuses_and_errors_v0.1.0.json",
            ["mathematical-zero", "direct ROUND context"],
        ),
        "ERROR-CONTRACT-0722": (
            "error_contract",
            "error.numeric.non_terminating",
            "statuses_and_errors_v0.1.0.json",
            ["non-finite base-10", "outside direct ROUND context"],
        ),
        "ERROR-CONTRACT-0723": (
            "error_contract",
            "error.numeric.unit_mismatch",
            "statuses_and_errors_v0.1.0.json",
            [
                "outside a required unit category",
                "exact MEASURE unit-identifier mismatch",
            ],
        ),
    }
    for case_id, (category, subject, source, required_text) in case_requirements.items():
        case = cases.get(case_id)
        expect(case is not None, f"missing division conformance case {case_id}")
        if case is None:
            continue
        expect(
            (
                case.get("category") == category
                and case.get("subject") == subject
                and case.get("source") == source
            ),
            f"{case_id} identity metadata is not exact",
        )
        requirement = case.get("requirement", "")
        missing = [token for token in required_text if token not in requirement]
        expect(not missing, f"{case_id} is missing division evidence: {missing}")

    expected_examples = {
        "08_EXAMPLES/VALID/11_EXACT_DIVISION_AND_ROUNDING.lcl": """LCL:
    VERSION: \"0.1.0\"

SPECIFICATION:
    ID: example.exact_division
    NAME: \"Exact division and explicit rounding\"
    VERSION: \"1.0.0\"
    KIND: kind.data

DATA:
    ID: data.integral_decimal_quotient
    TYPE: DECIMAL
    VALUE: 4 / 2

DATA:
    ID: data.finite_decimal_quotient
    TYPE: DECIMAL
    VALUE: 1 / 8

DATA:
    ID: data.rounded_non_terminating_quotient
    TYPE: DECIMAL
    VALUE: ROUND(1 / 3, 2)

DATA:
    ID: data.scaled_distance
    TYPE: MEASURE
    VALUE: MEASURE(9, unit.meter) / 4

DATA:
    ID: data.dimensionless_ratio
    TYPE: DECIMAL
    VALUE: MEASURE(9, unit.meter) / MEASURE(4, unit.meter)

DATA:
    ID: data.rounded_distance
    TYPE: MEASURE
    VALUE: ROUND(MEASURE(1, unit.meter) / 3, 2)
""",
        "08_EXAMPLES/INVALID/18_NON_TERMINATING_DIVISION.invalid.lcl": """LCL:
    VERSION: \"0.1.0\"

SPECIFICATION:
    ID: invalid.non_terminating_division
    NAME: \"Non-terminating division requires direct ROUND\"
    VERSION: \"1.0.0\"
    KIND: kind.data

DATA:
    ID: data.invalid_quotient
    TYPE: DECIMAL
    VALUE: 1 / 3
""",
        "08_EXAMPLES/INVALID/18_NON_TERMINATING_DIVISION.invalid.lcl.expected.txt": (
            "EXPECTED_ERROR: error.numeric.non_terminating\n"
            "EXPECTED_TERMINAL_STATUS: status.invalid\n"
            "RULE: Reject a non-terminating quotient outside the direct first-argument "
            "context of ROUND.\n"
        ),
        "08_EXAMPLES/INVALID/19_DIVISION_BY_ZERO.invalid.lcl": """LCL:
    VERSION: \"0.1.0\"

SPECIFICATION:
    ID: invalid.division_by_zero
    NAME: \"ROUND does not rescue division by zero\"
    VERSION: \"1.0.0\"
    KIND: kind.data

DATA:
    ID: data.invalid_zero_divisor
    TYPE: DECIMAL
    VALUE: ROUND(1 / 0, 2)
""",
        "08_EXAMPLES/INVALID/19_DIVISION_BY_ZERO.invalid.lcl.expected.txt": (
            "EXPECTED_ERROR: error.numeric.division_by_zero\n"
            "EXPECTED_TERMINAL_STATUS: status.invalid\n"
            "RULE: A mathematical-zero denominator is invalid even in the direct "
            "first-argument context of ROUND.\n"
        ),
    }
    for relative_path, expected_text in expected_examples.items():
        path = root / relative_path
        expect(path.is_file(), f"missing division example {relative_path}")
        actual_text = path.read_text(encoding="utf-8") if path.is_file() else ""
        expect(actual_text == expected_text, f"{relative_path} is not the exact static fixture")

    index_text = (root / "INDEX.txt").read_text(encoding="utf-8")
    for relative_path in expected_examples:
        index_entry = f"    {relative_path.removeprefix('08_EXAMPLES/')}\n"
        expect(
            index_text.count(index_entry) == 1,
            f"INDEX.txt must list the division example exactly once: {relative_path}",
        )
    return violations


def set_sort_contract_violations(
    root: Path,
    types: dict[str, Any],
    operator_functions: dict[str, Any],
    formats_registry: dict[str, Any],
    semantic_meta: dict[str, Any],
    statuses: dict[str, Any],
    operations: dict[str, Any],
    results_schema: dict[str, Any],
    keywords: dict[str, Any],
) -> list[str]:
    violations: list[str] = []

    def expect(condition: bool, message: str) -> None:
        if not condition:
            violations.append(message)

    expected_set_profile = {
        "legal_member_requirement": "homogeneous T with strict equality",
        "intrinsic_order": "none",
        "source_or_insertion_order_has_semantic_meaning": False,
        "duplicate_equal_source_members": "collapse_to_one_member",
        "direct_for_each": {
            "legal_when": (
                "Every pair of actual members is mutually order-compatible under "
                "operators_and_functions_v0.1.0.json#/ordered_types."
            ),
            "order": "canonical_ascending",
            "otherwise": (
                "Pass the SET directly to core.sort and iterate the returned LIST[T]."
            ),
            "error": "error.type.mismatch",
        },
        "core_sort": {
            "accepts_set_directly": True,
            "preconversion_to_list_required": False,
            "result": "LIST[T]",
        },
    }
    expect(
        types.get("set_iteration_profile") == expected_set_profile,
        "types.set_iteration_profile is not the exact approved SET contract",
    )
    expected_set_type = (
        "Homogeneous collection unique under strict equality and intrinsically unordered; "
        "source or insertion order has no semantic meaning and equal duplicates collapse. "
        "Direct FOR EACH uses canonical ascending order only when actual members are mutually "
        "order-compatible; otherwise pass the SET directly to core.sort and iterate its "
        "LIST[T] result."
    )
    expect(
        types.get("types", {}).get("SET[T]") == expected_set_type,
        "types.SET[T] meaning is not exact",
    )

    expected_ordered_types = [
        "INTEGER",
        "DECIMAL",
        "STRING",
        "DATE",
        "TIME",
        "DATETIME",
        "DURATION",
        "PERCENTAGE",
        "BYTES",
        "MEASURE[same unit]",
    ]
    expected_ordered_rules = {
        "INTEGER": "mathematical numeric value",
        "DECIMAL": "mathematical numeric value",
        "STRING": "lexicographic by Unicode scalar value",
        "DATE": "chronological Gregorian value of the RFC 3339 full-date",
        "TIME": (
            "subtract the declared offset, or +00:00 when omitted, from the RFC 3339 "
            "partial-time on a common nominal local date; retain signed previous-day or "
            "next-day displacement without modulo-24-hour wrapping and compare the "
            "resulting chronological value"
        ),
        "DATETIME": (
            "exact UTC instant after applying the RFC 3339 declared offset, or +00:00 "
            "when omitted"
        ),
        "DURATION": (
            "normalized exact elapsed-time magnitude under "
            "formats_encodings_units_v0.1.0.json#/duration_normalization"
        ),
        "PERCENTAGE": "exact numeric magnitude",
        "BYTES": "exact byte-count magnitude",
        "MEASURE[same unit]": (
            "numeric component after requiring the same exact unit identifier"
        ),
    }
    for temporal_type in ("DATE", "TIME", "DATETIME"):
        expected_ordered_rules[temporal_type] += (
            "; exact validity and order keys are defined by "
            "types_v0.1.0.json#/temporal_literal_contract"
        )
    expect(
        operator_functions.get("ordered_types") == expected_ordered_types,
        "operator ordered_types membership changed or is incomplete",
    )
    expect(
        operator_functions.get("ordered_type_rules") == expected_ordered_rules,
        "operator ordered_type_rules are not exact",
    )
    expected_ordered_value_equality = (
        "Within one declared ordered type, equal canonical order keys are strict-equal "
        "semantic values; equal offset-normalized TIME or DATETIME values and equal "
        "normalized DURATION magnitudes therefore collapse as SET duplicates before "
        "ordering."
    )
    expect(
        operator_functions.get("ordered_value_equality")
        == expected_ordered_value_equality,
        "ordered-value equality does not eliminate unordered SET ties",
    )

    expected_duration_normalization = {
        "base_unit": "unit.nanosecond",
        "factor_interpretation": "exact count of base units per source unit",
        "factors": {
            "unit.nanosecond": 1,
            "unit.microsecond": 1000,
            "unit.millisecond": 1000000,
            "unit.second": 1000000000,
            "unit.minute": 60000000000,
            "unit.hour": 3600000000000,
            "unit.day": 86400000000000,
            "unit.week": 604800000000000,
        },
        "semantic_rule": (
            "Multiply the exact DURATION numeric component by its factor; the normalized "
            "exact magnitude defines DURATION equality, arithmetic, and order. This "
            "profile does not authorize implicit MEASURE conversion."
        ),
    }
    expect(
        formats_registry.get("duration_normalization")
        == expected_duration_normalization,
        "DURATION normalization factors or semantics are not exact",
    )
    actual_duration_normalization = formats_registry.get("duration_normalization", {})
    duration_factor_units = set(actual_duration_normalization.get("factors", {}))
    registered_time_units = {
        unit_name
        for unit_name, category in formats_registry.get("units", {}).items()
        if category == "Time"
    }
    expect(
        duration_factor_units == registered_time_units,
        "DURATION normalization factors do not cover exactly the registered Time units",
    )
    expect(
        actual_duration_normalization.get("base_unit") in registered_time_units
        and actual_duration_normalization.get("factors", {}).get(
            actual_duration_normalization.get("base_unit")
        )
        == 1,
        "DURATION normalization base is not a factor-one registered Time unit",
    )
    expect(
        operator_functions.get("constructors", {}).get("DURATION", {}).get("normalization")
        == "formats_encodings_units_v0.1.0.json#/duration_normalization",
        "DURATION constructor does not link to the normalization profile",
    )
    expect(
        operator_functions.get("cross_type_numeric_order")
        == (
            "INTEGER and DECIMAL compare by exact mathematical value after "
            "INTEGER-to-DECIMAL promotion."
        ),
        "cross-type numeric ordering is not exact",
    )
    expect(
        operator_functions.get("non_material_ordering")
        == "MISSING and UNKNOWN are not orderable values.",
        "MISSING/UNKNOWN ordering exclusion is not exact",
    )
    expect(
        operator_functions.get("string_order") == expected_ordered_rules["STRING"],
        "STRING order differs from the registered ordered-type rule",
    )

    expected_meta_ordered = (
        "A material-value domain whose actual members are mutually order-compatible under "
        "operators_and_functions_v0.1.0.json#/ordered_types and #/ordered_type_rules; "
        "MISSING and UNKNOWN are excluded."
    )
    expect(
        semantic_meta.get("meta_types", {}).get("meta.ordered") == expected_meta_ordered,
        "meta.ordered does not point to the authoritative ordered-type registry",
    )

    expected_sort_fields = {
        "meaning": (
            "Return LIST[T] in one declared deterministic total order from LIST[T] or SET[T]."
        ),
        "category": "read_only",
        "determinism": {
            "category": "derived",
            "source": (
                "Derived from operators_and_functions_v0.1.0.json#/ordered_types, "
                "#/ordered_type_rules, the registered property-access projection or "
                "validated key operation, original LIST source position for ties, and "
                "the distinct-key rule for SET members: every valid invocation resolves "
                "deterministic; an invocation that cannot satisfy those rules fails."
            ),
        },
        "possible_dependencies": ["declared_state_only"],
        "possible_effects": ["none"],
        "invocation_resolution": (
            "Resolve natural order, one property_path, or one validated deterministic "
            "side-effect-free key operation whose fully resolved dependency set is "
            "exactly declared_state_only. No invocation may add a dependency or effect."
        ),
        "target": {"type": "LIST[T]|SET[T]", "required": True},
        "parameters": {
            "key": {
                "type": "STRING|REFERENCE",
                "required": False,
                "default": None,
                "meaning": (
                    "Optional total-order projection; STRING is one exact property_path "
                    "and REFERENCE is one validated kind.operation extractor."
                ),
                "constraints": [
                    (
                        "Omission is legal only when every pair of target members is mutually "
                        "order-compatible under operators_and_functions_v0.1.0.json#/ordered_types."
                    ),
                    "A STRING is exactly one property_path defined for every target member.",
                    (
                        "A REFERENCE resolves to a kind.operation with SIDE_EFFECT FALSE, "
                        "DETERMINISTIC TRUE, a fully resolved dependency set of exactly "
                        "declared_state_only, exactly one PARAMETER accepting T, and exactly "
                        "one RESULT of a concrete registered ordered type."
                    ),
                    (
                        "Every key value is present, known, and mutually order-compatible; "
                        "distinct SET members produce distinct keys."
                    ),
                ],
            },
            "direction": {
                "type": "ENUM[ascending|descending]",
                "required": False,
                "default": "ascending",
                "meaning": "Sort direction.",
                "constraints": [
                    (
                        "descending reverses primary-key order without reversing original "
                        "LIST source order among equal-key members"
                    )
                ],
            },
        },
        "positional_parameters": False,
        "result_schema": "result.collection",
        "result_value_type": "LIST[T]",
        "preconditions": [
            "an omitted key requires mutually order-compatible target members",
            (
                "a declared key is defined for every member and produces present, known, "
                "mutually order-compatible values"
            ),
            "distinct SET members produce distinct keys",
        ],
        "postconditions": [
            "result.collection.items is LIST[T]",
            (
                "the result contains exactly the target members, preserving LIST "
                "multiplicity and including each SET member once"
            ),
            "ordering follows the natural value or declared key and direction",
            "equal-key LIST members retain original LIST source order",
        ],
        "errors": [
            "error.operation.parameter",
            "error.reference.unresolved",
            "error.reference.kind",
            "error.type.mismatch",
            "error.required.missing",
            "error.value.unknown",
            "error.operation.precondition",
        ],
        "diagnostic_triggers": {
            "error.operation.parameter": (
                "A missing required TARGET, or an unknown, duplicate, or positional "
                "parameter, including unregistered stable or comparator."
            ),
            "error.reference.unresolved": (
                "The key REFERENCE does not resolve exactly once."
            ),
            "error.reference.kind": (
                "The key REFERENCE resolves to a declaration other than kind.operation."
            ),
            "error.type.mismatch": "direction is outside ENUM[ascending|descending].",
            "error.required.missing": "A declared key value is MISSING for any member.",
            "error.value.unknown": "A declared key value is UNKNOWN for any member.",
            "error.operation.precondition": (
                "An omitted key lacks natural total order; a STRING key is not one "
                "well-formed property_path defined for every member; a key-operation "
                "signature is incompatible; key values are not mutually order-compatible; "
                "distinct SET members produce equal keys; or the key operation has invalid "
                "determinism, effects, dependencies, or a missing, ambiguous, incomplete, "
                "or out-of-bounds immutable profile."
            ),
        },
    }
    sort_contract = operations.get("core.sort", {})
    for field, expected_value in expected_sort_fields.items():
        expect(
            sort_contract.get(field) == expected_value,
            f"core.sort.{field} is not the exact approved contract",
        )
    expect(
        "stable" not in sort_contract.get("parameters", {})
        and "comparator" not in sort_contract.get("parameters", {}),
        "core.sort exposes forbidden stable or comparator parameters",
    )

    collection_items = (
        results_schema.get("result.collection", {}).get("fields", {}).get("items")
    )
    expect(
        collection_items == {"type": "LIST[T]", "cardinality": "zero_or_one"},
        "result.collection.items does not preserve the required LIST[T] dependency",
    )

    expected_error_fields = {
        "error.operation.parameter": {
            "meaning": (
                "An ACTION, HANDLER, or FALLBACK invocation site omits TARGET when the "
                "selected operation marks it required and no handler-context binding "
                "supplies it, omits a named parameter that the operation marks required, "
                "supplies any positional argument, duplicates a named parameter, or supplies "
                "an unregistered named parameter. A FALLBACK operation identifier that "
                "requires any named parameter, or whose required TARGET no handler-context "
                "binding can supply, uses this error. A declared parameter value rejected by "
                "its type or semantic constraint uses the general or row-specific error and "
                "is not remapped to error.operation.parameter."
            ),
            "stage": "static_or_expression",
            "recoverable_with_declared_handler": False,
            "event": None,
            "default_status": "status.invalid",
        },
        "error.reference.unresolved": {
            "meaning": ("REF or an error/event/status alias BASE does not resolve to exactly "
                        "one declaration, binding, or permitted canonical core identifier."),
            "stage": "resolution",
            "recoverable_with_declared_handler": False,
            "event": None,
            "default_status": "status.invalid",
        },
        "error.reference.kind": {
            "meaning": ("A reference resolves to a declaration kind illegal in that field, an "
                        "error/event/status alias BASE resolves outside its own domain, or a "
                        "handler-context ACTION target binds a producer that is not an ACTION."),
            "stage": "resolution",
            "recoverable_with_declared_handler": False,
            "event": None,
            "default_status": "status.invalid",
        },
        "error.required.missing": {
            "meaning": (
                "A required value, source, output, evidence item, or declaration is MISSING."
            ),
            "stage": "execution",
            "recoverable_with_declared_handler": True,
            "event": "event.missing",
            "default_status": "status.blocked",
        },
        "error.value.unknown": {
            "meaning": ("A required value remains UNKNOWN after applicable explicit value, source, "
                        "DEFAULT, and ASSUME resolution. Emit this diagnostic before selecting "
                        "any eligible handler; successful handler recovery retains it as evidence."),
            "stage": "static_or_expression",
            "recoverable_with_declared_handler": True,
            "event": "event.unknown",
            "default_status": "status.blocked",
        },
        "error.operation.precondition": {
            "meaning": (
                "A registered operation precondition, including exact resolution of every "
                "required profile role, is false, missing, or unknown."
            ),
            "stage": "execution",
            "recoverable_with_declared_handler": False,
            "event": None,
            "default_status": "status.failed",
        },
        "error.type.mismatch": {
            "meaning": (
                "A value is incompatible with its declared type or with the type or "
                "registered order domain required by its use context."
            ),
            "stage": "static_or_expression",
            "recoverable_with_declared_handler": False,
            "event": None,
            "default_status": "status.invalid",
        },
    }
    for error_name, expected_fields in expected_error_fields.items():
        actual = statuses.get("errors", {}).get(error_name, {})
        expect(
            all(actual.get(field) == value for field, value in expected_fields.items()),
            f"{error_name} does not preserve the required SET/sort trigger metadata",
        )

    expected_set_keyword = (
        "An intrinsically unordered collection unique under strict equality using bracket "
        "value syntax; source or insertion order has no semantic meaning and equal "
        "duplicates collapse. Direct FOR EACH uses canonical ascending order only when "
        "actual members are mutually order-compatible; otherwise pass the SET directly "
        "to core.sort and iterate its LIST result."
    )
    expect(
        keywords.get("SET", {}).get("meaning") == expected_set_keyword,
        "SET keyword meaning is not exact",
    )

    prose_requirements = {
        "02_LEXICAL/06_KEYWORD_REFERENCE_N_TO_Z.txt": [expected_set_keyword],
        "02_LEXICAL/08_LIST_OBJECT_AND_DATA_LEXICAL_FORM.txt": [
            "equal members collapse under strict equality",
            "insertion order has no semantic meaning",
        ],
        "03_TYPES_AND_VALUES/01_TYPE_SYSTEM_RULES.txt": [
            "An unordered SET remains a legal value",
            "error.type.mismatch before that iteration begins or produces effects",
            "may instead produce the LIST required for iteration",
        ],
        "03_TYPES_AND_VALUES/02_BUILT_IN_TYPE_REFERENCE.txt": [
            "intrinsically",
            "registered total-order",
            "returned LIST[T]",
        ],
        "03_TYPES_AND_VALUES/03_COLLECTIONS_OBJECTS_ENUMS_AND_EQUALITY.txt": [
            "SET is intrinsically unordered",
            "equal source members collapse under strict equality",
            "equal canonical order",
            "keys within one declared ordered type",
            "DURATION equality",
        ],
        "03_TYPES_AND_VALUES/07_FORMATS_ENCODINGS_UNITS_BOUNDS_AND_PATTERNS.txt": [
            "DURATION has one exact normalized elapsed-time magnitude",
            "604,800,000,000,000 per week",
            "does not authorize implicit conversion between MEASURE values",
        ],
        "03_TYPES_AND_VALUES/10_COLLECTION_OBJECT_ENUM_AND_SCHEMA_FORMS.txt": [
            "source or insertion",
            "returned LIST[T]",
            "declaration order has no comparison, iteration, or sorting significance",
            "approved key extractor",
        ],
        "04_GRAMMAR/04_CONDITIONS_BRANCHES_AND_BOUNDED_ITERATION.txt": [
            "FOR EACH has no key or comparator syntax",
            "returned LIST iterated",
        ],
        "05_SEMANTICS/08_PHASE_SEQUENCE_STEP_BRANCH_LOOP_RETRY_AND_CONCURRENCY.txt": [
            "SET is intrinsically unordered",
            "iteration produces error.type.mismatch before that iteration or any of its",
            "iterate its returned LIST",
        ],
        "05_SEMANTICS/12_OPERATOR_FUNCTION_AND_SPECIAL_VALUE_SEMANTICS.txt": [
            "sole natural-order source for SET",
            "Unicode-scalar lexicographic order",
            "without modulo-24-hour wrapping",
            "formats_encodings_units_v0.1.0.json#/duration_normalization",
            "equal canonical order keys are strict-equal values",
            "No locale, insertion",
            "host collation, or inferred ENUM order",
        ],
        "06_STANDARD_LIBRARY/01_READ_ONLY_AND_ANALYTICAL_OPERATIONS.txt": [
            "Accept LIST[T] or SET[T] directly",
            "No comparator or stable parameter exists",
            "Determinism category: derived",
            "Determinism source: Derived from operators_and_functions_v0.1.0.json#/ordered_types",
            "registered property-access expression for a STRING key",
        ],
        "06_STANDARD_LIBRARY/10_CORE_OPERATION_PARAMETER_RULES.txt": [
            "core.sort has exactly key and direction",
            "error.operation.parameter",
            "equal keys for distinct SET members",
            "result.collection items are LIST[T]",
        ],
    }
    for relative_path, required_text in prose_requirements.items():
        prose = " ".join((root / relative_path).read_text(encoding="utf-8").split())
        missing = [token for token in required_text if " ".join(token.split()) not in prose]
        expect(not missing, f"{relative_path} is missing SET/sort text: {missing}")

    # Candidate/release claims are checked independently by check_release_state.
    # They cannot change the truth of the SET and sorting language contract.

    grammar = (root / "04_GRAMMAR/10_COMPLETE_EBNF.ebnf").read_text(encoding="utf-8")
    for_each_segment = grammar[
        grammar.index("FOR_EACH =") : grammar.index("EXECUTABLE_BODY =")
    ].strip()
    expected_for_each = (
        'FOR_EACH = "FOR", SPACE, "EACH", SPACE, SIMPLE_IDENTIFIER, SPACE, "IN", SPACE,\n'
        '    EXPRESSION, ":", NEWLINE, INDENT, EXECUTABLE_BODY, DEDENT ;'
    )
    expect(
        for_each_segment == expected_for_each,
        "FOR_EACH EBNF gained unapproved key/comparator/sort syntax",
    )

    catalog = load_json_strict(
        root / "09_CONFORMANCE/CASES/core_conformance_cases_v0.1.0.json"
    )
    cases = {case.get("id"): case for case in catalog.get("cases", [])}
    case_requirements = {
        "KEYWORD-VALID-0225": (
            "keyword_valid",
            "SET",
            "keywords_v0.1.0.json",
            ["intrinsically unordered", "equal duplicates collapse", "conditional direct iteration"],
        ),
        "TYPE-VALID-0337": (
            "type_valid",
            "SET[T]",
            "types_v0.1.0.json",
            [
                "any strict-equality T",
                "collapse equal duplicates",
                "canonical ascending",
                "TIME day displacement",
                "DURATION normalization",
            ],
        ),
        "TYPE-INVALID-0338": (
            "type_invalid",
            "SET[T]",
            "types_v0.1.0.json",
            ["direct FOR EACH", "without rejecting the SET value itself"],
        ),
        "OPERATION-BINDING-0571": (
            "operation_binding",
            "core.sort",
            "operations_v0.1.0.json",
            ["LIST[T] or SET[T] directly", "reject stable, comparator"],
        ),
        "OPERATION-EFFECTS-0572": (
            "operation_effects",
            "core.sort",
            "operations_v0.1.0.json",
            [
                "Derive determinism",
                "ordered-type membership and rules",
                "property-access projection",
                "Preserve LIST source position",
                "require distinct keys",
            ],
        ),
        "OPERATION-ERRORS-0573": (
            "operation_errors",
            "core.sort",
            "operations_v0.1.0.json",
            [
                "stable and comparator are unregistered",
                "error.type.mismatch",
                "malformed property_path",
                "MISSING or UNKNOWN",
                "equal keys for distinct SET",
            ],
        ),
        "ERROR-CONTRACT-0725": (
            "error_contract",
            "error.operation.parameter",
            "statuses_and_errors_v0.1.0.json",
            ["unregistered named", "core.sort stable or comparator"],
        ),
        "ERROR-CONTRACT-0747": (
            "error_contract",
            "error.type.mismatch",
            "statuses_and_errors_v0.1.0.json",
            [
                "context-required registered order domain",
                "direct FOR EACH",
                "lack registered total order",
            ],
        ),
        "RESULT-SCHEMAS-0775": (
            "result_schemas",
            "result.collection",
            "built_in_groups_and_results_v0.1.0.json",
            [
                "items LIST[T]",
                "count equal to the actual item count",
                "empty list with count 0",
                "items as the default OUTPUT projection",
            ],
        ),
    }
    for case_id, (category, subject, source, required_text) in case_requirements.items():
        case = cases.get(case_id)
        expect(case is not None, f"missing SET/sort conformance case {case_id}")
        if case is None:
            continue
        expect(
            (
                case.get("category") == category
                and case.get("subject") == subject
                and case.get("source") == source
            ),
            f"{case_id} identity metadata is not exact",
        )
        requirement = case.get("requirement", "")
        missing = [token for token in required_text if token not in requirement]
        expect(not missing, f"{case_id} is missing SET/sort evidence: {missing}")

    expected_example_hashes = {
        "08_EXAMPLES/VALID/12_SET_SORTING.lcl": (
            "cb83518d64624c1c062c5a078c8ffe1d2c853740708c4afe0bf22bded7476e78"
        ),
        "08_EXAMPLES/INVALID/20_UNORDERED_SET_DIRECT_ITERATION.invalid.lcl": (
            "584b7132859896748958647f11e6777459512469e92b50cbd3044d27bfb93949"
        ),
        "08_EXAMPLES/INVALID/20_UNORDERED_SET_DIRECT_ITERATION.invalid.lcl.expected.txt": (
            "29729b07d895640e6ea5e084368831c5822993f24b327dde60875dc5bd7eef68"
        ),
        "08_EXAMPLES/INVALID/21_SORT_STABLE_PARAMETER.invalid.lcl": (
            "dd2fa6d9953c6a6b9e79032ff684dda6d214fdfa2fd321e426fe61e5ee43646c"
        ),
        "08_EXAMPLES/INVALID/21_SORT_STABLE_PARAMETER.invalid.lcl.expected.txt": (
            "291763b7449d220675ea8b28920fd43c711dd0d4266cec1696e25aacf289b07e"
        ),
    }
    example_requirements = {
        "08_EXAMPLES/VALID/12_SET_SORTING.lcl": [
            "TYPE: SET[INTEGER]",
            "VALUE: [3, 1, 2, 2]",
            "TYPE: LIST[INTEGER]",
            "SIDE_EFFECT: FALSE",
            "DETERMINISTIC: TRUE",
            "VALUE: REF(sort.identity_key)",
        ],
        "08_EXAMPLES/INVALID/20_UNORDERED_SET_DIRECT_ITERATION.invalid.lcl": [
            "TYPE: SET[BOOLEAN]",
            "FOR EACH flag IN REF(input.flags):",
        ],
        "08_EXAMPLES/INVALID/20_UNORDERED_SET_DIRECT_ITERATION.invalid.lcl.expected.txt": [
            "EXPECTED_ERROR: error.type.mismatch",
            "EXPECTED_TERMINAL_STATUS: status.invalid",
            "returned LIST instead",
        ],
        "08_EXAMPLES/INVALID/21_SORT_STABLE_PARAMETER.invalid.lcl": [
            "OPERATION: core.sort",
            "NAME: stable",
        ],
        "08_EXAMPLES/INVALID/21_SORT_STABLE_PARAMETER.invalid.lcl.expected.txt": [
            "EXPECTED_ERROR: error.operation.parameter",
            "EXPECTED_TERMINAL_STATUS: status.invalid",
            "stable and comparator are unregistered",
        ],
    }
    for relative_path, expected_hash in expected_example_hashes.items():
        path = root / relative_path
        expect(path.is_file(), f"missing SET/sort example {relative_path}")
        actual_hash = sha256(path) if path.is_file() else ""
        expect(actual_hash == expected_hash, f"{relative_path} is not the exact static fixture")
        example = path.read_text(encoding="utf-8") if path.is_file() else ""
        missing = [
            token for token in example_requirements[relative_path] if token not in example
        ]
        expect(not missing, f"{relative_path} is missing SET/sort evidence: {missing}")

    index_text = (root / "INDEX.txt").read_text(encoding="utf-8")
    for relative_path in expected_example_hashes:
        index_entry = f"    {relative_path.removeprefix('08_EXAMPLES/')}\n"
        expect(
            index_text.count(index_entry) == 1,
            f"INDEX.txt must list the SET/sort example exactly once: {relative_path}",
        )
    return violations


EXPECTED_RESULT_CONTRACT_FINGERPRINT = (
    "034d7c5954f35a71b8f5c7f72271ee786e07d80f4b18ca97c950d05bc6aca352"
)
EXPECTED_RESULT_STATUS_FINGERPRINT = (
    "3cbfb6255538c404f89974caaa541e30d11880c288f745c0d71fe0edd5948bb7"
)
EXPECTED_RESULT_SCHEMA_FINGERPRINTS = {
    "result.value": "65fb266615cff90c8dbbcf1fb09f7de165277b3809e1ed314b5359ad673be652",
    "result.collection": "a5bd7a583beb10627739febe06454c559e3c47e1cb42772a68d340fb46d3332c",
    "result.operation": "6b2f64882d279e4d6ea7bb5c5c90c40c8d00a7af6a3e7c3ef6781d45d0c73c20",
    "result.command": "2b0b131e675295a996ccfb282197a555a782f1513c2b7849a67ed78739e75ae0",
    "result.validation": "4ee378cdb9f5932f190066d04b1bb1678cb9848ccb257fe8be175c8267dcf3c8",
    "result.verification": "62150b28f17e2893b68f6f355a475e1a16fbae44dabfb84a2f00c8fe90f299ab",
    "result.test": "09b60f093a049a8d023ebb228d3f2824d82daa62ff78b95499b6f63a78d0ae28",
    "result.message": "7d0d24c2087aee8517614699bec6c3db7dfeaeade90a4a537e00367a3373ba76",
    "result.transfer": "ad2e02fe946ec4ca56e3206dc5f63416e7d3823e0fbec0d3643cbba68da07e35",
}


def canonical_contract_fingerprint(value: Any) -> str:
    encoded = json.dumps(
        value,
        ensure_ascii=False,
        separators=(",", ":"),
        sort_keys=True,
    ).encode("utf-8")
    return hashlib.sha256(encoded).hexdigest()


def result_contract_violations(
    root: Path,
    groups_and_results: dict[str, Any],
    statuses: dict[str, Any],
    operations: dict[str, Any],
    blocks: dict[str, Any],
    fields: dict[str, Any],
) -> list[str]:
    violations: list[str] = []

    def expect(condition: bool, message: str) -> None:
        if not condition:
            violations.append(message)

    expect(
        set(groups_and_results)
        == {
            "language",
            "version",
            "closed",
            "contract_type_notation_source",
            "enum_groups",
            "result_contract",
            "result_schemas",
            "reserved_namespaces",
            "core_operation_ids",
        },
        "groups/result registry root fields are not exact",
    )
    result_contract_value = groups_and_results.get("result_contract")
    result_contract = result_contract_value if isinstance(result_contract_value, dict) else {}
    expect(
        set(result_contract)
        == {
            "effective_fields",
            "common_fields",
            "effect_schemas",
            "output_projection",
            "invariants",
        },
        "result_contract fields are not exact",
    )

    expected_common_fields = {
        "status": {
            "type": "qualified_identifier(status)",
            "cardinality": "exactly_one",
        },
        "output_binding": {
            "type": "ENUM[not_requested|unbound|bound|partial]",
            "cardinality": "exactly_one",
        },
        "execution_errors": {
            "type": "LIST[qualified_identifier(error)]",
            "cardinality": "exactly_one",
        },
        "failure_phase": {
            "type": "ENUM[none|pre_effect|post_effect|indeterminate]",
            "cardinality": "exactly_one",
        },
        "effect_state": {
            "type": "ENUM[none|applied|partial|indeterminate]",
            "cardinality": "exactly_one",
        },
        "observed_effects": {
            "type": "LIST[result.effect]",
            "cardinality": "exactly_one",
        },
    }
    common_fields = result_contract.get("common_fields")
    expect(
        common_fields == expected_common_fields,
        "common result fields, types, or cardinalities are not exact",
    )

    effect_schemas = result_contract.get("effect_schemas")
    expect(
        isinstance(effect_schemas, dict) and set(effect_schemas) == {"result.effect"},
        "result.effect is not the one exact auxiliary effect schema",
    )
    effect_schema_value = (
        effect_schemas.get("result.effect") if isinstance(effect_schemas, dict) else None
    )
    effect_schema = effect_schema_value if isinstance(effect_schema_value, dict) else {}
    expected_effect_fields = {
        "class": {
            "type": "ENUM[filesystem|network|process|package|message|memory|state]",
            "cardinality": "exactly_one",
        },
        "state": {
            "type": "ENUM[applied|partial|indeterminate]",
            "cardinality": "exactly_one",
        },
        "target": {"type": "target_expression", "cardinality": "zero_or_one"},
        "evidence": {
            "type": "LIST[REFERENCE[EVIDENCE]]",
            "cardinality": "exactly_one",
        },
    }
    expect(
        set(effect_schema) == {"closed", "fields", "constraints"}
        and effect_schema.get("closed") is True
        and effect_schema.get("fields") == expected_effect_fields
        and isinstance(effect_schema.get("constraints"), list)
        and bool(effect_schema.get("constraints")),
        "result.effect is not the exact closed effect-record shape",
    )

    expected_fields = {
        "result.value": {
            "value": {"type": "meta.material_value", "cardinality": "zero_or_one"},
            "evidence": {
                "type": "LIST[REFERENCE[EVIDENCE]]",
                "cardinality": "exactly_one",
            },
        },
        "result.collection": {
            "items": {"type": "LIST[T]", "cardinality": "zero_or_one"},
            "count": {"type": "INTEGER", "cardinality": "zero_or_one"},
        },
        "result.operation": {
            "changed": {"type": "BOOLEAN|UNKNOWN", "cardinality": "exactly_one"},
            "target": {"type": "target_expression", "cardinality": "exactly_one"},
            "value": {"type": "meta.material_value", "cardinality": "zero_or_one"},
        },
        "result.command": {
            "mode": {"type": "ENUM[non_graph|graph]", "cardinality": "exactly_one"},
            "started": {"type": "BOOLEAN", "cardinality": "zero_or_one"},
            "completed": {"type": "BOOLEAN", "cardinality": "zero_or_one"},
            "exit_code": {"type": "INTEGER", "cardinality": "zero_or_one"},
            "stdout": {"type": "STRING", "cardinality": "zero_or_one"},
            "stderr": {"type": "STRING", "cardinality": "zero_or_one"},
            "value": {"type": "meta.material_value", "cardinality": "zero_or_one"},
        },
        "result.validation": {
            "valid": {"type": "BOOLEAN", "cardinality": "zero_or_one"},
            "errors": {
                "type": "LIST[qualified_identifier(error)]",
                "cardinality": "exactly_one",
            },
        },
        "result.verification": {
            "verified": {"type": "BOOLEAN|UNKNOWN", "cardinality": "zero_or_one"},
            "observed": {"type": "OBJECT", "cardinality": "zero_or_one"},
            "errors": {
                "type": "LIST[qualified_identifier(error)]",
                "cardinality": "exactly_one",
            },
            "evidence": {
                "type": "LIST[REFERENCE[EVIDENCE]]",
                "cardinality": "exactly_one",
            },
        },
        "result.test": {
            "passed": {"type": "BOOLEAN|UNKNOWN", "cardinality": "zero_or_one"},
            "expected": {"type": "meta.material_value", "cardinality": "zero_or_one"},
            "actual": {"type": "meta.material_value", "cardinality": "zero_or_one"},
            "evidence": {
                "type": "LIST[REFERENCE[EVIDENCE]]",
                "cardinality": "exactly_one",
            },
        },
        "result.message": {
            "delivered": {"type": "BOOLEAN|UNKNOWN", "cardinality": "exactly_one"},
            "recipient": {"type": "target_expression", "cardinality": "exactly_one"},
            "message_id": {"type": "STRING|NULL", "cardinality": "exactly_one"},
        },
        "result.transfer": {
            "source": {"type": "target_expression", "cardinality": "exactly_one"},
            "destination": {"type": "target_expression", "cardinality": "exactly_one"},
            "bytes": {"type": "BYTES|UNKNOWN", "cardinality": "zero_or_one"},
            "checksum": {"type": "STRING|NULL", "cardinality": "zero_or_one"},
            "value": {"type": "meta.material_value", "cardinality": "zero_or_one"},
        },
    }
    expected_domains = {
        "result.value": ["value"],
        "result.collection": ["items", "count"],
        "result.operation": ["changed"],
        "result.command": ["started", "completed", "exit_code", "value"],
        "result.validation": ["valid"],
        "result.verification": ["verified"],
        "result.test": ["passed"],
        "result.message": ["delivered"],
        "result.transfer": ["bytes"],
    }
    expected_unknown = {
        "result.value": [],
        "result.collection": [],
        "result.operation": ["changed"],
        "result.command": [],
        "result.validation": [],
        "result.verification": ["verified"],
        "result.test": ["passed"],
        "result.message": ["delivered"],
        "result.transfer": ["bytes"],
    }
    expected_defaults = {
        "result.value": "value",
        "result.collection": "items",
        "result.operation": "changed",
        "result.command": "stdout",
        "result.validation": "valid",
        "result.verification": "verified",
        "result.test": "passed",
        "result.message": "delivered",
        "result.transfer": "bytes",
    }
    expected_projectable = {
        "result.value": ["value", "evidence"],
        "result.collection": ["items", "count"],
        "result.operation": ["changed", "target", "value"],
        "result.command": [
            "mode",
            "started",
            "completed",
            "exit_code",
            "stdout",
            "stderr",
            "value",
        ],
        "result.validation": ["valid", "errors"],
        "result.verification": ["verified", "observed", "errors", "evidence"],
        "result.test": ["passed", "expected", "actual", "evidence"],
        "result.message": ["delivered", "recipient", "message_id"],
        "result.transfer": ["source", "destination", "bytes", "checksum", "value"],
    }
    schemas_value = groups_and_results.get("result_schemas")
    schemas = schemas_value if isinstance(schemas_value, dict) else {}
    expect(
        list(schemas) == list(expected_fields),
        "result schema membership or canonical order is not exact",
    )
    schema_keys = {
        "common_fields",
        "unknown_fields",
        "field_order",
        "domain_outcome",
        "primary_output",
        "partial_output",
        "fields",
        "constraints",
    }
    common_names = set(expected_common_fields)
    cardinalities = set(groups_and_results.get("enum_groups", {}).get("cardinalities", []))
    typed_unknown_fields: set[tuple[str, str]] = set()
    for name, expected_local_fields in expected_fields.items():
        schema_value = schemas.get(name)
        schema = schema_value if isinstance(schema_value, dict) else {}
        expect(set(schema) == schema_keys, f"{name} schema fields are not exact")
        expect(schema.get("common_fields") is True, f"{name} does not inherit common fields")
        expect(schema.get("unknown_fields") == "forbidden", f"{name} is not closed")
        expect(schema.get("field_order") == "not_semantic", f"{name} field order is not exact")
        local_fields = schema.get("fields")
        expect(local_fields == expected_local_fields, f"{name} local fields are not exact")
        if isinstance(local_fields, dict):
            expect(
                not (set(local_fields) & common_names),
                f"{name} duplicates an inherited common field",
            )
            for field_name, specification in local_fields.items():
                expect(
                    isinstance(specification, dict)
                    and set(specification) == {"type", "cardinality"},
                    f"{name}.{field_name} field specification is not exact",
                )
                if isinstance(specification, dict):
                    expect(
                        specification.get("cardinality") in cardinalities,
                        f"{name}.{field_name} uses an unregistered cardinality",
                    )
                    if "UNKNOWN" in str(specification.get("type", "")):
                        typed_unknown_fields.add((name, field_name))
        domain_outcome = schema.get("domain_outcome")
        expect(
            domain_outcome
            == {
                "fields": expected_domains[name],
                "unknown_fields": expected_unknown[name],
            },
            f"{name} domain-outcome contract is not exact",
        )
        primary_output = schema.get("primary_output")
        expect(
            primary_output
            == {
                "default_property": expected_defaults[name],
                "projectable_fields": expected_projectable[name],
            },
            f"{name} primary OUTPUT contract is not exact",
        )
        partial_output = schema.get("partial_output")
        expected_partial = (
            {"supported": True, "fields": ["stdout", "stderr"]}
            if name == "result.command"
            else {"supported": False, "fields": []}
        )
        expect(partial_output == expected_partial, f"{name} partial OUTPUT contract is not exact")
        constraints = schema.get("constraints")
        expect(
            isinstance(constraints, list)
            and bool(constraints)
            and all(isinstance(item, str) and bool(item.strip()) for item in constraints),
            f"{name} constraints are not a non-empty string list",
        )
        expect(
            canonical_contract_fingerprint(schema)
            == EXPECTED_RESULT_SCHEMA_FINGERPRINTS[name],
            f"{name} differs from the approved result contract",
        )

    expected_unknown_set = {
        ("result.operation", "changed"),
        ("result.verification", "verified"),
        ("result.test", "passed"),
        ("result.message", "delivered"),
        ("result.transfer", "bytes"),
    }
    expect(
        typed_unknown_fields == expected_unknown_set,
        f"result UNKNOWN field set differs: {sorted(typed_unknown_fields)}",
    )
    contract_fingerprint = canonical_contract_fingerprint(
        {"result_contract": result_contract, "result_schemas": schemas}
    )
    expect(
        contract_fingerprint == EXPECTED_RESULT_CONTRACT_FINGERPRINT,
        f"combined result contract differs: {contract_fingerprint}",
    )

    status_records_value = statuses.get("statuses")
    status_records = status_records_value if isinstance(status_records_value, dict) else {}
    expect(len(status_records) == 12, "status registry does not contain exactly 12 statuses")
    for name, record_value in status_records.items():
        record = record_value if isinstance(record_value, dict) else {}
        expect(
            set(record) == {"meaning", "result_meaning", "terminal", "allowed_next", "scope"},
            f"{name} does not have the exact result-aware status shape",
        )
        expect(
            isinstance(record.get("result_meaning"), str)
            and bool(record.get("result_meaning", "").strip())
            and "result" in str(record.get("scope", "")),
            f"{name} lacks an explicit result meaning or scope",
        )
    status_fingerprint = canonical_contract_fingerprint(status_records)
    expect(
        status_fingerprint == EXPECTED_RESULT_STATUS_FINGERPRINT,
        f"result-aware status contract differs: {status_fingerprint}",
    )
    expect(
        "FALSE domain outcome" in status_records.get("status.succeeded", {}).get("result_meaning", "")
        and "neither implies partial OUTPUT binding"
        in status_records.get("status.partial", {}).get("result_meaning", "")
        and "pre-effect failure to start"
        in status_records.get("status.failed", {}).get("result_meaning", ""),
        "succeeded, partial, or failed result-status distinction is not exact",
    )

    expected_mapping_counts = {
        "result.value": 9,
        "result.collection": 3,
        "result.operation": 18,
        "result.command": 1,
        "result.validation": 1,
        "result.verification": 1,
        "result.test": 1,
        "result.message": 1,
        "result.transfer": 4,
    }
    actual_mapping_counts = {
        name: sum(
            1
            for operation in operations.values()
            if isinstance(operation, dict) and operation.get("result_schema") == name
        )
        for name in expected_mapping_counts
    }
    expect(
        len(operations) == 39 and actual_mapping_counts == expected_mapping_counts,
        f"operation-to-result mapping differs: {actual_mapping_counts}",
    )
    expect(
        all(
            isinstance(operation, dict) and operation.get("result_schema") in schemas
            for operation in operations.values()
        ),
        "an operation references an unavailable result schema",
    )
    expected_result_postconditions = {
        "core.generate": [
            "artifact satisfies specification and declared verification",
            "result.operation.value equals the produced material artifact",
        ],
        "core.convert": [
            "output has target_format and preserved properties",
            "result.operation.value equals the converted material value",
        ],
        "core.execute": [
            "result.command.mode is non_graph for PATH, URI, or STRING executable targets and graph for REFERENCE[TASK|PHASE|SEQUENCE|ACTION|TEST] targets",
            "a non_graph failure to start records started FALSE and completed FALSE without exit_code, stdout, or stderr; after start, started is TRUE and stdout and stderr are present, and exit_code is present exactly when completed is TRUE",
            "a completed non_graph command with a nonzero exit_code has producer status.succeeded unless another producer-contract failure occurred",
            "graph mode never synthesizes started, completed, exit_code, stdout, or stderr and records value exactly when the completed graph exposes one material primary result",
            "completion, failure phase, effects, and OUTPUT binding are recorded through the closed result.command contract",
        ],
        "core.download": [
            "destination bytes equal received source",
            "checksum matches when supplied",
            "result.transfer.value equals the received material content",
        ],
    }
    for operation_name, postconditions in expected_result_postconditions.items():
        operation_value = operations.get(operation_name)
        operation = operation_value if isinstance(operation_value, dict) else {}
        expect(
            operation.get("postconditions") == postconditions,
            f"{operation_name} result-population postconditions are not exact",
        )

    expected_output_rules = [
        "TARGET is destination, not produced VALUE.",
        "Zero PROPERTY occurrences select the applicable default result; one selects a scalar field; two or more select a closed OBJECT in declaration order.",
        "Every PROPERTY is unique and must name an available projectable result field.",
    ]
    expect(
        blocks.get("OUTPUT", {}).get("rules") == expected_output_rules
        and fields.get("blocks", {}).get("OUTPUT", {}).get("conditional_requirements")
        == expected_output_rules,
        "OUTPUT block and field-signature projection rules are not exact or do not agree",
    )

    prose_requirements = {
        "03_TYPES_AND_VALUES/09_MISSING_UNKNOWN_NULL_AND_OPTIONALITY.txt": [
            "transient result record may contain UNKNOWN",
            "result.operation.changed",
            "result.transfer.bytes",
            "cannot bind or partially bind OUTPUT",
        ],
        "04_GRAMMAR/07_RULE_CHECK_OUTPUT_AND_COMPLETION_FORM.txt": [
            "Zero PROPERTY occurrences",
            "one scalar result field",
            "two or more select a closed OBJECT",
        ],
        "05_SEMANTICS/05_INPUT_DATA_OUTPUT_RESULT_AND_FORMAT.txt": [
            "The six common fields are always present exactly once",
            "Two or more PROPERTY occurrences select a closed OBJECT",
            "failure_phase pre_effect",
            "Partial effects never imply partial OUTPUT",
            "nonzero exit_code is not failure to start",
            "graph OUTPUT must explicitly select value or mode",
            "count is a non-negative INTEGER exactly equal",
            "non-negative BYTES count of bytes actually transferred",
        ],
        "05_SEMANTICS/09_VALIDATION_EXECUTION_FAILURE_AND_TERMINATION.txt": [
            "failure_phase and effect_state as separate axes",
            "pre-effect failure requires effect_state none",
            "effect_state partial does not force status.partial",
        ],
        "05_SEMANTICS/10_VERIFY_TEST_EVIDENCE_SUCCESS_FAILURE_AND_STATUS.txt": [
            "A result record's status is instead scoped to the producer invocation",
            "passed FALSE",
            "valid FALSE",
            "material FALSE remains a valid bound result value",
        ],
        "06_STANDARD_LIBRARY/10_CORE_OPERATION_PARAMETER_RULES.txt": [
            "The nine result schemas",
            "exact cardinalities",
            "result-to-OUTPUT binding are closed",
        ],
    }
    for relative_path, tokens in prose_requirements.items():
        prose = " ".join((root / relative_path).read_text(encoding="utf-8").split())
        missing = [token for token in tokens if token not in prose]
        expect(not missing, f"{relative_path} is missing result-contract prose: {missing}")
        expect(
            "remain deferred to LCL-TASK-0005" not in prose,
            f"{relative_path} retains a Task-0005 deferral",
        )

    expected_example_blocks = {
        "08_EXAMPLES/VALID/03_IMPORTING_TASK.lcl": (
            "OUTPUT:\n"
            "    ID: output.copy\n"
            "    TYPE: PATH\n"
            "    FORMAT: format.binary\n"
            "    TARGET: PATH(REF(workspace.example), \"copy.bin\")\n"
            "    PROPERTY: destination\n"
        ),
        "08_EXAMPLES/VALID/04_AUTOMATED_CODING_TASK.lcl": (
            "OUTPUT:\n"
            "    ID: output.test\n"
            "    TYPE: OBJECT[REF(type.command_result)]\n"
            "    FORMAT: format.json\n"
            "    PROPERTY: exit_code\n"
            "    PROPERTY: stdout\n"
            "    PROPERTY: stderr\n"
        ),
        "08_EXAMPLES/VALID/06_EXPLICIT_CONTEXT_MEMORY_AND_STATE.lcl": (
            "OUTPUT:\n"
            "    ID: output.draft\n"
            "    TYPE: STRING\n"
            "    FORMAT: format.plain_text\n"
            "    PROPERTY: value\n"
        ),
        "08_EXAMPLES/VALID/08_AUTHORITY_OVERRIDE_HANDLER_AND_RETRY.lcl": (
            "OUTPUT:\n"
            "    ID: output.payload\n"
            "    TYPE: STRING\n"
            "    FORMAT: format.plain_text\n"
            "    PROPERTY: value\n"
        ),
    }
    for relative_path, output_block in expected_example_blocks.items():
        text = (root / relative_path).read_text(encoding="utf-8")
        expect(
            text.count(output_block) == 1,
            f"{relative_path} lacks its exact Task-0005 OUTPUT projection",
        )
    custom_example = (
        root / "08_EXAMPLES/VALID/07_DOMAIN_EXTENSION_OPERATION.lcl"
    ).read_text(encoding="utf-8")
    custom_output = (
        "OUTPUT:\n"
        "    ID: output.image\n"
        "    TYPE: OBJECT[REF(type.image_result)]\n"
        "    FORMAT: format.image\n"
    )
    expect(
        custom_example.count(custom_output) == 1
        and "KIND: kind.operation" in custom_example
        and "RESULT:\n        TYPE: OBJECT[REF(type.image_result)]" in custom_example,
        "custom-operation whole RESULT default projection example is not exact",
    )
    return violations


# These reviewed-row pins detect unexpected drift; the structural, semantic,
# cross-registry, and prose checks below establish the actual contract evidence.
# Completion decisions revised only calculate, compare, select, filter, group,
# retry, continue, stop, cancel, read, validate, and append. All other original
# row pins remain unchanged. The closure checker independently verifies mirrors
# and the newly closed schema, range, content, graph and result-binding rules.
EXPECTED_OPERATION_CONTRACT_FINGERPRINT = (
    "f81a05efa9dfc2a24ff5fbdc31cc5a007584e78f157acbd127d0f2c95c388354"
)
EXPECTED_OPERATION_ROW_FINGERPRINTS = {
    "core.inspect": "fc2ae5ad368d5bba7e4599740b68d627471585b039834d2ad5b5f766beeaf234",
    "core.read": "27a880debe46225033572f7fc4ca294974835bed32819b92624c86315b4c2463",
    "core.analyze": "ecbce69889d9ac22c0065186bd1f62fbac626983189e63c5c8d0fcb51ad4d462",
    "core.calculate": "d79ad6fe8a8b1c9059edf89f849f1e6b94037114c89734322ea90bbe18c539c3",
    "core.compare": "4dad19f69cfc5db996ec2fb413f3d227e418182233ee75a3d01b14f8c15ba500",
    "core.select": "04e33977164f92e814ba8180ae0ccc3b85c71ae624256253f062acf42a1531db",
    "core.filter": "3c5a731ca7e09d6efebab3ba6860382f8af932a970df0d4f2e6dd52218755861",
    "core.sort": "bf13927bbed4cd17d1a9f8a03d5249b8779768cd24d671677f7ba1bc0e3f2b16",
    "core.group": "48f3dc2c76ad0b86f7a2c3579202385599a9a2a3895b55b50aab11c3c9535400",
    "core.validate": "da051fd361d9309d291ca9cdfff54e796a9277466e3ec43960b8e606070e6605",
    "core.verify": "b8fb7cac6c5218ca5022ba73468ab7eea0a97c87c2cf4dcd45849a51be08e1ca",
    "core.test": "5d3d3927b3295c9eab0fed287f15b4364051af0dc7216b36eac1cfcb85996400",
    "core.report": "466c758adf4a24ca72af7d10d178825b16dd670b3daeb3be9c7e8e14e0dbb5d1",
    "core.return": "d412141b34ad45433f840758063e378c35fdad07b3c341a796ee657aaedfdb9e",
    "core.create": "f0a0d85c6ef3b0b8eea603b16de509c81c2fb8a27f47b66518cfab361b6c5100",
    "core.write": "bbfe03cc64ef1aeb1c04d428b466dfef63dfe9153fb1f9e7a1f7309240df2041",
    "core.append": "a130f0faa4b60236d784a82da32cb8f610ca6a6e55397727b58323e66bccbe7b",
    "core.modify": "29ff3bf523f0ee1c1e6fbee835adaf95d10b707f7a9d17267d61e4535732969c",
    "core.copy": "6ec868b6929463235966b2a801161621083b0958ced852ff0ae47508e6f8eefa",
    "core.move": "dc33c95e763f2d25d34995b957a92dabde7bdb0345d3e5cab50d1c9b378517fa",
    "core.rename": "06bc994fd1b85a499ee9b95d941a9724e0bc2b34dfe38f6aaf9e59ba6c4d4240",
    "core.delete": "c786bddf0832f48432fb9c813191e36d60cb1153d618b7d09e5f4d62281cf517",
    "core.generate": "b7a1e626ac246b08cb202643d5c976657397d637710f5fa68b6ffa406be5f969",
    "core.convert": "045e1a662376623b268a664e563991805c07f234f3321f8b39a68ce76f63922b",
    "core.execute": "24b5e8dc4d306d16422b2d3f92beb6556898bbcaac7e98bd1618cb99b2a7da4c",
    "core.install": "b9df3d850ef7e1de21223bd4cc078e27cf07ba0bc509953611eb6ee03f3997a4",
    "core.uninstall": "0e2db0cdbf6f4a6ce38c6b13fbde3d62df013528a103d8b79d5606a2a2f35dbf",
    "core.start": "73d937f8ea0bb2762624bd6e95f3cd16ef398dce01c17490fefa45f0051ba2d1",
    "core.stop": "17576100931c3a8ff80acc72a160eeae6a87c6b85d0dcc44c372d7c7df7baed2",
    "core.send": "fdae1894d136becbd31b3b40bc90f1618210e08ce612d925bbe610e638c1a3f6",
    "core.publish": "b94bb753e139d22511823f2afc588d91f6be4174425ebaa94ef0d2c07ff9af12",
    "core.upload": "c7efd0b692d5ccd9a818da3b87e0b4d9e5e9345841d278927d5dd3c3ea3193d4",
    "core.download": "9f603f23ac69a4ef1b64246c9e3c58112e0c1df11f800b9eb8bd4d407d04007f",
    "core.memory_write": "79c11d4767f792c59c0c502b6d8d33826b1a34670985f6269316c75c95323bfe",
    "core.state_update": "fd6f0d9d8c543e42125b67de2c3fe540d3de8fc2cbab48f572cbf85c4836ed12",
    "core.ask": "6e519747a5402e50996d39a21644770c4fc4a17425056e22ee904cc3a0fcb2fd",
    "core.retry": "8d5e1b231ca2cf4f64a16af062cced0013f0551a53d4eea4fb6a127184351047",
    "core.continue": "645575b7863319b9382c3b70d56adf7e4bad03a91561d6bca977dabe180ba5b0",
    "core.cancel": "72abf2f349ef8fff29757b27f6dc329c08b1c5d4f8276ed416b533565ca0f8d1",
}


def operation_contract_row_fingerprint(contract: Any) -> str:
    if not isinstance(contract, dict):
        return "invalid-structure"
    encoded = json.dumps(
        contract,
        ensure_ascii=False,
        separators=(",", ":"),
        sort_keys=True,
    ).encode("utf-8")
    return hashlib.sha256(encoded).hexdigest()


def operation_contract_fingerprint(operations: Any) -> str:
    if not isinstance(operations, dict) or not all(
        isinstance(contract, dict) for contract in operations.values()
    ):
        return "invalid-structure"
    encoded = json.dumps(
        operations,
        ensure_ascii=False,
        separators=(",", ":"),
        sort_keys=True,
    ).encode("utf-8")
    return hashlib.sha256(encoded).hexdigest()


def operation_contract_violations(
    operation_registry: dict[str, Any],
    groups_and_results: dict[str, Any],
    statuses: dict[str, Any],
) -> list[str]:
    violations: list[str] = []

    def expect(condition: bool, message: str) -> None:
        if not condition:
            violations.append(message)

    expected_root_fields = {
        "language",
        "version",
        "closed",
        "parameter_binding",
        "parameter_default_encoding",
        "contract_type_notation",
        "axis_contract",
        "expression_fragment_contract",
        "contracts",
    }
    expect(
        set(operation_registry) == expected_root_fields,
        "operation registry root fields are not exact",
    )
    raw_operations = operation_registry.get("contracts", {})
    expect(isinstance(raw_operations, dict), "operation contracts is not an object")
    operations = raw_operations if isinstance(raw_operations, dict) else {}
    enum_groups = groups_and_results.get("enum_groups", {})
    expected_axis_contract = {
        "custom_operation_resolution": (
            "A custom kind.operation declares its complete axis contract in its own "
            "DEFINE block and selects no implementation profile. SIDE_EFFECT FALSE "
            "declares possible_effects exactly none. A SIDE_EFFECT effect-class LIST "
            "declares one or more distinct concrete effect classes, excludes none, and "
            "is that operation's possible_effects maximum. DEPENDENCY declares the "
            "possible_dependencies maximum from the closed dependency vocabulary; an "
            "omitted DEPENDENCY declares exactly declared_state_only. Resolve the "
            "invocation effect set from the address classes of the resolved target and "
            "declared destination arguments under the same address-class rules that bind "
            "core rows, bounded by the declared maximum; an invocation that resolves an "
            "effect class outside that maximum, and an effect-class declaration whose "
            "invocation resolves no concrete effect class, each emit "
            "error.operation.precondition before effects. DETERMINISTIC TRUE requires the "
            "declared contract to satisfy determinism_identity over identical declared "
            "inputs and declared dependency snapshots; DETERMINISTIC FALSE conservatively "
            "permits either final category and never relaxes another rule. The declaration "
            "supplies the final category and the dependency and effect sets used by an "
            "ACTION graph."
        ),
        "determinism_identity": (
            "For a deterministic operation, identical declared inputs, dependency "
            "snapshots, selected profile-role bindings, and implementation versions require "
            "semantically equivalent results, effects, ordering, and status."
        ),
        "dependency_resolution": (
            "possible_dependencies is the registry maximum. Resolve the invocation "
            "dependency set from the target, source, destination, selected profile, "
            "arguments, and referenced execution graph. When no external capability is "
            "selected, the invocation set is exactly declared_state_only."
        ),
        "effect_resolution": (
            "possible_effects is the registry maximum. Resolve the invocation effect set "
            "from the selected targets, destinations, profiles, arguments, and referenced "
            "execution graph. When no observable effect can occur, the invocation set is "
            "exactly none."
        ),
        "category_definitions": {
            "read_only": (
                "Observes or computes without authorizing any observable effect; "
                "possible_effects is exactly none."
            ),
            "mutating": (
                "May change an external or addressable target; the permitted effect "
                "classes remain exactly bounded by possible_effects."
            ),
            "memory_state": (
                "Writes an LCL MEMORY or STATE target; the permitted effect classes "
                "remain exactly bounded by possible_effects."
            ),
            "control": (
                "Coordinates, delegates, stops, resumes, retries, or cancels execution, "
                "or requests authoritative input; the permitted effect classes remain "
                "exactly bounded by possible_effects."
            ),
        },
        "dependency_definitions": {
            "declared_state_only": (
                "Reads only declared LCL values, immutable definitions, and invocation-local "
                "or internal execution snapshots; selects no host, network, model, or human "
                "capability."
            ),
            "host": (
                "Reads or invokes an addressable resource or capability supplied by the "
                "execution host, excluding any separately declared network, model, or human "
                "capability."
            ),
            "network": (
                "Reads from or invokes a remote endpoint or transport capability; this "
                "dependency alone is not a network effect."
            ),
            "model": (
                "Obtains inference or generation from the selected LC or model capability."
            ),
            "human": "Obtains an authoritative response or decision from a human.",
        },
        "effect_definitions": {
            "none": (
                "Produces no observable state change or outbound communication and is exclusive."
            ),
            "filesystem": (
                "Creates, changes, moves, or deletes filesystem content or metadata."
            ),
            "network": (
                "Performs a primary transfer, copy, download, upload, or publication over a "
                "network, or creates, changes, or removes network-addressed content. "
                "Remote access used only as observation or computation input is a dependency. "
                "Remote transport used only to realize a process, package, message, memory, "
                "or state effect is not an additional network effect."
            ),
            "process": (
                "Starts, executes, stops, or otherwise changes a native process or service."
            ),
            "package": "Changes installed package state or package-manager records.",
            "message": (
                "Sends an externally observable message or request to a recipient."
            ),
            "memory": "Changes an LCL MEMORY target.",
            "state": (
                "Changes an LCL STATE or OUTPUT target, an authorized addressable target not "
                "classified as filesystem, network, or memory, or internal execution-unit "
                "or lifecycle state."
            ),
        },
        "marker_definitions": {
            "inherited": (
                "Resolve the marked axis exactly and transitively from the referenced ACTION; "
                "this marker appears only on core.retry and never combines with another member."
            )
        },
        "implementation_profile": {
            "selection": (
                "When an operation contract names one or more selected or implementation "
                "profile roles, the exact operation identifier, profile role, target or address "
                "class, arguments, implementation identifier, and implementation version "
                "select exactly one immutable profile for each role before effects."
            ),
            "required_properties": [
                "operation_id",
                "profile_role",
                "implementation_id",
                "implementation_version",
                "target_class",
                "determinism_category",
                "determinism_source",
                "possible_dependencies",
                "possible_effects",
                "invocation_resolution",
            ],
            "required_roles_by_operation": {
                "core.analyze": {"all": ["analysis"]},
                "core.verify": {"all": ["verification"]},
                "core.report": {"all": ["reporting"]},
                "core.create": {"all": ["target"]},
                "core.write": {"all": ["write"]},
                "core.modify": {"all": ["change"]},
                "core.copy": {"all": ["copy"]},
                "core.move": {"all": ["move"]},
                "core.rename": {"all": ["rename"]},
                "core.delete": {"all": ["delete"]},
                "core.generate": {"all": ["generation"]},
                "core.convert": {"all": ["conversion"]},
                "core.execute": {
                    "non_graph": ["execution"],
                    "graph": [],
                },
                "core.install": {"all": ["package"]},
                "core.uninstall": {"all": ["package"]},
                "core.start": {"all": ["start"]},
                "core.stop": {"all": ["stop"]},
                "core.send": {"all": ["transport"]},
                "core.publish": {"all": ["publication"]},
                "core.upload": {"all": ["transfer"]},
                "core.download": {"all": ["source", "transfer"]},
                "core.memory_write": {"all": ["storage"]},
                "core.state_update": {"all": ["storage"]},
            },
            "role_resolution": (
                "all applies to every invocation of that core operation. For core.execute, "
                "non_graph applies exactly to PATH, URI, or STRING targets and graph applies "
                "exactly to REFERENCE[TASK|PHASE|SEQUENCE|ACTION|TEST] targets; graph mode has "
                "no local execution profile and resolves every reachable operation profile "
                "transitively. A core operation absent from required_roles_by_operation "
                "requires no local core profile. A custom kind.operation selects no profile "
                "role and resolves under axis_contract.custom_operation_resolution. No "
                "unlisted local profile role may be selected."
            ),
            "determinism_category_domain": (
                "A profile declares exactly one final category: deterministic or "
                "nondeterministic. derived and inherited are operation-row resolution "
                "markers and are forbidden as profile categories."
            ),
            "bounds": (
                "For bounds checking, declared_state_only denotes an empty set of external "
                "dependency classes and none denotes an empty set of concrete effect classes. "
                "A core profile may narrow but never widen the row's concrete dependency or "
                "effect maxima; an empty narrowed set is represented exactly by its sentinel. "
                "A profile selected by a deterministic base row must declare deterministic; "
                "a nondeterministic profile is out of bounds. "
                "A custom kind.operation declares its own maxima from the same closed "
                "vocabularies in its DEFINE block. Every profile's invocation rule resolves "
                "actual sets within "
                "its maxima."
            ),
            "determinism_resolution": (
                "A deterministic base row remains deterministic and accepts only a deterministic "
                "profile. A nondeterministic base row "
                "resolves deterministic only when its immutable profile removes every permitted "
                "variation and declares an exact source satisfying determinism_identity; "
                "otherwise it remains nondeterministic. A derived row applies the exact "
                "operation-specific mapping named by its derivation source to the final "
                "categories of every resolved profile and/or graph. DETERMINISTIC TRUE is "
                "verified afterward and never causes category narrowing."
            ),
            "graph_resolution": (
                "For TASK, PHASE, SEQUENCE, ACTION, or TEST references, emit "
                "error.reference.cycle and fail before axis resolution when a prohibited "
                "reference cycle exists, then resolve every reachable core row profile or "
                "custom kind.operation declaration to a final category and invocation sets. "
                "The graph is "
                "deterministic exactly when every reachable resolved operation is deterministic, "
                "including when no operation is reachable; it is nondeterministic when any "
                "reachable resolved operation is nondeterministic. Form each transitive union "
                "from concrete external dependency classes and concrete effect classes only: "
                "omit declared_state_only when any external dependency is present and otherwise "
                "return exactly declared_state_only; omit none when any concrete effect is "
                "present and otherwise return exactly none."
            ),
            "failure": (
                "A missing, ambiguous, incomplete, or out-of-bounds required profile role "
                "emits error.operation.precondition and fails before effects."
            ),
        },
        "exclusive_dependency": "declared_state_only",
        "exclusive_effect": "none",
        "undeclared_dependencies": "forbidden",
        "undeclared_effects": "forbidden",
    }
    determinism_values = [
        "deterministic",
        "nondeterministic",
        "derived",
        "inherited",
    ]
    dependency_classes = [
        "declared_state_only",
        "host",
        "network",
        "model",
        "human",
    ]
    effect_classes = [
        "none",
        "filesystem",
        "network",
        "process",
        "package",
        "message",
        "memory",
        "state",
    ]
    expected_mismatch_error = {
        "meaning": (
            "A kind.operation definition declares DETERMINISTIC TRUE, but its fully "
            "resolved operation or selected profile set is nondeterministic."
        ),
        "stage": "validation",
        "recoverable_with_declared_handler": False,
        "event": None,
        "default_status": "status.invalid",
    }
    expected_precondition_error = {
        "meaning": (
            "A registered operation precondition, including exact resolution of every required "
            "profile role, is false, missing, or unknown."
        ),
        "stage": "execution",
        "recoverable_with_declared_handler": False,
        "event": None,
        "default_status": "status.failed",
    }
    expected_permission_error = {
        "meaning": "Required access or an effect is unauthorized or prohibited.",
        "stage": "execution",
        "recoverable_with_declared_handler": False,
        "event": None,
        "default_status": "status.failed",
    }
    expected_cycle_error = {
        "meaning": ("Immutable definitions, execution-unit references, same-domain error/event/status "
                    "aliases, or direct HANDLER FALLBACK references contain a prohibited dependency cycle."),
        "stage": "resolution",
        "recoverable_with_declared_handler": False,
        "event": None,
        "default_status": "status.invalid",
    }

    expect(operation_registry.get("language") == "LCL", "operation registry language is not LCL")
    expect(operation_registry.get("version") == "0.1.0", "operation registry version is not 0.1.0")
    expect(operation_registry.get("closed") is True, "operation registry is not closed")
    expect(
        operation_registry.get("parameter_binding") == "named_only",
        "operation registry parameter binding is not named_only",
    )
    expect(
        operation_registry.get("parameter_default_encoding")
        == (
            "In this registry only, JSON null in a parameter default field means no "
            "declared default and is not the LCL NULL value. A non-null default is the "
            "exact declared value applied only when an optional parameter is MISSING; "
            "required parameters never acquire a default."
        ),
        "operation registry JSON-null default encoding is not exact",
    )
    expect(
        operation_registry.get("axis_contract") == expected_axis_contract,
        "operation axis_contract is not the exact approved contract",
    )
    expect(
        enum_groups.get("determinism_values") == determinism_values,
        "determinism_values is not the exact closed vocabulary",
    )
    expect(
        enum_groups.get("operation_categories")
        == ["read_only", "mutating", "memory_state", "control"],
        "operation_categories is not the exact closed vocabulary",
    )
    expect(
        enum_groups.get("dependency_classes") == dependency_classes,
        "dependency_classes is not the exact closed vocabulary",
    )
    expect(
        enum_groups.get("effect_classes") == effect_classes,
        "effect_classes is not the exact closed vocabulary",
    )
    expect(
        enum_groups.get("operation_axis_markers") == ["inherited"],
        "operation_axis_markers must contain only inherited",
    )
    expect(
        "side_effect_classes" not in enum_groups,
        "legacy side_effect_classes remains registered",
    )

    expected_ids_value = groups_and_results.get("core_operation_ids", [])
    expect(isinstance(expected_ids_value, list), "core_operation_ids is not a list")
    expected_ids = expected_ids_value if isinstance(expected_ids_value, list) else []
    expect(len(expected_ids) == 39, "core_operation_ids does not contain exactly 39 rows")
    expect(
        list(operations) == expected_ids,
        "operation order or membership differs from core_operation_ids",
    )
    required_fields = {
        "meaning",
        "category",
        "determinism",
        "possible_dependencies",
        "possible_effects",
        "invocation_resolution",
        "target",
        "parameters",
        "positional_parameters",
        "result_schema",
        "preconditions",
        "postconditions",
        "errors",
    }
    base_fields = set(required_fields)
    error_resolution_rows = {
        "core.calculate",
        "core.select",
        "core.filter",
        "core.sort",
        "core.group",
        "core.test",
        "core.execute",
        "core.retry",
    }
    sort_fields = base_fields | {
        "result_value_type",
        "diagnostic_triggers",
        "error_resolution",
    }
    delegated_fields = base_fields | {"error_resolution"}
    legacy_fields = {
        "deterministic",
        "determinism_derivation",
        "side_effects",
        "additional_undeclared_effects",
    }
    allowed_dependencies = set(dependency_classes) | {"inherited"}
    allowed_effects = set(effect_classes) | {"inherited"}
    registered_errors = set(statuses.get("errors", {}))
    registered_categories = set(enum_groups.get("operation_categories", []))
    registered_results = set(groups_and_results.get("result_schemas", {}))
    category_counts = {value: 0 for value in determinism_values}

    for name, contract in operations.items():
        if not isinstance(contract, dict):
            violations.append(f"{name} contract is not an object")
            continue
        missing = sorted(required_fields - set(contract))
        expect(not missing, f"{name} is missing required contract fields: {missing}")
        legacy = sorted(legacy_fields & set(contract))
        expect(not legacy, f"{name} retains legacy axis fields: {legacy}")
        expected_fields = (
            sort_fields
            if name == "core.sort"
            else delegated_fields
            if name in error_resolution_rows
            else base_fields
        )
        expect(
            set(contract) == expected_fields,
            f"{name} contract fields are not the exact closed schema",
        )

        expect(
            isinstance(contract.get("meaning"), str) and bool(contract.get("meaning", "").strip()),
            f"{name}.meaning must be a non-empty string",
        )
        operation_category = contract.get("category")
        expect(
            isinstance(operation_category, str)
            and operation_category in registered_categories,
            f"{name}.category is not registered",
        )
        target = contract.get("target")
        expect(
            isinstance(target, dict)
            and set(target) == {"type", "required"}
            and isinstance(target.get("type"), str)
            and bool(target.get("type", "").strip())
            and type(target.get("required")) is bool,
            f"{name}.target is not the exact target schema",
        )
        parameters = contract.get("parameters")
        expect(isinstance(parameters, dict), f"{name}.parameters is not an object")
        if isinstance(parameters, dict):
            for parameter_name, parameter in parameters.items():
                expect(
                    isinstance(parameter_name, str)
                    and bool(parameter_name)
                    and isinstance(parameter, dict)
                    and set(parameter)
                    == {"type", "required", "default", "meaning", "constraints"}
                    and isinstance(parameter.get("type"), str)
                    and bool(parameter.get("type", "").strip())
                    and type(parameter.get("required")) is bool
                    and (
                        parameter.get("required") is False
                        or parameter.get("default") is None
                    )
                    and isinstance(parameter.get("meaning"), str)
                    and bool(parameter.get("meaning", "").strip())
                    and isinstance(parameter.get("constraints"), list)
                    and all(
                        isinstance(item, str) and bool(item.strip())
                        for item in parameter.get("constraints", [])
                    ),
                    f"{name}.parameters.{parameter_name} is not the exact parameter schema",
                )
        expect(
            contract.get("positional_parameters") is False,
            f"{name} must forbid positional parameters",
        )
        result_schema = contract.get("result_schema")
        expect(
            isinstance(result_schema, str) and result_schema in registered_results,
            f"{name}.result_schema is not registered",
        )
        for field in ("preconditions", "postconditions"):
            value = contract.get(field)
            expect(
                isinstance(value, list)
                and bool(value)
                and all(isinstance(item, str) and bool(item.strip()) for item in value),
                f"{name}.{field} must be a non-empty string set representation",
            )
        if name == "core.sort":
            expect(
                isinstance(contract.get("result_value_type"), str)
                and bool(contract.get("result_value_type", "").strip()),
                "core.sort.result_value_type must be a non-empty string",
            )
            triggers = contract.get("diagnostic_triggers")
            trigger_errors = contract.get("errors")
            trigger_error_set = (
                set(trigger_errors)
                if isinstance(trigger_errors, list)
                and all(isinstance(value, str) for value in trigger_errors)
                else set()
            )
            expect(
                isinstance(triggers, dict)
                and set(triggers) == trigger_error_set
                and all(
                    isinstance(value, str) and bool(value.strip())
                    for value in triggers.values()
                ),
                "core.sort diagnostic_triggers must cover exactly its errors",
            )
        if name in error_resolution_rows:
            expect(
                isinstance(contract.get("error_resolution"), str)
                and bool(contract.get("error_resolution", "").strip()),
                f"{name}.error_resolution must be a non-empty string",
            )

        determinism = contract.get("determinism")
        expect(
            isinstance(determinism, dict) and set(determinism) == {"category", "source"},
            f"{name}.determinism must contain exactly category and source",
        )
        category = determinism.get("category") if isinstance(determinism, dict) else None
        source = determinism.get("source") if isinstance(determinism, dict) else None
        expect(category in determinism_values, f"{name} has an unregistered determinism category")
        if isinstance(category, str) and category in category_counts:
            category_counts[category] += 1
        expect(
            isinstance(source, str) and bool(source.strip()),
            f"{name} lacks an exact determinism source",
        )
        if category == "derived":
            expect(
                isinstance(source, str) and source.startswith("Derived from "),
                f"{name} derived determinism does not identify its derivation source",
            )

        dependencies = contract.get("possible_dependencies")
        effects = contract.get("possible_effects")
        expect(
            isinstance(dependencies, list) and bool(dependencies),
            f"{name}.possible_dependencies must be a non-empty set representation",
        )
        expect(
            isinstance(effects, list) and bool(effects),
            f"{name}.possible_effects must be a non-empty set representation",
        )
        if isinstance(dependencies, list):
            dependency_strings = all(isinstance(value, str) for value in dependencies)
            expect(dependency_strings, f"{name}.possible_dependencies contains a non-string")
            if dependency_strings:
                expect(
                    len(dependencies) == len(set(dependencies)),
                    f"{name}.possible_dependencies contains duplicates",
                )
                expect(
                    set(dependencies) <= allowed_dependencies,
                    f"{name}.possible_dependencies contains unregistered values",
                )
                expected_order = (
                    ["inherited"]
                    if dependencies == ["inherited"]
                    else [value for value in dependency_classes if value in dependencies]
                )
                expect(
                    dependencies == expected_order,
                    f"{name}.possible_dependencies is not in canonical registry order",
                )
                expect(
                    "declared_state_only" not in dependencies or len(dependencies) == 1,
                    f"{name} combines exclusive declared_state_only with another dependency",
                )
                expect(
                    "inherited" not in dependencies or len(dependencies) == 1,
                    f"{name} combines inherited with another dependency",
                )
        if isinstance(effects, list):
            effect_strings = all(isinstance(value, str) for value in effects)
            expect(effect_strings, f"{name}.possible_effects contains a non-string")
            if effect_strings:
                expect(
                    len(effects) == len(set(effects)),
                    f"{name}.possible_effects contains duplicates",
                )
                expect(
                    set(effects) <= allowed_effects,
                    f"{name}.possible_effects contains unregistered values",
                )
                expected_order = (
                    ["inherited"]
                    if effects == ["inherited"]
                    else [value for value in effect_classes if value in effects]
                )
                expect(
                    effects == expected_order,
                    f"{name}.possible_effects is not in canonical registry order",
                )
                expect(
                    "none" not in effects or len(effects) == 1,
                    f"{name} combines exclusive none with another effect",
                )
                expect(
                    "inherited" not in effects or len(effects) == 1,
                    f"{name} combines inherited with another effect",
                )

        resolution = contract.get("invocation_resolution")
        expect(
            isinstance(resolution, str) and bool(resolution.strip()),
            f"{name} lacks an invocation-level resolution rule",
        )
        errors = contract.get("errors")
        expect(isinstance(errors, list), f"{name}.errors is not a list")
        if isinstance(errors, list):
            error_strings = all(isinstance(value, str) for value in errors)
            expect(error_strings, f"{name}.errors contains a non-string")
            if error_strings:
                expect(len(errors) == len(set(errors)), f"{name}.errors contains duplicates")
                expect(
                    set(errors) <= registered_errors,
                    f"{name}.errors contains undefined identifiers",
                )

        actual_row_fingerprint = operation_contract_row_fingerprint(contract)
        expected_row_fingerprint = EXPECTED_OPERATION_ROW_FINGERPRINTS.get(name)
        expect(
            actual_row_fingerprint == expected_row_fingerprint,
            f"{name} reviewed contract pin differs: {actual_row_fingerprint}",
        )

    expect(
        category_counts
        == {
            "deterministic": 27,
            "nondeterministic": 5,
            "derived": 6,
            "inherited": 1,
        },
        f"operation determinism category counts differ: {category_counts}",
    )
    parameter_error_omissions = sorted(
        name
        for name, contract in operations.items()
        if not isinstance(contract, dict)
        or not isinstance(contract.get("errors"), list)
        or "error.operation.parameter" not in contract["errors"]
    )
    expect(
        not parameter_error_omissions,
        "core operations omit universal error.operation.parameter: "
        f"{parameter_error_omissions}",
    )
    required_target_rows = sorted(
        name
        for name, contract in operations.items()
        if isinstance(contract, dict)
        and isinstance(contract.get("target"), dict)
        and contract["target"].get("required") is True
    )
    expect(
        len(required_target_rows) == 37
        and all(
            "error.operation.parameter" in operations[name].get("errors", [])
            for name in required_target_rows
        ),
        "required TARGET omission is not closed for exactly 37 operation rows",
    )

    def row_category(name: str) -> Any:
        row = operations.get(name)
        if not isinstance(row, dict):
            return None
        determinism = row.get("determinism")
        return determinism.get("category") if isinstance(determinism, dict) else None

    expected_derived_sources = {
        "core.sort": (
            "Derived from operators_and_functions_v0.1.0.json#/ordered_types, "
            "#/ordered_type_rules, the registered property-access projection or validated "
            "key operation, original LIST source position for ties, and the distinct-key "
            "rule for SET members: every valid invocation resolves deterministic; an "
            "invocation that cannot satisfy those rules fails."
        ),
        "core.verify": (
            "Derived from deterministic evaluation of the resolved assertion against the "
            "observed target snapshot and the immutable verification profile: the "
            "invocation copies that profile's final category."
        ),
        "core.test": (
            "Derived from the supplied assertion or expected-and-actual comparison and, "
            "when TARGET is a TASK or ACTION, its resolved reachable execution graph: "
            "comparison-only mode is deterministic; reference-execution mode copies the "
            "graph's final category before deterministic comparison."
        ),
        "core.execute": (
            "Derived from the exact invoked command, program, or referenced execution graph "
            "and the immutable profile required by axis_contract.implementation_profile: "
            "non-graph mode copies the execution profile's final category; graph mode copies "
            "the graph's final category."
        ),
        "core.publish": (
            "Derived from the exact selected destination and publication profile together "
            "with visibility and replacement policy: after those values are fixed, the "
            "invocation copies the publication profile's final category."
        ),
        "core.download": (
            "Derived from source pinning or mutability, the optional checksum, source "
            "profile, destination snapshot, and selected transfer profile: the invocation "
            "is deterministic exactly when the source profile fixes one immutable source "
            "identity and content snapshot and both source and transfer profiles are "
            "deterministic; otherwise it is nondeterministic."
        ),
    }
    actual_derived_sources = {
        name: contract.get("determinism", {}).get("source")
        for name, contract in operations.items()
        if isinstance(contract, dict)
        and isinstance(contract.get("determinism"), dict)
        and contract["determinism"].get("category") == "derived"
    }
    expect(
        actual_derived_sources == expected_derived_sources,
        "derived operation final-category mappings are not exact",
    )

    operation_category_counts = {
        category_name: sum(
            1
            for contract in operations.values()
            if isinstance(contract, dict) and contract.get("category") == category_name
        )
        for category_name in ("read_only", "mutating", "memory_state", "control")
    }
    expect(
        operation_category_counts
        == {"read_only": 13, "mutating": 18, "memory_state": 2, "control": 6},
        f"operation category counts differ: {operation_category_counts}",
    )
    read_only_effect_violations = sorted(
        name
        for name, contract in operations.items()
        if isinstance(contract, dict)
        and contract.get("category") == "read_only"
        and contract.get("possible_effects") != ["none"]
    )
    expect(
        not read_only_effect_violations,
        f"read_only operations permit effects: {read_only_effect_violations}",
    )

    callback_constraints = {
        "core.select": (
            "predicate",
            "A REFERENCE resolves exactly once to a kind.operation with SIDE_EFFECT FALSE, "
            "DETERMINISTIC TRUE, a fully resolved dependency set of exactly "
            "declared_state_only, exactly one PARAMETER accepting the member type, and "
            "exactly one BOOLEAN RESULT.",
        ),
        "core.filter": (
            "predicate",
            "A REFERENCE resolves exactly once to a kind.operation with SIDE_EFFECT FALSE, "
            "DETERMINISTIC TRUE, a fully resolved dependency set of exactly "
            "declared_state_only, exactly one PARAMETER accepting the member type, and "
            "exactly one BOOLEAN RESULT.",
        ),
        "core.sort": (
            "key",
            "A REFERENCE resolves to a kind.operation with SIDE_EFFECT FALSE, DETERMINISTIC "
            "TRUE, a fully resolved dependency set of exactly declared_state_only, exactly "
            "one PARAMETER accepting T, and exactly one RESULT of a concrete registered "
            "ordered type.",
        ),
        "core.group": (
            "key",
            "A REFERENCE resolves exactly once to a kind.operation with SIDE_EFFECT FALSE, "
            "DETERMINISTIC TRUE, a fully resolved dependency set of exactly "
            "declared_state_only, exactly one PARAMETER accepting the member type, and "
            "exactly one material RESULT usable as a grouping key.",
        ),
    }
    for name, (parameter_name, exact_constraint) in callback_constraints.items():
        contract_value = operations.get(name)
        contract = contract_value if isinstance(contract_value, dict) else {}
        parameters_value = contract.get("parameters")
        parameters = parameters_value if isinstance(parameters_value, dict) else {}
        parameter_value = parameters.get(parameter_name)
        parameter = parameter_value if isinstance(parameter_value, dict) else {}
        expect(
            contract.get("possible_dependencies") == ["declared_state_only"]
            and contract.get("possible_effects") == ["none"]
            and isinstance(parameter.get("constraints"), list)
            and exact_constraint in parameter.get("constraints", []),
            f"{name} does not close referenced {parameter_name} axes to declared_state_only/none",
        )
        resolution = contract.get("invocation_resolution")
        expect(
            isinstance(resolution, str)
            and "declared_state_only" in resolution
            and "no invocation may add a dependency or effect." in resolution.lower(),
            f"{name} invocation resolution permits hidden callback axes",
        )
        expect(
            contract.get("error_resolution")
            == (
                "Union the local errors with every applicable error of the referenced "
                f"{parameter_name} operation when a {parameter_name} REFERENCE is selected."
                + (" A STRING predicate retains all applicable expression diagnostics."
                   if parameter_name == "predicate" else "")
            ),
            f"{name} does not propagate referenced {parameter_name} errors",
        )

    reference_rows = sorted(
        name
        for name, contract in operations.items()
        if isinstance(contract, dict)
        and (
            "REFERENCE"
            in json.dumps(
                {
                    "target": contract.get("target"),
                    "parameters": contract.get("parameters"),
                },
                ensure_ascii=False,
            )
            or any(
                marker
                in str(
                    contract.get("target", {}).get("type", "")
                    if isinstance(contract.get("target"), dict)
                    else ""
                )
                for marker in ("meta.addressable", "meta.mutable_target")
            )
        )
    )
    unresolved_error_omissions = [
        name
        for name in reference_rows
        if not isinstance(operations[name].get("errors"), list)
        or "error.reference.unresolved" not in operations[name]["errors"]
    ]
    expect(
        not unresolved_error_omissions,
        "REFERENCE-admitting operations omit error.reference.unresolved: "
        f"{unresolved_error_omissions}",
    )
    kind_constrained_rows = {
        "core.select",
        "core.filter",
        "core.sort",
        "core.group",
        "core.validate",
    }
    expect(
        all(
            isinstance(operations.get(name), dict)
            and "error.reference.kind" in operations[name].get("errors", [])
            for name in kind_constrained_rows
        ),
        "operation-constrained bare references do not all map wrong kinds",
    )

    validate_value = operations.get("core.validate", {})
    validate_contract = validate_value if isinstance(validate_value, dict) else {}
    validate_parameters_value = validate_contract.get("parameters", {})
    validate_parameters = (
        validate_parameters_value if isinstance(validate_parameters_value, dict) else {}
    )
    expect(
        validate_parameters.get("schema")
        == {
            "type": "REFERENCE",
            "required": False,
            "default": None,
            "meaning": "Schema to validate against.",
            "constraints": [
                "The REFERENCE resolves exactly once, following transparent aliases, to a "
                "kind.type whose resolved type is OBJECT and whose schema applies to the "
                "target. Material OBJECT schema encodings are not admitted."
            ],
        }
        and validate_parameters.get("rules")
        == {
            "type": "LIST[REFERENCE]",
            "required": False,
            "default": [],
            "meaning": "Additional VALIDATE declarations.",
            "constraints": [
                "Every REFERENCE resolves exactly once to a VALIDATE declaration applicable "
                "to the target."
            ],
        }
        and "error.reference.kind" in validate_contract.get("errors", []),
        "core.validate does not map bare schema/rule reference domains exactly",
    )

    verify_value = operations.get("core.verify", {})
    verify_contract = verify_value if isinstance(verify_value, dict) else {}
    verify_parameters_value = verify_contract.get("parameters", {})
    verify_parameters = (
        verify_parameters_value if isinstance(verify_parameters_value, dict) else {}
    )
    expect(
        verify_parameters.get("assertion", {}).get("type")
        == "boolean_expression|REFERENCE[BOOLEAN]"
        and verify_parameters.get("evidence", {}).get("type")
        == "LIST[REFERENCE[EVIDENCE]]"
        and "error.reference.unresolved" in verify_contract.get("errors", [])
        and "error.reference.kind" not in verify_contract.get("errors", []),
        "core.verify typed references or local reference diagnostics are not exact",
    )

    select = operations.get("core.select", {})
    expect(
        row_category("core.select") == "nondeterministic",
        "core.select must be nondeterministic without a mandatory complete strategy",
    )
    expect(
        isinstance(select, dict)
        and select.get("postconditions")
        == [
            (
                "every returned member is one target member occurrence for which predicate "
                "is TRUE"
            ),
            "no target member occurrence is returned more than once",
            (
                "result cardinality is any integer from zero through the number of TRUE "
                "target member occurrences"
            ),
            "the selected TRUE subset and result order are otherwise unspecified",
        ],
        "core.select does not bound nondeterminism to a duplicate-free TRUE subset",
    )
    expect(
        "error.required.missing" in select.get("errors", [])
        and "error.value.unknown" in select.get("errors", [])
        and "A predicate result of MISSING produces error.required.missing"
        in str(select.get("invocation_resolution", "")),
        "core.select does not distinguish MISSING and UNKNOWN predicate results",
    )
    expect(
        row_category("core.filter") == "deterministic",
        "core.filter must retain its evidence-determined deterministic contract",
    )
    filter_value = operations.get("core.filter", {})
    filter_contract = filter_value if isinstance(filter_value, dict) else {}
    expect(
        filter_contract.get("target") == {"type": "LIST[T]", "required": True}
        and filter_contract.get("determinism")
        == {
            "category": "deterministic",
            "source": "All and only TRUE members are returned in their exact LIST source order.",
        }
        and filter_contract.get("preconditions")
        == ["target is LIST[T] and predicate accepts T"]
        and filter_contract.get("postconditions")
        == ["source order is preserved", "all and only TRUE members are returned"]
        and "error.type.mismatch" in filter_contract.get("errors", [])
        and "error.required.missing" in filter_contract.get("errors", [])
        and "error.value.unknown" in filter_contract.get("errors", []),
        "core.filter does not close deterministic output ordering to LIST[T]",
    )
    group_value = operations.get("core.group", {})
    group_contract = group_value if isinstance(group_value, dict) else {}
    expect(
        group_contract.get("target") == {"type": "LIST[T]", "required": True}
        and group_contract.get("determinism")
        == {
            "category": "deterministic",
            "source": (
                "The exact deterministic key partitions every LIST member into exactly one "
                "group; groups follow first key occurrence and members retain source order."
            ),
        }
        and group_contract.get("preconditions")
        == ["target is LIST[T] and key is defined for every T member"]
        and group_contract.get("postconditions")
        == [
            "result.value.value is a LIST of closed OBJECT records containing exactly key "
            "and items; key is the evaluated material grouping key and items is LIST[T].",
            "Group keys are compared by the registered strict == equality, with no coercion; "
            "every input member occurrence appears in exactly one items LIST.",
            "Groups follow first key occurrence and members within each group retain LIST "
            "source order; an empty input yields an empty LIST with the same group-record schema.",
        ]
        and "error.type.mismatch" in group_contract.get("errors", [])
        and "error.required.missing" in group_contract.get("errors", [])
        and "error.value.unknown" in group_contract.get("errors", []),
        "core.group does not close deterministic group/member ordering to LIST[T]",
    )

    inspect_value = operations.get("core.inspect", {})
    inspect_contract = inspect_value if isinstance(inspect_value, dict) else {}
    inspect_parameters_value = inspect_contract.get("parameters", {})
    inspect_parameters = (
        inspect_parameters_value if isinstance(inspect_parameters_value, dict) else {}
    )
    expect(
        inspect_parameters.get("depth", {}).get("constraints") == ["0..100"]
        and "error.value.out_of_range" in inspect_contract.get("errors", [])
        and "depth outside 0..100 produces error.value.out_of_range"
        in str(inspect_contract.get("invocation_resolution", "")),
        "core.inspect does not map the bounded depth constraint to value.out_of_range",
    )

    calculate_value = operations.get("core.calculate", {})
    calculate_contract = calculate_value if isinstance(calculate_value, dict) else {}
    calculate_resolution = str(calculate_contract.get("invocation_resolution", ""))
    expect(
        "error.required.missing" in calculate_contract.get("errors", [])
        and "error.value.unknown" in calculate_contract.get("errors", [])
        and "operators_and_functions_v0.1.0.json#/evaluation_contract" in calculate_resolution
        and "including ==, !=, and EXISTS exceptions" in calculate_resolution
        and "required final MISSING result produces error.required.missing" in calculate_resolution
        and "UNKNOWN produces error.value.unknown" in calculate_resolution,
        "core.calculate does not apply the registered MISSING/UNKNOWN expression rules",
    )

    compare_value = operations.get("core.compare", {})
    compare_contract = compare_value if isinstance(compare_value, dict) else {}
    compare_parameters_value = compare_contract.get("parameters", {})
    compare_parameters = (
        compare_parameters_value if isinstance(compare_parameters_value, dict) else {}
    )
    compare_resolution = str(compare_contract.get("invocation_resolution", ""))
    expect(
        compare_parameters.get("criteria", {}).get("default") == "=="
        and "registered == exact strict-equality comparison" in compare_resolution
        and "non-==/!= criterion encountering MISSING produces error.required.missing"
        in compare_resolution
        and "criterion whose result remains UNKNOWN produces error.value.unknown"
        in compare_resolution
        and "error.operator.operand" in compare_contract.get("errors", [])
        and "error.pattern.resource_limit" in compare_contract.get("errors", [])
        and "error.required.missing" in compare_contract.get("errors", [])
        and "error.value.unknown" in compare_contract.get("errors", []),
        "core.compare default or registered sentinel-error resolution is not exact",
    )

    return_value = operations.get("core.return", {})
    return_contract = return_value if isinstance(return_value, dict) else {}
    return_resolution = str(return_contract.get("invocation_resolution", ""))
    expect(
        return_contract.get("preconditions")
        == ["target resolves to a material value other than MISSING or UNKNOWN"]
        and "does not resolve exactly once produces error.reference.unresolved"
        in return_resolution
        and "resolves to MISSING produces error.required.missing" in return_resolution
        and "resolves to UNKNOWN produces error.value.unknown" in return_resolution
        and "error.required.missing" in return_contract.get("errors", [])
        and "error.value.unknown" in return_contract.get("errors", []),
        "core.return does not distinguish unresolved, MISSING, and UNKNOWN targets",
    )

    missing_error_rows = sorted(
        name
        for name, contract in operations.items()
        if isinstance(contract, dict)
        and "error.required.missing" in contract.get("errors", [])
    )
    unknown_error_rows = sorted(
        name
        for name, contract in operations.items()
        if isinstance(contract, dict)
        and "error.value.unknown" in contract.get("errors", [])
    )
    expect(
        missing_error_rows
        == sorted(
            {
                "core.calculate",
                "core.compare",
                "core.select",
                "core.filter",
                "core.sort",
                "core.group",
                "core.return",
                "core.ask",
                "core.retry",
            }
        ),
        f"error.required.missing operation coverage differs: {missing_error_rows}",
    )
    expect(
        unknown_error_rows
        == sorted(
            {
                "core.calculate",
                "core.compare",
                "core.select",
                "core.filter",
                "core.sort",
                "core.group",
                "core.return",
                "core.retry",
            }
        ),
        f"error.value.unknown operation coverage differs: {unknown_error_rows}",
    )

    test_value = operations.get("core.test", {})
    test_contract = test_value if isinstance(test_value, dict) else {}
    test_parameters_value = test_contract.get("parameters", {})
    test_parameters = (
        test_parameters_value if isinstance(test_parameters_value, dict) else {}
    )
    expect(
        test_contract.get("category") == "control"
        and test_contract.get("possible_dependencies")
        == ["host", "network", "model", "human"]
        and test_contract.get("possible_effects")
        == ["filesystem", "network", "process", "package", "message", "memory", "state"],
        "core.test is not control with the exact transitive execution maxima",
    )
    expect(
        test_contract.get("target")
        == {"type": "REFERENCE[TASK|ACTION]|meta.material_value", "required": False}
        and test_contract.get("preconditions")
        == [
            (
                "exactly one comparison form is supplied: assertion; or expected together "
                "with exactly one actual source, either the actual parameter or a "
                "material-value TARGET"
            ),
            (
                "a material-value TARGET is legal only as the actual source for expected, "
                "cannot accompany actual, and cannot accompany assertion"
            ),
            (
                "a TASK or ACTION TARGET is executed before the supplied comparison and "
                "TARGET alone is not a complete test"
            ),
        ]
        and test_contract.get("postconditions")
        == [
            (
                "passed reflects the supplied assertion or registered == strict-equality "
                "comparison after any referenced execution completes"
            )
        ],
        "core.test comparison and reference-execution modes are not exact",
    )
    expect(
        test_parameters.get("assertion", {}).get("type")
        == "boolean_expression|REFERENCE[BOOLEAN]"
        and test_parameters.get("actual", {}).get("type")
        == "meta.material_value|REFERENCE[meta.material_value]"
        and "error.reference.unresolved" in test_contract.get("errors", [])
        and "error.operator.operand" in test_contract.get("errors", [])
        and "error.reference.kind" not in test_contract.get("errors", []),
        "core.test typed comparison references or local diagnostics are not exact",
    )
    test_resolution = test_contract.get("invocation_resolution")
    expect(
        isinstance(test_resolution, str)
        and "declared_state_only" in test_resolution
        and "effects to none" in test_resolution,
        "core.test does not resolve comparison-only mode to declared state and no effects",
    )
    expect(
        isinstance(test_resolution, str)
        and "Exactly one comparison form is always supplied:" in test_resolution
        and "A material-value TARGET is the actual source" in test_resolution
        and "Expected-and-actual form always uses the registered == strict-equality operator"
        in test_resolution
        and "execute its reachable graph" in test_resolution
        and "normalized transitive dependency and effect unions" in test_resolution
        and "axis_contract.implementation_profile.graph_resolution" in test_resolution
        and "then evaluate the supplied assertion or registered == comparison"
        in test_resolution
        and test_contract.get("error_resolution")
        == (
            "Union the local errors, including error.reference.cycle for a prohibited graph "
            "cycle, with every applicable error of the referenced TASK or ACTION graph when "
            "reference-execution mode is selected."
        ),
        "core.test does not resolve comparison after transitive graph execution/errors",
    )

    profile_contract = operation_registry.get("axis_contract", {}).get(
        "implementation_profile", {}
    )
    actual_profile_roles = (
        profile_contract.get("required_roles_by_operation", {})
        if isinstance(profile_contract, dict)
        else {}
    )
    expected_profile_roles = expected_axis_contract["implementation_profile"][
        "required_roles_by_operation"
    ]
    expect(
        actual_profile_roles == expected_profile_roles,
        "required profile roles or core.execute mode switch differ",
    )
    expected_profile_rows = sorted(expected_profile_roles)
    actual_profile_rows = sorted(actual_profile_roles)
    expect(
        actual_profile_rows == expected_profile_rows and len(actual_profile_rows) == 23,
        f"required-profile operation set differs: {actual_profile_rows}",
    )
    profile_role_binding_count = sum(
        len(roles)
        for modes in actual_profile_roles.values()
        if isinstance(modes, dict)
        for roles in modes.values()
        if isinstance(roles, list)
    )
    expect(
        profile_role_binding_count == 24,
        f"required-profile direct role binding count differs: {profile_role_binding_count}",
    )
    expect(
        all(name in operations for name in actual_profile_rows),
        "required_roles_by_operation names an unregistered core operation",
    )
    unlisted_role_rows = sorted(
        name
        for name, contract in operations.items()
        if name not in actual_profile_roles
        and isinstance(contract, dict)
        and "local profile role"
        in json.dumps(contract, ensure_ascii=False).lower()
    )
    expect(
        not unlisted_role_rows,
        f"operations outside the closed role map claim a local profile role: {unlisted_role_rows}",
    )
    profile_error_violations = sorted(
        name
        for name in actual_profile_rows
        if not isinstance(operations.get(name), dict)
        or not isinstance(operations[name].get("errors"), list)
        or "error.operation.precondition" not in operations[name]["errors"]
    )
    expect(
        not profile_error_violations,
        f"required-profile operations omit error.operation.precondition: {profile_error_violations}",
    )
    expect(
        statuses.get("errors", {}).get("error.operation.precondition")
        == expected_precondition_error,
        "error.operation.precondition is not the exact core/custom profile-failure contract",
    )
    expect(
        statuses.get("errors", {}).get("error.permission.denied")
        == expected_permission_error,
        "error.permission.denied is not the exact access/effect-authorization contract",
    )
    expect(
        statuses.get("errors", {}).get("error.reference.cycle")
        == expected_cycle_error,
        "error.reference.cycle is not the exact graph-resolution contract",
    )

    generic_target_mutators = {
        "core.create",
        "core.write",
        "core.append",
        "core.modify",
        "core.rename",
        "core.delete",
        "core.generate",
    }
    generic_target_mutator_violations: list[str] = []
    for name in sorted(generic_target_mutators):
        contract_value = operations.get(name, {})
        contract = contract_value if isinstance(contract_value, dict) else {}
        resolution = str(contract.get("invocation_resolution", ""))
        if not (
            "target is not MEMORY or STATE; those mutations use core.memory_write or "
            "core.state_update" in contract.get("preconditions", [])
            and "address class of its resolved target, never by REFERENCE syntax"
            in resolution
            and "MEMORY and STATE targets are prohibited" in resolution
            and "core.memory_write or core.state_update" in resolution
            and "error.operation.precondition" in contract.get("errors", [])
        ):
            generic_target_mutator_violations.append(name)
    expect(
        not generic_target_mutator_violations,
        "generic target mutators do not classify resolved addresses or exclude MEMORY/STATE: "
        f"{generic_target_mutator_violations}",
    )

    move_value = operations.get("core.move", {})
    move_contract = move_value if isinstance(move_value, dict) else {}
    move_resolution = str(move_contract.get("invocation_resolution", ""))
    expect(
        "source is not MEMORY or STATE; those mutations use core.memory_write or "
        "core.state_update" in move_contract.get("preconditions", [])
        and "resolved source and destination addresses are distinct"
        in move_contract.get("preconditions", [])
        and "address class of its resolved target, never by REFERENCE syntax"
        in move_resolution
        and "removing OUTPUT or another authorized non-filesystem, non-network, "
        "non-memory, non-STATE addressable source adds state effect"
        in move_resolution
        and "MEMORY and STATE sources are prohibited" in move_resolution
        and "error.operation.precondition" in move_contract.get("errors", []),
        "core.move does not classify resolved sources or exclude MEMORY/STATE sources",
    )

    rename_value = operations.get("core.rename", {})
    rename_contract = rename_value if isinstance(rename_value, dict) else {}
    expect(
        "new_name differs from the current target name"
        in rename_contract.get("preconditions", [])
        and rename_contract.get("postconditions")
        == [
            "target is addressable by new_name and not by its prior name",
            "content and non-name properties are preserved",
        ]
        and "Require new_name to differ from the current target name"
        in str(rename_contract.get("invocation_resolution", "")),
        "core.rename permits a no-op or fails to require the exact name transition",
    )

    copy_value = operations.get("core.copy", {})
    copy_contract = copy_value if isinstance(copy_value, dict) else {}
    copy_resolution = str(copy_contract.get("invocation_resolution", ""))
    expect(
        "address class of its resolved target, never by REFERENCE syntax"
        in copy_resolution
        and "source observation dependencies and destination mutation effects independently"
        in copy_resolution
        and "MEMORY, STATE, OUTPUT" in copy_resolution
        and "no source-side effect" in copy_resolution,
        "core.copy does not classify resolved source observation without a source effect",
    )

    memory_write_value = operations.get("core.memory_write", {})
    memory_write = memory_write_value if isinstance(memory_write_value, dict) else {}
    memory_parameters_value = memory_write.get("parameters", {})
    memory_parameters = (
        memory_parameters_value if isinstance(memory_parameters_value, dict) else {}
    )
    expect(
        memory_parameters.get("merge")
        == {
            "type": "BOOLEAN",
            "required": False,
            "default": False,
            "meaning": (
                "Apply the closed shallow right-biased OBJECT merge instead of replacement."
            ),
            "constraints": [
                "TRUE requires the current MEMORY value and value parameter both to be OBJECT"
            ],
        }
        and memory_write.get("preconditions")
        == [
            "MEMORY mode permits write",
            "when merge is FALSE, value matches the declared MEMORY type",
            (
                "when merge is TRUE, the current MEMORY value and value parameter are "
                "OBJECT and the computed merged OBJECT matches the declared MEMORY type"
            ),
        ]
        and memory_write.get("postconditions")
        == [
            "when merge is FALSE, the persistent value equals the value parameter",
            (
                "when merge is TRUE, the persistent OBJECT contains the union of current "
                "and new top-level fields, with new same-name fields winning and no "
                "recursive merge"
            ),
        ]
        and "form the union of their top-level field names"
        in str(memory_write.get("invocation_resolution", ""))
        and "do not recurse into nested OBJECT values"
        in str(memory_write.get("invocation_resolution", "")),
        "core.memory_write does not close replacement and shallow right-biased merge",
    )

    ask_value = operations.get("core.ask")
    ask = ask_value if isinstance(ask_value, dict) else {}
    expect(
        ask.get("possible_dependencies") == ["human"]
        and ask.get("possible_effects") == ["message"]
        and ask.get("invocation_resolution")
        == (
            "Resolve one authoritative human responder. The request channel is internal to "
            "that human capability and adds no separate host or network dependency; the "
            "request is a message effect."
        ),
        "core.ask does not close its request channel to the human dependency",
    )
    ask_parameters_value = ask.get("parameters", {})
    ask_parameters = ask_parameters_value if isinstance(ask_parameters_value, dict) else {}
    expect(
        ask_parameters.get("options")
        == {
            "type": "LIST[meta.material_value]",
            "required": False,
            "default": None,
            "meaning": "Closed answer choices, each compatible with expected_type.",
            "constraints": ["Every option is compatible with expected_type."],
        }
        and ask.get("preconditions")
        == [
            "authoritative responder is available",
            "request and responder are authorized and in scope",
            "every option is compatible with expected_type",
        ]
        and ask.get("postconditions")
        == [
            "a non-MISSING answer is recorded and compatible with expected_type",
            "when options is supplied, every non-MISSING answer equals one listed option",
            (
                "when no authorized valid answer is provided, the value remains MISSING "
                "and error.required.missing applies"
            ),
        ]
        and "error.type.mismatch" in ask.get("errors", [])
        and "error.required.missing" in ask.get("errors", []),
        "core.ask does not close expected-type and supplied-options answer selection",
    )

    uninstall_value = operations.get("core.uninstall", {})
    uninstall = uninstall_value if isinstance(uninstall_value, dict) else {}
    expect(
        uninstall.get("postconditions")
        == [
            "target installation is absent",
            "when purge_data is FALSE, declared associated data is preserved",
            "when purge_data is TRUE, declared associated data is absent",
        ]
        and "purge_data TRUE removes all declared associated data in authorized scope"
        in str(uninstall.get("invocation_resolution", "")),
        "core.uninstall purge_data TRUE/FALSE branches are not exact",
    )

    cancel_value = operations.get("core.cancel", {})
    cancel = cancel_value if isinstance(cancel_value, dict) else {}
    expect(
        "target current status has status.cancelled in its registered allowed_next set"
        in cancel.get("preconditions", [])
        and "error.execution.order" in cancel.get("errors", [])
        and "require its registered allowed_next set to contain status.cancelled"
        in str(cancel.get("invocation_resolution", "")),
        "core.cancel does not enforce the registered status transition",
    )

    execute_value = operations.get("core.execute")
    execute = execute_value if isinstance(execute_value, dict) else {}
    execute_source_value = execute.get("determinism")
    execute_source = (
        execute_source_value.get("source", "")
        if isinstance(execute_source_value, dict)
        else ""
    )
    execute_resolution = execute.get("invocation_resolution")
    expect(
        "axis_contract.implementation_profile" in execute_source
        and isinstance(execute_resolution, str)
        and "exactly one immutable execution profile" in execute_resolution
        and "Every such non-graph invocation has the process effect"
        in execute_resolution
        and "union process with any other dependencies and effects selected by the profile"
        in execute_resolution
        and "has no mandatory local process effect" in execute_resolution
        and "invocation-declared external effects" not in execute_resolution
        and execute.get("postconditions")
        == [
            "result.command.mode is non_graph for PATH, URI, or STRING executable targets and graph for REFERENCE[TASK|PHASE|SEQUENCE|ACTION|TEST] targets",
            "a non_graph failure to start records started FALSE and completed FALSE without exit_code, stdout, or stderr; after start, started is TRUE and stdout and stderr are present, and exit_code is present exactly when completed is TRUE",
            "a completed non_graph command with a nonzero exit_code has producer status.succeeded unless another producer-contract failure occurred",
            "graph mode never synthesizes started, completed, exit_code, stdout, or stderr and records value exactly when the completed graph exposes one material primary result",
            "completion, failure phase, effects, and OUTPUT binding are recorded through the closed result.command contract",
        ]
        and execute.get("preconditions")
        == [
            "target is executable and authorized",
            "a non-graph executable target resolves to exactly one complete immutable execution profile",
            "resolved profile dependencies and effects are within this contract and declared scope",
        ],
        "core.execute does not resolve native axes through one bounded immutable profile",
    )
    retry_value = operations.get("core.retry", {})
    retry = retry_value if isinstance(retry_value, dict) else {}
    expect(
        row_category("core.retry") == "inherited"
        and set(retry.get("parameters", {})) == {"limit"}
        and retry.get("target") == {"type": "REFERENCE[ACTION]", "required": True}
        and retry.get("possible_dependencies") == ["inherited"]
        and retry.get("possible_effects") == ["inherited"]
        and "error.required.missing" in retry.get("errors", [])
        and "error.value.unknown" in retry.get("errors", [])
        and "error.value.out_of_range" in retry.get("errors", [])
        and "error.operation.precondition" in retry.get("errors", [])
        and retry.get("preconditions")
        == [
            "The selected handler is handling an event of this same active ACTION invocation "
            "aggregate; its failed attempt is immutable evidence and the existing RETRY budget is consumed.",
            "wrapped ACTION resolves exactly once before any retry decision",
            "wrapped ACTION declares a RETRY block whose LIMIT equals the resolved limit",
            "before an additional attempt after known effects, exact retained evidence "
            "proves safe repetition, resumption, or reconciliation within inherited "
            "authority, scope, and post-state",
            "an indeterminate prior attempt is reconciled before another attempt",
        ]
        and retry.get("postconditions")
        == [
            "attempt count never exceeds 1+limit",
            "every attempt and its local diagnostics, phase, effects, and OUTPUT binding "
            "are evidenced in attempt-index order",
            "aggregate failure phase and effects account for every attempt made",
            "A successful attempt ends retrying; a false condition or safety refusal is not "
            "count exhaustion and does not turn a failed attempt into success.",
        ]
        and "limit outside 0..100 produces error.value.out_of_range"
        in str(retry.get("invocation_resolution", ""))
        and "Before each additional attempt, evaluate RETRY.WHEN (default TRUE) as a required BOOLEAN condition"
        in str(retry.get("invocation_resolution", ""))
        and "A prior pre_effect attempt with effect_state none requires no additional "
        "effect-safety proof" in str(retry.get("invocation_resolution", ""))
        and "An indeterminate prior attempt prohibits another attempt until reconciled"
        in str(retry.get("invocation_resolution", ""))
        and "error.operation.precondition for proved-unsafe repetition"
        in str(retry.get("error_resolution", ""))
        and "a safety-blocked unmade attempt is not exhaustion"
        in str(retry.get("error_resolution", ""))
        and "including any wrapped operation or profile precondition error"
        in str(retry.get("error_resolution", "")),
        "core.retry does not inherit all three axes",
    )
    expect(all(token in str(operations.get("core.continue", {}).get("invocation_resolution", ""))
               for token in ("selected handler", "recovery and advancement commit together",
                             "error.operation.precondition", "error.execution.order")),
           "core.continue does not enforce selected-context and successor constraints")
    expect(all(token in str(operations.get("core.stop", {}).get("invocation_resolution", ""))
               for token in ("active invocation aggregate", "allowed_next", "status.stopped",
                             "error.execution.order")),
           "core.stop does not enforce active-aggregate transition legality")
    expect(all(token in str(operations.get("core.cancel", {}).get("invocation_resolution", ""))
               for token in ("active invocation aggregate", "never a completed attempt",
                             "Explicit cancellation outside a handler remains permitted")),
           "core.cancel does not distinguish explicit targets from injected handler targets")
    out_of_range_rows = sorted(
        name
        for name, contract in operations.items()
        if isinstance(contract, dict)
        and "error.value.out_of_range" in contract.get("errors", [])
    )
    expect(
        out_of_range_rows == ["core.calculate", "core.inspect", "core.read", "core.retry"],
        f"error.value.out_of_range operation coverage differs: {out_of_range_rows}",
    )
    inherited_rows = sorted(
        name
        for name, contract in operations.items()
        if isinstance(contract, dict)
        and (
            row_category(name) == "inherited"
            or contract.get("possible_dependencies") == ["inherited"]
            or contract.get("possible_effects") == ["inherited"]
        )
    )
    expect(inherited_rows == ["core.retry"], "inherited axis marker appears outside core.retry")

    mismatch_users = sorted(
        name
        for name, contract in operations.items()
        if isinstance(contract, dict)
        and isinstance(contract.get("errors"), list)
        and "error.determinism.mismatch" in contract.get("errors", [])
    )
    expect(
        mismatch_users == ["core.validate"],
        "error.determinism.mismatch must apply only to core.validate",
    )
    expect(
        statuses.get("errors", {}).get("error.determinism.mismatch")
        == expected_mismatch_error,
        "error.determinism.mismatch is not the exact validation-stage contract",
    )

    actual_fingerprint = operation_contract_fingerprint(operations)
    expect(
        actual_fingerprint == EXPECTED_OPERATION_CONTRACT_FINGERPRINT,
        "Reviewed complete 39-row contract pin differs: "
        f"{actual_fingerprint}",
    )
    return violations


def operation_prose_contract_violations(
    root: Path, operations: dict[str, Any]
) -> list[str]:
    violations: list[str] = []

    def expect(condition: bool, message: str) -> None:
        if not condition:
            violations.append(message)

    prose_profiles = {
        "06_STANDARD_LIBRARY/01_READ_ONLY_AND_ANALYTICAL_OPERATIONS.txt": {
            "read_only"
        },
        "06_STANDARD_LIBRARY/02_MUTATING_AND_EXTERNAL_OPERATIONS.txt": {
            "mutating",
            "memory_state",
        },
        "06_STANDARD_LIBRARY/03_CONTROL_OPERATIONS.txt": {"control"},
    }
    all_headings: list[str] = []
    heading_pattern = re.compile(r"^core\.[a-z_]+$", re.MULTILINE)
    for relative_path, categories in prose_profiles.items():
        text = (root / relative_path).read_text(encoding="utf-8")
        matches = list(heading_pattern.finditer(text))
        headings = [match.group(0) for match in matches]
        expected_headings = [
            name
            for name, contract in operations.items()
            if isinstance(contract, dict) and contract.get("category") in categories
        ]
        expect(
            headings == expected_headings,
            f"{relative_path} operation headings differ from the registry partition",
        )
        expect(
            "    Side effect:" not in text and "    Deterministic:" not in text,
            f"{relative_path} retains legacy operation-axis labels",
        )
        all_headings.extend(headings)
        for index, match in enumerate(matches):
            name = match.group(0)
            contract = operations.get(name)
            if not isinstance(contract, dict):
                violations.append(f"{relative_path} has no object contract for {name}")
                continue
            block_start = match.end() + 1
            block_end = matches[index + 1].start() if index + 1 < len(matches) else len(text)
            block = text[block_start:block_end]
            determinism_value = contract.get("determinism", {})
            determinism = determinism_value if isinstance(determinism_value, dict) else {}
            dependency_value = contract.get("possible_dependencies", [])
            dependencies = (
                dependency_value
                if isinstance(dependency_value, list)
                and all(isinstance(value, str) for value in dependency_value)
                else ["<invalid>"]
            )
            effect_value = contract.get("possible_effects", [])
            effects = (
                effect_value
                if isinstance(effect_value, list)
                and all(isinstance(value, str) for value in effect_value)
                else ["<invalid>"]
            )
            expected_labels = {
                "Meaning": contract.get("meaning"),
                "Determinism category": determinism.get("category"),
                "Determinism source": determinism.get("source"),
                "Possible dependencies": "{" + ", ".join(dependencies) + "}",
                "Possible effects": "{" + ", ".join(effects) + "}",
                "Invocation resolution": contract.get("invocation_resolution"),
                "Result schema": contract.get("result_schema"),
            }
            if "error_resolution" in contract:
                expected_labels["Error resolution"] = contract.get("error_resolution")
            governed_labels = (
                "Meaning",
                "Determinism category",
                "Determinism source",
                "Possible dependencies",
                "Possible effects",
                "Invocation resolution",
                "Result schema",
                "Error resolution",
            )
            for label in governed_labels:
                actual_count = len(
                    re.findall(rf"^[ \t]*{re.escape(label)}:", block, re.MULTILINE)
                )
                expected_count = 1 if label in expected_labels else 0
                expect(
                    actual_count == expected_count,
                    f"{relative_path} {name} has {actual_count} {label} labels; "
                    f"expected {expected_count}",
                )
            for label, expected_value in expected_labels.items():
                expected_line = f"    {label}: {expected_value}\n"
                expect(
                    block.count(expected_line) == 1,
                    f"{relative_path} {name} does not contain exact {label} parity",
                )

    expect(
        set(all_headings) == set(operations),
        "operation prose union is not the exact 39-row registry membership",
    )
    expect(
        len(all_headings) == len(set(all_headings)) == 39,
        "operation prose headings are not unique 39/39 coverage",
    )

    required_prose = {
        "05_SEMANTICS/11_DETERMINISM_EQUIVALENCE_AND_INTERPRETER_VARIATION.txt": [
            "The separate operation category is a closed behavioral classification",
            "declared inputs, dependency snapshots, selected",
            "possible_dependencies is the maximum dependency set",
            "possible_effects is the maximum effect set",
            "declared_state_only",
            "{none}",
            "Remote transport used only to realize a process, package, message, memory, or",
            "A REFERENCE used as an address is classified by the address class of its",
            "core.move",
            "prohibits MEMORY and STATE sources",
            "source observation adds no source-side effect",
            "derived and inherited are operation-row",
            "declared_state_only is omitted",
            "It is not an override",
            "error.determinism.mismatch exactly when",
        ],
        "03_TYPES_AND_VALUES/05_CUSTOM_TYPES_SCHEMAS_AND_DEFINITIONS.txt": [
            "DETERMINISTIC TRUE is a verified assertion",
            "declared custom-operation contract",
            "selects no implementation",
            "SIDE_EFFECT FALSE declares possible effects exactly {none}",
            "one LIST of one or more distinct concrete effect classes",
            "an omitted",
            "DEPENDENCY declares exactly {declared_state_only}",
            "declared_state_only denotes no external",
            "emits error.operation.precondition",
            "It is not an override",
            "error.determinism.mismatch",
        ],
        "06_STANDARD_LIBRARY/10_CORE_OPERATION_PARAMETER_RULES.txt": [
            "All 39 operation contracts",
            "possible_dependencies and possible_effects are registry maxima",
            "declared_state_only and none are exclusive singleton values",
            "core.test always requires exactly one comparison form",
            "material-value TARGET is legal only as that actual",
            "core.filter and core.group accept LIST[T], not SET[T]",
            "passing SET directly to",
            "error.type.mismatch before effects",
            "For core.select and core.filter, a required predicate result of MISSING",
            "core.compare evaluates one criterion under the registered operator",
            "Every core.ask option must be compatible with expected_type",
            "Before each additional attempt, RETRY.WHEN",
            "rule always includes the process effect because the invocation runs the",
            "normalized transitive union rule",
            "core.retry resolves the wrapped ACTION",
            "free-form or undeclared effect channel exists",
            "The nine result schemas, their common",
            "result-to-OUTPUT binding are closed",
            "OUTPUT PROPERTY projection may narrow the",
        ],
        "06_STANDARD_LIBRARY/06_CORE_ERROR_IDENTIFIERS_PART_1.txt": [
            "complete closed set of 77 error identifiers",
            "error.determinism.mismatch is emitted only during contract validation",
            "fully resolved operation or",
            "It never changes, overrides, or repairs",
            "error.operation.precondition is emitted before effects",
            "complete immutable implementation profile",
        ],
    }
    for relative_path, tokens in required_prose.items():
        text = (root / relative_path).read_text(encoding="utf-8")
        missing = [token for token in tokens if token not in text]
        expect(not missing, f"{relative_path} is missing operation prose: {missing}")

    keyword_registry = load_json_strict(root / "10_REGISTRIES/keywords_v0.1.0.json")
    deterministic_keyword = keyword_registry.get("keywords", {}).get("DETERMINISTIC", {})
    keyword_prose = (
        root / "02_LEXICAL/05_KEYWORD_REFERENCE_A_TO_M.txt"
    ).read_text(encoding="utf-8")
    expect(
        f"    Meaning: {deterministic_keyword.get('meaning')}\n" in keyword_prose
        and f"    Context: {deterministic_keyword.get('contexts')}\n" in keyword_prose,
        "DETERMINISTIC keyword prose and registry differ",
    )
    expect(
        "it is verified and never overrides nondeterminism"
        in deterministic_keyword.get("meaning", ""),
        "DETERMINISTIC keyword meaning lacks verified-assertion semantics",
    )
    return violations


EXPECTED_DIAGNOSTIC_SELECTION_FINGERPRINT = (
    "cdef5c0a8c1e6f532f1a16e2b95c5ed7262cd107fbb2638136e566328a36f1fe"
)
EXPECTED_FAILURE_LIFECYCLE_FINGERPRINT = (
    "2570a4234deec0a0378c182f784fd65956f7674cb44d5d6d2abb525024097904"
)
DIAGNOSTIC_SELECTION_VECTOR_COUNT = 12


def diagnostic_selection_contract_violations(
    root: Path, statuses: dict[str, Any]
) -> list[str]:
    violations: list[str] = []

    def expect(condition: bool, message: str) -> None:
        if not condition:
            violations.append(message)

    errors_value = statuses.get("errors", {})
    errors = errors_value if isinstance(errors_value, dict) else {}
    symbols = load_json_strict(root / "10_REGISTRIES/symbols_v0.1.0.json")
    excluded_symbols = symbols.get("excluded_exact_lexemes", [])
    selection_value = statuses.get("diagnostic_selection", {})
    selection = selection_value if isinstance(selection_value, dict) else {}
    expect(
        canonical_contract_fingerprint(selection)
        == EXPECTED_DIAGNOSTIC_SELECTION_FINGERPRINT,
        "diagnostic-selection policy fingerprint differs",
    )
    expected_selection_fields = {
        "stage_order",
        "severity",
        "specificity_rank",
        "supersedes",
        "selection_scope",
        "earliest_stage_rule",
        "multiplicity_rule",
        "supersession_rule",
        "duplicate_key",
        "duplicate_rule",
        "stable_order",
        "location_rule",
        "primary_rule",
        "secondary_rule",
        "expression_demand_resolution",
    }
    expect(set(selection) == expected_selection_fields, "diagnostic_selection fields differ")
    demand = selection.get("expression_demand_resolution", {})
    expect(set(demand) == {"context", "eligible_errors", "resolved_stage", "default_status",
                          "default_status_overrides", "resolution_rule", "exclusion_rule", "phase_rule"},
           "expression-demand metadata fields are not exact")
    eligible = demand.get("eligible_errors", {})
    expected_eligible = {
        "error.literal.invalid", "error.numeric.division_by_zero", "error.numeric.non_terminating",
        "error.numeric.unit_mismatch", "error.operator.operand", "error.pattern.mismatch",
        "error.pattern.resource_limit", "error.value.out_of_range", "error.required.missing",
        "error.value.unknown", "error.type.mismatch",
    }
    expect(set(eligible) == expected_eligible and set(eligible) <= set(errors),
           "expression-demand eligibility is not the closed eleven-error map")
    expect(all(isinstance(text, str) and text.strip() for text in eligible.values()),
           "expression-demand eligibility requires an exact trigger for every identifier")
    expect(demand.get("resolved_stage") == "execution" and demand.get("default_status") == "status.failed"
           and demand.get("default_status_overrides") == {
               "error.required.missing": "status.blocked", "error.value.unknown": "status.blocked"},
           "expression-demand stage/status overrides are not exact")
    demand_tokens = {
        "context": ("statically valid", "actually demanded", "skipped Boolean operand", "Preflight-required"),
        "resolution_rule": ("before supersession", "duplicate suppression", "phase lookup", "inherited unchanged"),
        "exclusion_rule": ("No source structure", "signature arity", "receiving-type", "Discovery time alone never"),
        "phase_rule": ("pre_effect, post_effect, or indeterminate", "producer-relative effect evidence"),
    }
    for field, tokens in demand_tokens.items():
        expect(all(token in demand.get(field, "") for token in tokens),
               f"expression-demand {field} loses static/dynamic separation")
    expect(all(token in eligible.get("error.literal.invalid", "") for token in
               ("well-typed", "never malformed source tokens", "unsupported constructor signature")),
           "dynamic literal.invalid improperly admits malformed source syntax")
    expect(all(token in eligible.get("error.operator.operand", "") for token in
               ("Only a registered SUM, MIN, or MAX", "arity, operand-family, property-schema", "never qualify")),
           "dynamic operator.operand improperly admits type or signature errors")
    expect(all(token in eligible.get("error.type.mismatch", "") for token in
               ("direct FOR EACH", "statically valid SET", "before any iteration", "receiving-type")),
           "dynamic type.mismatch admits more than the actual-member SET ordering failure")
    expected_stage_order = [
        "lexical",
        "grammar_or_schema",
        "resolution",
        "static_or_expression",
        "validation",
        "execution",
        "verification_or_completion",
    ]
    expect(selection.get("stage_order") == expected_stage_order, "stage order is not exact")
    expect(
        {value.get("stage") for value in errors.values() if isinstance(value, dict)}
        == set(expected_stage_order),
        "registered error stages differ from the closed selection stages",
    )
    expect(
        selection.get("severity")
        == {
            "closed_values": ["error"],
            "default_for_every_error": "error",
            "descending_order": ["error"],
        },
        "severity contract is not the exact one-value ordering",
    )
    expected_rank_overrides = {
        "error.block.duplicate": 200,
        "error.field.duplicate": 200,
        "error.field.required": 200,
        "error.indentation.jump": 200,
        "error.indentation.width": 200,
        "error.literal.escape": 200,
        "error.literal.unclosed": 200,
        "error.source.tab": 200,
    }
    specificity_value = selection.get("specificity_rank", {})
    specificity = specificity_value if isinstance(specificity_value, dict) else {}
    expect(
        specificity
        == {
            "default_for_every_error": 100,
            "higher_is_more_specific": True,
            "overrides": expected_rank_overrides,
        },
        "specificity-rank contract is not exact",
    )
    expected_supersedes = {
        "error.block.duplicate": ["error.block.occurrence"],
        "error.field.duplicate": ["error.field.cardinality"],
        "error.field.required": ["error.field.cardinality"],
        "error.indentation.jump": ["error.indentation.invalid"],
        "error.indentation.width": ["error.indentation.invalid"],
        "error.literal.escape": ["error.literal.invalid"],
        "error.literal.unclosed": ["error.literal.invalid"],
        "error.source.tab": ["error.indentation.invalid"],
    }
    supersedes_value = selection.get("supersedes", {})
    supersedes = supersedes_value if isinstance(supersedes_value, dict) else {}
    expect(
        supersedes
        == {"default_for_every_error": [], "overrides": expected_supersedes},
        "supersession contract is not exact",
    )
    error_names = set(errors)
    supersession_edges = {
        source: target
        for source, targets in expected_supersedes.items()
        for target in targets
    }
    expect(
        set(supersession_edges) <= error_names
        and set(supersession_edges.values()) <= error_names,
        "supersession references an unregistered error",
    )
    for source, target in supersession_edges.items():
        source_stage = errors.get(source, {}).get("stage")
        target_stage = errors.get(target, {}).get("stage")
        expect(source != target, f"self-supersession: {source}")
        expect(source_stage == target_stage, f"cross-stage supersession: {source}->{target}")
        source_rank = expected_rank_overrides.get(
            source, specificity.get("default_for_every_error")
        )
        target_rank = expected_rank_overrides.get(
            target, specificity.get("default_for_every_error")
        )
        expect(
            isinstance(source_rank, int)
            and isinstance(target_rank, int)
            and source_rank > target_rank,
            f"superseder is not more specific than target: {source}->{target}",
        )

    visiting: set[str] = set()
    visited: set[str] = set()

    def visit(error_name: str) -> bool:
        if error_name in visiting:
            return False
        if error_name in visited:
            return True
        visiting.add(error_name)
        for target in expected_supersedes.get(error_name, []):
            if not visit(target):
                return False
        visiting.remove(error_name)
        visited.add(error_name)
        return True

    expect(all(visit(name) for name in sorted(error_names)), "supersession graph has a cycle")
    resolved_metadata_count = 0
    for error_name, contract in errors.items():
        if not isinstance(contract, dict):
            continue
        severity = selection.get("severity", {}).get("default_for_every_error")
        rank = expected_rank_overrides.get(error_name, specificity.get("default_for_every_error"))
        targets = expected_supersedes.get(error_name, supersedes.get("default_for_every_error"))
        if (
            severity == "error"
            and isinstance(rank, int)
            and rank >= 0
            and isinstance(targets, list)
            and all(target in error_names for target in targets)
        ):
            resolved_metadata_count += 1
    expect(len(errors) == 77, "diagnostic metadata registry does not contain 77 errors")
    expect(resolved_metadata_count == 77, "not every error resolves selection metadata")
    expect(
        selection.get("duplicate_key")
        == [
            "error_identifier",
            "stage",
            "cause_identity",
            "canonical_locus",
            "producer_path",
            "iteration_index",
            "retry_attempt_index",
        ],
        "duplicate key is not exact",
    )
    expect(
        selection.get("stable_order")
        == [
            "stage_order ascending",
            "canonical source byte offset or declared execution-path order ascending",
            "iteration index ascending",
            "retry-attempt index ascending",
            "severity descending",
            "specificity_rank descending",
            "error identifier by Unicode scalar value ascending",
        ],
        "stable diagnostic order is not exact",
    )
    expect(
        selection.get("selection_scope")
        == "one exposed result producer or one source validation run",
        "diagnostic selection scope is not exact",
    )
    selection_tokens = {
        "earliest_stage_rule": ("stage_order", "do not evaluate later stages", "independent"),
        "multiplicity_rule": ("every independent", "Distinct source loci", "retry-attempt"),
        "supersession_rule": (
            "same cause",
            "same canonical locus",
            "iteration index",
            "retry-attempt index",
            "never suppresses",
        ),
        "duplicate_rule": ("duplicate_key", "Merge", "Discovery count"),
        "location_rule": (
            "zero-based",
            "first offending UTF-8 byte offset",
            "zero-width byte position",
            "first following nonblank line",
            "source byte length",
            "error.source.final_line_feed",
            "declaration order",
            "Host completion",
        ),
        "primary_rule": (
            "permitted declared-handler recovery",
            "first unhandled diagnostic",
            "no diagnostic is primary",
        ),
        "secondary_rule": (
            "Every other unhandled",
            "unsuccessful retry attempt",
            "superseded by successful retry or FALLBACK",
            "never controls that successful aggregate result",
            "Merely attempting a retry does not handle",
        ),
    }
    for field, tokens in selection_tokens.items():
        value = selection.get(field, "")
        expect(
            isinstance(value, str) and all(token in value for token in tokens),
            f"{field} lacks required deterministic selection terms",
        )

    stage_indexes = {name: index for index, name in enumerate(expected_stage_order)}

    def candidate(
        error_name: str,
        *,
        cause: str = "cause.same",
        locus: int = 10,
        producer: str = "source.unit",
        iteration: int = 0,
        retry_attempt: int = 0,
    ) -> dict[str, Any]:
        return {
            "error_identifier": error_name,
            "stage": errors.get(error_name, {}).get("stage"),
            "cause_identity": cause,
            "canonical_locus": locus,
            "producer_path": producer,
            "iteration_index": iteration,
            "retry_attempt_index": retry_attempt,
        }

    def supersedes_transitively(source: str, target: str) -> bool:
        pending = list(expected_supersedes.get(source, []))
        seen: set[str] = set()
        while pending:
            current = pending.pop()
            if current == target:
                return True
            if current in seen:
                continue
            seen.add(current)
            pending.extend(expected_supersedes.get(current, []))
        return False

    identity_fields = (
        "stage",
        "cause_identity",
        "canonical_locus",
        "producer_path",
        "iteration_index",
        "retry_attempt_index",
    )
    duplicate_fields = tuple(selection.get("duplicate_key", []))

    def select_diagnostics(candidates: list[dict[str, Any]]) -> list[str]:
        earliest = min(stage_indexes[item["stage"]] for item in candidates)
        staged = [item for item in candidates if stage_indexes[item["stage"]] == earliest]
        retained: list[dict[str, Any]] = []
        for index, item in enumerate(staged):
            item_identity = tuple(item[field] for field in identity_fields)
            suppressed = any(
                other_index != index
                and tuple(other[field] for field in identity_fields) == item_identity
                and supersedes_transitively(
                    other["error_identifier"], item["error_identifier"]
                )
                for other_index, other in enumerate(staged)
            )
            if not suppressed:
                retained.append(item)
        unique: list[dict[str, Any]] = []
        seen_keys: set[tuple[Any, ...]] = set()
        for item in retained:
            key = tuple(item[field] for field in duplicate_fields)
            if key not in seen_keys:
                seen_keys.add(key)
                unique.append(item)
        unique.sort(
            key=lambda item: (
                stage_indexes[item["stage"]],
                item["canonical_locus"],
                item["iteration_index"],
                item["retry_attempt_index"],
                -expected_rank_overrides.get(
                    item["error_identifier"], specificity["default_for_every_error"]
                ),
                item["error_identifier"],
            )
        )
        return [item["error_identifier"] for item in unique]

    selection_vectors = {
        "tab_over_generic_indentation": (
            [candidate("error.source.tab"), candidate("error.indentation.invalid")],
            ["error.source.tab"],
        ),
        "required_over_generic_cardinality": (
            [candidate("error.field.required"), candidate("error.field.cardinality")],
            ["error.field.required"],
        ),
        "duplicate_field_over_cardinality": (
            [candidate("error.field.duplicate"), candidate("error.field.cardinality")],
            ["error.field.duplicate"],
        ),
        "duplicate_block_over_occurrence": (
            [candidate("error.block.duplicate"), candidate("error.block.occurrence")],
            ["error.block.duplicate"],
        ),
        "unclosed_over_generic_literal": (
            [candidate("error.literal.unclosed"), candidate("error.literal.invalid")],
            ["error.literal.unclosed"],
        ),
        "escape_over_generic_literal": (
            [candidate("error.literal.escape"), candidate("error.literal.invalid")],
            ["error.literal.escape"],
        ),
        "same_identifier_distinct_locus": (
            [
                candidate("error.source.bom", locus=10),
                candidate("error.source.bom", locus=20),
            ],
            ["error.source.bom", "error.source.bom"],
        ),
        "same_identifier_distinct_iteration": (
            [
                candidate("error.execution.action", iteration=0),
                candidate("error.execution.action", iteration=1),
            ],
            ["error.execution.action", "error.execution.action"],
        ),
        "same_identifier_distinct_retry": (
            [
                candidate("error.execution.action", retry_attempt=0),
                candidate("error.execution.action", retry_attempt=1),
            ],
            ["error.execution.action", "error.execution.action"],
        ),
        "exact_duplicate_suppression": (
            [candidate("error.source.bom"), candidate("error.source.bom")],
            ["error.source.bom"],
        ),
        "earliest_stage_only": (
            [candidate("error.source.bom"), candidate("error.field.required")],
            ["error.source.bom"],
        ),
        "specificity_before_identifier": (
            [
                candidate("error.source.tab", cause="cause.tab"),
                candidate("error.source.bom", cause="cause.bom"),
            ],
            ["error.source.tab", "error.source.bom"],
        ),
    }
    expect(
        len(selection_vectors) == DIAGNOSTIC_SELECTION_VECTOR_COUNT,
        "diagnostic selection vector inventory differs",
    )
    for name, (inputs, expected) in selection_vectors.items():
        expect(select_diagnostics(inputs) == expected, f"selection vector failed: {name}")

    expect(
        isinstance(excluded_symbols, list)
        and {"=", ";", "'", "#", "%"} <= set(excluded_symbols),
        "excluded-symbol boundary inventory is incomplete",
    )
    expect(
        errors.get("error.lexical.unknown_symbol", {}).get("meaning")
        == "Punctuation outside strings is absent from both the adopted-symbol registry and the explicit excluded-symbol inventory.",
        "unknown-symbol trigger overlaps the excluded-symbol inventory",
    )
    expect(
        errors.get("error.symbol.invalid", {}).get("meaning")
        == "An exact lexeme or notation pattern in the explicit excluded-symbol inventory occurs outside string/data content.",
        "invalid-symbol trigger does not own the excluded-symbol inventory",
    )
    expect(
        "passed byte and character validation"
        in errors.get("error.lexical.malformed_token", {}).get("meaning", ""),
        "malformed-token trigger overlaps source byte/character validation",
    )
    expect(
        "other than an unclosed string or an invalid string escape"
        in errors.get("error.literal.invalid", {}).get("meaning", ""),
        "generic literal trigger overlaps specific literal diagnostics",
    )

    prose_tokens = {
        "01_FOUNDATION/03_NORMATIVE_PROCESSING_MODEL.txt": (
            "closed stage order",
            "grammar_or_schema",
            "verification_or_completion",
            "Discovery time",
        ),
        "06_STANDARD_LIBRARY/07_CORE_ERROR_IDENTIFIERS_PART_2.txt": (
            "one diagnostic severity, error",
            "first unhandled diagnostic in",
            "every other unhandled diagnostic is secondary",
            "retry or FALLBACK completion remain ordered local evidence",
            "at a distinct locus",
        ),
    }
    for relative, tokens in prose_tokens.items():
        text = (root / relative).read_text(encoding="utf-8")
        expect(all(token in text for token in tokens), f"{relative} lacks selection parity")
    return violations


def failure_lifecycle_contract_violations(
    root: Path,
    statuses: dict[str, Any],
    operations: dict[str, Any],
    groups_and_results: dict[str, Any],
) -> list[str]:
    violations: list[str] = []

    def expect(condition: bool, message: str) -> None:
        if not condition:
            violations.append(message)

    errors_value = statuses.get("errors", {})
    errors = errors_value if isinstance(errors_value, dict) else {}
    lifecycle_value = statuses.get("failure_lifecycle", {})
    lifecycle = lifecycle_value if isinstance(lifecycle_value, dict) else {}
    expect(
        canonical_contract_fingerprint(lifecycle)
        == EXPECTED_FAILURE_LIFECYCLE_FINGERPRINT,
        "failure-lifecycle policy fingerprint differs",
    )
    expect(
        set(lifecycle)
        == {
            "phase_scope",
            "error_phase_resolution",
            "baseline_error_profiles",
            "additional_mixed_error_profiles",
            "status_rule",
            "effect_state_rule",
            "output_binding_rule",
            "retry_safety",
            "evidence_requirements",
            "indeterminate_state_rule",
            "terminal_invocation_rule",
            "failure_mapping_rule",
        },
        "failure_lifecycle fields differ",
    )
    expected_defaults = {
        "lexical": ["pre_effect"],
        "grammar_or_schema": ["pre_effect"],
        "resolution": ["pre_effect"],
        "static_or_expression": ["pre_effect"],
        "validation": ["pre_effect"],
        "execution": ["pre_effect", "post_effect", "indeterminate"],
        "verification_or_completion": ["pre_effect", "post_effect", "indeterminate"],
    }
    expected_overrides = {
        "error.dependency.unsatisfied": ["pre_effect"],
        "error.scope.violation": ["pre_effect"],
        "error.value.unknown": ["pre_effect", "post_effect", "indeterminate"],
    }
    resolution_value = lifecycle.get("error_phase_resolution", {})
    resolution = resolution_value if isinstance(resolution_value, dict) else {}
    expect(
        set(resolution) == {"defaults_by_stage", "overrides", "resolution_rule"},
        "error_phase_resolution fields differ",
    )
    expect(resolution.get("defaults_by_stage") == expected_defaults, "phase defaults differ")
    expect(resolution.get("overrides") == expected_overrides, "phase overrides differ")
    resolution_rule = resolution.get("resolution_rule", "")
    expect(
        isinstance(resolution_rule, str)
        and all(
            token in resolution_rule
            for token in (
                "registered stage",
                "exact identifier override",
                "every registered error",
                "none is not an allowed failure phase",
            )
        ),
        "phase resolution rule is incomplete",
    )
    allowed_phase_values = {"pre_effect", "post_effect", "indeterminate"}

    def resolved_phases(error_name: str) -> list[str]:
        contract = errors.get(error_name, {})
        return expected_overrides.get(
            error_name, expected_defaults.get(contract.get("stage"), [])
        )

    resolved_phase_count = 0
    for error_name, contract in errors.items():
        if not isinstance(contract, dict):
            continue
        phases = resolved_phases(error_name)
        if phases and set(phases) <= allowed_phase_values and "none" not in phases:
            resolved_phase_count += 1
    expect(len(errors) == 77, "failure metadata registry does not contain 77 errors")
    expect(resolved_phase_count == 77, "not every error resolves allowed failure phases")
    expected_baseline = {
        "error.dependency.unsatisfied",
        "error.execution.order",
        "error.required.missing",
        "error.scope.violation",
    }
    baseline_value = lifecycle.get("baseline_error_profiles", {})
    baseline = baseline_value if isinstance(baseline_value, dict) else {}
    expect(set(baseline) == expected_baseline, "baseline four lifecycle profiles differ")
    expected_baseline_statuses = {
        "error.dependency.unsatisfied": "status.blocked",
        "error.execution.order": "status.failed",
        "error.required.missing": "status.blocked",
        "error.scope.violation": "status.failed",
    }
    expect(
        {
            name: errors.get(name, {}).get("default_status")
            for name in expected_baseline_statuses
        }
        == expected_baseline_statuses,
        "baseline lifecycle default statuses differ",
    )
    expect(
        resolved_phases("error.dependency.unsatisfied") == ["pre_effect"]
        and resolved_phases("error.scope.violation") == ["pre_effect"]
        and resolved_phases("error.execution.order")
        == ["pre_effect", "post_effect", "indeterminate"]
        and resolved_phases("error.required.missing")
        == ["pre_effect", "post_effect", "indeterminate"],
        "baseline lifecycle allowed phases differ",
    )
    baseline_tokens = {
        "error.dependency.unsatisfied": ("pre_effect only", "processing step 7"),
        "error.execution.order": ("pre_effect", "post_effect", "indeterminate"),
        "error.required.missing": ("pre_effect", "post_effect", "indeterminate"),
        "error.scope.violation": ("pre_effect only", "processing step 6"),
    }
    for error_name, tokens in baseline_tokens.items():
        value = baseline.get(error_name, "")
        expect(
            isinstance(value, str) and all(token in value for token in tokens),
            f"{error_name} lifecycle profile is incomplete",
        )
    expected_additional = {
        "error.cancelled",
        "error.evidence.missing",
        "error.execution.action",
        "error.host.constraint",
        "error.operation.postcondition",
        "error.operation.precondition",
        "error.permission.denied",
        "error.retry.exhausted",
        "error.success.unsatisfied",
        "error.value.unknown",
        "error.verification.failed",
    }
    additional_value = lifecycle.get("additional_mixed_error_profiles", {})
    additional = additional_value if isinstance(additional_value, dict) else {}
    expect(set(additional) == expected_additional, "additional mixed-phase set differs")
    expect(
        all(
            resolved_phases(error_name)
            == ["pre_effect", "post_effect", "indeterminate"]
            for error_name in expected_additional
        ),
        "an additional mixed-phase error lacks the exact three allowed phases",
    )
    expect(
        all(isinstance(value, str) and len(value) >= 80 for value in additional.values()),
        "an additional mixed-phase profile is not substantive",
    )
    lifecycle_tokens = {
        "phase_scope": ("exposed result producer", "retry attempt", "aggregate producer"),
        "status_rule": (
            "primary unhandled diagnostic",
            "secondary diagnostics and FAILURE mappings never override",
            "first remaining diagnostic",
            "every applicable diagnostic is recovered",
            "failure_phase never changes status",
        ),
        "effect_state_rule": ("pre_effect", "post_effect", "indeterminate"),
        "output_binding_rule": ("pre_effect leaves OUTPUT unbound", "Partial OUTPUT", "independently proven"),
        "indeterminate_state_rule": ("Preserve every known observation", "no retry", "exact retained evidence"),
    }
    for field, tokens in lifecycle_tokens.items():
        value = lifecycle.get(field, "")
        expect(
            isinstance(value, str) and all(token in value for token in tokens),
            f"{field} lacks required lifecycle terms",
        )
    for field, tokens in {
        "terminal_invocation_rule": ("status.blocked", "terminal outcome", "no outgoing transition",
                                     "status.ready to status.skipped", "forbidden for an execution root"),
        "failure_mapping_rule": ("no primary unhandled diagnostic", "first whose WHEN is TRUE",
                                 "does not emit a diagnostic or event", "without adding generic error.success.unsatisfied",
                                 "Execution roots cannot select status.skipped"),
    }.items():
        expect(all(token in lifecycle.get(field, "") for token in tokens),
               f"{field} loses the terminal outcome or failure-mapping boundary")
    status_rows = statuses.get("statuses", {})
    expect(status_rows.get("status.blocked", {}).get("terminal") is True
           and status_rows.get("status.blocked", {}).get("allowed_next") == []
           and "status.skipped" in status_rows.get("status.ready", {}).get("allowed_next", []),
           "blocked terminality or explicit non-root ready-to-skipped transition differs")
    demand = statuses.get("diagnostic_selection", {}).get("expression_demand_resolution", {})
    expect(all(token in resolution_rule for token in
               ("expression_demand_resolution", "resolved execution stage", "local failure phases")),
           "dynamic-demand phase lookup does not precede registered-stage fallback")
    expect(demand.get("resolved_stage") == "execution"
           and resolution.get("defaults_by_stage", {}).get(demand.get("resolved_stage"))
           == ["pre_effect", "post_effect", "indeterminate"]
           and resolved_phases("error.numeric.division_by_zero") == ["pre_effect"]
           and errors.get("error.numeric.division_by_zero", {}).get("default_status") == "status.invalid",
           "dynamic-demand phases do not retain the separate static invalid-source contract")
    retry_value = lifecycle.get("retry_safety", {})
    retry = retry_value if isinstance(retry_value, dict) else {}
    expect(
        set(retry)
        == {
            "eligibility_rule",
            "attempt_bound_rule",
            "condition_delay_rule",
            "pre_effect_rule",
            "known_effect_rule",
            "indeterminate_rule",
            "safety_error_mapping",
            "exhaustion_rule",
            "evidence_rule",
        },
        "retry_safety fields differ",
    )
    expect(
        retry.get("safety_error_mapping")
        == {
            "missing proof": "error.required.missing",
            "UNKNOWN proof or condition": "error.value.unknown",
            "proved unsafe": "error.operation.precondition",
        },
        "retry safety error mapping differs",
    )
    retry_tokens = {
        "eligibility_rule": ("Every ACTION", "syntactically eligible", "adds no authority"),
        "attempt_bound_rule": ("at most 1 + LIMIT", "stops on its first success", "does not reset",
                               "separate loop instances", "must equal RETRY.LIMIT"),
        "condition_delay_rule": ("only source of WHEN and DELAY", "Omitted WHEN is TRUE",
                                  "DURATION(0, unit.second)", "preserves the previous failure without exhaustion",
                                  "establish safety before applying DELAY"),
        "pre_effect_rule": ("pre_effect", "effect_state none", "declared retry count"),
        "known_effect_rule": ("applied or partial", "exact evidence", "non-idempotent"),
        "indeterminate_rule": ("prohibits another attempt", "exact reconciliation"),
        "exhaustion_rule": ("exactly 1 + RETRY.LIMIT", "actually made and all failed",
                            "FALSE WHEN before budget consumption", "does not satisfy exhaustion"),
        "evidence_rule": ("attempt-index order", "does not erase", "folds phase"),
    }
    for field, tokens in retry_tokens.items():
        value = retry.get(field, "")
        expect(
            isinstance(value, str) and all(token in value for token in tokens),
            f"retry_safety.{field} lacks required terms",
        )
    evidence_value = lifecycle.get("evidence_requirements", {})
    evidence = evidence_value if isinstance(evidence_value, dict) else {}
    expect(set(evidence) == {"common"} | expected_baseline, "evidence requirement keys differ")
    expected_common_evidence = [
        "canonical error identifier, registered stage, and resolved expression-demand stage when applicable",
        "producer declaration or invocation path",
        "canonical source locus or declared execution event",
        "iteration and retry-attempt indexes when applicable",
        "observations supporting failure_phase",
        "observations supporting effect_state and every observed_effects record",
        "observations supporting OUTPUT binding state",
    ]
    expect(
        evidence.get("common") == expected_common_evidence,
        "common lifecycle evidence is not the exact seven-part contract",
    )
    expected_baseline_evidence = {
        "error.dependency.unsatisfied": [
            "dependency identifier",
            "resolved FALSE, MISSING, or UNKNOWN state before effects",
        ],
        "error.execution.order": [
            "violated or cyclic ordering edge",
            "ordered child results preceding detection",
        ],
        "error.required.missing": [
            "required item identifier",
            "point at which absence became determinable",
        ],
        "error.scope.violation": [
            "resolved target",
            "effective scope",
            "failed containment or membership relation before effects",
        ],
    }
    expect(
        {name: evidence.get(name) for name in expected_baseline_evidence}
        == expected_baseline_evidence,
        "baseline lifecycle evidence differs from the exact contract",
    )
    retry_operation = operations.get("core.retry", {})
    retry_error_names = set(retry_operation.get("errors", []))
    safety_error_names = set(retry.get("safety_error_mapping", {}).values())
    expect(
        isinstance(retry_operation, dict)
        and safety_error_names
        == {
            "error.required.missing",
            "error.value.unknown",
            "error.operation.precondition",
        }
        and safety_error_names <= set(errors)
        and safety_error_names <= retry_error_names
        and "safety-blocked unmade attempt is not exhaustion"
        in retry_operation.get("error_resolution", ""),
        "core.retry does not close the lifecycle safety mapping",
    )

    result_contract_value = groups_and_results.get("result_contract", {})
    result_contract = (
        result_contract_value if isinstance(result_contract_value, dict) else {}
    )
    common_fields_value = result_contract.get("common_fields", {})
    common_fields = common_fields_value if isinstance(common_fields_value, dict) else {}
    expected_common_fields = {
        "status": {
            "type": "qualified_identifier(status)",
            "cardinality": "exactly_one",
        },
        "output_binding": {
            "type": "ENUM[not_requested|unbound|bound|partial]",
            "cardinality": "exactly_one",
        },
        "execution_errors": {
            "type": "LIST[qualified_identifier(error)]",
            "cardinality": "exactly_one",
        },
        "failure_phase": {
            "type": "ENUM[none|pre_effect|post_effect|indeterminate]",
            "cardinality": "exactly_one",
        },
        "effect_state": {
            "type": "ENUM[none|applied|partial|indeterminate]",
            "cardinality": "exactly_one",
        },
        "observed_effects": {
            "type": "LIST[result.effect]",
            "cardinality": "exactly_one",
        },
    }
    expect(
        common_fields == expected_common_fields,
        "lifecycle axes differ from the exact common result fields",
    )
    result_schemas_value = groups_and_results.get("result_schemas", {})
    result_schemas = (
        result_schemas_value if isinstance(result_schemas_value, dict) else {}
    )
    partial_output_profiles = {
        name: schema.get("partial_output")
        for name, schema in result_schemas.items()
        if isinstance(schema, dict)
    }
    expect(
        partial_output_profiles.get("result.command")
        == {"supported": True, "fields": ["stdout", "stderr"]}
        and all(
            profile == {"supported": False, "fields": []}
            for name, profile in partial_output_profiles.items()
            if name != "result.command"
        )
        and len(partial_output_profiles) == 9,
        "lifecycle partial-OUTPUT rule differs from the nine result schemas",
    )
    prose_tokens = {
        "05_SEMANTICS/09_VALIDATION_EXECUTION_FAILURE_AND_TERMINATION.txt": (
            "failure_phase is measured",
            "exposed result producer",
            "error.dependency.unsatisfied and error.scope.violation are pre_effect only",
            "The other mixed-phase identifiers",
            "safety-blocked unmade attempt is not exhaustion",
        ),
        "06_STANDARD_LIBRARY/07_CORE_ERROR_IDENTIFIERS_PART_2.txt": (
            "failure_phase is relative to the exposed result producer",
            "indeterminate effects",
            "prohibit another attempt",
            "does not constitute",
            "retry exhaustion",
        ),
    }
    for relative, tokens in prose_tokens.items():
        text = (root / relative).read_text(encoding="utf-8")
        expect(all(token in text for token in tokens), f"{relative} lacks lifecycle parity")
    return violations


CUSTOM_OPERATION_RETIRED_CLAIMS = (
    "Every custom kind.operation selects exactly one profile",
    "custom kind.operation requires the implementation role",
    "custom kind.operation requires exactly the implementation role",
    "Every custom kind.operation also selects exactly one such profile",
    "Every custom kind.operation also resolves through exactly one such profile",
    "custom kind.operation has exactly the implementation role",
    "SIDE_EFFECT TRUE",
)


def custom_operation_declaration_violations(
    root: Path,
    fields: dict[str, Any],
    blocks: dict[str, Any],
    operation_registry: dict[str, Any],
    groups_and_results: dict[str, Any],
) -> tuple[list[str], int]:
    """Verify that a custom kind.operation can express every axis the language demands.

    This is a static declaration check. It proves that the closed DEFINE surface admits
    the effect and dependency vocabularies, that no normative surface still requires a
    custom operation to select an implementation profile, and that every shipped
    kind.operation definition declares axis values inside the closed vocabularies. It
    does not parse LCL and therefore claims no coverage of full document semantics.
    """
    violations: list[str] = []
    enum_groups = groups_and_results.get("enum_groups", {})
    effect_classes = set(enum_groups.get("effect_classes", []))
    dependency_classes = set(enum_groups.get("dependency_classes", []))
    if not effect_classes or not dependency_classes:
        violations.append("closed effect or dependency vocabulary is unavailable")

    # 1. The DEFINE surface must admit both axis declarations.
    define_fields = fields.get("blocks", {}).get("DEFINE", {}).get("fields", {})
    define_schema = blocks.get("DEFINE", {})
    for field_name, expected_kind in (
        ("SIDE_EFFECT", "side_effect_declaration"),
        ("DEPENDENCY", "dependency_class_list"),
    ):
        actual = define_fields.get(field_name, {}).get("value_kind")
        if actual != expected_kind:
            violations.append(
                f"DEFINE.{field_name} value_kind is {actual!r}, expected {expected_kind!r}"
            )
        if field_name not in set(define_schema.get("optional", [])) | set(
            define_schema.get("required", [])
        ):
            violations.append(f"DEFINE block schema does not admit {field_name}")
    named_kinds = fields.get("value_kind_registry", {})
    for kind_name, vocabulary in (
        ("side_effect_declaration", "effect_classes"),
        ("dependency_class_list", "dependency_classes"),
    ):
        definition = named_kinds.get(kind_name)
        if not isinstance(definition, str) or vocabulary not in definition:
            violations.append(
                f"value kind {kind_name} does not resolve against the closed {vocabulary}"
            )

    # 2. No normative surface may still oblige a custom operation to select a profile.
    axis_contract = operation_registry.get("axis_contract", {})
    custom_rule = axis_contract.get("custom_operation_resolution")
    if not isinstance(custom_rule, str):
        violations.append("axis_contract.custom_operation_resolution is absent")
    else:
        for token in ("SIDE_EFFECT", "DEPENDENCY", "DETERMINISTIC", "selects no implementation profile"):
            if token not in custom_rule:
                violations.append(
                    f"custom_operation_resolution does not govern {token}"
                )
    profile_contract = axis_contract.get("implementation_profile", {})
    if "custom_operation_resolution" in profile_contract:
        violations.append(
            "custom_operation_resolution is still nested under implementation_profile"
        )
    for relative_path in (
        "03_TYPES_AND_VALUES/05_CUSTOM_TYPES_SCHEMAS_AND_DEFINITIONS.txt",
        "05_SEMANTICS/11_DETERMINISM_EQUIVALENCE_AND_INTERPRETER_VARIATION.txt",
        "06_STANDARD_LIBRARY/06_CORE_ERROR_IDENTIFIERS_PART_1.txt",
        "06_STANDARD_LIBRARY/10_CORE_OPERATION_PARAMETER_RULES.txt",
        "10_REGISTRIES/operations_v0.1.0.json",
        "09_CONFORMANCE/CASES/core_conformance_cases_v0.1.0.json",
    ):
        text = (root / relative_path).read_text(encoding="utf-8")
        for claim in CUSTOM_OPERATION_RETIRED_CLAIMS:
            if claim in text:
                violations.append(
                    f"{relative_path} still asserts the retired custom-operation claim {claim!r}"
                )

    # 3. Every shipped kind.operation definition declares axes inside the closed vocabularies.
    checked_definitions = 0
    for path in sorted((root / "08_EXAMPLES/VALID").glob("*.lcl")):
        lines = path.read_text(encoding="utf-8").split("\n")
        for index, line in enumerate(lines):
            if line.strip() != "KIND: kind.operation":
                continue
            checked_definitions += 1
            declared: dict[str, str] = {}
            for candidate in lines[index + 1 :]:
                if candidate and not candidate.startswith(" "):
                    break
                stripped = candidate.strip()
                for field_name in ("SIDE_EFFECT", "DEPENDENCY", "DETERMINISTIC"):
                    prefix = f"{field_name}: "
                    if stripped.startswith(prefix):
                        declared[field_name] = stripped[len(prefix) :]
            location = f"{path.name} definition {checked_definitions}"
            side_effect = declared.get("SIDE_EFFECT")
            if side_effect is None:
                violations.append(f"{location} omits required SIDE_EFFECT")
            elif side_effect == "TRUE":
                violations.append(
                    f"{location} uses retired bare SIDE_EFFECT TRUE without effect classes"
                )
            elif side_effect != "FALSE":
                if not (side_effect.startswith("[") and side_effect.endswith("]")):
                    violations.append(f"{location} SIDE_EFFECT is not FALSE or an effect LIST")
                else:
                    members = [item.strip() for item in side_effect[1:-1].split(",") if item.strip()]
                    if not members:
                        violations.append(f"{location} declares an empty effect LIST")
                    if "none" in members:
                        violations.append(f"{location} declares none among concrete effect classes")
                    if len(members) != len(set(members)):
                        violations.append(f"{location} repeats an effect class")
                    unknown = [item for item in members if item not in effect_classes]
                    if unknown:
                        violations.append(f"{location} declares unregistered effect classes {unknown}")
            dependency = declared.get("DEPENDENCY")
            if dependency is not None:
                if not (dependency.startswith("[") and dependency.endswith("]")):
                    violations.append(f"{location} DEPENDENCY is not a class LIST")
                else:
                    members = [item.strip() for item in dependency[1:-1].split(",") if item.strip()]
                    if not members:
                        violations.append(f"{location} declares an empty dependency LIST")
                    if len(members) != len(set(members)):
                        violations.append(f"{location} repeats a dependency class")
                    unknown = [item for item in members if item not in dependency_classes]
                    if unknown:
                        violations.append(
                            f"{location} declares unregistered dependency classes {unknown}"
                        )
                    if "declared_state_only" in members and len(members) > 1:
                        violations.append(
                            f"{location} combines declared_state_only with another class"
                        )
            if declared.get("DETERMINISTIC") not in {"TRUE", "FALSE"}:
                violations.append(f"{location} omits a BOOLEAN DETERMINISTIC declaration")
    if checked_definitions == 0:
        violations.append("no shipped kind.operation definition was checked")
    return violations, checked_definitions


def event_model_violations(
    root: Path,
    statuses: dict[str, Any],
    groups_and_results: dict[str, Any],
    formats_registry: dict[str, Any],
    operations: dict[str, Any],
) -> tuple[list[str], int]:
    """Verify the closed event model that makes declared-handler activation computable.

    This is a static contract check. It computes the diagnostic-to-event mapping invariants
    directly from the registries instead of comparing against a duplicated literal, proves the
    event vocabulary is identical on every surface, and resolves every shipped HANDLER event
    against the operations that handler can guard. It does not parse or execute LCL; the
    reachability test uses operation identifiers declared outside HANDLER blocks, because the
    non-reentrancy rule stops handler-side diagnostics from raising events.
    """
    violations: list[str] = []
    errors = statuses.get("errors", {})
    model = statuses.get("event_model", {})

    for key in (
        "event_source", "diagnostic_event_mapping", "mapping_invariant", "raise_rule",
        "attachment_sites", "handler_scope", "selection_order", "match_rule",
        "multiplicity_rule", "no_match_rule", "recovery_rule", "non_reentrancy_rule",
        "status_interaction",
    ):
        value = model.get(key)
        if not isinstance(value, str) or not value.strip():
            violations.append(f"event_model.{key} is absent or empty")

    # 1. The mapping must be total over the closed error registry.
    absent = sorted(name for name, entry in errors.items() if "event" not in entry)
    if absent:
        violations.append(f"errors carrying no event field: {absent}")

    # 2. event is non-null exactly when the error is handler-recoverable.
    for name, entry in errors.items():
        if "event" not in entry:
            continue
        raises = entry["event"] is not None
        recoverable = bool(entry.get("recoverable_with_declared_handler"))
        if raises != recoverable:
            violations.append(
                f"{name} declares event={entry['event']!r} but "
                f"recoverable_with_declared_handler={recoverable}"
            )

    # 3. The event vocabulary must be identical on every surface.
    group_events = list(groups_and_results.get("enum_groups", {}).get("events", []))
    builtin_events = list(formats_registry.get("builtins", {}).get("events", []))
    if group_events != builtin_events:
        violations.append("event vocabulary differs between the two closed registries")
    prose = (root / "06_STANDARD_LIBRARY/08_BUILT_IN_IDENTIFIER_GROUPS.txt").read_text(
        encoding="utf-8"
    )
    section = prose.split("EVENTS", 1)[-1].split("RESERVED_NAMESPACES", 1)[0]
    prose_events = [
        line.strip() for line in section.split("\n") if line.strip().startswith("event.")
    ]
    if prose_events != group_events:
        violations.append(
            f"06_STANDARD_LIBRARY/08 event list {prose_events} differs from the registry"
        )

    # 4. The mapping must be a bijection onto the registered vocabulary.
    image = [entry["event"] for entry in errors.values() if entry.get("event")]
    if len(image) != len(set(image)):
        violations.append("two registered errors map to the same event")
    if set(image) != set(group_events):
        violations.append(
            f"event image {sorted(set(image))} is not exactly the registered "
            f"vocabulary {sorted(group_events)}"
        )

    # 5. Every shipped HANDLER must name a registered event its own document can raise.
    checked_handlers = 0
    for path in sorted((root / "08_EXAMPLES/VALID").glob("*.lcl")):
        lines = path.read_text(encoding="utf-8").split("\n")
        # Operations a handler guards exclude handler-side operations: by the
        # non-reentrancy rule, diagnostics raised while executing a handler
        # invocation or its FALLBACK raise no event.
        named_operations: set[str] = set()
        current_block = ""
        for line in lines:
            if line and not line.startswith(" ") and line.endswith(":"):
                current_block = line[:-1]
            elif line.strip().startswith("OPERATION: ") and current_block != "HANDLER":
                named_operations.add(line.strip()[len("OPERATION: "):])
        raisable: set[str] = set()
        for operation_name in named_operations:
            for error_name in operations.get(operation_name, {}).get("errors", []):
                event = errors.get(error_name, {}).get("event")
                if event:
                    raisable.add(event)
        for index, line in enumerate(lines):
            if line != "HANDLER:":
                continue
            checked_handlers += 1
            declared_event = None
            declared_id = str(checked_handlers)
            for candidate in lines[index + 1:]:
                if candidate and not candidate.startswith(" "):
                    break
                stripped = candidate.strip()
                if stripped.startswith("EVENT: "):
                    declared_event = stripped[len("EVENT: "):]
                elif stripped.startswith("ID: "):
                    declared_id = stripped[len("ID: "):]
            location = f"{path.name} HANDLER {declared_id}"
            if declared_event is None:
                violations.append(f"{location} omits required EVENT")
            elif declared_event not in group_events:
                violations.append(f"{location} names unregistered event {declared_event}")
            elif declared_event not in raisable:
                violations.append(
                    f"{location} names {declared_event}, which no operation named by this "
                    f"document can raise; raisable events are {sorted(raisable)}"
                )
    if checked_handlers == 0:
        violations.append("no shipped HANDLER declaration was checked")
    return violations, checked_handlers


HANDLER_CONTEXT_TARGET_TYPES = ("REFERENCE[ACTION]", "REFERENCE[meta.execution_unit]")


def invocation_site_contract_violations(
    root: Path,
    fields: dict[str, Any],
    blocks: dict[str, Any],
    operations: dict[str, Any],
    statuses: dict[str, Any],
) -> tuple[list[str], int]:
    """Verify that ACTION, HANDLER, and FALLBACK operation contracts are total.

    This is a static contract check. It proves that the invocation-site rule is stated
    on every authoritative surface, that HANDLER carries the fields its rule requires,
    and that every shipped HANDLER declaration can satisfy the contract of the operation
    it names, including FALLBACK. It does not parse or execute LCL documents.
    """
    violations: list[str] = []

    # 1. The widened invocation-site rule must be stated on every authoritative surface.
    parameter_error = statuses.get("errors", {}).get("error.operation.parameter", {})
    meaning = parameter_error.get("meaning", "")
    for token in ("ACTION, HANDLER, or FALLBACK invocation site", "handler-context binding"):
        if token not in meaning:
            violations.append(f"error.operation.parameter meaning omits {token!r}")
    surfaces = {
        "06_STANDARD_LIBRARY/10_CORE_OPERATION_PARAMETER_RULES.txt": (
            "ACTION, HANDLER,",
            "are the only invocation sites",
        ),
        "05_SEMANTICS/06_MISSING_UNKNOWN_NULL_DEFAULT_ASSUME_AND_HANDLER_RESOLUTION.txt": (
            "A HANDLER is an invocation site",
            "handler-context binding",
            "FALLBACK is eligible only after",
        ),
        "05_SEMANTICS/08_PHASE_SEQUENCE_STEP_BRANCH_LOOP_RETRY_AND_CONCURRENCY.txt": (
            "only source of attempt bounds",
            "Total attempts are at most 1 + LIMIT",
            "opens a second, independent attempt budget and never multiplies attempts",
        ),
    }
    for relative_path, tokens in surfaces.items():
        text = (root / relative_path).read_text(encoding="utf-8")
        for token in tokens:
            if token not in text:
                violations.append(f"{relative_path} omits invocation-site prose {token!r}")

    # 2. HANDLER must carry the fields its normative rule requires.
    handler_fields = fields.get("blocks", {}).get("HANDLER", {}).get("fields", {})
    for field_name in ("OPERATION", "TARGET", "PARAMETER", "FALLBACK", "LIMIT"):
        if field_name not in handler_fields:
            violations.append(f"HANDLER does not carry {field_name}")
    handler_rules = blocks.get("HANDLER", {}).get("rules", [])
    if not any("invocation site" in rule for rule in handler_rules):
        violations.append("HANDLER block schema does not state the invocation-site rule")
    if not any("handler-context binding" in rule for rule in handler_rules):
        violations.append("HANDLER block schema does not state the target binding rule")
    if not any(rule.startswith("FALLBACK admits") for rule in handler_rules):
        violations.append("HANDLER block schema does not bound FALLBACK")
    retry_rules = blocks.get("RETRY", {}).get("rules", [])
    if not any("only source of attempt bounds" in rule for rule in retry_rules):
        violations.append("RETRY block schema does not state the attempt-bound rule")

    # 3. core.retry must bind its composition rule to the declared RETRY block.
    retry_contract = operations.get("core.retry", {})
    if "RETRY block is the only attempt bound" not in str(
        retry_contract.get("invocation_resolution", "")
    ):
        violations.append("core.retry invocation rule omits the RETRY composition bound")
    if not any(
        "RETRY block whose LIMIT equals the resolved limit" in precondition
        for precondition in retry_contract.get("preconditions", [])
    ):
        violations.append("core.retry does not require limit equality with the RETRY block")

    # 4. Every shipped HANDLER declaration must satisfy the contract it names.
    checked_handlers = 0
    for path in sorted((root / "08_EXAMPLES/VALID").glob("*.lcl")):
        lines = path.read_text(encoding="utf-8").split("\n")
        for index, line in enumerate(lines):
            if line != "HANDLER:":
                continue
            checked_handlers += 1
            declared: dict[str, str] = {}
            parameter_names: list[str] = []
            for candidate in lines[index + 1 :]:
                if candidate and not candidate.startswith(" "):
                    break
                stripped = candidate.strip()
                if stripped.startswith("NAME: "):
                    parameter_names.append(stripped[len("NAME: ") :])
                    continue
                for field_name in ("ID", "OPERATION", "TARGET", "FALLBACK", "LIMIT"):
                    prefix = f"{field_name}: "
                    if stripped.startswith(prefix):
                        declared.setdefault(field_name, stripped[len(prefix) :])
            location = f"{path.name} HANDLER {declared.get('ID', checked_handlers)}"
            operation_name = declared.get("OPERATION")
            contract = operations.get(operation_name)
            if operation_name is None:
                violations.append(f"{location} omits required OPERATION")
                continue
            if contract is None:
                if operation_name.startswith("core."):
                    violations.append(f"{location} names unregistered core operation {operation_name}")
                continue
            target_contract = contract.get("target", {})
            if target_contract.get("required") and "TARGET" not in declared:
                if target_contract.get("type") not in HANDLER_CONTEXT_TARGET_TYPES and not any(
                    admitted in str(target_contract.get("type", "")).split("|")
                    for admitted in HANDLER_CONTEXT_TARGET_TYPES
                ):
                    violations.append(
                        f"{location} omits a required TARGET that no handler-context binding supplies"
                    )
            registered_parameters = contract.get("parameters", {})
            if "LIMIT" in declared and "limit" not in registered_parameters:
                violations.append(f"{location} supplies LIMIT to an operation without a limit parameter")
            if "LIMIT" in declared and "limit" in parameter_names:
                violations.append(f"{location} duplicates limit through LIMIT and PARAMETER")
            supplied = set(parameter_names) | ({"limit"} if "LIMIT" in declared else set())
            missing_required = [
                name
                for name, parameter in registered_parameters.items()
                if parameter.get("required") and name not in supplied
            ]
            if missing_required:
                violations.append(f"{location} omits required parameters {missing_required}")
            unregistered = [name for name in parameter_names if name not in registered_parameters]
            if unregistered:
                violations.append(f"{location} supplies unregistered parameters {unregistered}")
            fallback = declared.get("FALLBACK")
            if fallback is not None and not fallback.startswith("REF("):
                fallback_contract = operations.get(fallback)
                if fallback_contract is None:
                    if fallback.startswith("core."):
                        violations.append(f"{location} FALLBACK names unregistered {fallback}")
                else:
                    required_parameters = [
                        name
                        for name, parameter in fallback_contract.get("parameters", {}).items()
                        if parameter.get("required")
                    ]
                    if required_parameters:
                        violations.append(
                            f"{location} FALLBACK {fallback} requires named parameters "
                            f"{required_parameters} it cannot supply"
                        )
                    fallback_target = fallback_contract.get("target", {})
                    if fallback_target.get("required") and not any(
                        admitted in str(fallback_target.get("type", "")).split("|")
                        for admitted in HANDLER_CONTEXT_TARGET_TYPES
                    ):
                        violations.append(
                            f"{location} FALLBACK {fallback} requires a TARGET no binding supplies"
                        )
    if checked_handlers == 0:
        violations.append("no shipped HANDLER declaration was checked")
    return violations, checked_handlers


CONTRACT_NOTATION_REQUIRED_KEYS = (
    "purpose",
    "closed",
    "applies_to",
    "separation_from_source_syntax",
    "grammar",
    "constructor_keywords",
    "atom_classes",
    "lexical_rules",
    "union_rules",
    "nesting",
    "omitted_parameter",
    "type_variable_binding",
    "invalid_forms",
    "defect_rule",
)

LANGUAGE_LEVEL_ATOM_CLASSES = frozenset(
    {"SCALAR_TYPE_NAME", "SPECIAL_VALUE_NAME", "BLOCK_NAME"}
)
SPECIFICATION_LEVEL_ATOM_CLASSES = frozenset(
    {"META_TYPE_NAME", "NAMED_VALUE_KIND", "RESULT_RECORD_NAME"}
)


class ContractTypeSyntaxError(Exception):
    """One contract type string violates the declared notation."""


def resolve_atom_vocabulary(root: Path, record: Any) -> set[str]:
    """Resolve one atom class against the source, pointer, and selection it declares."""
    if not isinstance(record, dict):
        raise ContractTypeSyntaxError("atom class record is not an object")
    source = record.get("source")
    if source == "inline":
        return set()
    pointer = record.get("pointer")
    selection = record.get("selection")
    if not isinstance(source, str) or not isinstance(pointer, str):
        raise ContractTypeSyntaxError(f"atom class declares no resolvable source: {record!r}")
    document = load_json_strict(root / source)
    node: Any = document
    for segment in [part for part in pointer.split("/") if part]:
        if not isinstance(node, dict) or segment not in node:
            raise ContractTypeSyntaxError(f"pointer {pointer} does not resolve in {source}")
        node = node[segment]
    if selection == "object_keys":
        if not isinstance(node, dict):
            raise ContractTypeSyntaxError(f"pointer {pointer} is not an object")
        return set(node)
    if selection == "object_keys_without_type_argument":
        if not isinstance(node, dict):
            raise ContractTypeSyntaxError(f"pointer {pointer} is not an object")
        return {key for key in node if "[" not in key}
    raise ContractTypeSyntaxError(f"unsupported atom selection {selection!r}")


class ContractTypeParser:
    """Recursive-descent parser for the declared contract type notation."""

    def __init__(
        self,
        text: str,
        vocabularies: dict[str, set[str]],
        maximum_depth: int,
        registered_variables: set[str],
    ):
        self.text = text
        self.position = 0
        self.vocabularies = vocabularies
        self.maximum_depth = maximum_depth
        self.registered_variables = registered_variables
        self.atom_uses: list[tuple[str, str]] = []
        self.observed_depth = 0

    def peek(self) -> str:
        return self.text[self.position] if self.position < len(self.text) else ""

    def take(self, character: str) -> None:
        if self.peek() != character:
            raise ContractTypeSyntaxError(
                f"expected {character!r} at offset {self.position} in {self.text!r}"
            )
        self.position += 1

    def word(self) -> str:
        start = self.position
        while self.position < len(self.text) and (
            self.text[self.position].isalnum() or self.text[self.position] in "_."
        ):
            self.position += 1
        if start == self.position:
            raise ContractTypeSyntaxError(
                f"empty atom at offset {start} in {self.text!r}"
            )
        return self.text[start:self.position]

    def parse(self) -> None:
        if any(character.isspace() for character in self.text):
            raise ContractTypeSyntaxError(f"whitespace in {self.text!r}")
        if not self.text:
            raise ContractTypeSyntaxError("empty contract type string")
        self.parse_union(0, "type")
        if self.position != len(self.text):
            raise ContractTypeSyntaxError(
                f"trailing input {self.text[self.position:]!r} in {self.text!r}"
            )

    def parse_union(self, depth: int, mode: str) -> list[str]:
        members = [self.parse_member(depth, mode)]
        while self.peek() == "|":
            self.take("|")
            members.append(self.parse_member(depth, mode))
        if len(members) != len(set(members)):
            raise ContractTypeSyntaxError(f"duplicate union member in {self.text!r}")
        return members

    def enter(self, depth: int) -> int:
        if depth + 1 > self.maximum_depth:
            raise ContractTypeSyntaxError(
                f"nesting deeper than {self.maximum_depth} in {self.text!r}"
            )
        self.observed_depth = max(self.observed_depth, depth + 1)
        return depth + 1

    def parse_member(self, depth: int, mode: str) -> str:
        if mode == "enum":
            value = self.word()
            if not re.fullmatch(r"[a-z][a-z0-9_]*", value):
                raise ContractTypeSyntaxError(f"invalid enum value {value!r}")
            self.atom_uses.append(("ENUM_VALUE", value))
            return value
        word = self.word()
        if word == "ENUM":
            self.take("[")
            values = self.parse_union(self.enter(depth), "enum")
            self.take("]")
            return "ENUM[" + "|".join(values) + "]"
        if word in ("LIST", "SET"):
            self.take("[")
            inner = self.parse_union(self.enter(depth), "type")
            self.take("]")
            return f"{word}[" + "|".join(inner) + "]"
        if word == "REFERENCE":
            if self.peek() == "[":
                self.take("[")
                inner = self.parse_union(self.enter(depth), "reference")
                self.take("]")
                return "REFERENCE[" + "|".join(inner) + "]"
            return "REFERENCE"
        if word == "qualified_identifier":
            if self.peek() == "(":
                self.take("(")
                domain = self.word()
                self.take(")")
                if domain not in self.vocabularies["DOMAIN_NAME"]:
                    raise ContractTypeSyntaxError(
                        f"unresolved qualified_identifier domain {domain!r}"
                    )
                self.atom_uses.append(("DOMAIN_NAME", domain))
                return f"qualified_identifier({domain})"
            return "qualified_identifier"
        return self.resolve_atom(word, mode)

    def resolve_atom(self, word: str, mode: str) -> str:
        if mode == "reference":
            order = ("BLOCK_NAME", "META_TYPE_NAME", "SCALAR_TYPE_NAME")
        else:
            order = (
                "SCALAR_TYPE_NAME",
                "SPECIAL_VALUE_NAME",
                "META_TYPE_NAME",
                "RESULT_RECORD_NAME",
                "NAMED_VALUE_KIND",
            )
        for atom_class in order:
            if word in self.vocabularies[atom_class]:
                self.atom_uses.append((atom_class, word))
                return word
        if mode != "reference" and re.fullmatch(r"[A-Z]", word):
            if word not in self.registered_variables:
                raise ContractTypeSyntaxError(
                    f"unregistered type variable {word!r} in {self.text!r}"
                )
            self.atom_uses.append(("TYPE_VARIABLE", word))
            return word
        raise ContractTypeSyntaxError(
            f"unresolved atom {word!r} in {self.text!r}"
        )


def collect_contract_type_strings(document: Any, prefix: str) -> list[tuple[str, str]]:
    found: list[tuple[str, str]] = []

    def walk(node: Any, path: str) -> None:
        if isinstance(node, dict):
            for key, value in node.items():
                if key == "type" and isinstance(value, str):
                    found.append((f"{path}/type", value))
                else:
                    walk(value, f"{path}/{key}")
        elif isinstance(node, list):
            for index, item in enumerate(node):
                walk(item, f"{path}/{index}")

    walk(document, prefix)
    return found


def contract_pointer_matches(pattern: str, path: str) -> bool:
    pattern_segments = pattern.split("/")
    path_segments = path.split("/")
    if len(pattern_segments) != len(path_segments):
        return False
    return all(
        expected in ("*", actual)
        for expected, actual in zip(pattern_segments, path_segments)
    )


def contract_type_notation_violations(
    root: Path, operations_registry: Any, groups_and_results: Any
) -> tuple[list[str], dict[str, Any]]:
    violations: list[str] = []
    summary: dict[str, Any] = {
        "checked_type_strings": 0,
        "distinct_type_strings": 0,
        "observed_maximum_depth": 0,
        "atom_class_usage": {},
    }

    notation = operations_registry.get("contract_type_notation")
    if not isinstance(notation, dict):
        return (["operations registry declares no contract_type_notation"], summary)
    for key in CONTRACT_NOTATION_REQUIRED_KEYS:
        if key not in notation:
            violations.append(f"contract_type_notation omits required key {key}")
    if notation.get("closed") is not True:
        violations.append("contract_type_notation is not declared closed")
    pointer_source = groups_and_results.get("contract_type_notation_source")
    if not isinstance(pointer_source, str) or "contract_type_notation" not in pointer_source:
        violations.append(
            "groups/result registry does not bind itself to the single notation definition"
        )
    if violations:
        return (violations, summary)

    atom_classes = notation.get("atom_classes")
    if not isinstance(atom_classes, dict):
        return (["contract_type_notation.atom_classes is not an object"], summary)
    vocabularies: dict[str, set[str]] = {}
    for atom_class, record in atom_classes.items():
        try:
            vocabularies[atom_class] = resolve_atom_vocabulary(root, record)
        except ContractTypeSyntaxError as error:
            violations.append(f"atom class {atom_class}: {error}")
    if violations:
        return (violations, summary)
    for required_class in (
        LANGUAGE_LEVEL_ATOM_CLASSES
        | SPECIFICATION_LEVEL_ATOM_CLASSES
        | {"DOMAIN_NAME", "ENUM_VALUE", "TYPE_VARIABLE"}
    ):
        if required_class not in vocabularies:
            violations.append(f"contract_type_notation omits atom class {required_class}")
    if violations:
        return (violations, summary)

    nesting = notation.get("nesting")
    if not isinstance(nesting, dict) or not isinstance(nesting.get("maximum_depth"), int):
        return (["contract_type_notation.nesting declares no integer maximum_depth"], summary)
    maximum_depth = nesting["maximum_depth"]
    binding = notation.get("type_variable_binding")
    if not isinstance(binding, dict) or not isinstance(
        binding.get("registered_variables"), list
    ):
        return (
            ["contract_type_notation.type_variable_binding declares no registered_variables"],
            summary,
        )
    registered_variables = set(binding["registered_variables"])

    constructor_keywords = set(notation.get("constructor_keywords", []))

    strings = collect_contract_type_strings(
        operations_registry, "10_REGISTRIES/operations_v0.1.0.json#"
    ) + collect_contract_type_strings(
        groups_and_results, "10_REGISTRIES/built_in_groups_and_results_v0.1.0.json#"
    )
    patterns = notation.get("applies_to", [])
    if not isinstance(patterns, list) or not patterns:
        return (["contract_type_notation.applies_to is not a nonempty list"], summary)
    matched_patterns: set[str] = set()
    for path, _ in strings:
        matches = [pattern for pattern in patterns if contract_pointer_matches(pattern, path)]
        if not matches:
            violations.append(f"contract type string at {path} matches no applies_to pattern")
        matched_patterns.update(matches)
    for pattern in patterns:
        if pattern not in matched_patterns:
            violations.append(f"applies_to pattern matches no contract type string: {pattern}")

    usage: dict[str, int] = {}
    observed_depth = 0
    for path, text in strings:
        parser = ContractTypeParser(
            text, vocabularies, maximum_depth, registered_variables
        )
        try:
            parser.parse()
        except ContractTypeSyntaxError as error:
            violations.append(f"{path}: {error}")
            continue
        observed_depth = max(observed_depth, parser.observed_depth)
        for atom_class, word in parser.atom_uses:
            usage[atom_class] = usage.get(atom_class, 0) + 1
            if word in constructor_keywords:
                violations.append(
                    f"{path}: constructor keyword {word!r} was classified as {atom_class}"
                )
            if atom_class in LANGUAGE_LEVEL_ATOM_CLASSES and word.lower() == word:
                violations.append(
                    f"{path}: language-level name {word!r} is not written in uppercase"
                )
            if atom_class in SPECIFICATION_LEVEL_ATOM_CLASSES and word.upper() == word:
                violations.append(
                    f"{path}: specification-level designator {word!r} is written in uppercase"
                )

    for atom_class in vocabularies:
        if atom_class not in usage:
            violations.append(f"atom class {atom_class} is declared but never used")

    declared_observed = nesting.get("observed_maximum_in_this_release")
    if declared_observed != observed_depth:
        violations.append(
            "nesting.observed_maximum_in_this_release is "
            f"{declared_observed!r} but the registries observe {observed_depth}"
        )

    summary["checked_type_strings"] = len(strings)
    summary["distinct_type_strings"] = len({text for _, text in strings})
    summary["observed_maximum_depth"] = observed_depth
    summary["atom_class_usage"] = dict(sorted(usage.items()))
    return (violations, summary)

def check_registry(root: Path, results: Results) -> None:
    registry = root / "10_REGISTRIES"
    keywords = load_json_strict(registry / "keywords_v0.1.0.json")["keywords"]
    blocks = load_json_strict(registry / "block_schemas_v0.1.0.json")["schemas"]
    fields = load_json_strict(registry / "field_signatures_v0.1.0.json")
    operation_document = load_json_strict(registry / "operations_v0.1.0.json")
    operation_registry = operation_document if isinstance(operation_document, dict) else {}
    operation_values = operation_registry.get("contracts", {})
    operations = operation_values if isinstance(operation_values, dict) else {}
    statuses = load_json_strict(registry / "statuses_and_errors_v0.1.0.json")
    groups_and_results = load_json_strict(registry / "built_in_groups_and_results_v0.1.0.json")
    results_schema = groups_and_results["result_schemas"]
    error_group = groups_and_results["enum_groups"]["errors"]
    semantic_meta = load_json_strict(registry / "semantic_meta_types_v0.1.0.json")
    meta_types = semantic_meta["meta_types"]
    types = load_json_strict(registry / "types_v0.1.0.json")
    operator_functions = load_json_strict(registry / "operators_and_functions_v0.1.0.json")
    symbols = load_json_strict(registry / "symbols_v0.1.0.json")
    formats_registry = load_json_strict(
        registry / "formats_encodings_units_v0.1.0.json"
    )
    units = formats_registry["units"]
    grammar = (root / "04_GRAMMAR/10_COMPLETE_EBNF.ebnf").read_text(encoding="utf-8")
    schema_prose = (
        root / "04_GRAMMAR/12_VALUE_ITEM_PROPERTY_SCHEMA_AND_OBJECT_BLOCKS.txt"
    ).read_text(encoding="utf-8")

    reserved_segment = grammar[grammar.index("RESERVED_WORD =") : grammar.index("SPACE =")]
    reserved = set(re.findall(r'"([A-Z][A-Z_]*)"', reserved_segment))
    results.add(
        "registry",
        "keyword_grammar_parity",
        "PASS" if reserved == set(keywords) else "FAIL",
        keyword_count=len(keywords),
        grammar_reserved_count=len(reserved),
        missing_from_grammar=sorted(set(keywords) - reserved),
        missing_from_registry=sorted(reserved - set(keywords)),
    )

    block_names = set(blocks)
    field_block_names = set(fields["blocks"])
    contract_conflicts = block_contract_conflicts(blocks, fields["blocks"])
    results.add(
        "registry",
        "block_field_schema_parity",
        "PASS" if block_names == field_block_names and not contract_conflicts else "FAIL",
        block_schema_count=len(block_names),
        field_signature_block_count=len(field_block_names),
        block_name_delta=sorted(block_names ^ field_block_names),
        contract_conflicts=contract_conflicts,
    )

    collection_violations = collection_contract_violations(
        types, keywords, blocks, fields, symbols, grammar
    )
    results.add(
        "registry",
        "collection_value_and_item_contract",
        "PASS" if not collection_violations else "FAIL",
        violations=collection_violations,
        decision="D-001",
    )

    used_values: set[str] = set()
    value_kind_shape_errors = []
    for block_name, block in fields["blocks"].items():
        for field_name, field in block["fields"].items():
            value = field.get("value_kind")
            if not isinstance(value, str) or not value:
                value_kind_shape_errors.append(
                    f"{block_name}.{field_name} value_kind must be a nonempty string"
                )
            else:
                used_values.add(value)
    named_value_kinds = fields.get("value_kind_registry", {})
    value_kind_templates = fields.get("value_kind_templates", {})
    qualified_domains, qualified_domain_errors = resolve_qualified_identifier_domains(
        root,
        fields.get("qualified_identifier_domains", {}),
        set(groups_and_results["enum_groups"]["definition_kinds"]),
    )
    reference_domains, reference_domain_errors = validate_reference_domains(
        semantic_meta.get("reference_domains", {}), meta_types, blocks
    )
    named_value_kind_errors = validate_named_value_kinds(named_value_kinds)
    template_errors = validate_value_kind_templates(value_kind_templates)
    value_kind_resolutions: dict[str, dict[str, Any]] = {}
    unresolved_value_kinds = []
    for value in sorted(used_values):
        resolution, error = resolve_value_kind(
            value,
            named_value_kinds,
            value_kind_templates,
            qualified_domains,
            blocks,
            reference_domains,
        )
        if error:
            unresolved_value_kinds.append(
                {"value_kind": value, "head": value_kind_head(value), "error": error}
            )
        elif resolution is not None:
            value_kind_resolutions[value] = resolution
    unresolved_heads = sorted(
        {
            value_kind_head(item["value_kind"])
            for item in unresolved_value_kinds
            if value_kind_head(item["value_kind"]) not in value_kind_templates
        }
    )
    default_errors = []
    for block_name, block in fields["blocks"].items():
        for field_name, field in block["fields"].items():
            value = field.get("value_kind")
            resolution = value_kind_resolutions.get(value) if isinstance(value, str) else None
            if resolution is None:
                continue
            error = default_value_error(field.get("default"), resolution, qualified_domains)
            if error:
                default_errors.append(
                    {"field": f"{block_name}.{field_name}", "error": error}
                )
    field_contract_violations = task_0002_field_contract_violations(fields, keywords)
    closure_errors = (
        value_kind_shape_errors
        + named_value_kind_errors
        + template_errors
        + qualified_domain_errors
        + reference_domain_errors
        + field_contract_violations
        + [item["error"] for item in default_errors]
        + [item["error"] for item in unresolved_value_kinds]
    )
    results.add(
        "registry",
        "field_value_kind_closure",
        "PASS" if not closure_errors else "FAIL",
        field_use_count=sum(len(block["fields"]) for block in fields["blocks"].values()),
        distinct_value_kind_count=len(used_values),
        registered_named_count=len(named_value_kinds),
        registered_template_count=len(value_kind_templates),
        resolved_named_count=sum(
            item["classification"] == "named" for item in value_kind_resolutions.values()
        ),
        resolved_template_count=sum(
            item["classification"] == "template" for item in value_kind_resolutions.values()
        ),
        qualified_identifier_domains={
            name: {
                "core_value_count": len(values),
                "defined_kind": fields["qualified_identifier_domains"][name].get(
                    "defined_kind"
                ),
            }
            for name, values in sorted(qualified_domains.items())
        },
        reference_domains={
            name: sorted(values) for name, values in sorted(reference_domains.items())
        },
        value_kind_shape_errors=value_kind_shape_errors,
        named_value_kind_errors=named_value_kind_errors,
        template_contract_errors=template_errors,
        qualified_identifier_domain_errors=qualified_domain_errors,
        reference_domain_errors=reference_domain_errors,
        field_contract_violations=field_contract_violations,
        default_contract_errors=default_errors,
        unresolved_value_kinds=unresolved_value_kinds,
        unresolved_template_heads=unresolved_heads,
        blocked_by=[],
    )

    forward_parent_errors = []
    for parent_name, block in fields["blocks"].items():
        for field_name, signature in block["fields"].items():
            value = signature.get("value_kind")
            resolution = value_kind_resolutions.get(value, {}) if isinstance(value, str) else {}
            for target in resolution.get("nested_targets", []):
                target_schema = fields["blocks"].get(target)
                if (
                    field_name != target
                    or target_schema is None
                    or parent_name not in target_schema["legal_parents"]
                ):
                    forward_parent_errors.append(
                        {"parent": parent_name, "field": field_name, "target": target, "value_kind": value}
                    )
    reverse_parent_errors = []
    for child_name, child in fields["blocks"].items():
        for parent_name in child["legal_parents"]:
            if parent_name in field_block_names:
                admitting_fields = []
                for field_name, signature in fields["blocks"][parent_name]["fields"].items():
                    value = signature.get("value_kind")
                    resolution = (
                        value_kind_resolutions.get(value, {})
                        if isinstance(value, str)
                        else {}
                    )
                    if (
                        field_name == child_name
                        and child_name in resolution.get("nested_targets", [])
                    ):
                        admitting_fields.append(field_name)
                if not admitting_fields:
                    reverse_parent_errors.append(
                        {
                            "child": child_name,
                            "parent": parent_name,
                            "error": "parent has no nested field admitting child",
                        }
                    )
            elif not pseudo_parent_is_admitted(
                parent_name, child_name, grammar, schema_prose
            ):
                reverse_parent_errors.append(
                    {
                        "child": child_name,
                        "parent": parent_name,
                        "error": "pseudo parent is not admitted by the EBNF",
                    }
                )
    accepted_parent_violations = accepted_parent_contract_violations(fields)
    parent_status = (
        "PASS"
        if not accepted_parent_violations
        and not forward_parent_errors
        and not reverse_parent_errors
        else "FAIL"
    )
    results.add(
        "registry",
        "nested_parent_closure",
        parent_status,
        accepted_contract_violations=accepted_parent_violations,
        forward_contradictions=forward_parent_errors,
        reverse_contradictions=reverse_parent_errors,
        blocked_by=[],
        decision="D-004",
    )

    all_errors = set(statuses["errors"])
    all_results = set(results_schema)
    referenced_errors: set[str] = set()
    referenced_results: set[str] = set()
    for operation in operations.values():
        if not isinstance(operation, dict):
            continue
        operation_errors = operation.get("errors")
        if isinstance(operation_errors, list):
            referenced_errors.update(
                error for error in operation_errors if isinstance(error, str)
            )
        operation_result = operation.get("result_schema")
        if isinstance(operation_result, str):
            referenced_results.add(operation_result)
    for family in ("constructors", "operators", "functions"):
        for contract in operator_functions.get(family, {}).values():
            referenced_errors.update(contract.get("errors", []))
    for profile in types.get("pattern_profiles", {}).values():
        resource_error = profile.get("resource_limit_error")
        if resource_error:
            referenced_errors.add(resource_error)
    meta_refs = set()
    meta_refs.update(re.findall(r"\bmeta\.[a-z_]+\b", json.dumps(operations)))
    meta_refs.update(re.findall(r"\bmeta\.[a-z_]+\b", json.dumps(operator_functions)))
    meta_refs.update(
        contract.get("meta_type")
        for contract in semantic_meta.get("reference_domains", {}).values()
        if isinstance(contract, dict) and isinstance(contract.get("meta_type"), str)
    )
    reference_errors = {
        "undefined_errors": sorted(referenced_errors - all_errors),
        "undefined_results": sorted(referenced_results - all_results),
        "undefined_meta_types": sorted(meta_refs - set(meta_types)),
        "errors_missing_from_group": sorted(all_errors - set(error_group)),
        "unregistered_group_errors": sorted(set(error_group) - all_errors),
        "cross_registry_conflicts": cross_registry_contract_violations(
            groups_and_results, formats_registry, statuses, operations
        ),
    }
    results.add(
        "registry",
        "operation_reference_closure",
        "PASS" if not any(reference_errors.values()) else "FAIL",
        **reference_errors,
    )

    priority_violations = priority_contract_violations(fields, blocks, statuses)
    results.add(
        "registry",
        "priority_contract",
        "PASS" if not priority_violations else "FAIL",
        violations=priority_violations,
        optional_default=0,
        inheritance="none",
        mandatory_omission_error="error.field.required",
        decision="D-005",
    )

    operation_violations = operation_contract_violations(
        operation_registry, groups_and_results, statuses
    )
    results.add(
        "registry",
        "operation_contracts",
        "FAIL" if operation_violations else "PASS",
        operation_contract_violations=operation_violations,
        operation_count=len(operations),
        approved_contract_sha256=operation_contract_fingerprint(operations),
        blocked_by=[],
    )
    operation_prose_violations = operation_prose_contract_violations(root, operations)
    results.add(
        "registry",
        "operation_prose_contracts",
        "FAIL" if operation_prose_violations else "PASS",
        operation_prose_contract_violations=operation_prose_violations,
        operation_count=len(operations),
        prose_file_count=3,
        blocked_by=[],
    )
    result_violations = result_contract_violations(
        root,
        groups_and_results,
        statuses,
        operations,
        blocks,
        fields,
    )
    results.add(
        "registry",
        "result_schema_cardinality_and_output_contracts",
        "FAIL" if result_violations else "PASS",
        result_contract_violations=result_violations,
        result_schema_count=len(results_schema),
        common_field_count=len(groups_and_results.get("result_contract", {}).get("common_fields", {})),
        operation_result_reference_count=sum(
            1
            for operation in operations.values()
            if isinstance(operation, dict) and "result_schema" in operation
        ),
        approved_contract_sha256=canonical_contract_fingerprint(
            {
                "result_contract": groups_and_results.get("result_contract"),
                "result_schemas": results_schema,
            }
        ),
        blocked_by=[],
    )

    division_violations = division_contract_violations(
        root, types, operator_functions, statuses, operations, keywords
    )
    set_sort_violations = set_sort_contract_violations(
        root,
        types,
        operator_functions,
        formats_registry,
        semantic_meta,
        statuses,
        operations,
        results_schema,
        keywords,
    )
    results.add(
        "registry",
        "division_semantics",
        "FAIL" if division_violations else "PASS",
        division_result_mapping_unresolved=bool(division_violations),
        division_contract_violations=division_violations,
        blocked_by=[],
    )
    results.add(
        "registry",
        "set_and_sort_semantics",
        "FAIL" if set_sort_violations else "PASS",
        set_iteration_sort_key_unresolved=bool(set_sort_violations),
        set_sort_contract_violations=set_sort_violations,
        blocked_by=[],
    )

    constructor_violations = constructor_pattern_contract_violations(
        types, operator_functions, statuses, error_group, units
    )
    results.add(
        "registry",
        "constructor_and_pattern_profiles",
        "PASS" if not constructor_violations else "FAIL",
        violations=constructor_violations,
        constructor_count=len(operator_functions.get("constructors", {})),
        pattern_profile_count=len(types.get("pattern_profiles", {})),
        decision="D-002/D-003",
    )

    compatibility = statuses.get("compatibility_note", "")
    diagnostic_violations = diagnostic_selection_contract_violations(root, statuses)
    results.add(
        "registry",
        "diagnostic_selection_contract",
        "FAIL" if diagnostic_violations else "PASS",
        error_metadata_resolved_count=(
            len(statuses.get("errors", {})) if not diagnostic_violations else 0
        ),
        diagnostic_selection_violations=diagnostic_violations,
        severity_values=statuses.get("diagnostic_selection", {})
        .get("severity", {})
        .get("closed_values", []),
        policy_fingerprint=canonical_contract_fingerprint(
            statuses.get("diagnostic_selection", {})
        ),
        selection_vector_count=DIAGNOSTIC_SELECTION_VECTOR_COUNT,
        compatibility_note=compatibility,
        closed_finding="LCL-AUDIT-014",
        blocked_by=[],
    )

    event_violations, event_handler_count = event_model_violations(
        root, statuses, groups_and_results, formats_registry, operations
    )
    results.add(
        "registry",
        "event_model_contract",
        "FAIL" if event_violations else "PASS",
        event_model_violations=event_violations,
        registered_event_count=len(
            groups_and_results.get("enum_groups", {}).get("events", [])
        ),
        event_raising_error_count=sum(
            1 for entry in statuses.get("errors", {}).values() if entry.get("event")
        ),
        mapped_error_count=len(statuses.get("errors", {})),
        checked_shipped_handlers=event_handler_count,
        policy_fingerprint=canonical_contract_fingerprint(statuses.get("event_model", {})),
        static_limit=(
            "Computes mapping totality, the event/recoverability invariant, cross-surface "
            "vocabulary agreement, bijection onto the registered vocabulary, and shipped "
            "HANDLER event reachability from the guarded operations of the same document; no LCL "
            "document is parsed or executed."
        ),
        blocked_by=[],
    )

    custom_violations, custom_definition_count = custom_operation_declaration_violations(
        root, fields, blocks, operation_registry, groups_and_results
    )
    results.add(
        "registry",
        "custom_operation_declaration_contract",
        "FAIL" if custom_violations else "PASS",
        custom_operation_violations=custom_violations,
        checked_shipped_definitions=custom_definition_count,
        declared_effect_vocabulary=groups_and_results["enum_groups"]["effect_classes"],
        declared_dependency_vocabulary=groups_and_results["enum_groups"]["dependency_classes"],
        static_limit=(
            "Checks declarability, retired-claim absence, and shipped definition axis "
            "closure only; no LCL document is parsed or executed."
        ),
        blocked_by=[],
    )

    invocation_violations, checked_handlers = invocation_site_contract_violations(
        root, fields, blocks, operations, statuses
    )
    results.add(
        "registry",
        "invocation_site_contract",
        "FAIL" if invocation_violations else "PASS",
        invocation_site_violations=invocation_violations,
        checked_shipped_handlers=checked_handlers,
        handler_context_target_types=list(HANDLER_CONTEXT_TARGET_TYPES),
        static_limit=(
            "Checks invocation-site rule presence, HANDLER schema totality, core.retry "
            "composition, and shipped HANDLER/FALLBACK contract satisfaction only; no LCL "
            "document is parsed or executed."
        ),
        blocked_by=[],
    )

    lifecycle_violations = failure_lifecycle_contract_violations(
        root, statuses, operations, groups_and_results
    )
    results.add(
        "registry",
        "mixed_phase_lifecycle_contracts",
        "FAIL" if lifecycle_violations else "PASS",
        error_phase_metadata_resolved_count=(
            len(statuses.get("errors", {})) if not lifecycle_violations else 0
        ),
        baseline_error_count=len(
            statuses.get("failure_lifecycle", {}).get("baseline_error_profiles", {})
        ),
        additional_mixed_error_count=len(
            statuses.get("failure_lifecycle", {}).get(
                "additional_mixed_error_profiles", {}
            )
        ),
        policy_fingerprint=canonical_contract_fingerprint(
            statuses.get("failure_lifecycle", {})
        ),
        mixed_phase_lifecycle_violations=lifecycle_violations,
        closed_finding="LCL-AUDIT-015",
        blocked_by=[],
    )

    notation_violations, notation_summary = contract_type_notation_violations(
        root, operation_registry, groups_and_results
    )
    results.add(
        "registry",
        "contract_type_notation",
        "FAIL" if notation_violations else "PASS",
        contract_type_notation_violations=notation_violations,
        checked_type_strings=notation_summary["checked_type_strings"],
        distinct_type_strings=notation_summary["distinct_type_strings"],
        observed_maximum_depth=notation_summary["observed_maximum_depth"],
        atom_class_usage=notation_summary["atom_class_usage"],
        static_limit=(
            "Parses every operation-contract and result-schema type string against the "
            "notation the registry itself declares and resolves every atom against the "
            "source that notation names; no LCL document is parsed or executed."
        ),
        blocked_by=[],
    )


    language_checker = runpy.run_path(str(root / "09_CONFORMANCE/TOOLS/validate_language_contracts.py"))
    language_result = language_checker["validate"](root)
    if not isinstance(language_result, dict) or not isinstance(language_result.get("violations"), list):
        raise ValueError("language contract checker returned no structured violation list")
    results.add(
        "registry", "language_decision_contracts",
        "FAIL" if language_result["violations"] else "PASS", **language_result,
    )


def check_catalog(root: Path, results: Results) -> None:
    def operation() -> dict[str, Any]:
        catalog = load_json_strict(root / "09_CONFORMANCE/CASES/core_conformance_cases_v0.1.0.json")
        expected_catalog_fields = {
            "language",
            "version",
            "normative",
            "case_count",
            "category_counts",
            "cases",
        }
        assert isinstance(catalog, dict), "catalog root is not an object"
        assert set(catalog) == expected_catalog_fields, "catalog root fields are not exact"
        assert catalog.get("language") == "LCL", "catalog language is not LCL"
        assert catalog.get("version") == "0.1.0", "catalog version is not 0.1.0"
        assert catalog.get("normative") is True, "catalog is not normative"
        registry = root / "10_REGISTRIES"
        keywords = load_json_strict(registry / "keywords_v0.1.0.json")["keywords"]
        symbols = load_json_strict(registry / "symbols_v0.1.0.json")["adopted"]
        types = load_json_strict(registry / "types_v0.1.0.json")["types"]
        blocks = load_json_strict(registry / "block_schemas_v0.1.0.json")["schemas"]
        operator_functions = load_json_strict(registry / "operators_and_functions_v0.1.0.json")
        operations = load_json_strict(registry / "operations_v0.1.0.json")["contracts"]
        statuses = load_json_strict(registry / "statuses_and_errors_v0.1.0.json")
        groups_and_results = load_json_strict(registry / "built_in_groups_and_results_v0.1.0.json")
        cases = catalog["cases"]
        assert isinstance(cases, list), "catalog cases is not a list"
        expected_case_fields = {
            "id",
            "category",
            "subject",
            "requirement",
            "expected",
            "source",
        }
        expected_category_counts = {
            "block_extra": 41,
            "block_minimum": 41,
            "block_missing": 41,
            "contract_type_notation": 1,
            "diagnostic_policy": 1,
            "enum_groups": 22,
            "error_contract": 77,
            "event_model": 1,
            "failure_lifecycle": 1,
            "function_invalid": 11,
            "function_valid": 11,
            "keyword_invalid": 141,
            "keyword_valid": 141,
            "operation_binding": 39,
            "operation_effects": 39,
            "operation_errors": 39,
            "operator_invalid": 19,
            "operator_valid": 19,
            "reserved_namespaces": 9,
            "result_schemas": 9,
            "status_transition": 12,
            "symbol_invalid": 21,
            "symbol_valid": 21,
            "type_invalid": 21,
            "type_valid": 21,
        }
        expected_sources = {
            "block_extra": "field_signatures_v0.1.0.json",
            "block_minimum": "field_signatures_v0.1.0.json",
            "block_missing": "field_signatures_v0.1.0.json",
            "contract_type_notation": "operations_v0.1.0.json",
            "diagnostic_policy": "statuses_and_errors_v0.1.0.json",
            "enum_groups": "built_in_groups_and_results_v0.1.0.json",
            "error_contract": "statuses_and_errors_v0.1.0.json",
            "event_model": "statuses_and_errors_v0.1.0.json",
            "failure_lifecycle": "statuses_and_errors_v0.1.0.json",
            "function_invalid": "operators_and_functions_v0.1.0.json",
            "function_valid": "operators_and_functions_v0.1.0.json",
            "keyword_invalid": "keywords_v0.1.0.json",
            "keyword_valid": "keywords_v0.1.0.json",
            "operation_binding": "operations_v0.1.0.json",
            "operation_effects": "operations_v0.1.0.json",
            "operation_errors": "operations_v0.1.0.json",
            "operator_invalid": "operators_and_functions_v0.1.0.json",
            "operator_valid": "operators_and_functions_v0.1.0.json",
            "reserved_namespaces": "built_in_groups_and_results_v0.1.0.json",
            "result_schemas": "built_in_groups_and_results_v0.1.0.json",
            "status_transition": "statuses_and_errors_v0.1.0.json",
            "symbol_invalid": "symbols_v0.1.0.json",
            "symbol_valid": "symbols_v0.1.0.json",
            "type_invalid": "types_v0.1.0.json",
            "type_valid": "types_v0.1.0.json",
        }
        assert catalog.get("category_counts") == expected_category_counts, (
            "catalog category_counts are not the exact 25-category contract"
        )
        for index, case in enumerate(cases, start=1):
            assert isinstance(case, dict), f"catalog case {index} is not an object"
            assert set(case) == expected_case_fields, (
                f"catalog case {index} fields are not exact"
            )
            assert all(
                isinstance(case[field], str) and bool(case[field])
                for field in expected_case_fields
            ), f"catalog case {index} fields must be non-empty strings"
            expected_identifier = (
                f"{case['category'].replace('_', '-').upper()}-{index:04d}"
            )
            assert case["id"] == expected_identifier, (
                f"catalog case {index} ID/order differs"
            )
            assert case["source"] == expected_sources.get(case["category"]), (
                f"catalog case {index} source/category mapping differs"
            )
        assert catalog["case_count"] == len(cases) == 799, "case_count mismatch"
        identifiers = [case["id"] for case in cases]
        assert len(identifiers) == len(set(identifiers)), "duplicate case IDs"
        counts: dict[str, int] = {}
        concrete = 0
        concrete_keys = {"input", "fixture", "source_text", "operands", "arguments", "actual"}
        for case in cases:
            counts[case["category"]] = counts.get(case["category"], 0) + 1
            if concrete_keys & set(case):
                concrete += 1
        assert counts == catalog["category_counts"], "category_counts mismatch"

        def exact_subjects(category: str, expected: set[str]) -> None:
            actual_list = [case["subject"] for case in cases if case["category"] == category]
            assert len(actual_list) == len(expected), f"{category} subject cardinality mismatch"
            assert set(actual_list) == expected, f"{category} registry coverage mismatch"

        exact_subjects("keyword_valid", set(keywords))
        exact_subjects("keyword_invalid", set(keywords))
        for case in cases:
            if case.get("category") == "keyword_invalid":
                assert case.get("expected") == "error.keyword.case", (
                    f"{case.get('id')} has an ambiguous keyword-case expectation"
                )
                requirement = case.get("requirement", "")
                assert all(token in requirement for token in
                           ("mixed-case spelling", "not legal identifiers", "remain identifiers")), (
                    f"{case.get('id')} does not preserve lowercase identifier contexts"
                )
        exact_subjects("symbol_valid", set(symbols))
        exact_subjects("symbol_invalid", set(symbols))
        exact_subjects("type_valid", set(types))
        exact_subjects("type_invalid", set(types))
        exact_subjects("block_minimum", set(blocks))
        exact_subjects("block_missing", set(blocks))
        exact_subjects("block_extra", set(blocks))
        exact_subjects("operator_valid", set(operator_functions["operators"]))
        exact_subjects("operator_invalid", set(operator_functions["operators"]))
        exact_subjects("function_valid", set(operator_functions["functions"]))
        exact_subjects("function_invalid", set(operator_functions["functions"]))
        for category in ("operation_binding", "operation_effects", "operation_errors"):
            exact_subjects(category, set(operations))
        parameter_error_prefix = (
            "Reject omission of TARGET when this row marks it required, omission of any "
            "named parameter that this row marks required, any positional argument, any "
            "duplicate named parameter, and any unregistered named parameter with "
            "error.operation.parameter. "
        )
        operation_error_cases = [
            case for case in cases if case["category"] == "operation_errors"
        ]
        assert all(
            case["requirement"].startswith(parameter_error_prefix)
            for case in operation_error_cases
        ), "operation error cases do not all specify the exact universal parameter trigger"
        assert len(operation_error_cases) == 39, "operation error case cardinality is not 39"
        exact_subjects("status_transition", set(statuses["statuses"]))
        exact_subjects("error_contract", set(statuses["errors"]))
        exact_subjects("enum_groups", set(groups_and_results["enum_groups"]))
        exact_subjects("result_schemas", set(groups_and_results["result_schemas"]))
        exact_subjects("reserved_namespaces", set(groups_and_results["reserved_namespaces"]))
        exact_subjects("diagnostic_policy", {"core.error_selection"})
        exact_subjects("event_model", {"event_model"})
        exact_subjects("failure_lifecycle", {"core.failure_lifecycle"})

        by_id = {case["id"]: case for case in cases}
        task_0001_tokens = {
            "KEYWORD-VALID-0129": ("BASE ENUM",),
            "TYPE-VALID-0335": ("bracket",),
            "TYPE-VALID-0337": ("bracket",),
            "TYPE-VALID-0345": ("WORKSPACE", "containment"),
            "TYPE-VALID-0347": ("RFC 3986", "scheme"),
            "TYPE-VALID-0349": ("workspace-relative", "**"),
            "TYPE-VALID-0351": ("frozen closed LCL REGEX", "ASCII-only i folding", "canonical-order"),
            "TYPE-VALID-0353": ("RFC 3339",),
            "TYPE-VALID-0355": ("RFC 3339", "UTC"),
            "TYPE-VALID-0357": ("RFC 3339", "UTC"),
            "TYPE-VALID-0365": ("registered unit", "Time-category"),
            "OPERATOR-VALID-0540": ("entire", "full-match"),
            "ERROR-CONTRACT-0792": ("pattern-resource exhaustion",),
        }
        for identifier in (
            "BLOCK-MINIMUM-0412",
            "BLOCK-MINIMUM-0424",
            "BLOCK-MINIMUM-0427",
            "BLOCK-MINIMUM-0430",
            "BLOCK-MINIMUM-0433",
            "BLOCK-MINIMUM-0436",
        ):
            task_0001_tokens[identifier] = ("PRIORITY", "integer 0", "without inheritance")
        for identifier, tokens in task_0001_tokens.items():
            requirement = by_id.get(identifier, {}).get("requirement", "")
            assert all(token in requirement for token in tokens), f"{identifier} is not decision-specific"

        task_0004_tokens = {
            "KEYWORD-VALID-0061": (
                "DETERMINISTIC TRUE",
                "fully resolved operation",
                "identical declared inputs",
                "verified assertion",
                "not an override",
            ),
            "KEYWORD-INVALID-0062": (
                "mixed-case spelling", "not legal identifiers", "remain identifiers",
            ),
            "OPERATION-BINDING-0565": (
                "DETERMINISTIC TRUE",
                "SIDE_EFFECT FALSE",
                "dependencies exactly {declared_state_only}",
            ),
            "OPERATION-ERRORS-0558": (
                "error.operation.precondition",
                "required analysis profile",
                "before effects",
            ),
            "OPERATION-BINDING-0550": (
                "null means no declared default",
                "never the LCL NULL value",
                "MISSING optional parameter",
            ),
            "OPERATION-ERRORS-0552": (
                "depth outside 0..100",
                "error.value.out_of_range",
            ),
            "OPERATION-ERRORS-0561": (
                "evaluation_contract",
                "EXISTS and skipped-operand exceptions",
                "required final MISSING or UNKNOWN",
            ),
            "OPERATION-EFFECTS-0563": (
                "Omitted criteria uses ==",
                "non-==/!= criterion encountering MISSING",
                "result remaining UNKNOWN",
            ),
            "OPERATION-ERRORS-0564": (
                "error.operator.operand",
                "error.pattern.resource_limit",
                "non-equality MISSING",
                "propagated UNKNOWN",
            ),
            "OPERATION-BINDING-0568": (
                "LIST[T] target",
                "Reject SET input",
                "dependencies exactly {declared_state_only}",
            ),
            "OPERATION-ERRORS-0570": (
                "non-LIST target",
                "error.type.mismatch before effects",
                "predicate, reference, MISSING, UNKNOWN, and precondition",
            ),
            "OPERATION-BINDING-0571": (
                "LIST[T] or SET[T]",
                "reject stable, comparator, positional",
                "dependencies exactly {declared_state_only}",
            ),
            "OPERATION-ERRORS-0573": (
                "stable and comparator are unregistered",
                "direction outside ENUM[ascending|descending]",
                "malformed property_path",
                "invalid key-operation axes",
                "out-of-bounds key-operation profiles",
            ),
            "OPERATION-BINDING-0574": (
                "LIST[T] target",
                "Reject SET input",
                "material grouping result",
            ),
            "OPERATION-ERRORS-0576": (
                "non-LIST target",
                "error.type.mismatch before effects",
                "key, reference, MISSING, UNKNOWN, and precondition",
            ),
            "OPERATION-ERRORS-0582": (
                "error.operation.precondition",
                "required verification profile",
                "before effects",
            ),
            "OPERATION-BINDING-0583": (
                "exactly one actual source",
                "material-value TARGET",
                "registered == strict equality",
                "cannot accompany actual or assertion",
            ),
            "OPERATION-ERRORS-0585": (
                "six local errors",
                "error.operator.operand",
                "unresolved typed value references",
                "custom-operation profile precondition",
                "host-constraint failures",
            ),
            "OPERATION-ERRORS-0588": (
                "error.operation.precondition",
                "required reporting profile",
                "before effects",
            ),
            "OPERATION-ERRORS-0591": (
                "unresolved REFERENCE",
                "resolved MISSING",
                "resolved UNKNOWN",
            ),
            "OPERATION-EFFECTS-0608": (
                "REFERENCE source by its resolved address class",
                "removing OUTPUT",
                "MEMORY and STATE sources are prohibited",
            ),
            "OPERATION-BINDING-0607": (
                "source and destination addresses to be distinct",
                "same-address move",
            ),
            "OPERATION-ERRORS-0609": (
                "equal resolved source and destination addresses",
            ),
            "OPERATION-BINDING-0610": (
                "new_name to differ from the current name",
            ),
            "OPERATION-EFFECTS-0611": (
                "addressable by new_name and not by its prior name",
            ),
            "OPERATION-ERRORS-0612": (
                "same-name request",
                "illegal new name",
            ),
            "OPERATION-BINDING-0622": (
                "non-graph target",
                "exactly one complete immutable execution profile",
            ),
            "OPERATION-EFFECTS-0623": (
                "always has the process effect",
                "no mandatory local process effect",
                "normalized transitive dependency/effect unions",
                "failure to start records started FALSE",
                "nonzero is a completed outcome",
                "Graph mode synthesizes no native command observations",
                "failure phase, effect state, observed effects, and OUTPUT binding independently",
            ),
            "OPERATION-ERRORS-0624": (
                "nine local failure paths",
                "error.operation.precondition",
                "required execution profile",
                "before effects",
            ),
            "OPERATION-EFFECTS-0629": (
                "purge_data FALSE preserves declared associated data",
                "purge_data TRUE makes every declared associated data item",
            ),
            "OPERATION-BINDING-0655": (
                "every option to be compatible with expected_type",
                "before the message effect",
            ),
            "OPERATION-EFFECTS-0656": (
                "non-MISSING answer",
                "equal one listed option",
                "error.required.missing",
            ),
            "OPERATION-ERRORS-0657": (
                "incompatible with expected_type",
                "error.type.mismatch before invocation",
            ),
            "OPERATION-ERRORS-0660": (
                "limit outside 0..100",
                "error.value.out_of_range",
                "MISSING or UNKNOWN required when condition",
                "no separate retryable marker exists",
                "safe repetition, resumption, or reconciliation evidence",
                "proved-unsafe repetition uses error.operation.precondition",
                "safety-blocked unmade attempt is not exhaustion",
            ),
            "OPERATION-EFFECTS-0665": (
                "registered allowed_next set",
                "status.cancelled",
            ),
            "OPERATION-ERRORS-0666": (
                "error.execution.order",
                "does not allow status.cancelled",
            ),
            "ERROR-CONTRACT-0725": (
                "ACTION, HANDLER, or FALLBACK invocation site",
                "when a selected operation marks it required",
                "no handler-context binding supplies it",
                "FALLBACK operation identifier that requires any named parameter",
                "any named parameter that a row marks required",
                "every positional argument",
                "unregistered named parameter",
                "general or row-specific error",
                "instead of being remapped universally",
            ),
            "ERROR-CONTRACT-0727": (
                "false, missing, or unknown",
                "exactly one complete immutable profile",
                "every required core profile role",
                "outside its declared maximum",
                "stage, recoverability, and default status",
            ),
            "ERROR-CONTRACT-0732": (
                "unauthorized or prohibited required access or effects",
                "execution stage",
                "status.failed",
            ),
            "ERROR-CONTRACT-0733": (
                "prohibited TASK, PHASE, SEQUENCE, ACTION, or TEST reference cycle",
                "before graph-axis resolution",
                "resolution stage",
            ),
            "ENUM-GROUPS-0763": (
                "read_only, mutating, memory_state, and control",
                "read_only requires effects exactly {none}",
                "core.test is control",
            ),
            "ENUM-GROUPS-0764": (
                "deterministic, nondeterministic, derived, and inherited",
                "immutable profile has one final deterministic or nondeterministic category",
                "closed 23-operation local role map",
                "download=source+transfer",
                "core.execute uses execution only in non-graph mode",
                "custom kind.operation selects no profile role",
                "never causes narrowing",
            ),
            "ENUM-GROUPS-0765": (
                "declared_state_only, host, network, model, and human",
                "declared_state_only is exclusive",
                "omitted from a graph union",
            ),
            "ERROR-CONTRACT-0793": (
                "during validation",
                "DETERMINISTIC TRUE",
                "fully resolved operation",
                "nondeterministic",
                "never overrides",
            ),
            "ENUM-GROUPS-0794": (
                "none, filesystem, network, process, package, message, memory, and state",
                "none is exclusive",
                "resolved target address class",
                "network-addressed content",
                "Generic target mutators and core.move sources reject MEMORY and STATE",
                "Every non-graph core.execute has process",
                "Graph unions omit none",
            ),
            "ENUM-GROUPS-0795": (
                "inherited as the only operation-axis marker",
                "only for core.retry",
                "wrapped ACTION",
            ),
        }
        for identifier, tokens in task_0004_tokens.items():
            requirement = by_id.get(identifier, {}).get("requirement", "")
            assert all(
                token in requirement for token in tokens
            ), f"{identifier} is not Task-0004-specific"
        expected_task_0004_outcomes = {
            "KEYWORD-VALID-0061": "accept after successful contract validation",
            "KEYWORD-INVALID-0062": "error.keyword.case",
            "ERROR-CONTRACT-0727": (
                "execution; non-recoverable; status.failed before effects"
            ),
            "ERROR-CONTRACT-0793": "validation; non-recoverable; status.invalid",
        }
        for identifier, expected in expected_task_0004_outcomes.items():
            assert by_id.get(identifier, {}).get("expected") == expected, (
                f"{identifier} expected outcome is not exact"
            )

        focused_task_0004_case_ids = {
            "KEYWORD-VALID-0061",
            "KEYWORD-INVALID-0062",
            "OPERATION-ERRORS-0558",
            "OPERATION-BINDING-0565",
            "OPERATION-BINDING-0568",
            "OPERATION-ERRORS-0570",
            "OPERATION-BINDING-0571",
            "OPERATION-ERRORS-0573",
            "OPERATION-BINDING-0574",
            "OPERATION-ERRORS-0576",
            "OPERATION-ERRORS-0582",
            "OPERATION-BINDING-0583",
            "OPERATION-ERRORS-0585",
            "OPERATION-ERRORS-0588",
            "OPERATION-BINDING-0622",
            "OPERATION-ERRORS-0624",
            "ERROR-CONTRACT-0727",
            "ERROR-CONTRACT-0725",
            "ERROR-CONTRACT-0732",
            "ERROR-CONTRACT-0733",
            "ENUM-GROUPS-0763",
            "ENUM-GROUPS-0764",
            "ENUM-GROUPS-0765",
            "ERROR-CONTRACT-0793",
            "ENUM-GROUPS-0794",
            "ENUM-GROUPS-0795",
        }
        task_0004_case_ids = focused_task_0004_case_ids | {
            case["id"]
            for case in cases
            if case["category"]
            in {"operation_binding", "operation_effects", "operation_errors"}
        }
        assert len(task_0004_case_ids) == 129, "Task-0004 case set cardinality is not 129"
        task_0004_cases = sorted(
            (case for case in cases if case["id"] in task_0004_case_ids),
            key=lambda case: case["id"],
        )
        assert len(task_0004_cases) == 129, "Task-0004 cases are missing"
        task_0004_catalog_sha256 = hashlib.sha256(
            json.dumps(
                task_0004_cases,
                ensure_ascii=False,
                separators=(",", ":"),
                sort_keys=True,
            ).encode("utf-8")
        ).hexdigest()
        # Reviewed completion revisions: keyword-case witness 0062, expression
        # demand 0561, and bounded retry 0660. Structural coverage above remains.
        assert task_0004_catalog_sha256 == (
            "7bb5f2b4663f9908aaea4a3d7e1a6ed98a42e701bfddc5e48a49ef6883a5c735"
        ), "Task-0004 catalog cases differ from the approved contract"

        task_0005_subjects = {
            "RESULT-SCHEMAS-0774": "result.value",
            "RESULT-SCHEMAS-0775": "result.collection",
            "RESULT-SCHEMAS-0776": "result.operation",
            "RESULT-SCHEMAS-0777": "result.command",
            "RESULT-SCHEMAS-0778": "result.validation",
            "RESULT-SCHEMAS-0779": "result.verification",
            "RESULT-SCHEMAS-0780": "result.test",
            "RESULT-SCHEMAS-0781": "result.message",
            "RESULT-SCHEMAS-0782": "result.transfer",
        }
        task_0005_tokens = {
            "RESULT-SCHEMAS-0774": (
                "value as zero-or-one meta.material_value",
                "value required on producer success",
                "FALSE, zero, and empty material values",
            ),
            "RESULT-SCHEMAS-0775": (
                "co-present zero-or-one items LIST[T] and count INTEGER",
                "non-negative count equal to the actual item count",
                "empty list with count 0",
            ),
            "RESULT-SCHEMAS-0776": (
                "changed BOOLEAN|UNKNOWN and target exactly once",
                "optional material value",
                "known changed effect truth after failure",
            ),
            "RESULT-SCHEMAS-0777": (
                "mode exactly once and mode-specific conditional fields",
                "failure to start has started FALSE and completed FALSE",
                "nonzero remains a completed producer outcome",
                "Graph mode has no native command-observation fields",
                "only stdout/stderr permit declared partial binding",
            ),
            "RESULT-SCHEMAS-0778": (
                "zero-or-one valid BOOLEAN and exactly one domain-errors list",
                "distinguish domain errors from execution_errors",
                "producer status.succeeded with valid FALSE",
            ),
            "RESULT-SCHEMAS-0779": (
                "verified BOOLEAN|UNKNOWN and observed OBJECT",
                "exactly one domain-errors list and evidence list",
                "producer success with FALSE or UNKNOWN",
            ),
            "RESULT-SCHEMAS-0780": (
                "passed BOOLEAN|UNKNOWN and material expected/actual",
                "assertion form without expected/actual",
                "comparison form with both",
                "tested NULL is present material data",
            ),
            "RESULT-SCHEMAS-0781": (
                "delivered BOOLEAN|UNKNOWN, recipient, and nullable message_id exactly once",
                "NULL means no identifier was assigned",
                "successful dispatch processing with FALSE or UNKNOWN",
            ),
            "RESULT-SCHEMAS-0782": (
                "source and destination target_expression exactly once",
                "bytes BYTES|UNKNOWN, checksum STRING|NULL, and material value",
                "BYTES(0) is valid",
                "interrupted-transfer count as effect truth without partial OUTPUT",
            ),
        }
        task_0005_expected = {
            "RESULT-SCHEMAS-0774": (
                "exact closed result.value cardinality, outcome, effect, failure, and OUTPUT contract"
            ),
            "RESULT-SCHEMAS-0775": (
                "exact closed result.collection cardinality, count, outcome, effect, failure, and OUTPUT contract"
            ),
            "RESULT-SCHEMAS-0776": (
                "exact closed result.operation cardinality, outcome, effect truth, failure, and OUTPUT contract"
            ),
            "RESULT-SCHEMAS-0777": (
                "exact closed result.command cardinality, start/completion, mode, effect, failure, and OUTPUT contract"
            ),
            "RESULT-SCHEMAS-0778": (
                "exact closed result.validation cardinality, domain-error, effect, failure, and OUTPUT contract"
            ),
            "RESULT-SCHEMAS-0779": (
                "exact closed result.verification cardinality, evidence, domain-error, effect, failure, and OUTPUT contract"
            ),
            "RESULT-SCHEMAS-0780": (
                "exact closed result.test cardinality, comparison outcome, evidence, effect, failure, and OUTPUT contract"
            ),
            "RESULT-SCHEMAS-0781": (
                "exact closed result.message cardinality, delivery outcome, effect, failure, and OUTPUT contract"
            ),
            "RESULT-SCHEMAS-0782": (
                "exact closed result.transfer cardinality, byte-count, content, effect, failure, and OUTPUT contract"
            ),
        }
        for identifier, subject in task_0005_subjects.items():
            case = by_id.get(identifier, {})
            assert case.get("category") == "result_schemas", (
                f"{identifier} category is not result_schemas"
            )
            assert case.get("subject") == subject, f"{identifier} subject is not exact"
            assert case.get("source") == "built_in_groups_and_results_v0.1.0.json", (
                f"{identifier} source is not exact"
            )
            requirement = case.get("requirement", "")
            assert all(token in requirement for token in task_0005_tokens[identifier]), (
                f"{identifier} is not Task-0005-specific"
            )
            assert case.get("expected") == task_0005_expected[identifier], (
                f"{identifier} expected outcome is not exact"
            )

        task_0005_case_ids = {
            case["id"] for case in cases if case.get("category") == "result_schemas"
        }
        assert task_0005_case_ids == set(task_0005_subjects), (
            "Task-0005 result-schema case set is not exact"
        )
        task_0005_cases = sorted(
            (case for case in cases if case["id"] in task_0005_case_ids),
            key=lambda case: case["id"],
        )
        assert len(task_0005_cases) == 9, "Task-0005 cases are missing"
        task_0005_catalog_sha256 = hashlib.sha256(
            json.dumps(
                task_0005_cases,
                ensure_ascii=False,
                separators=(",", ":"),
                sort_keys=True,
            ).encode("utf-8")
        ).hexdigest()
        assert task_0005_catalog_sha256 == (
            "2c65296a7da92f7d66c3fea373f20832976929476eb6e6f3c160185eba4dfb1d"
        ), "Task-0005 catalog cases differ from the approved contract"

        task_0006_contracts = {
            "DIAGNOSTIC-POLICY-0796": {
                "category": "diagnostic_policy",
                "subject": "core.error_selection",
                "tokens": (
                    "complete metadata for all 77 errors",
                    "exactly one severity, error",
                    "seven registered stages",
                    "every independent unsuppressed diagnostic",
                    "same cause, canonical locus, stage, producer, iteration, and retry attempt",
                    "exact duplicate keys",
                    "without discovery-time influence",
                    "permitted declared-handler recovery",
                    "first unhandled diagnostic as primary",
                    "successful-substitution locals as ordered evidence",
                    "merely retried failed diagnostic remains unhandled",
                    "expression_demand_resolution",
                    "preserve source-validation classification",
                    "select no primary",
                ),
                "expected": (
                    "deterministic unhandled primary and ordered unhandled, handled, and "
                    "successful-substitution local evidence with exact same-cause suppression"
                ),
            },
            "FAILURE-LIFECYCLE-0797": {
                "category": "failure_lifecycle",
                "subject": "core.failure_lifecycle",
                "tokens": (
                    "allowed failure phases for every error",
                    "exposed result producer",
                    "error.dependency.unsatisfied and error.scope.violation as pre_effect",
                    "error.execution.order and error.required.missing",
                    "every additional mixed-phase identifier",
                    "primary unhandled diagnostic's resolved default status after expression_demand_resolution control",
                    "promote the first remaining unhandled diagnostic after recovery",
                    "every applicable diagnostic is recovered",
                    "OUTPUT binding independent",
                    "fail closed for indeterminate state",
                    "safe repetition, resumption, or reconciliation evidence",
                    "not retry exhaustion",
                ),
                "expected": (
                    "exact producer-relative phase, status, effect, OUTPUT, evidence, "
                    "indeterminate, and retry-safety behavior"
                ),
            },
        }
        for identifier, contract in task_0006_contracts.items():
            case = by_id.get(identifier, {})
            assert case.get("category") == contract["category"], (
                f"{identifier} category is not exact"
            )
            assert case.get("subject") == contract["subject"], (
                f"{identifier} subject is not exact"
            )
            assert case.get("source") == "statuses_and_errors_v0.1.0.json", (
                f"{identifier} source is not exact"
            )
            requirement = case.get("requirement", "")
            assert all(token in requirement for token in contract["tokens"]), (
                f"{identifier} is not Task-0006-specific"
            )
            assert case.get("expected") == contract["expected"], (
                f"{identifier} expected outcome is not exact"
            )
        task_0006_case_ids = {
            case["id"]
            for case in cases
            if case.get("category") in {"diagnostic_policy", "failure_lifecycle"}
        }
        assert task_0006_case_ids == set(task_0006_contracts), (
            "Task-0006 case set is not exact"
        )
        task_0006_cases = sorted(
            (case for case in cases if case["id"] in task_0006_case_ids),
            key=lambda case: case["id"],
        )
        task_0006_catalog_sha256 = hashlib.sha256(
            json.dumps(
                task_0006_cases,
                ensure_ascii=False,
                separators=(",", ":"),
                sort_keys=True,
            ).encode("utf-8")
        ).hexdigest()
        # Reviewed demand-stage and successful-substitution corrections; both
        # complete policy cases remain checked against their precise contracts.
        assert task_0006_catalog_sha256 == (
            "0994998cc913eb1b639582255e830c604b9b200fb06bec620b8d3c93cc2310be"
        ), "Task-0006 catalog cases differ from the approved contract"

        operation_axis_case_count = 0
        for case in cases:
            if case.get("category") != "operation_effects":
                continue
            requirement = case.get("requirement", "").lower()
            assert all(
                token in requirement
                for token in ("determinism", "dependency", "effect", "invocation")
            ), f"{case.get('id')} does not cover all operation axes"
            operation_axis_case_count += 1
        assert operation_axis_case_count == 39, "operation-axis case cardinality is not 39"

        return {
            "case_count": len(cases),
            "category_count": len(counts),
            "concrete_case_count": concrete,
            "closed_registry_categories_checked": 23,
            "task_0001_requirements_checked": len(task_0001_tokens),
            "task_0004_requirements_checked": len(task_0004_tokens),
            "task_0004_catalog_cases_checked": len(task_0004_cases),
            "task_0004_catalog_sha256": task_0004_catalog_sha256,
            "task_0004_operation_axis_cases_checked": operation_axis_case_count,
            "task_0005_requirements_checked": len(task_0005_tokens),
            "task_0005_catalog_cases_checked": len(task_0005_cases),
            "task_0005_catalog_sha256": task_0005_catalog_sha256,
            "task_0006_requirements_checked": len(task_0006_contracts),
            "task_0006_catalog_cases_checked": len(task_0006_cases),
            "task_0006_catalog_sha256": task_0006_catalog_sha256,
        }

    before = len(results.checks)
    results.guarded("catalog", "requirements_index_integrity", operation)
    if len(results.checks) > before and results.checks[-1]["status"] == "PASS":
        concrete = results.checks[-1]["details"]["concrete_case_count"]
        results.add(
            "catalog",
            "semantic_case_execution",
            "OUT_OF_SCOPE",
            concrete_case_count=concrete,
            semantic_implementation_present=False,
            classification="BARE_LANGUAGE_IMPLEMENTATION_ARTIFACT",
            reason="The catalog is a descriptive requirements index; an executable semantic engine is outside package scope.",
            retired_package_tasks=[
                "original LCL-TASK-0008 (reference semantic interpreter)",
                "original LCL-TASK-0009 (executable conformance)",
            ],
        )


def check_integrity(root: Path, results: Results) -> None:
    files = {path.relative_to(root).as_posix(): path for path in all_files(root)}

    def manifest_operation() -> dict[str, Any]:
        manifest = load_json_strict(root / "MANIFEST.json")
        records = manifest["files"]
        paths = [record["path"] for record in records]
        assert paths == sorted(paths, key=str.encode), "manifest paths are not bytewise sorted"
        assert len(paths) == len(set(paths)), "duplicate manifest paths"
        expected = set(files) - INTEGRITY_FILES
        assert set(paths) == expected, f"manifest path set mismatch: missing={sorted(expected-set(paths))}, extra={sorted(set(paths)-expected)}"
        for record in records:
            path = files[record["path"]]
            assert record["bytes"] == path.stat().st_size, f"manifest size mismatch: {record['path']}"
            assert record["sha256"] == sha256(path), f"manifest hash mismatch: {record['path']}"
        assert manifest["manifest_record_count"] == len(records)
        return {"record_count": len(records), "manifest_sha256": sha256(root / "MANIFEST.json")}

    def checksum_operation() -> dict[str, Any]:
        path = root / "SHA256SUMS.txt"
        data = path.read_bytes()
        assert data.endswith(b"\n") and b"\r" not in data
        records: dict[str, str] = {}
        ordered = []
        for line in data.decode("utf-8").splitlines():
            match = re.fullmatch(r"([0-9a-f]{64})  (.+)", line)
            assert match, f"invalid checksum line: {line!r}"
            digest, relative = match.groups()
            assert relative not in records, f"duplicate checksum path: {relative}"
            records[relative] = digest
            ordered.append(relative)
        assert ordered == sorted(ordered, key=str.encode), "checksum paths are not bytewise sorted"
        expected = set(files) - {"SHA256SUMS.txt"}
        assert set(records) == expected, f"checksum path set mismatch: missing={sorted(expected-set(records))}, extra={sorted(set(records)-expected)}"
        for relative, digest in records.items():
            assert digest == sha256(files[relative]), f"checksum hash mismatch: {relative}"
        return {"record_count": len(records), "checksum_file_sha256": sha256(path)}

    results.guarded("integrity", "manifest_set_size_hash", manifest_operation)
    results.guarded("integrity", "checksum_set_and_hash", checksum_operation)


def check_release_state(root: Path, results: Results) -> None:
    """Keep language quality evidence separate from permission to assert release."""
    ledger_path = root / "00_RELEASE/05_LANGUAGE_CLOSURE.json"
    if not ledger_path.is_file():
        results.add("release", "language_decisions_and_release_state", "BLOCKED",
                    reason="The language-decision closure ledger is absent.")
        return
    ledger = load_json_strict(ledger_path)
    assert isinstance(ledger, dict) and set(ledger) == {
        "language", "version", "scope", "decisions", "release_gate_permitted"
    }, "language closure ledger fields are not exact"
    assert ledger["language"] == "LCL" and ledger["version"] == "0.1.0"
    assert ledger["scope"] == "bare_language"
    assert type(ledger["release_gate_permitted"]) is bool
    decisions = ledger["decisions"]
    assert isinstance(decisions, list) and decisions, "language decision ledger is empty"
    ids = []
    for decision in decisions:
        assert isinstance(decision, dict) and set(decision) == {"id", "status", "summary"}
        assert isinstance(decision["id"], str) and decision["id"]
        assert decision["status"] in {"closed", "pending"}
        assert isinstance(decision["summary"], str) and decision["summary"].strip()
        ids.append(decision["id"])
    assert len(ids) == len(set(ids)), "language decision IDs are duplicated"
    assert set(ids) == {
        "types_and_references", "expressions_and_functions", "patterns", "library_parameters",
        "control_and_diagnostics", "host_surface_boundary", "normative_alignment",
        "independent_review", "static_validation",
    }, "language decision inventory omits or adds an unreviewed closure area"
    pending = [item["id"] for item in decisions if item["status"] != "closed"]
    reasons = []
    if pending: reasons.append("Language decisions remain pending.")
    if not ledger["release_gate_permitted"]:
        reasons.append("The language decision ledger has not permitted the final release gate.")
    status_text = (root / "00_RELEASE/01_RELEASE_STATUS_AND_BOUNDARY.txt").read_text(encoding="utf-8")
    active_lines = set(status_text.splitlines())
    language_lines = [line for line in active_lines if line.startswith("LANGUAGE_STATUS:")]
    package_lines = [line for line in active_lines if line.startswith("PACKAGE_STATUS:")]
    assert len(language_lines) <= 1 and len(package_lines) <= 1, "conflicting active release status lines"
    assert not language_lines or language_lines[0] in {
        "LANGUAGE_STATUS: INCOMPLETE", "LANGUAGE_STATUS: BARE_SPECIFICATION_COMPLETE"
    }, "unregistered active language status"
    assert not package_lines or package_lines[0] in {
        "PACKAGE_STATUS: UNRELEASED_CANDIDATE", "PACKAGE_STATUS: BARE_LANGUAGE_RELEASE"
    }, "unregistered active package status"
    active_complete = "LANGUAGE_STATUS: BARE_SPECIFICATION_COMPLETE" in active_lines
    active_release = "PACKAGE_STATUS: BARE_LANGUAGE_RELEASE" in active_lines
    # Historical occurrences in prose never count as active status declarations.
    if (pending or not ledger["release_gate_permitted"]) and (active_complete or active_release):
        results.add("release", "language_decisions_and_release_state", "FAIL",
                    pending_decisions=pending,
                    reason="Active completion/release claims precede closure of language decisions.")
        return
    if not active_complete or not active_release:
        reasons.append("Current status metadata still identifies an unreleased candidate.")
    manifest = load_json_strict(root / "MANIFEST.json")
    if not (manifest.get("status") == "bare_language_release"
            and manifest.get("release_ready") is True
            and manifest.get("package_scope") == "bare_language_specification"):
        reasons.append("Manifest release metadata is not in its final bare-language release state.")
    results.add("release", "language_decisions_and_release_state",
                "BLOCKED" if reasons else "PASS", pending_decisions=pending,
                decision_count=len(decisions), release_gate_permitted=ledger["release_gate_permitted"],
                reasons=reasons, integrity_verified_by="separate integrity scope")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[2])
    parser.add_argument("--scope", choices=("all",) + SCOPES, default="all")
    arguments = parser.parse_args()
    root = arguments.root.resolve()
    results = Results()
    selected = SCOPES if arguments.scope == "all" else (arguments.scope,)
    actions = {
        "filesystem": check_filesystem,
        "text": check_text,
        "structured": check_structured,
        "grammar": check_grammar,
        "registry": check_registry,
        "catalog": check_catalog,
        "integrity": check_integrity,
        "release": check_release_state,
    }
    if sys.flags.optimize:
        results.add(
            selected[0],
            "python_optimization_mode",
            "FAIL",
            reason=(
                "Validation uses assertions for fail-closed checks and must not run with "
                "Python optimization enabled."
            ),
        )
    else:
        for scope in selected:
            try:
                actions[scope](root, results)
            except (
                AssertionError,
                KeyError,
                OSError,
                TypeError,
                UnicodeError,
                ValueError,
                json.JSONDecodeError,
            ) as error:
                results.add(scope, "scope_execution", "FAIL", error=str(error))

    counts = {
        status_value: sum(1 for check in results.checks if check["status"] == status_value)
        for status_value in RESULT_STATUSES
    }
    scope_ready = counts["FAIL"] == 0 and counts["BLOCKED"] == 0
    release_ready = arguments.scope == "all" and scope_ready and any(
        check["check"] == "language_decisions_and_release_state" and check["status"] == "PASS"
        for check in results.checks
    )
    output = {
        "tool": "validate_release.py",
        "root": str(root),
        "selected_scopes": list(selected),
        "counts": counts,
        "scope_ready": scope_ready,
        "release_ready": release_ready,
        "checks": results.checks,
    }
    print(json.dumps(output, indent=2, sort_keys=True, ensure_ascii=False))
    return 0 if scope_ready else 1


if __name__ == "__main__":
    sys.exit(main())

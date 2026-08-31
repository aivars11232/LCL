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
import stat
import subprocess
import sys
from pathlib import Path
from typing import Any, Callable


SCOPES = ("filesystem", "text", "structured", "grammar", "registry", "catalog", "integrity")
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
        except (AssertionError, OSError, UnicodeError, ValueError, json.JSONDecodeError) as error:
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
        retired_task="LCL-TASK-0007",
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
        "An operation_identifier or one REF resolving to HANDLER."
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
    "type_or_format_base": (
        "For DEFINE kind.type, one TYPE_EXPRESSION or one REF resolving to DEFINE "
        "kind.type; for DEFINE kind.format, one qualified_identifier(format)."
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
    "terminal_non_success_status": {
        "source": "10_REGISTRIES/statuses_and_errors_v0.1.0.json",
        "pointer": "/statuses",
        "selection": "object_keys_where",
        "where": {"terminal": True},
        "exclude": ["status.succeeded"],
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
        optional = {"where", "exclude", "defined_kind"}
        if not required <= set(contract) or set(contract) - required - optional:
            errors.append(
                f"qualified-identifier domain {domain} has invalid contract keys {sorted(contract)}"
            )
            continue
        source = contract["source"]
        pointer = contract["pointer"]
        selection = contract["selection"]
        defined_kind = contract.get("defined_kind")
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
    expect(constructors.get("DATE", {}).get("profile") == "RFC 3339 full-date", "DATE profile is not RFC 3339")
    expect(
        constructors.get("TIME", {}).get("profile") == "RFC 3339 partial-time with optional time-offset"
        and constructors.get("TIME", {}).get("omitted_timezone") == "UTC",
        "TIME profile/default timezone is not exact",
    )
    expect(
        constructors.get("DATETIME", {}).get("profile")
        == "RFC 3339 full-date T partial-time with optional time-offset"
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
        "style": "workspace-relative minimatch/gitignore-style subset",
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
        "syntax": "conservative_ecmascript_compatible_subset",
        "allowed_flags": ["i", "m", "s"],
        "canonical_flag_order": "ims",
        "default_flags": "",
        "duplicate_flags_allowed": False,
        "unknown_flags_allowed": False,
        "stateful_flags_allowed": False,
        "unicode_text_handling": "always_enabled_independently_of_user_flags",
        "user_unicode_flag_allowed": False,
        "forbidden_features": ["lookbehind", "unicode_property_escapes"],
        "match_semantics": "full_string",
        "full_match_implementation": "semantic_boundary_check_not_multiline_anchors",
        "resource_limit_error": "error.pattern.resource_limit",
    }
    profiles = types.get("pattern_profiles", {})
    expect(profiles.get("GLOB") == expected_glob, "GLOB pattern profile is not exact")
    expect(profiles.get("REGEX") == expected_regex, "REGEX pattern profile is not exact")
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
            "default_status": "status.invalid",
        },
        "error.numeric.division_by_zero": {
            "meaning": (
                "A division denominator is mathematical zero, including a zero MEASURE "
                "numeric component and a quotient evaluated inside ROUND."
            ),
            "stage": "static_or_expression",
            "recoverable_with_declared_handler": False,
            "default_status": "status.invalid",
        },
        "error.numeric.non_terminating": {
            "meaning": (
                "An exact division quotient has no finite base-10 DECIMAL representation "
                "outside the direct first-argument context of ROUND."
            ),
            "stage": "static_or_expression",
            "recoverable_with_declared_handler": False,
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
            "default_status": "status.invalid",
        },
        "error.value.out_of_range": {
            "meaning": "A value violates an exact bound.",
            "stage": "static_or_expression",
            "recoverable_with_declared_handler": False,
            "default_status": "status.invalid",
        },
        "error.host.constraint": {
            "meaning": "A host/provider limitation outside portable LCL prevents execution.",
            "stage": "execution",
            "recoverable_with_declared_handler": True,
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
        "deterministic": "derived",
        "determinism_derivation": {
            "natural_order_membership": (
                "operators_and_functions_v0.1.0.json#/ordered_types"
            ),
            "natural_order_rules": (
                "operators_and_functions_v0.1.0.json#/ordered_type_rules"
            ),
            "property_path_projection": (
                "operators_and_functions_v0.1.0.json#/operators/property access"
            ),
            "key_reference": "validated deterministic side-effect-free kind.operation",
            "list_equal_key_tie_breaker": "original_LIST_source_position",
            "set_equal_key_policy": "distinct_members_require_distinct_keys",
        },
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
                        "DETERMINISTIC TRUE, exactly one PARAMETER accepting T, and exactly "
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
            "error.required.missing",
            "error.value.unknown",
            "error.operation.precondition",
        ],
        "diagnostic_triggers": {
            "error.operation.parameter": (
                "An unknown, duplicate, or mistyped parameter; stable or comparator; "
                "invalid direction; malformed property_path; or invalid key-extractor signature."
            ),
            "error.reference.unresolved": (
                "The key REFERENCE does not resolve exactly once."
            ),
            "error.reference.kind": (
                "The key REFERENCE resolves to a declaration other than kind.operation."
            ),
            "error.required.missing": "A declared key value is MISSING for any member.",
            "error.value.unknown": "A declared key value is UNKNOWN for any member.",
            "error.operation.precondition": (
                "An omitted key lacks natural total order, key values are not mutually "
                "order-compatible, or distinct SET members produce equal keys."
            ),
        },
        "side_effects": [],
        "additional_undeclared_effects": "forbidden",
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
        collection_items == "LIST[T]",
        "result.collection.items does not preserve the required LIST[T] dependency",
    )

    expected_error_fields = {
        "error.operation.parameter": {
            "meaning": (
                "An ACTION omits, duplicates, mistypes, or supplies an unregistered named "
                "operation parameter."
            ),
            "stage": "static_or_expression",
            "recoverable_with_declared_handler": False,
            "default_status": "status.invalid",
        },
        "error.reference.unresolved": {
            "meaning": "REF does not resolve to exactly one declaration/binding.",
            "stage": "resolution",
            "recoverable_with_declared_handler": False,
            "default_status": "status.invalid",
        },
        "error.reference.kind": {
            "meaning": "Reference resolves to a declaration kind illegal in that field.",
            "stage": "resolution",
            "recoverable_with_declared_handler": False,
            "default_status": "status.invalid",
        },
        "error.required.missing": {
            "meaning": (
                "A required value, source, output, evidence item, or declaration is MISSING."
            ),
            "stage": "execution",
            "recoverable_with_declared_handler": True,
            "default_status": "status.blocked",
        },
        "error.value.unknown": {
            "meaning": "A required value is UNKNOWN and no valid handler resolves it.",
            "stage": "static_or_expression",
            "recoverable_with_declared_handler": True,
            "default_status": "status.blocked",
        },
        "error.operation.precondition": {
            "meaning": "A core operation precondition is false, missing, or unknown.",
            "stage": "execution",
            "recoverable_with_declared_handler": False,
            "default_status": "status.failed",
        },
        "error.type.mismatch": {
            "meaning": (
                "A value is incompatible with its declared type or with the type or "
                "registered order domain required by its use context."
            ),
            "stage": "static_or_expression",
            "recoverable_with_declared_handler": False,
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
            "error.type.mismatch before iteration or side effects",
            "core.sort may instead produce",
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
            "error.type.mismatch before any iteration or side effect",
            "returned LIST instead",
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
            "Deterministic: DERIVED",
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
        prose = (root / relative_path).read_text(encoding="utf-8")
        missing = [token for token in required_text if token not in prose]
        expect(not missing, f"{relative_path} is missing SET/sort text: {missing}")

    readme = (root / "README.txt").read_text(encoding="utf-8")
    stale_readme_claims = [
        "unresolved value-kind combinators",
        "field value-kind templates remain unresolved",
        "SET sorting",
        "division, SET",
    ]
    present_stale_claims = [claim for claim in stale_readme_claims if claim in readme]
    expect(
        not present_stale_claims,
        f"README still reports resolved Task-0002/0003 work: {present_stale_claims}",
    )
    expect(
        "closed block, field-signature, value-kind, and parameterized-template contracts"
        in readme,
        "README does not report the closed Task-0002 value-kind contract",
    )

    release_status_requirements = {
        "00_RELEASE/00_CANONICAL_SOURCE_AND_PROVENANCE.txt": [
            "LCL-AUDIT-010/K-003, 011, and 012 are resolved",
            "result-binding portion of LCL-AUDIT-007",
            "LCL-AUDIT-013 through",
            "LCL-AUDIT-016 is outside the bare-language package scope",
        ],
        "00_RELEASE/01_RELEASE_STATUS_AND_BOUNDARY.txt": [
            "LCL-AUDIT-010/K-003, 011, and 012",
            "result-binding portion of LCL-AUDIT-007",
            "LCL-AUDIT-013 through",
            "LCL-AUDIT-016 is outside the bare-language scope",
        ],
    }
    for relative_path, required_text in release_status_requirements.items():
        release_status = (root / relative_path).read_text(encoding="utf-8")
        missing = [token for token in required_text if token not in release_status]
        expect(not missing, f"{relative_path} has stale Task status: {missing}")
        expect(
            "value-kind closure, 011" not in release_status,
            f"{relative_path} still lists resolved value-kind/division/SET work as blocked",
        )

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
                "preserve LIST source position",
                "require distinct keys",
            ],
        ),
        "OPERATION-ERRORS-0573": (
            "operation_errors",
            "core.sort",
            "operations_v0.1.0.json",
            ["operation-parameter defects", "MISSING or UNKNOWN", "equal keys for distinct SET"],
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
            ["result.collection.items is LIST[T]", "without claiming", "cardinality"],
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


def check_registry(root: Path, results: Results) -> None:
    registry = root / "10_REGISTRIES"
    keywords = load_json_strict(registry / "keywords_v0.1.0.json")["keywords"]
    blocks = load_json_strict(registry / "block_schemas_v0.1.0.json")["schemas"]
    fields = load_json_strict(registry / "field_signatures_v0.1.0.json")
    operations = load_json_strict(registry / "operations_v0.1.0.json")["contracts"]
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
    referenced_errors = {error for operation in operations.values() for error in operation["errors"]}
    for family in ("constructors", "operators", "functions"):
        for contract in operator_functions.get(family, {}).values():
            referenced_errors.update(contract.get("errors", []))
    for profile in types.get("pattern_profiles", {}).values():
        resource_error = profile.get("resource_limit_error")
        if resource_error:
            referenced_errors.add(resource_error)
    referenced_results = {operation["result_schema"] for operation in operations.values()}
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

    deterministic_unknown = sorted(name for name, operation in operations.items() if operation["deterministic"] is None)
    incomplete_results = sorted(name for name, schema in results_schema.items() if set(schema) == {"fields"})
    results.add(
        "registry",
        "operation_determinism_and_result_contracts",
        "BLOCKED" if deterministic_unknown or incomplete_results else "PASS",
        undeclared_determinism=deterministic_unknown,
        result_schemas_without_cardinality=incomplete_results,
        blocked_by=["LCL-AUDIT-013"],
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
    diagnostic_metadata = any(
        key in value for value in statuses["errors"].values() for key in ("supersedes", "rank", "severity")
    )
    results.add(
        "registry",
        "diagnostic_selection_contract",
        "BLOCKED" if not diagnostic_metadata else "PASS",
        selection_metadata_present=diagnostic_metadata,
        compatibility_note=compatibility,
        blocked_by=["LCL-AUDIT-014"],
    )

    mixed = [
        name
        for name in ("error.execution.order", "error.scope.violation", "error.required.missing", "error.dependency.unsatisfied")
        if name in statuses["errors"]
    ]
    results.add(
        "registry",
        "mixed_phase_lifecycle_contracts",
        "BLOCKED" if mixed else "PASS",
        unresolved_errors=mixed,
        blocked_by=["LCL-AUDIT-015"],
    )


def check_catalog(root: Path, results: Results) -> None:
    def operation() -> dict[str, Any]:
        catalog = load_json_strict(root / "09_CONFORMANCE/CASES/core_conformance_cases_v0.1.0.json")
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
        assert catalog["case_count"] == len(cases), "case_count mismatch"
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
        exact_subjects("status_transition", set(statuses["statuses"]))
        exact_subjects("error_contract", set(statuses["errors"]))
        exact_subjects("enum_groups", set(groups_and_results["enum_groups"]))
        exact_subjects("result_schemas", set(groups_and_results["result_schemas"]))
        exact_subjects("reserved_namespaces", set(groups_and_results["reserved_namespaces"]))

        by_id = {case["id"]: case for case in cases}
        task_0001_tokens = {
            "KEYWORD-VALID-0129": ("BASE ENUM",),
            "TYPE-VALID-0335": ("bracket",),
            "TYPE-VALID-0337": ("bracket",),
            "TYPE-VALID-0345": ("WORKSPACE", "containment"),
            "TYPE-VALID-0347": ("RFC 3986", "scheme"),
            "TYPE-VALID-0349": ("workspace-relative", "**"),
            "TYPE-VALID-0351": ("ECMAScript", "canonical-order"),
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

        return {
            "case_count": len(cases),
            "category_count": len(counts),
            "concrete_case_count": concrete,
            "closed_registry_categories_checked": 21,
            "task_0001_requirements_checked": len(task_0001_tokens),
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
            retired_tasks=["LCL-TASK-0008", "LCL-TASK-0009"],
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
    }
    for scope in selected:
        try:
            actions[scope](root, results)
        except (AssertionError, KeyError, OSError, UnicodeError, ValueError, json.JSONDecodeError) as error:
            results.add(scope, "scope_execution", "FAIL", error=str(error))

    counts = {
        status_value: sum(1 for check in results.checks if check["status"] == status_value)
        for status_value in RESULT_STATUSES
    }
    scope_ready = counts["FAIL"] == 0 and counts["BLOCKED"] == 0
    release_ready = arguments.scope == "all" and scope_ready
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

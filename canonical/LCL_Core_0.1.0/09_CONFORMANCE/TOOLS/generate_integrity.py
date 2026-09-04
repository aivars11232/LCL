#!/usr/bin/env python3
"""Generate acyclic LCL package manifest and checksum metadata."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import sys
import tempfile
from pathlib import Path
from typing import Any


PACKAGE_STATUSES = {
    "blocked_repair_candidate": ("LCL Core 0.1.0 Repair Candidate", False),
    "bare_specification_complete_candidate": (
        "LCL Core 0.1.0 Bare-Specification-Complete Candidate",
        False,
    ),
    "bare_language_release": ("LCL Core 0.1.0 Bare Language Specification Release", True),
}

OUT_OF_SCOPE_ARTIFACTS = [
    "lexer",
    "parser",
    "interpreter",
    "compiler",
    "runtime",
    "semantic_execution_engine",
    "ui",
    "ide",
    "provider_integration",
    "deployment_tooling",
]

MANIFEST_EXCLUSIONS = {
    "MANIFEST.json": "self-reference",
    "VALIDATION_REPORT.txt": "generated after the manifest and binds the manifest hash",
    "SHA256SUMS.txt": "generated after manifest and validation; checksum self-reference",
}


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def files(root: Path) -> list[Path]:
    return sorted(
        (path for path in root.rglob("*") if path.is_file()),
        key=lambda path: path.relative_to(root).as_posix().encode(),
    )


def load_json(path: Path) -> Any:
    return json.loads(path.read_text(encoding="utf-8"))


def atomic_write(path: Path, content: str) -> None:
    temporary_name: str | None = None
    try:
        with tempfile.NamedTemporaryFile(
            mode="w",
            encoding="utf-8",
            newline="\n",
            prefix=f".{path.name}.",
            suffix=".tmp",
            dir=path.parent,
            delete=False,
        ) as stream:
            temporary_name = stream.name
            stream.write(content)
            stream.flush()
            os.fsync(stream.fileno())
        os.chmod(temporary_name, 0o644)
        os.replace(temporary_name, path)
        temporary_name = None
    finally:
        if temporary_name is not None:
            Path(temporary_name).unlink(missing_ok=True)


def component_counts(root: Path) -> dict[str, int]:
    registries = root / "10_REGISTRIES"
    return {
        "keywords": len(load_json(registries / "keywords_v0.1.0.json")["keywords"]),
        "adopted_symbols": len(load_json(registries / "symbols_v0.1.0.json")["adopted"]),
        "excluded_exact_lexemes": len(load_json(registries / "symbols_v0.1.0.json")["excluded_exact_lexemes"]),
        "types": len(load_json(registries / "types_v0.1.0.json")["types"]),
        "blocks": len(load_json(registries / "block_schemas_v0.1.0.json")["schemas"]),
        "field_signature_blocks": len(load_json(registries / "field_signatures_v0.1.0.json")["blocks"]),
        "field_signatures": sum(
            len(block["fields"])
            for block in load_json(registries / "field_signatures_v0.1.0.json")["blocks"].values()
        ),
        "operators": len(load_json(registries / "operators_and_functions_v0.1.0.json")["operators"]),
        "functions": len(load_json(registries / "operators_and_functions_v0.1.0.json")["functions"]),
        "operations": len(load_json(registries / "operations_v0.1.0.json")["contracts"]),
        "statuses": len(load_json(registries / "statuses_and_errors_v0.1.0.json")["statuses"]),
        "errors": len(load_json(registries / "statuses_and_errors_v0.1.0.json")["errors"]),
        "conformance_requirements": load_json(
            root / "09_CONFORMANCE/CASES/core_conformance_cases_v0.1.0.json"
        )["case_count"],
        "concrete_source_fixtures": len(list((root / "09_CONFORMANCE/SOURCE_FIXTURES").glob("*.lcl"))),
    }


def generate_manifest(root: Path, generated_utc: str, status: str) -> dict[str, Any]:
    if not re.fullmatch(r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z", generated_utc):
        raise ValueError("--generated-utc must be UTC in YYYY-MM-DDTHH:MM:SSZ form")
    if status not in PACKAGE_STATUSES:
        raise ValueError(f"--status must be one of {sorted(PACKAGE_STATUSES)}")
    package_label, release_ready = PACKAGE_STATUSES[status]
    package_files = files(root)
    payload_files = [
        path for path in package_files if path.relative_to(root).as_posix() not in MANIFEST_EXCLUSIONS
    ]
    records = [
        {
            "path": path.relative_to(root).as_posix(),
            "bytes": path.stat().st_size,
            "sha256": sha256(path),
        }
        for path in payload_files
    ]
    manifest = {
        "package": package_label,
        "formal_version": "0.1.0",
        "status": status,
        "release_ready": release_ready,
        "package_scope": "bare_language_specification",
        "out_of_scope_artifacts": OUT_OF_SCOPE_ARTIFACTS,
        "package_root": root.name,
        "generated_utc": generated_utc,
        "package_file_count": len(package_files),
        "manifest_record_count": len(records),
        "checksum_record_count": len(package_files) - 1,
        "integrity_model": {
            "algorithm": "SHA-256",
            "path_basis": "package-root-relative POSIX paths",
            "manifest_exclusions": [
                {"path": path, "reason": reason} for path, reason in MANIFEST_EXCLUSIONS.items()
            ],
            "checksum_exclusions": [
                {"path": "SHA256SUMS.txt", "reason": "self-reference"}
            ],
        },
        "component_counts": component_counts(root),
        "files": records,
    }
    atomic_write(root / "MANIFEST.json", json.dumps(manifest, indent=2, ensure_ascii=False) + "\n")
    return manifest


def generate_checksums(root: Path) -> int:
    checksum_path = root / "SHA256SUMS.txt"
    checksum_files = [path for path in files(root) if path != checksum_path]
    lines = [f"{sha256(path)}  {path.relative_to(root).as_posix()}" for path in checksum_files]
    atomic_write(checksum_path, "\n".join(lines) + "\n")
    return len(lines)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[2])
    subparsers = parser.add_subparsers(dest="command", required=True)
    manifest_parser = subparsers.add_parser("manifest")
    manifest_parser.add_argument("--generated-utc", required=True)
    manifest_parser.add_argument("--status", required=True, choices=sorted(PACKAGE_STATUSES))
    subparsers.add_parser("checksum")
    arguments = parser.parse_args()
    root = arguments.root.resolve()
    try:
        if arguments.command == "manifest":
            manifest = generate_manifest(root, arguments.generated_utc, arguments.status)
            output = {
                "generated": "MANIFEST.json",
                "record_count": manifest["manifest_record_count"],
                "package_file_count": manifest["package_file_count"],
                "status": manifest["status"],
                "release_ready": manifest["release_ready"],
            }
        else:
            output = {"generated": "SHA256SUMS.txt", "record_count": generate_checksums(root)}
        print(json.dumps(output, indent=2, sort_keys=True))
        return 0
    except (OSError, UnicodeError, ValueError, json.JSONDecodeError) as error:
        print(json.dumps({"generated": None, "error": str(error)}, indent=2, sort_keys=True))
        return 1


if __name__ == "__main__":
    sys.exit(main())

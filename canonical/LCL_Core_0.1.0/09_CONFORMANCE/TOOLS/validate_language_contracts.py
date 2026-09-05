#!/usr/bin/env python3
"""Check bare-language contract alignment; never parse or execute LCL source."""

from __future__ import annotations

import argparse
import json
import re
from pathlib import Path


def validate(root: Path) -> dict:
    violations: list[str] = []
    checks = 0

    def expect(condition: bool, message: str) -> None:
        nonlocal checks
        checks += 1
        if not condition:
            violations.append(message)

    def read(relative: str) -> str:
        return (root / relative).read_text(encoding="utf-8")

    def registry(name: str) -> dict:
        return json.loads(read(f"10_REGISTRIES/{name}_v0.1.0.json"))

    types = registry("types")
    fields = registry("field_signatures")
    schemas = registry("block_schemas")["schemas"]
    operations = registry("operations")
    operators = registry("operators_and_functions")
    statuses = registry("statuses_and_errors")
    errors = statuses["errors"]
    grammar = read("04_GRAMMAR/10_COMPLETE_EBNF.ebnf")

    def production(name: str) -> str:
        match = re.search(rf"(?m)^{name}\s*=\s*(.*?);", grammar, re.S)
        expect(match is not None, f"missing grammar production {name}")
        return match.group(1) if match else ""

    expect(production("ARGUMENT").strip() == "EXPRESSION", "expression calls must be positional only")
    expect('"NULL"' not in production("SCALAR_TYPE"), "NULL has duplicate ordinary expression derivations")
    expect('"NULL"' in production("LITERAL"), "NULL material literal is absent")
    expect("REFERENCE_CALL" in production("TYPE_EXPRESSION"), "defined source types lack explicit REF form")
    expect(types["source_type_contract"]["grammar"].endswith("#/TYPE_EXPRESSION"), "source type contract does not bind the grammar")
    expect("source_type_contract" in fields["value_kind_registry"]["type_expression"], "TYPE fields bypass the source type contract")

    for name in ("DEFINE", "ACTION", "STEP", "RETRY", "HANDLER"):
        block = fields["blocks"][name]
        schema = schemas[name]
        expect(block["legal_parents"] == schema["contexts"], f"{name} parent registries disagree")
        expect(block["conditional_requirements"] == schema["rules"], f"{name} conditional rule registries disagree")
        expect(set(block["fields"]) == set(schema["required"] + schema["optional"]), f"{name} field registries disagree")

    retry = fields["blocks"]["RETRY"]
    retry_operation = operations["contracts"]["core.retry"]
    expect(retry["legal_parents"] == ["ACTION"], "RETRY has a non-ACTION parent")
    expect("RETRY" not in fields["blocks"]["STEP"]["fields"], "STEP retains an undefined retry budget")
    expect(fields["blocks"]["ACTION"]["fields"]["RETRY"]["value_kind"] == "nested_block(RETRY)", "RETRY admits an unaddressable reference")
    expect(retry["fields"]["WHEN"]["default"] is True, "RETRY.WHEN has no exact TRUE default")
    expect(retry["fields"]["DELAY"]["default"] == "DURATION(0, unit.second)", "RETRY.DELAY has no exact zero default")
    expect(set(retry_operation["parameters"]) == {"limit"}, "retry operation duplicates block-owned delay/condition")
    expect("at most 1 +" in retry_operation["invocation_resolution"], "retry has no attempt ceiling")

    for domain in ("error", "event", "status"):
        expect(fields["qualified_identifier_domains"][domain]["defined_kind"] == f"kind.{domain}", f"{domain} aliases excluded from receiving domain")
    expect(statuses["statuses"]["status.blocked"]["terminal"] is True, "blocked is not a terminal invocation outcome")
    expect(statuses["statuses"]["status.blocked"]["allowed_next"] == [], "terminal blocked permits an outgoing transition")
    expect("status.skipped" in statuses["statuses"]["status.ready"]["allowed_next"], "skipped is unreachable from ready")

    demand = statuses["diagnostic_selection"]["expression_demand_resolution"]
    expected_demand_errors = {
        "error.literal.invalid", "error.numeric.division_by_zero",
        "error.numeric.non_terminating", "error.numeric.unit_mismatch",
        "error.operator.operand", "error.pattern.mismatch",
        "error.pattern.resource_limit", "error.value.out_of_range",
        "error.required.missing", "error.value.unknown",
        "error.type.mismatch",
    }
    expect(set(demand["eligible_errors"]) == expected_demand_errors, "dynamic expression diagnostics have an unreviewed domain")
    expect(set(demand["eligible_errors"]) <= set(errors), "dynamic diagnostic is unregistered")
    expect(demand["resolved_stage"] == "execution", "dynamic demand keeps a source-validation stage")
    expect(demand["default_status_overrides"] == {"error.required.missing": "status.blocked", "error.value.unknown": "status.blocked"}, "dynamic special-value status differs")
    expect("error.reference.unresolved" not in demand["eligible_errors"], "dynamic demand masks invalid source references")

    truth = operators["unknown_logic"]
    expected_truth = {"NOT FALSE": "TRUE", "NOT TRUE": "FALSE", "NOT UNKNOWN": "UNKNOWN"}
    for left in ("FALSE", "TRUE", "UNKNOWN"):
        for right in ("FALSE", "TRUE", "UNKNOWN"):
            expected_truth[f"{left} AND {right}"] = "FALSE" if "FALSE" in (left, right) else "UNKNOWN" if "UNKNOWN" in (left, right) else "TRUE"
            expected_truth[f"{left} OR {right}"] = "TRUE" if "TRUE" in (left, right) else "UNKNOWN" if "UNKNOWN" in (left, right) else "FALSE"
    expect(truth == expected_truth, "Boolean registry is not the complete strong Kleene table")
    index = operators["operators"]["index access"]
    expect(index["operands"] == ["LIST,INTEGER", "OBJECT,STRING"], "unregistered index overload")
    expect("zero-based" in index["semantics"] and "no negative-index wrapping" in index["semantics"], "index bounds are not closed")
    expect("reference_context_contract" in operators["evaluation_contract"]["reference_read"], "expression references bypass contextual reading")
    expect("UNKNOWN" in operators["quantifier_argument"]["boundary"] and "transient" in operators["quantifier_argument"]["boundary"], "UNKNOWN can escape through material quantifier data")

    for name in ("GLOB", "REGEX"):
        profile = types["pattern_profiles"][name]
        expect(profile.get("closed") is True, f"{name} profile is open")
        expect(profile["invalid_pattern_error"] in errors and profile["resource_limit_error"] in errors, f"{name} diagnostic is unregistered")
        expect(all(profile.get(key) for key in ("decoded_text", "character_class", "matching")), f"{name} lacks decoding/class/matching rules")
    expect(types["pattern_profiles"]["REGEX"]["syntax"] == "closed_lcl_regex_0_1_0", "REGEX depends on an unspecified external dialect")

    fragment = operations["expression_fragment_contract"]
    expect("exactly one EXPRESSION" in fragment["syntax"], "calculation fragment has no exact syntax boundary")
    expect("item" in fragment["predicate"] and "target" in fragment["bindings"], "fragment binding environment is incomplete")
    expect(operations["contracts"]["core.compare"]["parameters"]["criteria"]["default"] == "==", "comparison omission changes strict equality")
    expect("exactly key and items" in operations["contracts"]["core.group"]["postconditions"][0], "group result shape is open")

    # Cross-surface parity is stronger than merely pinning the source registry hash.
    block_prose = read("04_GRAMMAR/08_CORE_BLOCK_SCHEMAS_A.txt") + read("04_GRAMMAR/09_CORE_BLOCK_SCHEMAS_B.txt")
    for name, schema in schemas.items():
        match = re.search(rf"(?m)^{name}\n(.*?)(?=^[A-Z_]+\n|\Z)", block_prose, re.S)
        if not match:
            expect(False, f"block prose missing {name}")
            continue
        section = match.group(1)
        for key, label in (("contexts", "Contexts"), ("required", "Required"), ("optional", "Optional"), ("repeatable", "Repeatable")):
            expect(f"    {label}: {', '.join(schema[key]) or 'none'}\n" in section, f"{name} prose {label} differs from registry")
        expect(re.findall(r"(?m)^    Rule: (.+)$", section) == schema["rules"], f"{name} prose rules differ from registry")

    cases = json.loads(read("09_CONFORMANCE/CASES/language_decision_cases_v0.1.0.json"))
    expect(cases["executed"] is False and cases["evidence_kind"] == "descriptive_language_decision_witnesses", "descriptive witnesses claim executable evidence")
    expect(cases["case_count"] == len(cases["cases"]), "decision witness count differs")
    identifiers = [case["id"] for case in cases["cases"]]
    expect(len(identifiers) == len(set(identifiers)), "duplicate decision witness ID")
    required_contracts = {"source_type", "reference", "index", "call", "keyword_case", "evaluation", "quantifier", "reduction", "collection", "count", "fragment", "compare", "group", "retry", "alias", "continue", "fallback", "status", "pattern"}
    expect(required_contracts <= {case["contract"] for case in cases["cases"]}, "decision witness coverage is incomplete")
    for case in cases["cases"]:
        expect(set(case) == {"id", "contract", "witness", "expected"}, f"{case['id']} witness shape differs")
        used_errors = set(re.findall(r"error\.[a-z_]+\.[a-z_]+", case["expected"]))
        expect(used_errors <= set(errors), f"{case['id']} cites an unregistered diagnostic")

    return {"checks": checks, "violations": violations, "descriptive_witness_count": len(cases["cases"]), "executed_lcl_cases": 0}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[2])
    result = validate(parser.parse_args().root.resolve())
    print(json.dumps(result, indent=2))
    return 1 if result["violations"] else 0


if __name__ == "__main__":
    raise SystemExit(main())

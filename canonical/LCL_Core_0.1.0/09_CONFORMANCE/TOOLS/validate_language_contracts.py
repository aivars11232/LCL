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

    # Independently check the semantic repairs and their normative mirrors.
    # These checks compare specification documents; none executes LCL.
    normalize = lambda value: " ".join(value.split())
    graph = registry("block_schemas")["execution_graph_contract"]
    selected_checks = statuses["check_selection_contract"]
    projection = registry("built_in_groups_and_results")["result_contract"]["output_projection"]
    mirrors = [
        (types["material_identity_contract"], "03_TYPES_AND_VALUES/03_COLLECTIONS_OBJECTS_ENUMS_AND_EQUALITY.txt"),
        (types["object_type_contract"], "03_TYPES_AND_VALUES/05_CUSTOM_TYPES_SCHEMAS_AND_DEFINITIONS.txt"),
        (types["temporal_literal_contract"], "03_TYPES_AND_VALUES/04_TYPED_CONSTRUCTORS_AND_REFERENCES.txt"),
        (graph, "05_SEMANTICS/01_DECLARATION_RESOLUTION_REACHABILITY_AND_EXECUTION_GRAPH.txt"),
        (selected_checks, "05_SEMANTICS/10_VERIFY_TEST_EVIDENCE_SUCCESS_FAILURE_AND_STATUS.txt"),
        ({key: projection[key] for key in ("producer_ownership", "instance_binding", "retry_binding", "read_order")}, "05_SEMANTICS/05_INPUT_DATA_OUTPUT_RESULT_AND_FORMAT.txt"),
    ]
    for contract, path in mirrors:
        text = normalize(read(path))
        for key, value in contract.items():
            expect(normalize(value) in text, f"{path} does not mirror {key}")
    expect(set(types["material_identity_contract"]) == {"scope", "URI", "GLOB", "REGEX", "PATH", "BOOLEAN", "BYTES", "PERCENTAGE", "other_types"}, "material identity domain differs")
    expect(set(types["object_type_contract"]) == {"identity", "constraints", "anonymous", "combined_schema"}, "object type contract is incomplete")
    expect(set(types["temporal_literal_contract"]) == {"date", "time", "datetime", "normalization", "error"}, "temporal literal contract is incomplete")
    expect(set(graph) == {"candidate_graph", "child_order", "activation_identity", "ordering", "parallel", "loop_instances", "successor"}, "execution graph contract is incomplete")
    expect(set(selected_checks) == {"selection", "prerequisites", "demand", "failure", "domain_results", "root_success", "lifecycle"}, "check selection contract is incomplete")
    expect(fields["blocks"]["ACTION"]["fields"]["OUTPUT"]["value_kind"] == "reference(OUTPUT)", "ACTION admits multiple OUTPUT declarations")
    expect("output_projection" in types["reference_context_contract"]["output_instance"], "reference read omits OUTPUT instance scope")
    for state in ("status.ready", "status.validating"):
        expect("status.failed" in statuses["statuses"][state]["allowed_next"], f"{state} cannot represent early execution failure")
    expect("TEST" in errors["error.verification.failed"]["meaning"], "required TEST failure is not classified")
    for text, tokens, label in [
        (graph["candidate_graph"], ("both IF", "never activate"), "graph activation"),
        (graph["activation_identity"], ("error.execution.order", "error.reference.cycle"), "duplicate/cyclic activation"),
        (graph["ordering"], ("error.execution.order", "sibling"), "order edges"),
        (projection["producer_ownership"], ("one", "error.execution.order"), "OUTPUT ownership"),
        (projection["instance_binding"], ("error.reference.unresolved", "No implicit last-value"), "OUTPUT instance scope"),
        (selected_checks["demand"], ("Every selected and applicable VALIDATE", "before any effect", "REQUIRED controls", "skipped check has no result"), "check demand"),
        (selected_checks["selection"], ("material/address TARGET", "actually activated", "not an unselected IF branch"), "check target selection"),
        (types["object_type_contract"]["combined_schema"], ("error.object.schema",), "combined object schema"),
        (types["temporal_literal_contract"]["date"], ("0001", "9999", "400"), "Gregorian date validity"),
        (types["temporal_literal_contract"]["normalization"], ("86400",), "exact time normalization"),
        (operators["evaluation_contract"]["unit_constraint"], ("error.numeric.unit_mismatch", "SUM", "MIN", "MAX"), "common unit constraint"),
    ]:
        for token in tokens:
            expect(token in text, f"{label} omits {token}")
    expect("error.numeric.unit_mismatch" in operators["evaluation_contract"]["errors"], "unit mismatch missing from common diagnostics")
    for name in ("DATE", "TIME", "DATETIME"):
        expect("temporal_literal_contract" in operators["constructors"][name]["profile"], f"{name} profile is not narrowed")
        expect("temporal_literal_contract" in operators["ordered_type_rules"][name], f"{name} order ignores exact temporal keys")
    literal = types["string_literal_contract"]
    expect(literal["decoding_passes"] == 1 and literal["unicode_normalization"] == "none", "string decoding is not exactly once without normalization")
    expected_literal = {
        "source": "02_LEXICAL/07_LITERALS_AND_ESCAPE_SEQUENCES.txt#STRING VALUE DECODING",
        "decoding_passes": 1,
        "unicode_normalization": "none",
        "multiline_indent": "Strip exactly containing declaration indentation plus four spaces from nonblank content lines; keep extra indentation and each content-line LF. Opening LF and closing-line indentation are syntax.",
        "unicode_escape": "Four hex digits denote one non-surrogate scalar, or an immediately adjacent high/low surrogate escape pair denotes its combined scalar. Lone or malformed surrogate escapes use error.literal.escape.",
        "raw_controls": "Raw source forbids C0/C1 and DEL except LF; escaped decoded control scalars are legal STRING values.",
    }
    expect(literal == expected_literal, "string literal registry contract differs from reviewed decoding rules")
    literal_text = read("02_LEXICAL/07_LITERALS_AND_ESCAPE_SEQUENCES.txt")
    for token in ("STRING VALUE DECODING", "surrogate", "LF", "error.literal.escape"):
        expect(token in literal_text, f"literal prose omits {token}")
    for clause in (
        "The opening LF and closing-line indentation are syntax and contribute no value.",
        "Each nonblank content line must begin with the declaration's indentation plus four ASCII spaces; strip exactly that prefix.",
        "Preserve any additional spaces and the LF ending every content line.",
        "A blank content line contributes one LF.",
        "Adjacent opening and closing lines yield the empty STRING.",
        "decode escapes exactly once from left to right.",
        "No Unicode normalization is performed.",
        "A high-surrogate escape in D800..DBFF must immediately be followed by a low-surrogate escape in DC00..DFFF",
        "U+10000 + (high - D800) * 400 + (low - DC00)",
        "An unpaired surrogate, an invalid pair, or any malformed or unsupported escape uses error.literal.escape.",
    ):
        expect(clause in normalize(literal_text), f"literal prose differs from reviewed rule: {clause}")
    multiline = re.search(r"(?m)^MULTILINE_CHARACTER\s*=.*?;", read("04_GRAMMAR/10_COMPLETE_EBNF.ebnf"), re.S)
    expect(multiline is not None and all(token in multiline.group() for token in ("LF", "backslash")), "multiline grammar admits an unclosed raw/escape boundary")
    process = read("01_FOUNDATION/03_NORMATIVE_PROCESSING_MODEL.txt")
    expect(process.index("structural candidate graph membership") < process.index("8. Run every selected"), "check selection precedes candidate membership")
    expect("9. Finalize and check ordering edges" in process, "final graph ordering stage is undefined")
    parameters = operations["contracts"]
    expect(parameters["core.validate"]["parameters"]["schema"]["type"] == "REFERENCE", "undefined material schema encoding is admitted")
    expect(parameters["core.append"]["parameters"]["content"]["type"] == "STRING|LIST[T]", "BYTES count admitted as append content")
    read_range = parameters["core.read"]["parameters"]["range"]
    range_text = " ".join(read_range["constraints"])
    for token in ("exactly unit: STRING, start: INTEGER, and end: INTEGER", "scalar, line, item, or byte", "0 <= start <= end <= sequence length", "error.value.out_of_range", "error.operation.parameter", "No clipping", "LF-terminated", "0..255"):
        expect(token in range_text, f"read range omits {token}")
    expect("error.value.out_of_range" in parameters["core.read"]["errors"], "read range bound diagnostic is undeclared")
    parameter_prose = normalize(read("06_STANDARD_LIBRARY/10_CORE_OPERATION_PARAMETER_RULES.txt"))
    for operation, parameter in (("core.read", "range"), ("core.append", "content"), ("core.validate", "schema")):
        for constraint in parameters[operation]["parameters"][parameter]["constraints"]:
            expect(normalize(constraint) in parameter_prose, f"{operation}.{parameter} prose differs")

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
    required_contracts = {"source_type", "reference", "index", "call", "keyword_case", "evaluation", "quantifier", "reduction", "collection", "count", "fragment", "compare", "group", "retry", "alias", "continue", "fallback", "status", "pattern", "literal", "equality", "object_type", "temporal", "read_range", "check_selection", "graph", "output_instance", "output_owner", "retry_output", "validate_schema", "append_content", "unit_error", "root_completion"}
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

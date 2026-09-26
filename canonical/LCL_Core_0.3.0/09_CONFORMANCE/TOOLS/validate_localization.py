#!/usr/bin/env python3
"""Validate the LCL 0.3.0 localization contract and its conformance fixtures.

Static specification evidence only. This tool re-derives the localization
surface registry, checks the locale profile schema, validates locale profiles,
and evaluates the pre-grammar localization stage (locale directive, locale
selection and localized word classification) for concrete fixture sources.
It performs no grammar, resolution, typing or execution and is not an LCL
implementation.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import runpy
import sys
from pathlib import Path
from typing import Any

VERSION = "0.3.0"
PROFILE_FORMAT = "lcl-locale-profile/1"
REQUIRED_PROFILE_FIELDS = {"coverage", "format", "lcl_version", "locale", "preferred", "provenance", "repertoires", "spellings"}
OPTIONAL_PROFILE_FIELDS = {"display_labels"}
TAG = re.compile(r"([A-Za-z]{2,3})(?:-([A-Za-z]{4}))?(?:-([A-Za-z]{2}|[0-9]{3}))?")
SIMPLE_IDENTIFIER = re.compile(r"[a-z][a-z0-9_]*")
QUALIFIED = re.compile(r"[a-z][a-z0-9_]*(?:\.[a-z][a-z0-9_]*)+")
WORD_FORM = re.compile(r"[A-Z][A-Z_]*")
CONTINUATION = set(range(0x30, 0x3A)) | {0x5F}


class Invalid(Exception):
    def __init__(self, error: str, detail: str, offset: int | None = None) -> None:
        super().__init__(detail)
        self.error = error
        self.detail = detail
        self.offset = offset


def load_json_strict_text(text: str, label: str) -> Any:
    def reject(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
        result: dict[str, Any] = {}
        for key, value in pairs:
            if key in result:
                raise ValueError(f"duplicate JSON key {key!r} in {label}")
            result[key] = value
        return result

    return json.loads(text, object_pairs_hook=reject)


def load_json_strict(path: Path) -> Any:
    return load_json_strict_text(path.read_text(encoding="utf-8"), str(path))


class Contract:
    def __init__(self, root: Path) -> None:
        registries = root / "10_REGISTRIES"
        self.root = root
        self.keywords = load_json_strict(registries / f"keywords_v{VERSION}.json")["keywords"]
        self.symbols = load_json_strict(registries / f"symbols_v{VERSION}.json")
        self.operators = load_json_strict(registries / f"operators_and_functions_v{VERSION}.json")
        self.types = load_json_strict(registries / f"types_v{VERSION}.json")["types"]
        self.groups = load_json_strict(registries / f"built_in_groups_and_results_v{VERSION}.json")
        self.formats = load_json_strict(registries / f"formats_encodings_units_v{VERSION}.json")
        self.operations = load_json_strict(registries / f"operations_v{VERSION}.json")["contracts"]
        self.meta_types = load_json_strict(registries / f"semantic_meta_types_v{VERSION}.json")["meta_types"]
        self.errors = load_json_strict(registries / f"statuses_and_errors_v{VERSION}.json")["errors"]
        self.ambiguous = load_json_strict(registries / f"ambiguous_replacements_v{VERSION}.json")["replacements"]
        self.surface = load_json_strict(registries / f"localization_surface_v{VERSION}.json")
        self.schema = load_json_strict(registries / f"locale_profile_schema_v{VERSION}.json")
        self.letters = {
            name: parse_ranges(entry["letters"])
            for name, entry in self.schema["lexical_repertoires"].items()
            if isinstance(entry, dict)
        }
        self.confusable = {
            parse_code_point(source): parse_code_point(target)
            for source, target in self.schema["confusable_skeleton"]["map"].items()
        }
        self.reserved = set(self.keywords)
        self.canonical_skeletons = {self.skeleton(word): word for word in self.reserved}
        self.display_members = set()
        for group, members in self.groups["enum_groups"].items():
            if all(QUALIFIED.fullmatch(member) for member in members):
                self.display_members.update(members)
        self.display_members.update(self.operations)
        for key in ("formats", "encodings", "units"):
            self.display_members.update(self.formats[key])

    def repertoire_of_letter(self, code_point: int) -> str | None:
        for name, ranges in self.letters.items():
            if in_ranges(code_point, ranges):
                return name
        return None

    def is_word_scalar(self, char: str) -> bool:
        code_point = ord(char)
        return char.isascii() and (char.isalnum() or char == "_") or self.repertoire_of_letter(code_point) is not None

    def skeleton(self, word: str) -> str:
        return "".join(chr(self.confusable.get(ord(char), ord(char))) for char in word)


def parse_code_point(text: str) -> int:
    match = re.fullmatch(r"U\+([0-9A-F]{4,6})", text)
    if not match:
        raise ValueError(f"malformed code point {text!r}")
    return int(match.group(1), 16)


def parse_ranges(items: list[str]) -> list[tuple[int, int]]:
    ranges = []
    for item in items:
        if ".." in item:
            start, end = item.split("..")
            ranges.append((parse_code_point(start), parse_code_point(end)))
        else:
            code_point = parse_code_point(item)
            ranges.append((code_point, code_point))
    return ranges


def in_ranges(code_point: int, ranges: list[tuple[int, int]]) -> bool:
    return any(start <= code_point <= end for start, end in ranges)


def normalize_tag(tag: str) -> str:
    match = TAG.fullmatch(tag)
    if not match or not tag.isascii():
        raise Invalid("error.localization.locale_invalid", f"malformed locale tag {tag!r}")
    language, script, region = match.groups()
    normalized = language.lower()
    if script:
        normalized += "-" + script[0].upper() + script[1:].lower()
    if region:
        normalized += "-" + region.upper()
    return normalized


# ---------------------------------------------------------------- surface ---

def derive_surface(contract: Contract) -> dict[str, Any]:
    problems = []
    reserved_words = {
        word: {
            "classification": "localized_lexeme",
            "registry_category": entry["category"],
            "source": f"keywords_v{VERSION}.json#/keywords/{word}",
        }
        for word, entry in sorted(contract.keywords.items())
    }

    def word_forms(label: str, names: Any) -> dict[str, Any]:
        out = {}
        for name in sorted(names):
            if WORD_FORM.fullmatch(name):
                if name not in contract.keywords:
                    problems.append(f"{label} {name} is a word form but not a reserved word")
                out[name] = {"lexical_form": "reserved_word", "classification": "localized_lexeme", "reserved_word": name}
            else:
                base = re.match(r"[A-Z][A-Z_]*", name)
                if base and base.group(0) in contract.keywords:
                    out[name] = {"lexical_form": "reserved_word_with_symbols", "classification": "localized_lexeme",
                                 "reserved_word": base.group(0), "symbols": "canonical_invariant"}
                else:
                    out[name] = {"lexical_form": "symbol_or_structure", "classification": "canonical_invariant"}
        return out

    def domain(members: list[str], source: str, classification: str = "localized_preferred_display_only") -> dict[str, Any]:
        if not all(QUALIFIED.fullmatch(member) for member in members):
            problems.append(f"{source} has a non-qualified member")
        return {"classification": classification, "source": source, "member_count": len(members)}

    groups = contract.groups["enum_groups"]
    qualified = {}
    for group, members in sorted(groups.items()):
        if all(QUALIFIED.fullmatch(member) for member in members):
            qualified[group] = domain(members, f"built_in_groups_and_results_v{VERSION}.json#/enum_groups/{group}")
    qualified["core_operation_ids"] = domain(list(contract.operations), f"operations_v{VERSION}.json#/contracts")
    for key in ("formats", "encodings", "units"):
        qualified[key] = domain(list(contract.formats[key]), f"formats_encodings_units_v{VERSION}.json#/{key}")
    qualified["semantic_meta_types"] = domain(list(contract.meta_types), f"semantic_meta_types_v{VERSION}.json#/meta_types",
                                              "canonical_invariant")
    metadata = {
        group: {"classification": "canonical_invariant", "source": f"built_in_groups_and_results_v{VERSION}.json#/enum_groups/{group}",
                "member_count": len(members), "note": "registry metadata vocabulary; never a source word"}
        for group, members in sorted(groups.items()) if group not in qualified
    }
    derived = {
        "reserved_words": reserved_words,
        "word_form_cross_checks": {
            "operators": word_forms("operator", contract.operators["operators"]),
            "functions": word_forms("function", contract.operators["functions"]),
            "constructors": word_forms("constructor", contract.operators["constructors"]),
            "types": word_forms("type", contract.types),
        },
        "symbols": {
            "adopted": {symbol: "canonical_invariant" for symbol in contract.symbols["adopted"]},
            "excluded_exact_lexemes": {symbol: "canonical_invariant" for symbol in contract.symbols["excluded_exact_lexemes"]},
        },
        "qualified_identifier_domains": qualified,
        "registry_metadata_vocabularies": metadata,
    }
    if problems:
        raise ValueError("; ".join(problems))
    return derived


def check_surface(contract: Contract) -> dict[str, Any]:
    surface = contract.surface
    derived = derive_surface(contract)
    mismatched = [key for key, value in derived.items() if surface.get(key) != value]
    values = set(surface["classification_values"])
    expected_values = {"localized_lexeme", "localized_preferred_display_only", "canonical_invariant", "user_data_never_translate"}
    used = set()
    for section in ("structural_source_classes", "user_data_token_classes"):
        used.update(surface[section].values())
    for section in ("qualified_identifier_domains", "registry_metadata_vocabularies", "prose_guidance_registries"):
        used.update(entry["classification"] for entry in surface[section].values())
    used.update(entry["classification"] for entry in surface["reserved_words"].values())
    counts_ok = surface["counts"] == {
        "reserved_words": len(surface["reserved_words"]),
        "localized_lexeme": sum(1 for entry in surface["reserved_words"].values() if entry["classification"] == "localized_lexeme"),
        "adopted_symbols": len(surface["symbols"]["adopted"]),
        "excluded_exact_lexemes": len(surface["symbols"]["excluded_exact_lexemes"]),
        "structural_source_classes": len(surface["structural_source_classes"]),
        "qualified_identifier_domains": len(surface["qualified_identifier_domains"]),
        "registry_metadata_vocabularies": len(surface["registry_metadata_vocabularies"]),
        "user_data_token_classes": len(surface["user_data_token_classes"]),
    }
    passed = (not mismatched and values == expected_values and used <= expected_values and counts_ok
              and surface.get("language") == "LCL" and surface.get("version") == VERSION and surface.get("closed") is True
              and set(surface["reserved_words"]) == contract.reserved)
    return {"check": "localization_surface_derivation", "passed": passed, "mismatched_sections": mismatched,
            "counts_ok": counts_ok, "reserved_words": len(surface["reserved_words"])}


def check_schema(contract: Contract) -> dict[str, Any]:
    schema = contract.schema
    problems = []
    if schema.get("language") != "LCL" or schema.get("version") != VERSION or schema.get("closed") is not True:
        problems.append("header")
    if schema.get("profile_format") != PROFILE_FORMAT:
        problems.append("profile_format")
    if set(schema["profile_document"]["fields"]) != REQUIRED_PROFILE_FIELDS:
        problems.append("required profile fields")
    if set(schema["profile_document"]["optional_fields"]) != OPTIONAL_PROFILE_FIELDS:
        problems.append("optional profile fields")
    names = list(contract.letters)
    for index, first in enumerate(names):
        for second in names[index + 1:]:
            if any(a <= d and c <= b for a, b in contract.letters[first] for c, d in contract.letters[second]):
                problems.append(f"repertoires {first} and {second} overlap")
    for name, ranges in contract.letters.items():
        count = sum(end - start + 1 for start, end in ranges)
        if count != schema["lexical_repertoires"][name]["letter_count"]:
            problems.append(f"{name} letter_count")
        if any(0x61 <= start <= 0x7A or 0x61 <= end <= 0x7A or start <= 0x61 <= end for start, end in ranges):
            problems.append(f"{name} includes ASCII lowercase")
    for source, target in contract.confusable.items():
        if contract.repertoire_of_letter(source) == contract.repertoire_of_letter(target):
            problems.append(f"confusable U+{source:04X} maps within one repertoire")
    for rule, error in schema["diagnostics"].items():
        if error not in contract.errors:
            problems.append(f"diagnostic {rule} names unregistered {error}")
        elif error != "error.keyword.unknown" and contract.errors[error]["stage"] != "localization":
            problems.append(f"diagnostic {rule} is not a localization-stage error")
    localization_errors = {name for name, entry in contract.errors.items() if entry["stage"] == "localization"}
    if localization_errors - set(schema["diagnostics"].values()):
        problems.append(f"localization errors without a schema rule: {sorted(localization_errors - set(schema['diagnostics'].values()))}")
    return {"check": "locale_profile_schema", "passed": not problems, "problems": problems}


# --------------------------------------------------------------- profiles ---

def validate_profile(contract: Contract, data: bytes, expected_locale: str | None = None) -> dict[str, Any]:
    identity = "sha256:" + hashlib.sha256(data).hexdigest()
    if data.startswith(b"\xef\xbb\xbf"):
        raise Invalid("error.localization.profile_invalid", "profile has a byte-order mark")
    try:
        text = data.decode("utf-8")
        profile = load_json_strict_text(text, "profile")
    except (UnicodeDecodeError, ValueError) as error:
        raise Invalid("error.localization.profile_invalid", f"profile is not strict UTF-8 JSON: {error}")
    if not isinstance(profile, dict):
        raise Invalid("error.localization.profile_invalid", "profile is not an object")
    fields = set(profile)
    if not REQUIRED_PROFILE_FIELDS <= fields or fields - REQUIRED_PROFILE_FIELDS - OPTIONAL_PROFILE_FIELDS:
        raise Invalid("error.localization.profile_invalid", f"profile fields are not exact: {sorted(fields)}")
    if profile["format"] != PROFILE_FORMAT:
        raise Invalid("error.localization.profile_invalid", "unknown profile format")
    if profile["lcl_version"] != VERSION:
        raise Invalid("error.localization.profile_invalid", f"incompatible LCL version {profile['lcl_version']!r}")
    if not isinstance(profile["locale"], str):
        raise Invalid("error.localization.locale_invalid", "locale is not a string")
    normalized = normalize_tag(profile["locale"])
    if normalized != profile["locale"]:
        raise Invalid("error.localization.profile_invalid", "locale is not written in normalized form")
    if expected_locale is not None and normalized != expected_locale:
        raise Invalid("error.localization.profile_invalid", f"profile locale {normalized} is not the selected {expected_locale}")
    repertoires = profile["repertoires"]
    if (not isinstance(repertoires, list) or not repertoires or repertoires != sorted(set(repertoires))
            or any(name not in contract.letters for name in repertoires)):
        raise Invalid("error.localization.profile_invalid", f"repertoires are not a sorted set of registered names: {repertoires!r}")
    spellings = profile["spellings"]
    if not isinstance(spellings, dict) or not spellings:
        raise Invalid("error.localization.profile_invalid", "spellings is empty or not an object")
    skeletons: dict[str, str] = {}
    for spelling, word in spellings.items():
        if not isinstance(word, str) or word not in contract.reserved:
            raise Invalid("error.localization.profile_invalid", f"{spelling!r} maps to unknown reserved word {word!r}")
        if contract.surface["reserved_words"][word]["classification"] != "localized_lexeme":
            raise Invalid("error.localization.profile_invalid", f"{word} is not localizable")
        if not 1 <= len(spelling) <= 64:
            raise Invalid("error.localization.profile_invalid", f"{spelling!r} length")
        repertoire = contract.repertoire_of_letter(ord(spelling[0]))
        if repertoire is None or repertoire not in repertoires:
            raise Invalid("error.localization.profile_invalid", f"{spelling!r} does not start with a letter of a listed repertoire")
        for char in spelling[1:]:
            code_point = ord(char)
            if code_point not in CONTINUATION and not in_ranges(code_point, contract.letters[repertoire]):
                raise Invalid("error.localization.profile_invalid", f"{spelling!r} has U+{code_point:04X} outside repertoire {repertoire}")
        if SIMPLE_IDENTIFIER.fullmatch(spelling) or QUALIFIED.fullmatch(spelling):
            raise Invalid("error.localization.profile_invalid", f"{spelling!r} has an identifier form")
        if spelling in contract.reserved and spelling != word:
            raise Invalid("error.localization.profile_invalid", f"{spelling!r} is the canonical reserved word {spelling}, not {word}")
        skeleton = contract.skeleton(spelling)
        canonical = contract.canonical_skeletons.get(skeleton)
        if canonical is not None and canonical != word:
            raise Invalid("error.localization.profile_invalid", f"{spelling!r} is confusable with canonical {canonical}")
        if skeleton in skeletons:
            raise Invalid("error.localization.profile_invalid", f"{spelling!r} is confusable with {skeletons[skeleton]!r}")
        skeletons[skeleton] = spelling
    preferred = profile["preferred"]
    if not isinstance(preferred, dict) or any(spellings.get(value) != key for key, value in preferred.items()):
        raise Invalid("error.localization.profile_invalid", "preferred spellings do not map to their reserved words")
    mapped = set(spellings.values())
    coverage = profile["coverage"]
    if (not isinstance(coverage, dict) or set(coverage) != {"complete", "mapped_reserved_words"}
            or coverage["mapped_reserved_words"] != len(mapped)
            or coverage["complete"] is not (mapped == contract.reserved)):
        raise Invalid("error.localization.profile_invalid", "coverage does not describe the mapping")
    provenance = profile["provenance"]
    if (not isinstance(provenance, dict) or set(provenance) != {"provider_class", "provider_identity"}
            or provenance["provider_class"] not in contract.schema["profile_document"]["provider_classes"]
            or not isinstance(provenance["provider_identity"], str) or not provenance["provider_identity"]
            or not provenance["provider_identity"].isascii()):
        raise Invalid("error.localization.profile_invalid", "provenance is not exact")
    labels = profile.get("display_labels", {})
    if not isinstance(labels, dict) or any(key not in contract.display_members or not isinstance(value, str) or not value
                                          for key, value in labels.items()):
        raise Invalid("error.localization.profile_invalid", "display_labels name unregistered identifiers")
    return {"locale": normalized, "identity": identity, "spellings": dict(spellings), "skeletons": set(skeletons)}


# ----------------------------------------------------------------- source ---

def byte_offsets(text: str) -> list[int]:
    offsets, total = [], 0
    for char in text:
        offsets.append(total)
        total += len(char.encode("utf-8"))
    offsets.append(total)
    return offsets


def directive(text: str, offsets: list[int]) -> tuple[str | None, list[Invalid]]:
    problems: list[Invalid] = []
    tag = None
    start = 0
    for number, line in enumerate(text.split("\n")):
        if line.startswith("@locale"):
            if number != 0:
                problems.append(Invalid("error.localization.directive_invalid", "misplaced or repeated locale directive", offsets[start]))
            elif not line.startswith("@locale ") or line.count(" ") != 1 or len(line) == len("@locale "):
                problems.append(Invalid("error.localization.directive_invalid", "malformed locale directive", offsets[start]))
            else:
                try:
                    tag = normalize_tag(line[len("@locale "):])
                except Invalid as error:
                    error.offset = offsets[start + len("@locale ")]
                    problems.append(error)
        start += len(line) + 1
    return tag, problems


def words(contract: Contract, text: str, mask: list[bool], offsets: list[int], skip: int) -> list[tuple[str, int]]:
    found = []
    index = skip
    while index < len(text):
        if mask[index] and contract.is_word_scalar(text[index]):
            end = index
            while end < len(text) and mask[end] and contract.is_word_scalar(text[end]):
                end += 1
            word = text[index:end]
            first = word[0]
            starts = (first.isascii() and first.isupper()) or (not first.isascii())
            if starts and not any("a" <= char <= "z" for char in word):
                found.append((word, offsets[index]))
            index = end
        else:
            index += 1
    return found


def evaluate_source(contract: Contract, data: bytes, profiles: dict[str, dict[str, Any]], outside_string_mask: Any,
                    pin: dict[str, str] | None = None, resolver_available: bool = True) -> dict[str, Any]:
    try:
        text = data.decode("utf-8")
    except UnicodeDecodeError as error:
        # Invalid UTF-8 holds no words to localize; the offset is the first
        # offending byte of the original source, never of a replacement text.
        return {"error": "error.encoding.invalid", "offset": error.start}
    offsets = byte_offsets(text)
    tag, problems = directive(text, offsets)
    if problems:
        first = min(problems, key=lambda item: (item.offset, item.error))
        return {"error": first.error, "offset": first.offset}
    mask, _unclosed = outside_string_mask(text)
    skip = text.index("\n") + 1 if tag is not None else 0
    candidates = words(contract, text, mask, offsets, skip)
    non_canonical = [(word, offset) for word, offset in candidates if word not in contract.reserved]
    selected = None
    record: dict[str, Any] = {}
    if tag is not None:
        if pin is not None and pin["locale"] != tag:
            return {"error": "error.localization.profile_drift", "offset": 0}
        selected = profiles.get(tag) if resolver_available else None
        if selected is None:
            return {"error": "error.localization.profile_unavailable", "offset": 0}
        if pin is not None and pin["identity"] != selected["identity"]:
            return {"error": "error.localization.profile_drift", "offset": 0}
        record = {"method": "explicit", "locale": tag}
    elif pin is not None:
        selected = profiles.get(pin["locale"]) if resolver_available else None
        if selected is None:
            return {"error": "error.localization.profile_unavailable", "offset": 0}
        if pin["identity"] != selected["identity"]:
            return {"error": "error.localization.profile_drift", "offset": 0}
        record = {"method": "pinned", "locale": pin["locale"]}
    elif not non_canonical:
        record = {"method": "canonical"}
    else:
        if not resolver_available:
            return {"error": "error.localization.profile_unavailable", "offset": non_canonical[0][1]}
        matches = sorted(locale for locale, profile in profiles.items()
                         if any(word in profile["spellings"] for word, _ in non_canonical))
        if len(matches) > 1:
            return {"error": "error.localization.detection_ambiguous", "offset": non_canonical[0][1], "matches": matches}
        if not matches:
            return {"error": "error.localization.detection_failed", "offset": non_canonical[0][1]}
        selected = profiles[matches[0]]
        record = {"method": "auto", "locale": matches[0]}
    localization, lexical = [], []
    mapped = []
    for word, offset in candidates:
        if selected is None:
            if word not in contract.reserved:
                lexical.append(("error.keyword.unknown", offset))
            else:
                mapped.append(word)
            continue
        if word in selected["spellings"]:
            mapped.append(selected["spellings"][word])
        elif word in contract.reserved:
            localization.append(("error.localization.mixed", offset))
        elif (len({contract.repertoire_of_letter(ord(char)) for char in word if not (ord(char) in CONTINUATION)}) > 1
              or contract.skeleton(word) in selected["skeletons"]):
            localization.append(("error.localization.confusable", offset))
        else:
            lexical.append(("error.keyword.unknown", offset))
    if localization or lexical:
        error, offset = min(localization or lexical, key=lambda item: (item[1], item[0]))
        return {"error": error, "offset": offset, **record}
    if selected is not None:
        record["identity"] = selected["identity"]
    record["canonical_words"] = mapped
    return record


# --------------------------------------------------------------- fixtures ---

def mutation_cases(contract: Contract, fixtures: Path) -> list[dict[str, Any]]:
    results = []
    lv = fixtures / "profiles" / "lv-LV.json"
    ru = fixtures / "profiles" / "ru-RU.json"
    if not lv.is_file() or not ru.is_file():
        return [{"case": "mutation_bases_present", "passed": False}]
    base_lv = json.loads(lv.read_text(encoding="utf-8"))
    base_ru = json.loads(ru.read_text(encoding="utf-8"))
    cyrillic_a, latin_a = chr(0x0410), chr(0x0041)

    def encode(value: Any) -> bytes:
        return (json.dumps(value, ensure_ascii=False, indent=2, sort_keys=True) + "\n").encode("utf-8")

    def mutate(base: dict[str, Any], change: Any) -> bytes:
        value = json.loads(json.dumps(base))
        change(value)
        return encode(value)

    def add_spelling(value: dict[str, Any], spelling: str, word: str) -> None:
        value["spellings"][spelling] = word

    task_lv = base_lv["preferred"]["TASK"]
    cases = [
        ("valid_lv_roundtrip", encode(base_lv), None),
        ("incompatible_version", mutate(base_lv, lambda v: v.update(lcl_version="0.1.0")), "error.localization.profile_invalid"),
        ("malformed_tag", mutate(base_lv, lambda v: v.update(locale="lv_LV")), "error.localization.locale_invalid"),
        ("unnormalized_tag", mutate(base_lv, lambda v: v.update(locale="LV-lv")), "error.localization.profile_invalid"),
        ("duplicate_key", encode(base_lv).replace(b'"format"', b'"format": "x",\n  "format"', 1), "error.localization.profile_invalid"),
        ("unknown_reserved_word", mutate(base_lv, lambda v: add_spelling(v, "DARBS", "JOB")), "error.localization.profile_invalid"),
        ("spelling_is_other_canonical_word", mutate(base_lv, lambda v: add_spelling(v, "TEST", "TASK")), "error.localization.profile_invalid"),
        ("lowercase_spelling", mutate(base_lv, lambda v: add_spelling(v, "uzdevums", "TASK")), "error.localization.profile_invalid"),
        ("combining_mark", mutate(base_lv, lambda v: add_spelling(v, "UZDEVUMS" + latin_a + chr(0x0304), "TASK")), "error.localization.profile_invalid"),
        ("mixed_repertoire_spelling", mutate(base_ru, lambda v: add_spelling(v, chr(0x0417) + chr(0x0410) + chr(0x0414) + chr(0x0410) + chr(0x0427) + latin_a, "TASK")), "error.localization.profile_invalid"),
        ("confusable_with_profile_spelling", mutate(base_ru, lambda v: add_spelling(v, "BCE", "ALL")), "error.localization.profile_invalid"),
        ("confusable_with_other_canonical_word", mutate(base_ru, lambda v: add_spelling(v, chr(0x0405) + chr(0x0415) + chr(0x0422), "STEP")), "error.localization.profile_invalid"),
        ("unknown_repertoire", mutate(base_lv, lambda v: v.update(repertoires=["greek", "latin"])), "error.localization.profile_invalid"),
        ("preferred_mismatch", mutate(base_lv, lambda v: v["preferred"].update(TASK=base_lv["preferred"]["STEP"])), "error.localization.profile_invalid"),
        ("coverage_mismatch", mutate(base_lv, lambda v: v["coverage"].update(mapped_reserved_words=1)), "error.localization.profile_invalid"),
        ("unknown_provider_class", mutate(base_lv, lambda v: v["provenance"].update(provider_class="vendor")), "error.localization.profile_invalid"),
        ("extra_field", mutate(base_lv, lambda v: v.update(extra=True)), "error.localization.profile_invalid"),
        ("byte_order_mark", b"\xef\xbb\xbf" + encode(base_lv), "error.localization.profile_invalid"),
        ("empty_spellings", mutate(base_lv, lambda v: v.update(spellings={}, preferred={})), "error.localization.profile_invalid"),
        ("display_label_unregistered", mutate(base_lv, lambda v: v.update(display_labels={"status.unheard": "X"})), "error.localization.profile_invalid"),
        ("wrong_selected_locale", encode(base_lv), "error.localization.profile_invalid"),
    ]
    assert cyrillic_a != latin_a and task_lv
    for name, data, expected in cases:
        try:
            validate_profile(contract, data, expected_locale="nl-NL" if name == "wrong_selected_locale" else None)
            actual = None
        except Invalid as error:
            actual = error.error
        results.append({"case": name, "expected": expected or "valid", "actual": actual or "valid", "passed": actual == expected})
    return results


def check_invalid_utf8(contract: Contract, fixtures: Path, profiles: dict[str, dict[str, Any]], outside_string_mask: Any) -> dict[str, Any]:
    """Invalid UTF-8 before or after VERSION is rejected at its original byte offset."""
    base = (fixtures / "sources" / "auto_lv.lcl").read_bytes()
    after = base.index(b"\n", base.index(b"VERSIJA")) + 1
    cases = [
        ("before_version", b"\xff" + base, 0),
        ("after_version", base[:after] + b"\xe2\x82" + base[after:], after),
        ("after_directive", b"@locale lv-LV\n\xc3\x28" + base, 14),
    ]
    problems = []
    for name, data, offset in cases:
        actual = evaluate_source(contract, data, profiles, outside_string_mask)
        view = {"error": actual.get("error"), "offset": actual.get("offset")}
        if view != {"error": "error.encoding.invalid", "offset": offset}:
            problems.append({"case": name, "expected_offset": offset, "actual": view})
    return {"check": "invalid_utf8_rejected_on_original_bytes", "passed": not problems, "problems": problems}


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[2])
    arguments = parser.parse_args()
    root = arguments.root.resolve()
    fixtures = root / "09_CONFORMANCE" / "LOCALIZATION_FIXTURES"
    try:
        contract = Contract(root)
        source_tool = runpy.run_path(str(root / "09_CONFORMANCE" / "TOOLS" / "validate_source_fixtures.py"))
        outside_string_mask = source_tool["outside_string_mask"]
        checks = [check_surface(contract), check_schema(contract)]
        expected = load_json_strict(fixtures / "expected_results.json")
        profile_results = []
        profiles: dict[str, dict[str, Any]] = {}
        listed_profiles = set(expected["profiles"])
        present_profiles = {path.relative_to(fixtures).as_posix() for path in (fixtures / "profiles").glob("*.json")}
        if listed_profiles != present_profiles:
            raise ValueError(f"profile inventory mismatch: {sorted(listed_profiles ^ present_profiles)}")
        for relative, wanted in sorted(expected["profiles"].items()):
            data = (fixtures / relative).read_bytes()
            try:
                validated = validate_profile(contract, data)
                profiles[validated["locale"]] = validated
                actual = "valid"
                if Path(relative).stem != validated["locale"]:
                    actual = "file name differs from locale"
            except Invalid as error:
                actual = error.error
            profile_results.append({"profile": relative, "expected": wanted, "actual": actual, "passed": actual == wanted})
        source_results = []
        listed_sources = set(expected["sources"])
        present_sources = {path.relative_to(fixtures).as_posix() for path in (fixtures / "sources").glob("*.lcl")}
        if listed_sources != present_sources:
            raise ValueError(f"source inventory mismatch: {sorted(listed_sources ^ present_sources)}")
        for relative, case in sorted(expected["sources"].items()):
            available = {locale: profiles[locale] for locale in case["available"]}
            pin = case.get("pin")
            if pin is not None and pin.get("identity") == "@profile":
                pin = {"locale": pin["locale"], "identity": profiles[pin["locale"]]["identity"]}
            actual = evaluate_source(contract, (fixtures / relative).read_bytes(), available, outside_string_mask,
                                     pin=pin, resolver_available=case.get("resolver_available", True))
            actual_view = {key: actual[key] for key in case["expected"] if key in actual}
            source_results.append({"source": relative, "expected": case["expected"], "actual": actual_view,
                                   "passed": actual_view == case["expected"]})
        checks.append(check_invalid_utf8(contract, fixtures, profiles, outside_string_mask))
        mutations = mutation_cases(contract, fixtures)
        passed = (all(check["passed"] for check in checks) and all(result["passed"] for result in profile_results)
                  and all(result["passed"] for result in source_results) and all(result["passed"] for result in mutations))
        output = {
            "tool": "validate_localization.py",
            "scope": "localization_contract_profiles_and_pre_grammar_selection",
            "semantic_execution": "UNVERIFIED",
            "checks": checks,
            "profiles": profile_results,
            "profile_mutations": mutations,
            "sources": source_results,
            "counts": {
                "profiles": len(profile_results), "profile_mutations": len(mutations), "sources": len(source_results),
                "failed": sum(1 for group in (profile_results, mutations, source_results) for item in group if not item["passed"])
                + sum(1 for check in checks if not check["passed"]),
            },
            "passed": passed,
        }
        print(json.dumps(output, indent=2, sort_keys=True, ensure_ascii=False))
        return 0 if passed else 1
    except (OSError, UnicodeError, ValueError, KeyError, json.JSONDecodeError) as error:
        print(json.dumps({"passed": False, "error": f"{type(error).__name__}: {error}"}, indent=2, sort_keys=True))
        return 2


if __name__ == "__main__":
    sys.exit(main())

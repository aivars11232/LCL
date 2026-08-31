#!/usr/bin/env python3
"""Validate the ISO-14977-profile EBNF used by LCL Core.

This is release conformance infrastructure, not an LCL parser or interpreter.
"""

from __future__ import annotations

import argparse
import json
import sys
from dataclasses import dataclass
from pathlib import Path


class GrammarError(ValueError):
    pass


@dataclass(frozen=True)
class Token:
    kind: str
    value: str
    offset: int


@dataclass(frozen=True)
class Node:
    kind: str
    value: str | None = None
    children: tuple["Node", ...] = ()


SYMBOLS = frozenset("=;,|[]{}()")


def tokenize(source: str) -> list[Token]:
    tokens: list[Token] = []
    index = 0
    length = len(source)
    while index < length:
        character = source[index]
        if character.isspace():
            index += 1
            continue
        if source.startswith("(*", index):
            end = source.find("*)", index + 2)
            if end < 0:
                raise GrammarError(f"unclosed comment at byte {index}")
            index = end + 2
            continue
        if character.isalpha() or character == "_":
            end = index + 1
            while end < length and (source[end].isalnum() or source[end] == "_"):
                end += 1
            tokens.append(Token("NAME", source[index:end], index))
            index = end
            continue
        if character in "'\"":
            delimiter = character
            end = source.find(delimiter, index + 1)
            if end < 0:
                raise GrammarError(f"unclosed terminal string at byte {index}")
            tokens.append(Token("TERMINAL", source[index + 1 : end], index))
            index = end + 1
            continue
        if character == "?":
            end = source.find("?", index + 1)
            if end < 0:
                raise GrammarError(f"unclosed special sequence at byte {index}")
            value = source[index + 1 : end].strip()
            if not value:
                raise GrammarError(f"empty special sequence at byte {index}")
            tokens.append(Token("SPECIAL", value, index))
            index = end + 1
            continue
        if character in SYMBOLS:
            tokens.append(Token(character, character, index))
            index += 1
            continue
        raise GrammarError(f"unexpected EBNF character {character!r} at byte {index}")
    tokens.append(Token("EOF", "", length))
    return tokens


class Parser:
    def __init__(self, tokens: list[Token]):
        self.tokens = tokens
        self.position = 0

    @property
    def current(self) -> Token:
        return self.tokens[self.position]

    def take(self, kind: str) -> Token:
        token = self.current
        if token.kind != kind:
            raise GrammarError(
                f"expected {kind}, found {token.kind} at byte {token.offset}"
            )
        self.position += 1
        return token

    def parse(self) -> dict[str, Node]:
        productions: dict[str, Node] = {}
        while self.current.kind != "EOF":
            name_token = self.take("NAME")
            self.take("=")
            definition = self.parse_alternatives()
            self.take(";")
            if name_token.value in productions:
                raise GrammarError(f"duplicate production {name_token.value}")
            productions[name_token.value] = definition
        if not productions:
            raise GrammarError("grammar has no productions")
        return productions

    def parse_alternatives(self) -> Node:
        alternatives = [self.parse_sequence()]
        while self.current.kind == "|":
            self.take("|")
            alternatives.append(self.parse_sequence())
        if len(alternatives) == 1:
            return alternatives[0]
        return Node("alternative", children=tuple(alternatives))

    def parse_sequence(self) -> Node:
        terms = [self.parse_term()]
        while self.current.kind == ",":
            self.take(",")
            terms.append(self.parse_term())
        if len(terms) == 1:
            return terms[0]
        return Node("sequence", children=tuple(terms))

    def parse_term(self) -> Node:
        token = self.current
        if token.kind == "NAME":
            self.position += 1
            return Node("reference", value=token.value)
        if token.kind == "TERMINAL":
            self.position += 1
            return Node("terminal", value=token.value)
        if token.kind == "SPECIAL":
            self.position += 1
            return Node("special", value=token.value)
        delimiters = {
            "(": (")", "group"),
            "[": ("]", "optional"),
            "{": ("}", "repetition"),
        }
        if token.kind in delimiters:
            close, kind = delimiters[token.kind]
            self.position += 1
            child = self.parse_alternatives()
            self.take(close)
            return Node(kind, children=(child,))
        raise GrammarError(f"expected EBNF term at byte {token.offset}")


def references(node: Node) -> set[str]:
    if node.kind == "reference":
        return {node.value or ""}
    found: set[str] = set()
    for child in node.children:
        found.update(references(child))
    return found


def is_nullable(node: Node, nullable: set[str]) -> bool:
    if node.kind == "reference":
        return (node.value or "") in nullable
    if node.kind in {"terminal", "special"}:
        return False
    if node.kind in {"optional", "repetition"}:
        return True
    if node.kind == "alternative":
        return any(is_nullable(child, nullable) for child in node.children)
    return all(is_nullable(child, nullable) for child in node.children)


def is_productive(node: Node, productive: set[str]) -> bool:
    if node.kind == "reference":
        return (node.value or "") in productive
    if node.kind in {"terminal", "special", "optional", "repetition"}:
        return True
    if node.kind == "alternative":
        return any(is_productive(child, productive) for child in node.children)
    return all(is_productive(child, productive) for child in node.children)


def leading_references(node: Node, nullable: set[str]) -> set[str]:
    if node.kind == "reference":
        return {node.value or ""}
    if node.kind in {"terminal", "special"}:
        return set()
    if node.kind in {"group", "optional", "repetition"}:
        return leading_references(node.children[0], nullable)
    if node.kind == "alternative":
        result: set[str] = set()
        for child in node.children:
            result.update(leading_references(child, nullable))
        return result
    result: set[str] = set()
    for child in node.children:
        result.update(leading_references(child, nullable))
        if not is_nullable(child, nullable):
            break
    return result


def find_cycles(graph: dict[str, set[str]]) -> list[list[str]]:
    cycles: set[tuple[str, ...]] = set()

    def visit(node: str, path: list[str]) -> None:
        if node in path:
            cycle = path[path.index(node) :] + [node]
            body = cycle[:-1]
            rotations = [tuple(body[i:] + body[:i]) for i in range(len(body))]
            cycles.add(min(rotations))
            return
        for target in graph.get(node, set()):
            visit(target, path + [node])

    for name in graph:
        visit(name, [])
    return [list(cycle) for cycle in sorted(cycles)]


def validate(path: Path, start: str) -> dict[str, object]:
    source = path.read_text(encoding="utf-8")
    productions = Parser(tokenize(source)).parse()
    names = set(productions)
    if start not in names:
        raise GrammarError(f"missing start production {start}")

    all_references = set().union(*(references(node) for node in productions.values()))
    undefined = sorted(all_references - names)

    reachable = {start}
    changed = True
    while changed:
        changed = False
        for name in tuple(reachable):
            for target in references(productions[name]) & names:
                if target not in reachable:
                    reachable.add(target)
                    changed = True

    nullable: set[str] = set()
    productive: set[str] = set()
    changed = True
    while changed:
        changed = False
        for name, node in productions.items():
            if name not in nullable and is_nullable(node, nullable):
                nullable.add(name)
                changed = True
            if name not in productive and is_productive(node, productive):
                productive.add(name)
                changed = True

    left_graph = {
        name: leading_references(node, nullable) & names
        for name, node in productions.items()
    }
    left_cycles = find_cycles(left_graph)
    diagnostics = {
        "undefined_nonterminals": undefined,
        "unreachable_nonterminals": sorted(names - reachable),
        "unproductive_nonterminals": sorted(names - productive),
        "left_recursive_cycles": left_cycles,
    }
    result: dict[str, object] = {
        "grammar": str(path),
        "profile": "ISO/IEC 14977 subset declared by the grammar",
        "start": start,
        "production_count": len(productions),
        "terminal_count": len(
            {
                token.value
                for token in tokenize(source)
                if token.kind in {"TERMINAL", "SPECIAL"}
            }
        ),
        "nullable_nonterminals": sorted(nullable),
        "diagnostics": diagnostics,
        "passed": not any(diagnostics.values()),
    }
    return result


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("grammar", type=Path)
    parser.add_argument("--start", default="DOCUMENT")
    arguments = parser.parse_args()
    try:
        result = validate(arguments.grammar, arguments.start)
    except (OSError, UnicodeError, GrammarError) as error:
        print(json.dumps({"passed": False, "error": str(error)}, indent=2))
        return 1
    print(json.dumps(result, indent=2, sort_keys=True))
    return 0 if result["passed"] else 1


if __name__ == "__main__":
    sys.exit(main())

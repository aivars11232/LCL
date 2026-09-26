LCL CORE 0.3.0 BARE LANGUAGE SPECIFICATION

Formal language version: 0.3.0
Language-definition state: no active completion status; the independent review
  of the Core 0.3.0 project additions is pending
Package state: UNRELEASED_CANDIDATE; integrity metadata binds candidate bytes only
Package scope: bare language specification only

LCL (Learned Computing Language) is a declarative technical-English language for
expressing operational intent to learned-computing systems. A person writes a
precise specification; a compatible LC interprets that specification and may
produce implementation code or other artifacts. Core 0.2.0 let a document spell
reserved words in a human language through a validated locale profile without
changing meaning. Core 0.3.0 keeps that and lets one specification be written as
a project: one entry and the role-typed part files it declares.

THIS PACKAGE CONTAINS

- the vocabulary and machine-readable symbol inventory;
- multi-file projects: the kind.project entry, eight project part kinds, the
  PART block, one shared project namespace, deterministic project source order,
  required and optional parts, and project admission;
- lexical, grammar, type, value, expression, and execution rules;
- localized source spelling: the locale directive, the localization surface
  registry, the locale profile schema, deterministic locale selection, and
  original-byte locations;
- bracket-only LIST/SET values, enum-only ITEM, and closed typed-constructor and
  GLOB/REGEX profiles;
- closed block, field-signature, value-kind, and parameterized-template
  contracts;
- rule, permission, authority, conflict, state, memory, and evidence semantics;
- closed 39-row core-operation determinism, dependency, effect, invocation, and
  applicable-error contracts, plus pure-function contracts;
- nine closed result schemas with separate execution status, domain outcome,
  failure/effect truth, and explicit OUTPUT PROPERTY projection contracts;
- status and error contracts with deterministic stage selection, specificity,
  supersession, duplicate handling, canonical omission locations, stable order,
  primary/secondary propagation, and declared-handler recovery;
- producer-relative failure-phase, effect-state, OUTPUT-binding,
  retained-evidence, indeterminate-state, and bounded retry-safety contracts;
- versioning and extension rules;
- examples and normative conformance requirements.

SCOPE AND VALIDATION BOUNDARY

- the 830-entry catalog is a descriptive requirements index, not an executed
  semantic suite; executable semantic evidence is outside the bare-language
  package scope and its absence is not a release blocker;
- 74 language-decision witnesses are descriptive; no LCL cases were executed;
- validate_projects.py checks the project contract's structure and 26 project
  fixtures with a bounded static reading, not a parser;
- validate_localization.py checks the localization contract, four minimal
  fixture profiles (lv-LV, nl-NL, ru-RU and zh-CN) and 31 localization source
  fixtures; the fixture profiles prove the mechanism, not translation quality or
  coverage;
- static language-contract checks, the Core 0.1.0 independent semantic reviews
  and the owner-accepted independent review of Core 0.2.0 support the closed
  decisions; the Core 0.3.0 project additions have had no independent review
  yet (see 00_RELEASE/05_LANGUAGE_CLOSURE.json); bounded source-hygiene checks
  are not a complete lexer;
- MANIFEST.json, VALIDATION_REPORT.txt and SHA256SUMS.txt bind the current
  candidate payload for integrity only, and no archive of this candidate exists;
- LCL Core 0.1.0 remains released as canonical/LCL_Core_0.1.0 with its archive
  LCL_Core_0.1.0_Bare_Language_2026-09-05.zip, and LCL Core 0.2.0 remains the
  unreleased candidate canonical/LCL_Core_0.2.0. A filename or status
  declaration alone is never proof that its bytes satisfy the specification.

THIS PACKAGE DOES NOT CONTAIN

- a UI or editor;
- an LCL workspace-management application;
- an LC provider integration;
- a locale detection or translation provider, or a dictionary of LCL vocabulary
  for every human language;
- an interpreter, compiler, parser executable, or runtime;
- a database implementation;
- software, image, video, audio, or 3D domain extensions.

No interpreter file is required to read this specification tree. Every normative
artifact is plain UTF-8 text, EBNF, JSON, or LCL source.

Begin with 00_RELEASE/00_CANONICAL_SOURCE_AND_PROVENANCE.txt,
00_RELEASE/01_RELEASE_STATUS_AND_BOUNDARY.txt, and INDEX.txt.
VALIDATION_REPORT.txt records fresh focused checks bound to the current
MANIFEST.json hash. Final integrity and release checks are recorded outside the
package to avoid a hash cycle. Historical task packages and reports are
provenance, not language inputs.

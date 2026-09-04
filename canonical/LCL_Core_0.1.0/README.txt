LCL CORE 0.1.0 BARE LANGUAGE SPECIFICATION RELEASE

Formal language version: 0.1.0
Language-definition state: BARE_SPECIFICATION_COMPLETE through LCL-TASK-0006
Package state: BARE_LANGUAGE_RELEASE packaged by LCL-TASK-0007
Package scope: bare language specification only

LCL (Learned Computing Language) is a declarative technical-English language for
expressing operational intent to learned-computing systems. A person writes a
precise specification; a compatible LC interprets that specification and may
produce implementation code or other artifacts.

THIS PACKAGE CONTAINS

- the vocabulary and machine-readable symbol inventory;
- lexical, grammar, type, value, expression, and execution rules;
- bracket-only LIST/SET values, enum-only ITEM, and closed typed-constructor and
  GLOB/REGEX profiles;
- closed block, field-signature, value-kind, and parameterized-template contracts;
- rule, permission, authority, conflict, state, memory, and evidence semantics;
- closed 39-row core-operation determinism, dependency, effect, invocation, and
  applicable-error contracts, plus pure-function contracts;
- nine closed result schemas with separate execution status, domain outcome,
  failure/effect truth, and explicit OUTPUT PROPERTY projection contracts;
- status and error contracts with deterministic stage selection, specificity,
  supersession, duplicate handling, canonical omission locations, stable order,
  primary/secondary propagation, and declared-handler recovery;
- producer-relative failure-phase, effect-state, OUTPUT-binding, retained-evidence,
  indeterminate-state, and bounded retry-safety contracts;
- versioning and extension rules;
- examples and normative conformance requirements.

SCOPE BOUNDARIES OF THIS RELEASE

- the 797-entry catalog is a descriptive requirements index, not an executed
  semantic suite; executable semantic evidence is outside the bare-language
  package scope and its absence is not a release blocker;
- MANIFEST.json, VALIDATION_REPORT.txt, and SHA256SUMS.txt were regenerated in
  that acyclic order from the released bytes and validate this tree;
- the release archive is created outside this package root, so no archive file
  is a member of the tree that it packages.

THIS PACKAGE DOES NOT CONTAIN

- a UI or editor;
- an LCL workspace-management application;
- an LC provider integration;
- an interpreter, compiler, parser executable, or runtime;
- a database implementation;
- software, image, video, audio, or 3D domain extensions.

No interpreter file is required to read this candidate tree. Every normative
artifact is plain UTF-8 text, EBNF, JSON, or LCL source.

Begin with 00_RELEASE/00_CANONICAL_SOURCE_AND_PROVENANCE.txt,
00_RELEASE/01_RELEASE_STATUS_AND_BOUNDARY.txt, and INDEX.txt. VALIDATION_REPORT.txt
is the current in-package validation snapshot bound to the exact MANIFEST.json
hash; the external report under reports/ records the final checksum and archive
evidence.

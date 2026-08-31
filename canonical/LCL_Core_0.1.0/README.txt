LCL CORE 0.1.0 REPAIR CANDIDATE

Formal language version: 0.1.0
Release state: blocked partial repair candidate; not release-ready

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
- core operation and pure-function contracts, with unresolved determinism and
  result-binding decisions;
- status and error contracts;
- versioning and extension rules;
- examples and normative conformance requirements.

KNOWN CANDIDATE LIMITATIONS

- result binding, diagnostic selection, and mixed-phase lifecycle behavior require
  owner decisions;
- the 792-entry catalog is a descriptive requirements index, not an executed
  semantic suite; executable semantic evidence is outside the bare-language
  package scope and its absence is not a release blocker;
- no repair archive is produced from this blocked candidate.

THIS PACKAGE DOES NOT CONTAIN

- a UI or editor;
- an LCL workspace-management application;
- an LC provider integration;
- an interpreter, compiler, parser executable, or runtime;
- a database implementation;
- software, image, video, audio, or 3D domain extensions.

No interpreter file is required to read this archive. Every normative artifact is
plain UTF-8 text, EBNF, JSON, or LCL source.

Begin with 00_RELEASE/00_CANONICAL_SOURCE_AND_PROVENANCE.txt,
00_RELEASE/01_RELEASE_STATUS_AND_BOUNDARY.txt, VALIDATION_REPORT.txt, and INDEX.txt.

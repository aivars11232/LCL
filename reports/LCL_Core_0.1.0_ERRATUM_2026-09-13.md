# LCL Core 0.1.0 — non-normative count erratum

- **Erratum date:** 2026-09-13. The recount and verification below were performed on 2026-09-14.
- **Status:** NON-NORMATIVE external erratum.
  - It changes no byte of `canonical/LCL_Core_0.1.0`.
  - It regenerates no manifest or checksum and changes no trust anchor.
  - It is not a new language version.
- **Finding:** DOC-01 (combined F-04), resolved under LCL-CLOSURE-4T Task LCL-CLOSE-02.

## Canonical identity, unchanged

- **Package identity:** `00d648b162939d06c44838481a67c39bc12c64bdd6d105035c24150148fe67ed`, the identity `lcl-spec` pins as its trust anchor.
- **`SHA256SUMS.txt`:** all 175 entries verify against the package on 2026-09-14.

## Corrections

| Where the count is stated | Stated | Correct | Derivation from the authoritative objects |
| --- | --- | --- | --- |
| `04_GRAMMAR/13_EXACT_FIELD_SIGNATURES.txt`, line 11: "Its 335 fields use 67 distinct value-kind expressions." | 335 fields | **334** field uses | `10_REGISTRIES/field_signatures_v0.1.0.json#/blocks/*/fields/*`: 334 field objects across the 41 block objects at `#/blocks` |
| The same line | 67 distinct value-kind expressions | **68** | `#/blocks/*/fields/*/value_kind`: 68 distinct exact strings. No whitespace or ordering normalization merges any two |
| The combined audit summary's canonical inventory, as quoted in the LCL-CLOSURE-4T audit inputs: "11 operators / 12 functions" | 11 operators, 12 functions | **19** operators, **11** functions | `10_REGISTRIES/operators_and_functions_v0.1.0.json#/operators`: 19 keys. `#/functions`: 11 keys. `#/constructors`, a separate population: 11 keys |

**Cross-check.** `MANIFEST.json#/component_counts` agrees with every recount:

| Component | MANIFEST count |
| --- | --- |
| `field_signatures` | 334 |
| `field_signature_blocks` | 41 |
| `blocks` | 41 |
| `operators` | 19 |
| `functions` | 11 |

LCL-CLOSE-02 Phase 0 also found every other MANIFEST component count equal to its registry recount.

## Classification

These are count-only errors, in one prose sentence and one external summary.

- No machine-readable registry, grammar or manifest value is wrong.
- The 334 field objects and their 68 value kinds are exactly what implementations consume.
- No language meaning changes, so no change-control or reissue decision is required.

This erratum stays visible in the release verdict. The package is not claimed to have zero known defects in every
byte.

## Historical reports

Three earlier, non-canonical task results state 65 distinct value kinds over 334 field uses: LCL-TASK-0002,
LCL-TASK-0003 and LCL-TASK-0007. The unchanged registry holds 68 distinct exact strings, and those reports do not
record the counting method that produced 65. They are left unchanged as historical records.

## Inputs (SHA-256)

| File in `canonical/LCL_Core_0.1.0` | SHA-256 |
| --- | --- |
| `04_GRAMMAR/13_EXACT_FIELD_SIGNATURES.txt` | `5af8ec7967bdb371f81f24d83f493ee75406a741b7880a422c5421558998ab36` |
| `MANIFEST.json` | `e79fbc7f9dab4fef30aa341056b4f6753aa2b2b2311e90031b9f397af45e7aea` |
| `10_REGISTRIES/field_signatures_v0.1.0.json` | `d9ba3853abdaf023e9cf9718935b4fcf8133ed613830a5cff13e37dc7a83abfe` |
| `10_REGISTRIES/operators_and_functions_v0.1.0.json` | `a5de41b694bf33d4b4437c77c15009452093bf2a45b8dc919dc005c1207e5f9c` |
| `SHA256SUMS.txt` | `f4a119ed1b8418623d9265c77f61d9cffc5c5250ce486bbbe80e2102ba0943ab` |

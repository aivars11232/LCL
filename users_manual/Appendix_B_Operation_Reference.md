# Appendix B. Operation reference

Every built-in operation of LCL Core 0.1.0, generated from the
specification's operation registry, `10_REGISTRIES/operations_v0.1.0.json`.
The prose version is in `06_STANDARD_LIBRARY/01_...`, `02_...` and `03_...`.

How to read an entry:

* **Target**: the type the ACTION's `TARGET` must have, and whether it is
  required. `meta.material_value` means any ordinary value;
  `meta.addressable` a file, address or declaration that can be located;
  `meta.mutable_target` something that can be changed.
* **Parameters**: the named `PARAMETER`s the operation accepts. Any other
  name is `error.operation.parameter`.
* **Result**: the result record, and the field an OUTPUT receives by default
  (Chapter 8, "Result records and PROPERTY").
* **Effects**: what the operation may change. `none` means it only
  computes.
* **Used in**: the chapters of this manual where the operation runs on the
  reference tool. Operations without a chapter number are part of the
  language, but this manual does not exercise them, and the reference tool
  may need a capability or profile that it does not have.

## Summary table

| Operation | Meaning | Effects | Used in |
|---|---|---|---|
| `core.inspect` | Observe metadata or structure without changing the target. | none | — |
| `core.read` | Retrieve accessible exact content without changing its source. | none | 11 |
| `core.analyze` | Derive stated findings from declared input. | none | — |
| `core.calculate` | Compute a result from declared values and rules. | none | 3, 6, 8 |
| `core.compare` | Compare artifacts or values against exact criteria. | none | 8 |
| `core.select` | Choose members satisfying a predicate. | none | — |
| `core.filter` | Return all members of one LIST that satisfy a predicate, preserving source order. | none | 8, 10 |
| `core.sort` | Return LIST[T] in one declared deterministic total order from LIST[T] or SET[T]. | none | 10 |
| `core.group` | Partition one LIST by one declared key, preserving deterministic group and member order. | none | 10 |
| `core.validate` | Check syntax, type, reference, dependency, and constraints before effects. | none | — |
| `core.verify` | Check observable postconditions. | none | — |
| `core.test` | Evaluate one declared comparison, optionally after executing one referenced TASK or ACTION. | filesystem, network, process, package, message, memory, state | 9 (through TEST) |
| `core.report` | Produce a factual report from declared results and evidence. | none | — |
| `core.return` | Return one declared value to the invoking channel. | none | 2, 8 |
| `core.create` | Establish an authorized target with the declared content or type. | filesystem, network, state | 11, 12 |
| `core.write` | Establish or replace complete content of an authorized target. | filesystem, network, state | 11, 12 |
| `core.append` | Add content at the declared end of an authorized target. | filesystem, network, state | 11 |
| `core.modify` | Change selected content or properties of an existing target. | filesystem, network, state | — |
| `core.copy` | Establish a destination containing an exact copy of a source. | filesystem, network | — |
| `core.move` | Relocate a target without changing declared content. | filesystem, network, state | — |
| `core.rename` | Change a target identifier/path name without changing content. | filesystem, network, state | — |
| `core.delete` | Ensure an authorized target is absent. | filesystem, network, state | — |
| `core.generate` | Produce a new artifact from declared input and constraints. | filesystem, network, state | — |
| `core.convert` | Produce an artifact in a declared target type or format. | filesystem, network, state | — |
| `core.execute` | Run an exact command, program, task, phase, sequence, action, or test. | filesystem, network, process, package, message, memory, state | — |
| `core.install` | Add declared software or resources to an external environment. | filesystem, process, package | — |
| `core.uninstall` | Remove declared software or resources. | filesystem, process, package | — |
| `core.start` | Put a process or service into running state. | process, state | — |
| `core.stop` | Terminate the selected path, process, or service. | process, state | — |
| `core.send` | Transmit declared content to an external recipient or endpoint. | message | — |
| `core.publish` | Make declared content externally available. | filesystem, network | — |
| `core.upload` | Transfer declared content to an external destination. | filesystem, network | — |
| `core.download` | Transfer declared content from an external source into scope. | filesystem, network | — |
| `core.memory_write` | Create or update explicitly authorized persistent memory data. | memory | — |
| `core.state_update` | Update explicitly authorized STATE data. | state | — |
| `core.ask` | Request one exact missing value from an available authoritative source. | message | — |
| `core.retry` | Repeat the associated action under RETRY LIMIT and WHEN. | inherited | 13 (through a handler) |
| `core.continue` | Request continuation at the declared successor after the selected handler successfully handles its originating event. | state | — |
| `core.cancel` | Terminate because the invoking authority cancelled execution. | state | — |

## Details

### `core.inspect`

Observe metadata or structure without changing the target.

* **Target:** `meta.addressable`, required
* **Parameters:**
  * `depth` (`INTEGER`, optional, default `1`): Maximum structural depth.
  * `properties` (`LIST[STRING]`, optional): Exact properties to inspect.
* **Result:** `result.value`, default field `value`
* **Effects:** none; **depends on:** host, network
* **Determinism:** deterministic

### `core.read`

Retrieve accessible exact content without changing its source.

* **Target:** `meta.addressable`, required
* **Parameters:**
  * `format` (`qualified_identifier(format)`, optional): Requested returned representation.
  * `range` (`OBJECT`, optional): Closed zero-based half-open selection in the target representation.
* **Result:** `result.value`, default field `value`
* **Effects:** none; **depends on:** host, network
* **Determinism:** deterministic
* **Used in chapter:** 11

### `core.analyze`

Derive stated findings from declared input.

* **Target:** `meta.material_value|meta.addressable`, required
* **Parameters:**
  * `criteria` (`STRING\|OBJECT\|REFERENCE`, required): Declared analytical criteria.
  * `method` (`STRING\|REFERENCE`, optional): Declared analysis method.
* **Result:** `result.value`, default field `value`
* **Effects:** none; **depends on:** host, network, model
* **Determinism:** nondeterministic

### `core.calculate`

Compute a result from declared values and rules.

* **Target:** `meta.material_value`, optional
* **Parameters:**
  * `expression` (`STRING\|REFERENCE`, required): One expression fragment STRING, or REF to a DEFINE kind.constant whose declared value is such a STRING.
  * `bindings` (`OBJECT`, optional, default `{}`): Named expression bindings.
* **Result:** `result.value`, default field `value`
* **Effects:** none; **depends on:** declared_state_only
* **Determinism:** deterministic
* **Used in chapter:** 3, 6, 8

### `core.compare`

Compare artifacts or values against exact criteria.

* **Target:** `meta.material_value|meta.addressable`, required
* **Parameters:**
  * `against` (`meta.material_value\|meta.addressable`, required): Comparison counterpart.
  * `criteria` (`STRING\|OBJECT\|REFERENCE`, optional, default `"=="`): Exact comparison criteria; omission selects the registered == strict-equality comparison.
* **Result:** `result.value`, default field `value`
* **Effects:** none; **depends on:** host, network
* **Determinism:** deterministic
* **Used in chapter:** 8

### `core.select`

Choose members satisfying a predicate.

* **Target:** `meta.collection`, required
* **Parameters:**
  * `predicate` (`STRING\|REFERENCE`, required): Boolean member predicate.
* **Result:** `result.collection`, default field `items`
* **Effects:** none; **depends on:** declared_state_only
* **Determinism:** nondeterministic

### `core.filter`

Return all members of one LIST that satisfy a predicate, preserving source order.

* **Target:** `LIST[T]`, required
* **Parameters:**
  * `predicate` (`STRING\|REFERENCE`, required): Boolean member predicate.
* **Result:** `result.collection`, default field `items`
* **Effects:** none; **depends on:** declared_state_only
* **Determinism:** deterministic
* **Used in chapter:** 8, 10

### `core.sort`

Return LIST[T] in one declared deterministic total order from LIST[T] or SET[T].

* **Target:** `LIST[T]|SET[T]`, required
* **Parameters:**
  * `key` (`STRING\|REFERENCE`, optional): Optional total-order projection; STRING is one exact property_path and REFERENCE is one validated kind.operation extractor.
  * `direction` (`ENUM[ascending\|descending]`, optional, default `"ascending"`): Sort direction.
* **Result:** `result.collection`, default field `items`
* **Effects:** none; **depends on:** declared_state_only
* **Determinism:** derived
* **Used in chapter:** 10

### `core.group`

Partition one LIST by one declared key, preserving deterministic group and member order.

* **Target:** `LIST[T]`, required
* **Parameters:**
  * `key` (`STRING\|REFERENCE`, required): Exact grouping key.
* **Result:** `result.value`, default field `value`
* **Effects:** none; **depends on:** declared_state_only
* **Determinism:** deterministic
* **Used in chapter:** 10

### `core.validate`

Check syntax, type, reference, dependency, and constraints before effects.

* **Target:** `meta.material_value|meta.addressable|REFERENCE`, required
* **Parameters:**
  * `schema` (`REFERENCE`, optional): Schema to validate against.
  * `rules` (`LIST[REFERENCE]`, optional, default `[]`): Additional VALIDATE declarations.
* **Result:** `result.validation`, default field `valid`
* **Effects:** none; **depends on:** host, network
* **Determinism:** deterministic

### `core.verify`

Check observable postconditions.

* **Target:** `meta.material_value|meta.addressable|REFERENCE`, required
* **Parameters:**
  * `assertion` (`boolean_expression\|REFERENCE[BOOLEAN]`, required): Observable postcondition.
  * `evidence` (`LIST[REFERENCE[EVIDENCE]]`, optional, default `[]`): Required evidence declarations.
* **Result:** `result.verification`, default field `verified`
* **Effects:** none; **depends on:** host, network
* **Determinism:** derived

### `core.test`

Evaluate one declared comparison, optionally after executing one referenced TASK or ACTION.

* **Target:** `REFERENCE[TASK|ACTION]|meta.material_value`, optional
* **Parameters:**
  * `assertion` (`boolean_expression\|REFERENCE[BOOLEAN]`, optional): Direct pass condition.
  * `expected` (`meta.material_value`, optional): Expected value.
  * `actual` (`meta.material_value\|REFERENCE[meta.material_value]`, optional): Actual value.
* **Result:** `result.test`, default field `passed`
* **Effects:** filesystem, network, process, package, message, memory, state; **depends on:** host, network, model, human
* **Determinism:** derived
* **Used in chapter:** 9 (through TEST)

### `core.report`

Produce a factual report from declared results and evidence.

* **Target:** `meta.material_value|REFERENCE`, required
* **Parameters:**
  * `format` (`qualified_identifier(format)`, optional, default `"format.plain_text"`): Report format.
  * `include_evidence` (`BOOLEAN`, optional, default `true`): Include evidence references.
* **Result:** `result.value`, default field `value`
* **Effects:** none; **depends on:** model
* **Determinism:** nondeterministic

### `core.return`

Return one declared value to the invoking channel.

* **Target:** `meta.material_value|REFERENCE`, required
* **Parameters:** none
* **Result:** `result.value`, default field `value`
* **Effects:** none; **depends on:** declared_state_only
* **Determinism:** deterministic
* **Used in chapter:** 2, 8

### `core.create`

Establish an authorized target with the declared content or type.

* **Target:** `meta.mutable_target`, required
* **Parameters:**
  * `content` (`meta.material_value`, optional): Initial content.
  * `target_type` (`STRING\|qualified_identifier(format)`, optional): Created artifact kind.
  * `fail_if_exists` (`BOOLEAN`, optional, default `true`): Reject an existing target when TRUE; when FALSE, permit replacing or reconciling it to the declared post-state.
* **Result:** `result.operation`, default field `changed`
* **Effects:** filesystem, network, state; **depends on:** host, network
* **Determinism:** deterministic
* **Used in chapter:** 11, 12

### `core.write`

Establish or replace complete content of an authorized target.

* **Target:** `meta.mutable_target`, required
* **Parameters:**
  * `content` (`meta.material_value`, required): Complete replacement content.
  * `create_if_missing` (`BOOLEAN`, optional, default `false`): Permit creation if absent.
* **Result:** `result.operation`, default field `changed`
* **Effects:** filesystem, network, state; **depends on:** host, network
* **Determinism:** deterministic
* **Used in chapter:** 11, 12

### `core.append`

Add content at the declared end of an authorized target.

* **Target:** `meta.mutable_target`, required
* **Parameters:**
  * `content` (`STRING\|LIST[T]`, required): Content appended at exact end.
* **Result:** `result.operation`, default field `changed`
* **Effects:** filesystem, network, state; **depends on:** host, network
* **Determinism:** deterministic
* **Used in chapter:** 11

### `core.modify`

Change selected content or properties of an existing target.

* **Target:** `meta.mutable_target`, required
* **Parameters:**
  * `change` (`STRING\|OBJECT\|REFERENCE`, required): Exact change specification.
  * `selection` (`OBJECT\|STRING\|REFERENCE`, optional): Exact bounded selection.
  * `expected_before` (`meta.material_value`, optional): Required pre-state guard.
* **Result:** `result.operation`, default field `changed`
* **Effects:** filesystem, network, state; **depends on:** host, network
* **Determinism:** deterministic

### `core.copy`

Establish a destination containing an exact copy of a source.

* **Target:** `meta.addressable`, required
* **Parameters:**
  * `destination` (`PATH\|URI`, required): Exact destination.
  * `overwrite` (`BOOLEAN`, optional, default `false`): Permit replacement of destination.
* **Result:** `result.transfer`, default field `bytes`
* **Effects:** filesystem, network; **depends on:** host, network
* **Determinism:** deterministic

### `core.move`

Relocate a target without changing declared content.

* **Target:** `meta.addressable`, required
* **Parameters:**
  * `destination` (`PATH\|URI`, required): Exact destination.
  * `overwrite` (`BOOLEAN`, optional, default `false`): Permit replacement of destination.
* **Result:** `result.transfer`, default field `bytes`
* **Effects:** filesystem, network, state; **depends on:** host, network
* **Determinism:** deterministic

### `core.rename`

Change a target identifier/path name without changing content.

* **Target:** `meta.mutable_target`, required
* **Parameters:**
  * `new_name` (`STRING`, required): One exact destination name.
  * `overwrite` (`BOOLEAN`, optional, default `false`): Permit existing destination replacement.
* **Result:** `result.operation`, default field `changed`
* **Effects:** filesystem, network, state; **depends on:** host, network
* **Determinism:** deterministic

### `core.delete`

Ensure an authorized target is absent.

* **Target:** `meta.mutable_target`, required
* **Parameters:**
  * `recursive` (`BOOLEAN`, optional, default `false`): Permit recursive child deletion.
  * `require_exists` (`BOOLEAN`, optional, default `true`): Fail when target is absent.
* **Result:** `result.operation`, default field `changed`
* **Effects:** filesystem, network, state; **depends on:** host, network
* **Determinism:** deterministic

### `core.generate`

Produce a new artifact from declared input and constraints.

* **Target:** `meta.mutable_target|REFERENCE[OUTPUT]`, required
* **Parameters:**
  * `specification` (`STRING\|OBJECT\|REFERENCE`, required): Complete generation specification.
  * `format` (`qualified_identifier(format)`, optional): Required artifact format.
  * `variation` (`OBJECT`, optional, default `{}`): Explicit acceptable-variation bounds.
* **Result:** `result.operation`, default field `changed`
* **Effects:** filesystem, network, state; **depends on:** host, network, model
* **Determinism:** nondeterministic

### `core.convert`

Produce an artifact in a declared target type or format.

* **Target:** `meta.addressable|meta.material_value`, required
* **Parameters:**
  * `target_format` (`qualified_identifier(format)\|type_expression`, required): Required destination representation.
  * `destination` (`PATH\|URI\|REFERENCE[OUTPUT]`, optional): Exact destination.
  * `preserve` (`LIST[STRING]`, optional, default `[]`): Properties required unchanged.
* **Result:** `result.operation`, default field `changed`
* **Effects:** filesystem, network, state; **depends on:** host, network
* **Determinism:** deterministic

### `core.execute`

Run an exact command, program, task, phase, sequence, action, or test.

* **Target:** `PATH|URI|REFERENCE[TASK|PHASE|SEQUENCE|ACTION|TEST]|STRING`, required
* **Parameters:**
  * `arguments` (`LIST[STRING]`, optional, default `[]`): Ordered exact arguments.
  * `working_directory` (`PATH`, optional): Execution directory.
  * `environment` (`OBJECT`, optional, default `{}`): Explicit environment additions/overrides.
  * `timeout` (`DURATION`, optional): Finite execution limit.
* **Result:** `result.command`, default field `stdout`
* **Effects:** filesystem, network, process, package, message, memory, state; **depends on:** host, network, model, human
* **Determinism:** derived

### `core.install`

Add declared software or resources to an external environment.

* **Target:** `STRING|PATH|URI|REFERENCE`, required
* **Parameters:**
  * `version` (`STRING`, optional): Exact package/resource version.
  * `source` (`PATH\|URI`, optional): Exact installation source.
  * `scope` (`STRING\|PATH`, optional): Installation scope.
* **Result:** `result.operation`, default field `changed`
* **Effects:** filesystem, process, package; **depends on:** host, network
* **Determinism:** deterministic

### `core.uninstall`

Remove declared software or resources.

* **Target:** `STRING|REFERENCE`, required
* **Parameters:**
  * `purge_data` (`BOOLEAN`, optional, default `false`): Also delete associated declared data.
* **Result:** `result.operation`, default field `changed`
* **Effects:** filesystem, process, package; **depends on:** host, network
* **Determinism:** deterministic

### `core.start`

Put a process or service into running state.

* **Target:** `PATH|STRING|REFERENCE`, required
* **Parameters:**
  * `arguments` (`LIST[STRING]`, optional, default `[]`): Ordered start arguments.
  * `environment` (`OBJECT`, optional, default `{}`): Explicit environment.
  * `timeout` (`DURATION`, optional): Readiness timeout.
* **Result:** `result.operation`, default field `changed`
* **Effects:** process, state; **depends on:** host, network
* **Determinism:** deterministic

### `core.stop`

Terminate the selected path, process, or service.

* **Target:** `PATH|STRING|REFERENCE[meta.execution_unit]`, required
* **Parameters:**
  * `force` (`BOOLEAN`, optional, default `false`): Permit forced termination.
  * `timeout` (`DURATION`, optional): Graceful stop limit.
* **Result:** `result.operation`, default field `changed`
* **Effects:** process, state; **depends on:** host, network
* **Determinism:** deterministic

### `core.send`

Transmit declared content to an external recipient or endpoint.

* **Target:** `PATH|URI|STRING|BYTES|REFERENCE`, required
* **Parameters:**
  * `recipient` (`STRING\|URI\|REFERENCE`, required): Exact recipient/endpoint.
  * `subject` (`STRING`, optional): Message subject.
  * `format` (`qualified_identifier(format)`, optional): Message representation.
* **Result:** `result.message`, default field `delivered`
* **Effects:** message; **depends on:** host, network
* **Determinism:** deterministic

### `core.publish`

Make declared content externally available.

* **Target:** `PATH|URI|STRING|BYTES|REFERENCE`, required
* **Parameters:**
  * `destination` (`URI\|PATH\|REFERENCE`, required): Exact publication destination.
  * `visibility` (`ENUM[private\|restricted\|public]`, required): Publication visibility.
  * `replace` (`BOOLEAN`, optional, default `false`): Permit replacing existing publication.
* **Result:** `result.operation`, default field `changed`
* **Effects:** filesystem, network; **depends on:** host, network
* **Determinism:** derived

### `core.upload`

Transfer declared content to an external destination.

* **Target:** `PATH|BYTES|REFERENCE`, required
* **Parameters:**
  * `destination` (`URI\|PATH`, required): Exact remote/external destination.
  * `overwrite` (`BOOLEAN`, optional, default `false`): Permit replacement.
  * `checksum` (`STRING`, optional): Expected source checksum.
* **Result:** `result.transfer`, default field `bytes`
* **Effects:** filesystem, network; **depends on:** host, network
* **Determinism:** deterministic

### `core.download`

Transfer declared content from an external source into scope.

* **Target:** `URI|PATH|REFERENCE`, required
* **Parameters:**
  * `destination` (`PATH`, required): Exact local/in-scope destination.
  * `overwrite` (`BOOLEAN`, optional, default `false`): Permit replacement.
  * `checksum` (`STRING`, optional): Expected received checksum.
* **Result:** `result.transfer`, default field `bytes`
* **Effects:** filesystem, network; **depends on:** host, network
* **Determinism:** derived

### `core.memory_write`

Create or update explicitly authorized persistent memory data.

* **Target:** `REFERENCE[MEMORY]`, required
* **Parameters:**
  * `value` (`meta.material_value`, required): New persistent memory value.
  * `merge` (`BOOLEAN`, optional, default `false`): Apply the closed shallow right-biased OBJECT merge instead of replacement.
* **Result:** `result.operation`, default field `changed`
* **Effects:** memory; **depends on:** host
* **Determinism:** deterministic

### `core.state_update`

Update explicitly authorized STATE data.

* **Target:** `REFERENCE[STATE]`, required
* **Parameters:**
  * `value` (`meta.material_value`, required): New state value.
  * `expected_before` (`meta.material_value`, optional): Compare-and-set guard.
* **Result:** `result.operation`, default field `changed`
* **Effects:** state; **depends on:** host
* **Determinism:** deterministic

### `core.ask`

Request one exact missing value from an available authoritative source.

* **Target:** `REFERENCE|qualified_identifier`, required
* **Parameters:**
  * `question` (`STRING`, required): One exact question.
  * `expected_type` (`type_expression`, required): Required answer type.
  * `options` (`LIST[meta.material_value]`, optional): Closed answer choices, each compatible with expected_type.
* **Result:** `result.value`, default field `value`
* **Effects:** message; **depends on:** human
* **Determinism:** nondeterministic

### `core.retry`

Repeat the associated action under RETRY LIMIT and WHEN.

* **Target:** `REFERENCE[ACTION]`, required
* **Parameters:**
  * `limit` (`INTEGER`, required): Additional attempts.
* **Result:** `result.operation`, default field `changed`
* **Effects:** inherited; **depends on:** inherited
* **Determinism:** inherited
* **Used in chapter:** 13 (through a handler)

### `core.continue`

Request continuation at the declared successor after the selected handler successfully handles its originating event.

* **Target:** `REFERENCE[meta.execution_unit]`, required
* **Parameters:** none
* **Result:** `result.operation`, default field `changed`
* **Effects:** state; **depends on:** declared_state_only
* **Determinism:** deterministic

### `core.cancel`

Terminate because the invoking authority cancelled execution.

* **Target:** `REFERENCE[meta.execution_unit]`, required
* **Parameters:**
  * `reason` (`STRING`, required): Recorded cancellation reason.
* **Result:** `result.operation`, default field `changed`
* **Effects:** state; **depends on:** declared_state_only
* **Determinism:** deterministic

## Formats and encodings

Formats (for `FORMAT`): `format.plain_text`, `format.lcl`, `format.json`, `format.yaml`, `format.xml`, `format.markdown`, `format.csv`, `format.tsv`, `format.source_code`, `format.binary`, `format.image`, `format.video`, `format.audio`, `format.document`.

Encodings (for `ENCODING`): `encoding.utf_8`, `encoding.ascii`, `encoding.binary`.

Units are listed in Chapter 5, section 5.4.


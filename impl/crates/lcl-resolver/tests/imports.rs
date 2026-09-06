//! Import, extension, version, checksum and namespace resolution.

mod common;

use common::{canonical_root, ids, resolve, resolve_with};
use lcl_resolver::{ImportOutcome, Outcome};

fn example(name: &str) -> String {
    std::fs::read_to_string(canonical_root().join("08_EXAMPLES/VALID").join(name))
        .unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn header(kind: &str, version: &str) -> String {
    format!(
        "LCL:\n    VERSION: \"0.1.0\"\n\nSPECIFICATION:\n    ID: test.doc\n    NAME: \"T\"\n    VERSION: \"{version}\"\n    KIND: {kind}\n"
    )
}

/// A `kind.library` document with one DATA declaration.
fn library(version: &str) -> String {
    format!(
        "{}\nDATA:\n    ID: data.value\n    TYPE: INTEGER\n    VALUE: 1\n",
        header("kind.library", version)
    )
}

fn importer(body: &str) -> String {
    format!("{}\n{body}", header("kind.data", "1.0.0"))
}

#[test]
fn the_canonical_importing_example_resolves_its_library() {
    // 08_EXAMPLES/VALID/03 imports 02 with PATH("02_IMPORT_LIBRARY.lcl"),
    // relative to the importing file.
    let root = example("03_IMPORTING_TASK.lcl");
    let resolved = resolve_with(
        &root,
        &[("02_IMPORT_LIBRARY.lcl", &example("02_IMPORT_LIBRARY.lcl"))],
    );
    assert_eq!(ids(&resolved), Vec::<String>::new());
    assert_eq!(resolved.unit_count(), 2);
    assert_eq!(resolved.imports().len(), 1);
    assert_eq!(
        resolved.imports()[0].outcome,
        ImportOutcome::Loaded(lcl_resolver::SourceId::new("02_IMPORT_LIBRARY.lcl"))
    );
    assert_eq!(resolved.imports()[0].namespace, "file_rules");
}

#[test]
fn imported_declarations_take_the_importing_prefix() {
    // 07/02: "Imported IDs are referenced namespace.id."
    let root = example("03_IMPORTING_TASK.lcl");
    let resolved = resolve_with(
        &root,
        &[("02_IMPORT_LIBRARY.lcl", &example("02_IMPORT_LIBRARY.lcl"))],
    );
    let qualified: Vec<String> = resolved
        .declarations()
        .all()
        .iter()
        .map(|d| d.id.qualified())
        .collect();
    // The library declares scope.files and rule.no_delete; both are reachable
    // only under the importing prefix.
    assert!(
        qualified.contains(&"file_rules.scope.files".to_string()),
        "{qualified:?}"
    );
    assert!(
        qualified.contains(&"file_rules.rule.no_delete".to_string()),
        "{qualified:?}"
    );
    // The unprefixed spelling is not an identity in the importing document.
    assert!(!qualified.contains(&"scope.files".to_string()));
}

#[test]
fn an_unresolvable_source_is_reported_once() {
    let root = importer("IMPORT:\n    ID: import.missing\n    SOURCE: PATH(\"gone.lcl\")\n    NAMESPACE: gone\n    VERSION: \"1.0.0\"\n");
    let resolved = resolve(&root);
    assert_eq!(ids(&resolved), ["error.import.not_found"]);
    assert_eq!(resolved.terminal_status(), Some("status.invalid"));
    assert_eq!(resolved.imports()[0].outcome, ImportOutcome::NotFound);
}

#[test]
fn an_import_version_must_equal_the_imported_specification_version() {
    let root = importer("IMPORT:\n    ID: import.lib\n    SOURCE: PATH(\"lib.lcl\")\n    NAMESPACE: lib\n    VERSION: \"2.0.0\"\n");
    let resolved = resolve_with(&root, &[("lib.lcl", &library("1.0.0"))]);
    assert_eq!(ids(&resolved), ["error.version.mismatch"]);
    let primary = resolved.primary().expect("primary");
    assert!(primary
        .detail
        .as_ref()
        .expect("detail")
        .contains("\"1.0.0\""));
}

#[test]
fn a_matching_checksum_is_accepted() {
    let lib = library("1.0.0");
    let digest = lcl_spec::sha256::hex_digest(lib.as_bytes());
    let root = importer(&format!(
        "IMPORT:\n    ID: import.lib\n    SOURCE: PATH(\"lib.lcl\")\n    NAMESPACE: lib\n    VERSION: \"1.0.0\"\n    CHECKSUM: \"sha256:{digest}\"\n"
    ));
    let resolved = resolve_with(&root, &[("lib.lcl", &lib)]);
    assert_eq!(ids(&resolved), Vec::<String>::new());
    assert_eq!(resolved.outcome(), Outcome::Resolved);
}

#[test]
fn a_mismatched_checksum_is_rejected() {
    let root = importer(&format!(
        "IMPORT:\n    ID: import.lib\n    SOURCE: PATH(\"lib.lcl\")\n    NAMESPACE: lib\n    VERSION: \"1.0.0\"\n    CHECKSUM: \"sha256:{}\"\n",
        "0".repeat(64)
    ));
    let resolved = resolve_with(&root, &[("lib.lcl", &library("1.0.0"))]);
    assert_eq!(ids(&resolved), ["error.import.checksum"]);
    assert_eq!(
        resolved.imports()[0].outcome,
        ImportOutcome::ChecksumMismatch
    );
    // A rejected import loads nothing.
    assert_eq!(resolved.unit_count(), 1);
}

#[test]
fn a_checksum_in_another_algorithm_is_rejected() {
    // "Core 0.1.0 recognizes only sha256."
    let root = importer(&format!(
        "IMPORT:\n    ID: import.lib\n    SOURCE: PATH(\"lib.lcl\")\n    NAMESPACE: lib\n    VERSION: \"1.0.0\"\n    CHECKSUM: \"sha512:{}\"\n",
        "a".repeat(64)
    ));
    let resolved = resolve_with(&root, &[("lib.lcl", &library("1.0.0"))]);
    assert_eq!(ids(&resolved), ["error.import.checksum"]);
}

#[test]
fn an_uppercase_checksum_digest_is_rejected() {
    // "Checksum form is algorithm:lowercase_hex."
    let lib = library("1.0.0");
    let digest = lcl_spec::sha256::hex_digest(lib.as_bytes()).to_uppercase();
    let root = importer(&format!(
        "IMPORT:\n    ID: import.lib\n    SOURCE: PATH(\"lib.lcl\")\n    NAMESPACE: lib\n    VERSION: \"1.0.0\"\n    CHECKSUM: \"sha256:{digest}\"\n"
    ));
    let resolved = resolve_with(&root, &[("lib.lcl", &lib)]);
    assert_eq!(ids(&resolved), ["error.import.checksum"]);
}

#[test]
fn an_import_cycle_fails() {
    // 07/02: "Import cycles fail."
    let a = format!(
        "{}\nIMPORT:\n    ID: import.b\n    SOURCE: PATH(\"b.lcl\")\n    NAMESPACE: b\n    VERSION: \"1.0.0\"\n",
        header("kind.library", "1.0.0")
    );
    let b = format!(
        "{}\nIMPORT:\n    ID: import.a\n    SOURCE: PATH(\"root.lcl\")\n    NAMESPACE: a\n    VERSION: \"1.0.0\"\n",
        header("kind.library", "1.0.0")
    );
    let resolved = resolve_with(&a, &[("b.lcl", &b), ("root.lcl", &a)]);
    assert!(
        ids(&resolved).contains(&"error.import.cycle".to_string()),
        "got {:?}",
        ids(&resolved)
    );
    let cyclic = resolved
        .imports()
        .iter()
        .find(|r| r.outcome == ImportOutcome::Cycle)
        .expect("a cyclic import is recorded");
    assert_eq!(cyclic.namespace, "a");
}

#[test]
fn a_reserved_namespace_prefix_is_refused() {
    // 07/02: "A prefix cannot be a reserved built-in namespace."
    let root = importer("IMPORT:\n    ID: import.lib\n    SOURCE: PATH(\"lib.lcl\")\n    NAMESPACE: status\n    VERSION: \"1.0.0\"\n");
    let resolved = resolve_with(&root, &[("lib.lcl", &library("1.0.0"))]);
    assert_eq!(ids(&resolved), ["error.namespace.invalid"]);
    // Nothing was requested: the block was rejected before a load.
    assert_eq!(resolved.unit_count(), 1);
}

#[test]
fn two_imports_cannot_own_one_prefix() {
    // 07/02: "Duplicate namespace ownership also uses error.id.duplicate."
    let root = importer(concat!(
        "IMPORT:\n    ID: import.one\n    SOURCE: PATH(\"lib.lcl\")\n    NAMESPACE: lib\n    VERSION: \"1.0.0\"\n\n",
        "IMPORT:\n    ID: import.two\n    SOURCE: PATH(\"other.lcl\")\n    NAMESPACE: lib\n    VERSION: \"1.0.0\"\n",
    ));
    let resolved = resolve_with(
        &root,
        &[
            ("lib.lcl", &library("1.0.0")),
            ("other.lcl", &library("1.0.0")),
        ],
    );
    assert_eq!(ids(&resolved), ["error.id.duplicate"]);
}

#[test]
fn a_local_id_cannot_occupy_an_imported_prefix() {
    // 07/02, verbatim: "namespace library and a local ID library.value collide
    // and use error.id.duplicate."
    let root = importer(concat!(
        "IMPORT:\n    ID: import.lib\n    SOURCE: PATH(\"lib.lcl\")\n    NAMESPACE: library\n    VERSION: \"1.0.0\"\n\n",
        "DATA:\n    ID: library.value\n    TYPE: INTEGER\n    VALUE: 1\n",
    ));
    let resolved = resolve_with(&root, &[("lib.lcl", &library("1.0.0"))]);
    assert_eq!(ids(&resolved), ["error.id.duplicate"]);
    let primary = resolved.primary().expect("primary");
    assert!(
        primary
            .detail
            .as_ref()
            .expect("detail")
            .contains("library.value"),
        "{primary}"
    );
}

#[test]
fn an_extension_must_load_a_kind_extension_document() {
    // 07/03: "An extension is kind.extension."
    // The block's own ID must not occupy the segment its NAMESPACE owns, so
    // `load.one` is spelled apart from the prefix `vocab`.
    let root = importer("EXTENSION:\n    ID: load.one\n    SOURCE: PATH(\"lib.lcl\")\n    NAMESPACE: vocab\n    VERSION: \"1.0.0\"\n");
    let resolved = resolve_with(&root, &[("lib.lcl", &library("1.0.0"))]);
    assert_eq!(ids(&resolved), ["error.extension.invalid"]);
}

#[test]
fn a_definition_only_extension_loads() {
    let ext = format!(
        "{}\nDEFINE:\n    ID: term.kind\n    KIND: kind.term\n    MEANING: \"A term.\"\n",
        header("kind.extension", "1.0.0")
    );
    let root = importer("EXTENSION:\n    ID: load.one\n    SOURCE: PATH(\"ext.lcl\")\n    NAMESPACE: vocab\n    VERSION: \"1.0.0\"\n");
    let resolved = resolve_with(&root, &[("ext.lcl", &ext)]);
    assert_eq!(ids(&resolved), Vec::<String>::new());
    assert_eq!(resolved.outcome(), Outcome::Resolved);
}

#[test]
fn a_floating_lcl_version_is_unsupported() {
    // 08_EXAMPLES/INVALID/12_FLOATING_VERSION.invalid.lcl
    let source = std::fs::read_to_string(
        canonical_root().join("08_EXAMPLES/INVALID/12_FLOATING_VERSION.invalid.lcl"),
    )
    .expect("example present");
    let resolved = resolve(&source);
    assert_eq!(ids(&resolved), ["error.version.unsupported"]);
    assert_eq!(resolved.terminal_status(), Some("status.invalid"));
}

#[test]
fn an_unsupported_version_stops_that_unit_resolving_further() {
    // The document also duplicates an ID. Under 07/05 an interpreter validates
    // "only exact supported language versions", so nothing else about this
    // document is judged under 0.1.0 semantics.
    let source = concat!(
        "LCL:\n    VERSION: \"9.9.9\"\n\n",
        "SPECIFICATION:\n    ID: test.doc\n    NAME: \"T\"\n    VERSION: \"1.0.0\"\n    KIND: kind.data\n\n",
        "DATA:\n    ID: data.same\n    TYPE: INTEGER\n    VALUE: 1\n\n",
        "DATA:\n    ID: data.same\n    TYPE: INTEGER\n    VALUE: 2\n",
    );
    let resolved = resolve(source);
    assert_eq!(ids(&resolved), ["error.version.unsupported"]);
    assert!(resolved.declarations().is_empty());
}

#[test]
fn an_imported_unit_with_an_unsupported_version_is_not_indexed() {
    let lib = "LCL:\n    VERSION: \"9.9.9\"\n\nSPECIFICATION:\n    ID: lib.doc\n    NAME: \"L\"\n    VERSION: \"1.0.0\"\n    KIND: kind.library\n\nDATA:\n    ID: data.value\n    TYPE: INTEGER\n    VALUE: 1\n".to_string();
    let root = importer("IMPORT:\n    ID: import.lib\n    SOURCE: PATH(\"lib.lcl\")\n    NAMESPACE: lib\n    VERSION: \"1.0.0\"\n");
    let resolved = resolve_with(&root, &[("lib.lcl", &lib)]);
    assert_eq!(ids(&resolved), ["error.version.unsupported"]);
    let qualified: Vec<String> = resolved
        .declarations()
        .all()
        .iter()
        .map(|d| d.id.qualified())
        .collect();
    assert!(
        !qualified.iter().any(|q| q.starts_with("lib.")),
        "{qualified:?}"
    );
}

#[test]
fn nested_imports_prepend_each_prefix_in_order() {
    // 07/02: "nested imports prepend each prefix in order."
    let inner = library("1.0.0");
    let middle = format!(
        "{}\nIMPORT:\n    ID: import.inner\n    SOURCE: PATH(\"inner.lcl\")\n    NAMESPACE: inner\n    VERSION: \"1.0.0\"\n",
        header("kind.library", "1.0.0")
    );
    let root = importer("IMPORT:\n    ID: import.mid\n    SOURCE: PATH(\"middle.lcl\")\n    NAMESPACE: mid\n    VERSION: \"1.0.0\"\n");
    let resolved = resolve_with(&root, &[("middle.lcl", &middle), ("inner.lcl", &inner)]);
    assert_eq!(ids(&resolved), Vec::<String>::new());
    let qualified: Vec<String> = resolved
        .declarations()
        .all()
        .iter()
        .map(|d| d.id.qualified())
        .collect();
    assert!(
        qualified.contains(&"mid.inner.data.value".to_string()),
        "{qualified:?}"
    );
}

#[test]
fn one_unit_imported_twice_is_loaded_once_under_both_identities() {
    // "Distinct acyclic import paths do not override one another."
    let root = importer(concat!(
        "IMPORT:\n    ID: import.a\n    SOURCE: PATH(\"lib.lcl\")\n    NAMESPACE: a\n    VERSION: \"1.0.0\"\n\n",
        "IMPORT:\n    ID: import.b\n    SOURCE: PATH(\"lib.lcl\")\n    NAMESPACE: b\n    VERSION: \"1.0.0\"\n",
    ));
    let resolved = resolve_with(&root, &[("lib.lcl", &library("1.0.0"))]);
    assert_eq!(ids(&resolved), Vec::<String>::new());
    assert_eq!(resolved.unit_count(), 2, "the library is loaded once");
    assert_eq!(resolved.paths().len(), 3, "root plus two import paths");
    let qualified: Vec<String> = resolved
        .declarations()
        .all()
        .iter()
        .map(|d| d.id.qualified())
        .collect();
    assert!(
        qualified.contains(&"a.data.value".to_string()),
        "{qualified:?}"
    );
    assert!(
        qualified.contains(&"b.data.value".to_string()),
        "{qualified:?}"
    );
}

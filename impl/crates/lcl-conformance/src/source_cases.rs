//! Concrete source-conformance inputs shared by the tests and report command.
//! Case construction is distinct from the independently pinned obligations.

use crate::{judge, ExecutedCase, Expectation, Reached, Runner};
use lcl_diagnostics::{DiagnosticRegistry, Stage};
use lcl_parser::{FormSet, Grammar};
use lcl_spec::{
    json::{self, Json},
    SpecPackage,
};

#[derive(Debug, Clone)]
pub struct SourceCase {
    pub id: String,
    pub authority: String,
    pub bytes: Vec<u8>,
    pub expectation: Expectation,
}

impl SourceCase {
    pub fn execute(&self, runner: &Runner) -> ExecutedCase {
        let observed = runner.run_source(&self.bytes);
        let verdict = judge(&self.expectation, &observed);
        let source = String::from_utf8(self.bytes.clone())
            .unwrap_or_else(|_| format!("exact source bytes: {:?}", self.bytes));
        ExecutedCase {
            id: self.id.clone(),
            contract: self.authority.clone(),
            source,
            expectation: self.expectation.clone(),
            observed,
            verdict,
        }
    }
}

/// Read only bytes covered by the verified canonical package and recheck their
/// digest at consumption, so a stale handle cannot silently consume drift.
fn canonical_bytes(spec: &SpecPackage, path: &str) -> Result<Vec<u8>, String> {
    if !spec.is_authoritative() {
        return Err("source cases require the approved package".into());
    }
    let expected = spec
        .checksums()
        .get(path)
        .ok_or_else(|| format!("{path} is not a canonical payload file"))?;
    let bytes = std::fs::read(spec.root().join(path)).map_err(|e| format!("{path}: {e}"))?;
    if lcl_spec::sha256::hex_digest(&bytes) != *expected {
        return Err(format!("canonical source drift at {path}"));
    }
    Ok(bytes)
}

pub fn cases(spec: &SpecPackage) -> Result<Vec<SourceCase>, String> {
    let mut cases = Vec::new();
    let fixture_path = "09_CONFORMANCE/SOURCE_FIXTURES/expected_results.json";
    let fixture_bytes = canonical_bytes(spec, fixture_path)?;
    let fixtures = json::parse(std::str::from_utf8(&fixture_bytes).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    for (name, expected) in fixtures.as_object().ok_or("fixture expectations absent")? {
        let expected = expected.as_str().ok_or("non-string fixture expectation")?;
        cases.push(SourceCase {
            id: format!("source/fixture/{name}"),
            authority: fixture_path.into(),
            bytes: canonical_bytes(spec, &format!("09_CONFORMANCE/SOURCE_FIXTURES/{name}"))?,
            expectation: if expected == "accept" {
                Expectation::SourcePass(Reached::Lexical)
            } else {
                Expectation::Rejects(expected.into())
            },
        });
    }
    let diagnostics = DiagnosticRegistry::load(spec).map_err(|e| e.to_string())?;
    for kind in ["VALID", "INVALID"] {
        let prefix = format!("08_EXAMPLES/{kind}/");
        for path in spec
            .file_records()
            .keys()
            .filter(|p| p.starts_with(&prefix) && p.ends_with(".lcl"))
        {
            let expectation = if kind == "VALID" {
                Expectation::SourcePass(Reached::Grammar)
            } else {
                let text =
                    String::from_utf8(canonical_bytes(spec, &format!("{path}.expected.txt"))?)
                        .map_err(|e| e.to_string())?;
                let id = text
                    .lines()
                    .find_map(|line| line.strip_prefix("EXPECTED_ERROR: "))
                    .ok_or("example lacks expected diagnostic")?;
                let diagnostic = diagnostics
                    .error(id)
                    .ok_or("unregistered example diagnostic")?;
                if matches!(diagnostic.stage, Stage::Lexical | Stage::GrammarOrSchema) {
                    // All diagnostics are retained: example 10's missing SCOPE
                    // is secondary to its forbidden document-kind context.
                    Expectation::Diagnostic(id.into())
                } else {
                    Expectation::SourcePass(Reached::Grammar)
                }
            };
            cases.push(SourceCase {
                id: format!("source/example/{kind}/{}", &path[prefix.len()..]),
                authority: path.clone(),
                bytes: canonical_bytes(spec, path)?,
                expectation,
            });
        }
    }
    let keywords = spec
        .registry("keywords")
        .and_then(|r| r.get("keywords"))
        .and_then(Json::as_object)
        .ok_or("keywords absent")?;
    for (word, _) in keywords {
        for (suffix, spelling, expectation) in [
            (
                "exact",
                word.clone(),
                Expectation::SourcePass(Reached::Lexical),
            ),
            (
                "case",
                format!("{}{}", &word[..1], word[1..].to_ascii_lowercase()),
                Expectation::Rejects("error.keyword.case".into()),
            ),
        ] {
            cases.push(SourceCase {
                id: format!("source/keyword/{word}/{suffix}"),
                authority: format!("10_REGISTRIES/keywords_v0.1.0.json#/keywords/{word}"),
                bytes: format!("VALUE: {spelling}\n").into_bytes(),
                expectation,
            });
        }
    }
    cases.extend(symbol_cases(spec)?);
    cases.extend(grammar_cases(spec)?);
    cases.extend(block_cases(spec)?);
    cases.extend(field_form_cases(spec)?);
    cases.extend(field_constraint_cases(spec)?);
    Ok(cases)
}



fn symbol_cases(spec: &SpecPackage) -> Result<Vec<SourceCase>, String> {
    let symbols = spec.registry("symbols").ok_or("symbols absent")?;
    let mut cases = Vec::new();
    for (index, (symbol, _)) in symbols.get("adopted").and_then(Json::as_object).ok_or("adopted symbols absent")?.iter().enumerate() {
        let source = match symbol.as_str() {
            ":" => "VALUE: 1\n".into(),
            "\"" => "VALUE: \"text\"\n".into(),
            "\"\"\"" => "VALUE: \"\"\"\n    text\n\"\"\"\n".into(),
            "\\" => "VALUE: \"\\n\"\n".into(),
            "(" | ")" => "VALUE: (1)\n".into(),
            "[" | "]" | "," => "VALUE: [1, 2]\n".into(),
            "." => "VALUE: sample.value\n".into(),
            "_" => "VALUE: sample_value\n".into(),
            "+" | "-" | "*" | "/" | "==" | "!=" | "<" | "<=" | ">" | ">=" => format!("VALUE: 1 {symbol} 2\n"),
            _ => return Err(format!("unmapped adopted symbol {symbol}")),
        };
        cases.push(SourceCase { id: format!("source/symbol/{index}/exact"),
            authority: format!("10_REGISTRIES/symbols_v0.1.0.json#/adopted/{symbol}"),
            bytes: source.into_bytes(), expectation: Expectation::SourcePass(Reached::Lexical) });
    }
    let mut source = String::new();
    let mut assertions = Vec::new();
    for value in symbols.get("excluded_exact_lexemes").and_then(Json::as_array).ok_or("excluded symbols absent")? {
        let lexeme = value.as_str().ok_or("non-string excluded lexeme")?;
        source.push_str("VALUE: ");
        let start = source.len();
        source.push_str(lexeme);
        assertions.push(Expectation::DiagnosticAt { id: "error.symbol.invalid".into(), start, end: source.len() });
        source.push('\n');
    }
    cases.push(SourceCase { id: "source/symbol/excluded/all".into(),
        authority: "10_REGISTRIES/symbols_v0.1.0.json#/excluded_exact_lexemes".into(),
        bytes: source.into_bytes(), expectation: Expectation::All(assertions) });
    Ok(cases)
}


fn grammar_cases(spec: &SpecPackage) -> Result<Vec<SourceCase>, String> {
    let grammar = Grammar::load(spec).map_err(|e| e.to_string())?;
    let mut cases = Vec::new();
    let data = |ty: &str, value: &str| -> Result<String, String> {
        let mut fields = minimum_fields(&grammar, "DATA")?;
        inline(&mut fields, "TYPE", ty); inline(&mut fields, "VALUE", value);
        document(&grammar, "DATA", &fields, "top_level")
    };
    let header = render_block("LCL", &minimum_fields(&grammar, "LCL")?)
        + &render_block("SPECIFICATION", &minimum_fields(&grammar, "SPECIFICATION")?);
    let constructors = [
        ("PATH", "PATH(\"/case\")"), ("URI", "URI(\"https://example.invalid\")"),
        ("GLOB", "GLOB(\"*.txt\")"), ("REGEX", "REGEX(\"a+\")"),
        ("DATE", "DATE(\"2026-09-12\")"), ("TIME", "TIME(\"12:34:56\")"),
        ("DATETIME", "DATETIME(\"2026-09-12T12:34:56Z\")"),
        ("DURATION", "DURATION(1, unit.second)"), ("PERCENTAGE", "PERCENTAGE(20)"),
        ("BYTES", "BYTES(4)"), ("MEASURE", "MEASURE(2, unit.second)"),
    ];
    let mut all_constructors = header.clone();
    for (i, (ty, value)) in constructors.iter().enumerate() {
        all_constructors.push_str(&format!("DATA:\n    ID: data.c{i}\n    TYPE: {ty}\n    VALUE: {value}\n"));
    }
    let mut all_types = header.clone();
    for (i, ty) in grammar.scalar_types().chain([
        "LIST[INTEGER]", "SET[STRING]", "LIST[SET[INTEGER]]", "OBJECT[REF(type.object)]",
        "REFERENCE[REF(type.object)]", "REF(type.alias)", "NULL",
    ]).enumerate() {
        all_types.push_str(&format!("DATA:\n    ID: data.t{i}\n    TYPE: {ty}\n    VALUE: MISSING\n"));
    }
    let sequence = "SEQUENCE:\n    ID: sequence.case\n    IF (TRUE) THEN:\n        FOR EACH item IN [1, 2]:\n            STEP:\n                ID: step.case\n                ACTION: REF(action.sample)\n    ELSE:\n        COMMENT:\n            CONTENT: \"other arm\"\n";
    let task_header = header.replace("kind.data", "kind.task");
    let control = format!("{task_header}{sequence}EXECUTE:\n    REFERENCE: REF(sequence.case)\n");
    let mut expressions = data("BOOLEAN", "(1 + 2 * 3 == 7) AND NOT FALSE OR COUNT([1, 2]) == 2")?;
    expressions.push_str("DATA:\n    ID: data.postfix\n    TYPE: STRING\n    VALUE: REF(data.object).title[0]\n");
    let basic = data("INTEGER", "1")?;
    let swapped = render_block("SPECIFICATION", &minimum_fields(&grammar, "SPECIFICATION")?)
        + &render_block("LCL", &minimum_fields(&grammar, "LCL")?);
    let extension = format!("{}DEFINE:\n    ID: type.text\n    KIND: kind.type\n    BASE: STRING\n", header.replace("kind.data", "kind.extension"));
    let bad_kind = format!("{header}ACTION:\n    ID: action.bad\n    OPERATION: core.inspect\n");
    for (name, positive, negative, error) in [
        ("document-order", format!("\n\n{basic}"), swapped, "error.block.context"),
        ("document-kind", extension, bad_kind, "error.block.context"),
        ("expression", expressions, data("INTEGER", "COUNT(value: 1)")?, "error.grammar.invalid"),
        ("control", control.clone(), control.replace("IF (TRUE) THEN:", "IF TRUE THEN:"), "error.grammar.invalid"),
        ("collection", data("LIST[INTEGER]", "[\n    1,\n    2\n]")?, data("LIST[INTEGER]", "[1 2]")?, "error.grammar.invalid"),
        ("type-expression", all_types, data("mystery", "1")?, "error.field.type"),
        ("constructor", all_constructors, data("DATE", "DATE(\"2026-02-30\")")?, "error.literal.invalid"),
        ("literal-boundaries", data("DECIMAL", "-0.25")?, data("INTEGER", "01")?, "error.literal.invalid"),
    ] {
        cases.push(SourceCase { id: format!("source/grammar/{name}/positive"),
            authority: "04_GRAMMAR/10_COMPLETE_EBNF.ebnf".into(), bytes: positive.into_bytes(),
            expectation: Expectation::SourcePass(Reached::Grammar) });
        cases.push(SourceCase { id: format!("source/grammar/{name}/negative"),
            authority: "04_GRAMMAR/10_COMPLETE_EBNF.ebnf".into(), bytes: negative.into_bytes(),
            expectation: Expectation::Diagnostic(error.into()) });
    }
    Ok(cases)
}

type Fields = Vec<(String, String)>;
fn indent(text: &str) -> String {
    text.lines().map(|line| format!("    {line}\n")).collect()
}
fn render_block(name: &str, fields: &Fields) -> String {
    let body: String = fields.iter().map(|(_, text)| text.as_str()).collect();
    format!("{name}:\n{}", indent(&body))
}
fn put(fields: &mut Fields, name: &str, text: String) {
    fields.retain(|(key, _)| key != name);
    fields.push((name.into(), text));
}
fn inline(fields: &mut Fields, name: &str, value: &str) {
    put(fields, name, format!("{name}: {value}\n"));
}
fn field_minimum(g: &Grammar, block: &str, name: &str) -> Result<String, String> {
    let field = g.schema(block).and_then(|s| s.field(name)).ok_or("missing field signature")?;
    let candidates = [
        (FormSet::STRING, "\"0.1.0\""), (FormSet::INTEGER, "1"),
        (FormSet::BOOLEAN, "TRUE"), (FormSet::TYPE_EXPRESSION, "STRING"),
        (FormSet::SIMPLE_IDENTIFIER, "sample"), (FormSet::QUALIFIED_IDENTIFIER, "sample.value"),
        (FormSet::REFERENCE, "REF(sample.value)"), (FormSet::REFERENCE_LIST, "[REF(sample.value)]"),
        (FormSet::EXPRESSION, "TRUE"),
    ];
    if let Some((_, value)) = candidates.iter().find(|(form, _)| field.forms.intersects(*form)) {
        return Ok(format!("{name}: {value}\n"));
    }
    if field.forms.intersects(FormSet::NESTED) {
        if let Some(child) = &field.nested_block {
            return Ok(render_block(child, &minimum_fields(g, child)?));
        }
        return Ok(format!("{name}:\n    sample: 1\n"));
    }
    Err(format!("no minimum field form for {block}.{name}"))
}
fn minimum_fields(g: &Grammar, block: &str) -> Result<Fields, String> {
    let schema = g.schema(block).ok_or_else(|| format!("unknown block {block}"))?;
    let mut fields = Vec::new();
    for field in schema.fields.iter().filter(|f| f.required) {
        fields.push((field.name.clone(), field_minimum(g, block, &field.name)?));
    }
    match block {
        "LCL" => inline(&mut fields, "VERSION", "\"0.1.0\""),
        "SPECIFICATION" => {
            inline(&mut fields, "ID", "example.source");
            inline(&mut fields, "NAME", "\"Source evidence\"");
            inline(&mut fields, "VERSION", "\"1.0.0\"");
            inline(&mut fields, "KIND", "kind.data");
        }
        "DEFINE" => { inline(&mut fields, "KIND", "kind.type"); inline(&mut fields, "BASE", "STRING"); }
        "INPUT" | "CONTEXT" | "MEMORY" | "STATE" | "EVIDENCE" => inline(&mut fields, "VALUE", "\"value\""),
        "IMPORT" | "EXTENSION" => inline(&mut fields, "SOURCE", "PATH(\"library.lcl\")"),
        "GOAL" | "REQUIRE" | "PREFER" | "TEST" => inline(&mut fields, "ASSERT", "TRUE"),
        "TASK" | "STEP" => inline(&mut fields, "ACTION", "REF(action.sample)"),
        "PHASE" | "SEQUENCE" => put(&mut fields, "STEP", render_block("STEP", &minimum_fields(g, "STEP")?)),
        "SUCCESS" => inline(&mut fields, "ALL", "TRUE"),
        _ => {}
    }
    if fields.is_empty() { return Err(format!("empty baseline {block}")); }
    Ok(fields)
}
fn document(g: &Grammar, block: &str, fields: &Fields, parent: &str) -> Result<String, String> {
    if parent.starts_with("top_level") {
        let kind = ["kind.library", "kind.data", "kind.task", "kind.test", "kind.extension"]
            .into_iter().find(|kind| g.document_kind_blocks(kind).is_some_and(|set| set.contains(block)))
            .unwrap_or("kind.data");
        let mut spec = minimum_fields(g, "SPECIFICATION")?;
        inline(&mut spec, "KIND", kind);
        let mut text = if block == "LCL" { render_block(block, fields) }
            else { render_block("LCL", &minimum_fields(g, "LCL")?) };
        text.push_str(&if block == "SPECIFICATION" { render_block(block, fields) } else { render_block("SPECIFICATION", &spec) });
        if !matches!(block, "LCL" | "SPECIFICATION") { text.push_str(&render_block(block, fields)); }
        if matches!(kind, "kind.task" | "kind.test") && block != "EXECUTE" {
            text.push_str("EXECUTE:\n    REFERENCE: REF(task.sample)\n");
        }
        return Ok(text);
    }
    if matches!(parent, "IF" | "ELSE" | "FOR_EACH") {
        let body = render_block(block, fields);
        let control = match parent {
            "IF" => format!("IF (TRUE) THEN:\n{}", indent(&body)),
            "FOR_EACH" => format!("FOR EACH item IN [1]:\n{}", indent(&body)),
            _ => format!("IF (TRUE) THEN:\n    COMMENT:\n        CONTENT: \"other arm\"\nELSE:\n{}", indent(&body)),
        };
        let mut sequence = minimum_fields(g, "SEQUENCE")?;
        sequence.retain(|(key, _)| key != "STEP");
        sequence.push(("control".into(), control));
        return document(g, "SEQUENCE", &sequence, "top_level");
    }
    if parent == "SCHEMA" {
        let mut output = minimum_fields(g, "OUTPUT")?;
        put(&mut output, "SCHEMA", format!("SCHEMA:\n{}", indent(&render_block(block, fields))));
        return document(g, "OUTPUT", &output, "top_level");
    }
    let mut outer = minimum_fields(g, parent)?;
    if parent == "DEFINE" && block == "FIELD" { inline(&mut outer, "BASE", "OBJECT"); }
    put(&mut outer, block, render_block(block, fields));
    let enclosing = g.schema(parent).and_then(|schema| schema.parents.first()).ok_or("parent has no context")?;
    document(g, parent, &outer, enclosing)
}
fn block_cases(spec: &SpecPackage) -> Result<Vec<SourceCase>, String> {
    let grammar = Grammar::load(spec).map_err(|e| e.to_string())?;
    let mut cases = Vec::new();
    for schema in grammar.schemas() {
        let block = &schema.name;
        let fields = minimum_fields(&grammar, block)?;
        let parent = schema.parents.first().ok_or("block lacks a context")?;
        let authority = format!("10_REGISTRIES/field_signatures_v0.1.0.json#/blocks/{block}");
        cases.push(SourceCase { id: format!("source/block/{block}/minimum"), authority: authority.clone(),
            bytes: document(&grammar, block, &fields, parent)?.into_bytes(), expectation: Expectation::SourcePass(Reached::Grammar) });
        let mut forbidden = fields.clone();
        let intruder = if schema.field("ITEM").is_none() { "ITEM" } else { "LIMIT" };
        assert!(schema.field(intruder).is_none());
        inline(&mut forbidden, intruder, "TRUE");
        cases.push(SourceCase { id: format!("source/block/{block}/forbidden"), authority: authority.clone(),
            bytes: document(&grammar, block, &forbidden, parent)?.into_bytes(),
            expectation: Expectation::All(vec![Expectation::SourcePass(Reached::Lexical), Expectation::Diagnostic("error.field.forbidden".into())]) });
        for parent in &schema.parents {
            cases.push(SourceCase { id: format!("source/block/{block}/parent/{parent}"), authority: authority.clone(),
                bytes: document(&grammar, block, &fields, parent)?.into_bytes(), expectation: Expectation::SourcePass(Reached::Grammar) });
        }
    }
    Ok(cases)
}


/// Prepare the discriminated sibling context before varying one field form.
/// These are fixture choices under the canonical conditional requirements.
fn field_context(g: &Grammar, block: &str, name: &str) -> Result<Fields, String> {
    let mut fields = minimum_fields(g, block)?;
    let alternatives: &[&str] = match block {
        "INPUT" | "CONTEXT" | "MEMORY" | "STATE" | "EVIDENCE" if matches!(name, "VALUE" | "SOURCE") => &["VALUE", "SOURCE"],
        "REQUIRE" | "PREFER" if matches!(name, "ASSERT" | "ACTION") => &["ASSERT", "ACTION"],
        "STEP" if matches!(name, "ACTION" | "SEQUENCE" | "PHASE" | "TASK") => &["ACTION", "SEQUENCE", "PHASE", "TASK"],
        "SUCCESS" if matches!(name, "ALL" | "ANY" | "NONE") => &["ALL", "ANY", "NONE"],
        _ => &[],
    };
    fields.retain(|(key, _)| !alternatives.contains(&key.as_str()));
    if block == "DEFINE" {
        match name {
            "ITEM" => inline(&mut fields, "BASE", "ENUM"),
            "FIELD" => inline(&mut fields, "BASE", "OBJECT"),
            "PARAMETER" | "RESULT" => inline(&mut fields, "KIND", "kind.operation"),
            _ => {}
        }
    }
    Ok(fields)
}
fn form_field(g: &Grammar, block: &str, name: &str, form: &str) -> Result<String, String> {
    let field = g.schema(block).and_then(|s| s.field(name)).ok_or("field absent")?;
    let value = match form {
        "string" => "\"0.1.0\"", "integer" => "1", "boolean" => "TRUE",
        "simple" => "sample", "qualified" => "sample.value", "reference" => "REF(sample.value)",
        "reference_list" => "[REF(sample.value)]", "type" => "STRING", "expression" => "TRUE",
        "multiline" => "\"\"\"\n    text\n\"\"\"",
        "nested" => {
            if let Some(child) = &field.nested_block {
                return Ok(render_block(child, &minimum_fields(g, child)?));
            }
            if field.value_kind == "schema_reference_or_nested_schema" {
                return Ok(format!("{name}:\n{}", indent(&render_block("FIELD", &minimum_fields(g, "FIELD")?))));
            }
            return Ok(format!("{name}:\n    sample: 1\n"));
        }
        _ => return Err(format!("unknown mapped form {form}")),
    };
    Ok(format!("{name}: {value}\n"))
}
fn field_form_cases(spec: &SpecPackage) -> Result<Vec<SourceCase>, String> {
    let grammar = Grammar::load(spec).map_err(|e| e.to_string())?;
    let inventory = crate::obligations::Obligations::load(spec)?;
    let mut cases = Vec::new();
    for row in inventory.rows().filter(|row| row.id.starts_with("source/field/")) {
        let names: Vec<_> = row.id.split('/').collect();
        let (block, name) = (names[2], names[3]);
        let parent = &grammar.schema(block).ok_or("block absent")?.parents[0];
        for id in &row.probes {
            let Some(form) = id.strip_prefix(&format!("{}/form/", row.id)) else { continue; };
            let mut fields = field_context(&grammar, block, name)?;
            put(&mut fields, name, form_field(&grammar, block, name, form)?);
            cases.push(SourceCase { id: id.clone(), authority: row.authority.clone(),
                bytes: document(&grammar, block, &fields, parent)?.into_bytes(),
                expectation: Expectation::SourcePass(Reached::Grammar) });
        }
    }
    Ok(cases)
}


fn omitted_fields(g: &Grammar, block: &str, name: &str) -> Result<Fields, String> {
    let mut fields = minimum_fields(g, block)?;
    fields.retain(|(key, _)| key != name);
    let signature = g.schema(block).and_then(|s| s.field(name)).ok_or("field absent")?;
    if !signature.required {
        match (block, name) {
            ("INPUT" | "CONTEXT" | "MEMORY" | "STATE" | "EVIDENCE", "VALUE") => inline(&mut fields, "SOURCE", "PATH(\"value.txt\")"),
            ("REQUIRE" | "PREFER", "ASSERT") => inline(&mut fields, "ACTION", "REF(action.sample)"),
            ("STEP", "ACTION") => inline(&mut fields, "TASK", "REF(task.sample)"),
            ("SUCCESS", "ALL") => inline(&mut fields, "ANY", "TRUE"),
            ("GOAL", "ASSERT") => put(&mut fields, "RESULT", render_block("RESULT", &minimum_fields(g, "RESULT")?)),
            ("TASK", "ACTION") | ("PHASE", "STEP") => inline(&mut fields, "SEQUENCE", "REF(sequence.sample)"),
            ("SEQUENCE", "STEP") => fields.push(("control".into(), "IF (TRUE) THEN:\n    COMMENT:\n        CONTENT: \"branch\"\n".into())),
            ("TEST", "ASSERT") => { inline(&mut fields, "EXPECTED", "1"); inline(&mut fields, "ACTUAL", "1"); }
            _ => {}
        }
    }
    // Keep an otherwise legal body where one exists. A one-field-only block
    // has the canonical earlier lexical empty-block error when that field is
    // omitted; such cases are executed rather than silently skipped.
    if fields.is_empty() {
        if let Some(other) = g.schema(block).unwrap().fields.iter().find(|f| f.name != name) {
            fields.push((other.name.clone(), field_minimum(g, block, &other.name)?));
        }
    }
    Ok(fields)
}
fn field_constraint_cases(spec: &SpecPackage) -> Result<Vec<SourceCase>, String> {
    let grammar = Grammar::load(spec).map_err(|e| e.to_string())?;
    let inventory = crate::obligations::Obligations::load(spec)?;
    let mut cases = Vec::new();
    for row in inventory.rows().filter(|row| row.id.starts_with("source/field/")) {
        let names: Vec<_> = row.id.split('/').collect();
        let (block, name) = (names[2], names[3]);
        let schema = grammar.schema(block).ok_or("block absent")?;
        let signature = schema.field(name).ok_or("field absent")?;
        let parent = &schema.parents[0];
        let fields = omitted_fields(&grammar, block, name)?;
        let expectation = if fields.is_empty() { Expectation::Rejects("error.indentation.empty_block".into()) }
            else if signature.required { Expectation::All(vec![Expectation::SourcePass(Reached::Lexical), Expectation::Diagnostic("error.field.required".into())]) }
            else { Expectation::SourcePass(Reached::Grammar) };
        cases.push(SourceCase { id: format!("{}/absent", row.id), authority: row.authority.clone(),
            bytes: document(&grammar, block, &fields, parent)?.into_bytes(), expectation });

        let mut fields = field_context(&grammar, block, name)?;
        let field = field_minimum(&grammar, block, name)?;
        put(&mut fields, name, field.clone());
        fields.push((name.into(), field.replace("sample", "other")));
        let expectation = if signature.maximum_occurrences == Some(1) {
            Expectation::All(vec![Expectation::SourcePass(Reached::Lexical), Expectation::Diagnostic("error.field.duplicate".into())])
        } else { Expectation::SourcePass(Reached::Grammar) };
        cases.push(SourceCase { id: format!("{}/cardinality", row.id), authority: row.authority.clone(),
            bytes: document(&grammar, block, &fields, parent)?.into_bytes(), expectation });

        let mut fields = field_context(&grammar, block, name)?;
        let (invalid, error) = if signature.forms.intersects(FormSet::EXPRESSION) {
            if signature.forms.intersects(FormSet::NESTED) {
                (format!("{name}:\n    ACTION:\n        ID: action.invalid\n        OPERATION: core.inspect\n"), "error.block.field")
            } else { (format!("{name}:\n    sample: 1\n"), "error.field.type") }
        } else if signature.forms.intersects(FormSet::BOOLEAN) {
            (format!("{name}: \"not boolean\"\n"), "error.field.type")
        } else { (format!("{name}: FALSE\n"), "error.field.type") };
        put(&mut fields, name, invalid);
        cases.push(SourceCase { id: format!("{}/invalid-form", row.id), authority: row.authority.clone(),
            bytes: document(&grammar, block, &fields, parent)?.into_bytes(),
            expectation: Expectation::All(vec![Expectation::SourcePass(Reached::Lexical), Expectation::Diagnostic(error.into())]) });
    }
    Ok(cases)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn canonical_examples_and_keyword_cases_execute() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../canonical/LCL_Core_0.1.0");
        let spec = SpecPackage::open(root).unwrap();
        let runner = Runner::new(&spec).unwrap();
        let cases = cases(&spec).unwrap();
        assert_eq!(
            cases
                .iter()
                .filter(|c| c.id.starts_with("source/fixture/"))
                .count(),
            15
        );
        assert_eq!(
            cases
                .iter()
                .filter(|c| c.id.starts_with("source/example/"))
                .count(),
            34
        );
        assert_eq!(
            cases
                .iter()
                .filter(|c| c.id.starts_with("source/keyword/"))
                .count(),
            282
        );
        let inventory = crate::obligations::Obligations::load(&spec).unwrap();
        let expected: std::collections::BTreeSet<_> = inventory.probes()
            .filter(|(id, _)| id.starts_with("source/block/")).map(|(id, _)| id.to_string()).collect();
        let actual = cases.iter().filter(|c| c.id.starts_with("source/block/")).map(|c| c.id.clone()).collect();
        assert_eq!(expected, actual);
        let expected_forms: std::collections::BTreeSet<_> = inventory.probes()
            .filter(|(id, _)| id.starts_with("source/field/"))
            .map(|(id, _)| id.to_string()).collect();
        let actual_forms = cases.iter().filter(|c| c.id.starts_with("source/field/"))
            .map(|c| c.id.clone()).collect();
        assert_eq!(expected_forms, actual_forms);
        let required_source: std::collections::BTreeSet<_> = inventory.probes()
            .filter(|(_, level)| *level == crate::report::ClaimLevel::Source)
            .map(|(id, _)| id.to_string()).collect();
        let actual_source: std::collections::BTreeSet<_> = cases.iter().map(|c| c.id.clone()).collect();
        assert_eq!(actual_source.len(), cases.len(), "no duplicate source probe IDs");
        assert_eq!(required_source, actual_source, "full source inventory must execute");
        println!("source probes executed: {}", cases.len());
        let failed: Vec<_> = cases
            .iter()
            .map(|c| c.execute(&runner))
            .filter(|c| !c.passed())
            .map(|c| c.serialize())
            .collect();
        assert!(failed.is_empty(), "{}", failed.join("\n"));
    }
}

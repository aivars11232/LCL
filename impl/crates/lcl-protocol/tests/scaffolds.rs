//! Core 0.3.0 role scaffolds and Master templates, against the real Core 0.3.0
//! engine and its canonical contracts.

use lcl_protocol::json::Node;
use lcl_protocol::scaffold::{
    check_master, check_plan, check_text, document_type, document_types, entry, guided_shape,
    parse_master, part, roles, LocaleTag, Mark, MarkKind, Master, Mode, Plan, PlannedPart,
    Scaffold, ScaffoldError,
};
use lcl_protocol::Engine;
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::OnceLock;

fn canonical(version: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("../../../canonical/LCL_Core_{version}"))
}

/// The canonical fixture locales. Each profile is partial: 60 reserved words.
const LOCALES: [&str; 4] = ["lv-LV", "nl-NL", "ru-RU", "zh-CN"];

fn open() -> Engine {
    let profiles: Vec<PathBuf> = LOCALES
        .iter()
        .map(|l| {
            canonical("0.3.0").join(format!(
                "09_CONFORMANCE/LOCALIZATION_FIXTURES/profiles/{l}.json"
            ))
        })
        .collect();
    Engine::open_project(canonical("0.3.0"), &profiles).expect("the Core 0.3.0 engine opens")
}

/// The Core 0.3.0 engine with the fixture locale profiles, opened once.
fn engine() -> &'static Engine {
    static ENGINE: OnceLock<Engine> = OnceLock::new();
    ENGINE.get_or_init(open)
}

fn tag(locale: &str) -> LocaleTag {
    LocaleTag::parse(locale).expect("a locale tag")
}

fn lv() -> LocaleTag {
    tag("lv-LV")
}

const MODES: [Mode; 2] = [Mode::Minimal, Mode::Guided];

/// Top-level block words of a canonical-spelling text, in order.
fn blocks(text: &str) -> Vec<&str> {
    text.lines()
        .filter(|line| !line.starts_with(' ') && !line.starts_with('@'))
        .filter_map(|line| line.strip_suffix(':'))
        .collect()
}

fn slot_lines(marks: &[Mark]) -> BTreeSet<usize> {
    marks
        .iter()
        .filter(|m| matches!(m.kind, MarkKind::RequiredSlot | MarkKind::OptionalSlot))
        .map(|m| m.line)
        .collect()
}

fn mark(kind: MarkKind, line: usize, block: &str, field: &str) -> Mark {
    Mark {
        kind,
        line,
        block: block.to_string(),
        field: field.to_string(),
    }
}

/// The engine accepts the scaffold once its slots are filled, and the
/// engine-side slot reader finds exactly the slots the generator marked.
fn assert_valid(role: &str, scaffold: &Scaffold) {
    let found = check_text(engine(), role, &scaffold.text)
        .unwrap_or_else(|e| panic!("{role}: {e}\n{}", scaffold.text));
    assert_eq!(
        slot_lines(&found),
        slot_lines(&scaffold.marks),
        "{role}\n{}",
        scaffold.text
    );
}

fn assert_role_scaffold(role: &str, guided: &[&str]) {
    let path = format!("{}.lcl", role.trim_start_matches("kind.part."));
    let kind = format!("\n    KIND: {role}\n");
    let minimal = part(engine(), role, Mode::Minimal, &path, None).expect("a Minimal scaffold");
    assert_eq!(blocks(&minimal.text), ["LCL", "SPECIFICATION"]);
    assert!(minimal.text.contains(&kind), "{}", minimal.text);
    assert_valid(role, &minimal);
    let full = part(engine(), role, Mode::Guided, &path, None).expect("a Guided scaffold");
    let mut expected = vec!["LCL", "SPECIFICATION", "COMMENT"];
    expected.extend_from_slice(guided);
    assert_eq!(blocks(&full.text), expected);
    assert!(full.text.contains(&kind), "{}", full.text);
    assert_valid(role, &full);
}

#[test]
fn new_task_produces_task_scaffold() {
    assert_role_scaffold("kind.part.task", &["GOAL", "ACTION", "SUCCESS", "TASK"]);
}

#[test]
fn new_description_produces_description_scaffold() {
    assert_role_scaffold("kind.part.description", &["COMMENT"]);
}

#[test]
fn new_rules_produces_rules_scaffold() {
    assert_role_scaffold("kind.part.rules", &["REQUIRE", "FORBID"]);
}

#[test]
fn new_context_produces_context_scaffold() {
    assert_role_scaffold("kind.part.context", &["CONTEXT"]);
}

#[test]
fn new_data_produces_data_scaffold() {
    assert_role_scaffold("kind.part.data", &["INPUT", "DATA"]);
}

#[test]
fn new_output_produces_output_scaffold() {
    assert_role_scaffold("kind.part.output", &["OUTPUT"]);
}

#[test]
fn new_checks_produces_checks_scaffold() {
    assert_role_scaffold("kind.part.checks", &["VALIDATE", "VERIFY"]);
}

#[test]
fn new_definitions_produces_definitions_scaffold() {
    assert_role_scaffold("kind.part.definitions", &["DEFINE"]);
}

#[test]
fn minimal_scaffold_is_only_the_required_header() {
    let s = part(engine(), "kind.part.task", Mode::Minimal, "task.lcl", None).unwrap();
    assert_eq!(
        s.text,
        "LCL:\n    VERSION: \"0.3.0\"\n\nSPECIFICATION:\n    ID: specification.task\n    NAME:\n    \
         VERSION:\n    KIND: kind.part.task\n"
    );
    assert_eq!(
        s.marks,
        [
            mark(MarkKind::GeneratedId, 5, "SPECIFICATION", "ID"),
            mark(MarkKind::RequiredSlot, 6, "SPECIFICATION", "NAME"),
            mark(MarkKind::RequiredSlot, 7, "SPECIFICATION", "VERSION"),
        ]
    );
    for role in roles(engine()) {
        let s = part(engine(), &role, Mode::Minimal, "x.lcl", None).unwrap();
        assert_eq!(s.marks.len(), 3, "{role}");
        assert!(
            !s.text.contains("DESCRIPTION") && !s.text.contains("COMMENT"),
            "{role}"
        );
    }
}

#[test]
fn guided_scaffold_adds_common_sections_and_guidance() {
    let role = "kind.part.task";
    let allowed: Vec<&str> = engine()
        .grammar()
        .document_kind_blocks(role)
        .expect("the canonical task part blocks")
        .iter()
        .map(String::as_str)
        .collect();
    let s = part(engine(), role, Mode::Guided, "build.lcl", None).unwrap();
    let expected = format!(
        "LCL:\n    VERSION: \"0.3.0\"\n\nSPECIFICATION:\n    ID: specification.build\n    NAME:\n    \
         VERSION:\n    KIND: kind.part.task\n    DESCRIPTION:\n\nCOMMENT:\n    CONTENT: \"This file \
         is the task part of a project. The blocks allowed here are {}. A field with nothing after \
         its colon is a slot to fill in; an optional slot you do not need can be deleted.\"\n\n\
         GOAL:\n    ID: goal.build\n    ASSERT:\n\nACTION:\n    ID: action.build\n    OPERATION:\n\n\
         SUCCESS:\n    ID: success.build\n    ALL:\n\nTASK:\n    ID: task.build\n    GOAL: \
         REF(goal.build)\n    ACTION: REF(action.build)\n    SUCCESS: REF(success.build)\n",
        allowed.join(", ")
    );
    assert_eq!(s.text, expected);
    use MarkKind::{GeneratedId as G, Guidance, OptionalSlot, RequiredSlot as R};
    assert_eq!(
        s.marks,
        [
            mark(G, 5, "SPECIFICATION", "ID"),
            mark(R, 6, "SPECIFICATION", "NAME"),
            mark(R, 7, "SPECIFICATION", "VERSION"),
            mark(OptionalSlot, 9, "SPECIFICATION", "DESCRIPTION"),
            mark(Guidance, 12, "COMMENT", "CONTENT"),
            mark(G, 15, "GOAL", "ID"),
            mark(R, 16, "GOAL", "ASSERT"),
            mark(G, 19, "ACTION", "ID"),
            mark(R, 20, "ACTION", "OPERATION"),
            mark(G, 23, "SUCCESS", "ID"),
            mark(R, 24, "SUCCESS", "ALL"),
            mark(G, 27, "TASK", "ID"),
            mark(G, 28, "TASK", "GOAL"),
            mark(G, 29, "TASK", "ACTION"),
            mark(G, 30, "TASK", "SUCCESS"),
        ]
    );
}

#[test]
fn scaffolds_match_the_canonical_role_contract() {
    let grammar = engine().grammar();
    let kinds: BTreeSet<String> = grammar
        .document_kinds()
        .filter(|kind| kind.starts_with("kind.part."))
        .map(str::to_string)
        .collect();
    assert_eq!(kinds.len(), 8);
    assert_eq!(roles(engine()).into_iter().collect::<BTreeSet<_>>(), kinds);
    // Every role has its own type, and every narrower type is a type of a role.
    let types = document_types(engine());
    assert_eq!(types.len(), 12);
    for role in &kinds {
        assert_eq!(document_type(engine(), role).unwrap().role, role);
    }
    let mut texts = BTreeSet::new();
    for t in types {
        let role = &t.role.to_string();
        assert!(kinds.contains(role), "{} is a type of {role}", t.id);
        let shape = guided_shape(t.id).unwrap_or_else(|| panic!("no Guided scaffold for {}", t.id));
        let legal = grammar.document_kind_blocks(role).unwrap();
        for (block, fields) in shape {
            assert!(legal.contains(block), "{block} is not legal in {role}");
            let schema = grammar.schema(block).unwrap();
            for (field, kind) in &fields {
                let sig = schema
                    .field(field)
                    .unwrap_or_else(|| panic!("{field} is not a field of {block}"));
                if *kind == Some(MarkKind::OptionalSlot) {
                    assert!(!sig.required, "{block} {field} is required");
                }
            }
            for sig in schema.fields.iter().filter(|f| f.required) {
                assert!(
                    fields.iter().any(|(field, _)| *field == sig.name),
                    "{role}: {block} lacks its required {}",
                    sig.name
                );
            }
        }
        for mode in MODES {
            assert_valid(role, &part(engine(), t.id, mode, "file.lcl", None).unwrap());
        }
        // No two types start from the same Guided text.
        let guided = part(engine(), t.id, Mode::Guided, "file.lcl", None).unwrap();
        assert!(
            texts.insert(guided.text),
            "{} repeats another type's text",
            t.id
        );
    }
    let refused = part(engine(), "kind.task", Mode::Guided, "x.lcl", None).unwrap_err();
    assert!(refused.0.contains("not a project file role"), "{refused}");
}

#[test]
fn project_entry_lists_planned_parts_in_order() {
    let mut plan = Plan {
        mode: Mode::Minimal,
        entry: "main.lcl".to_string(),
        parts: vec![planned("task.lcl", "kind.part.task")],
    };
    plan.parts.push(PlannedPart {
        path: "notes/context.lcl".to_string(),
        role: "kind.part.context".to_string(),
        doc_type: "kind.part.context".to_string(),
        required: false,
        master: None,
    });
    let s = entry(engine(), &plan, None).unwrap();
    assert_eq!(
        s.text,
        "LCL:\n    VERSION: \"0.3.0\"\n\nSPECIFICATION:\n    ID: specification.main\n    NAME:\n    \
         VERSION:\n    KIND: kind.project\n\nPART:\n    ID: part.task\n    SOURCE: PATH(\"task.lcl\")\n    \
         KIND: kind.part.task\n\nPART:\n    ID: part.context\n    SOURCE: PATH(\"notes/context.lcl\")\n    \
         KIND: kind.part.context\n    REQUIRED: FALSE\n\nEXECUTE:\n    REFERENCE:\n"
    );
    assert_valid("kind.project", &s);
    let guided = entry(engine(), &Plan::canonical(Mode::Guided, ".lcl"), None).unwrap();
    assert!(guided
        .text
        .contains("Every part declares the SPECIFICATION VERSION this entry declares."));
    assert!(guided
        .text
        .contains("The parts are read in the order they are listed here."));
    assert_valid("kind.project", &guided);
}

/// A new project has one file of every document type, each in its own
/// folder, and its entry lists them in the read order: description, context,
/// definitions, rules, contracts, bindings, usage, stop conditions, task,
/// checks, data, output. Every name takes the chosen ending.
#[test]
fn canonical_project_has_every_type_in_read_order() {
    for ending in [".lcl", ".lcl.txt"] {
        for mode in MODES {
            let plan = Plan::canonical(mode, ending);
            assert_eq!(plan.entry, format!("main{ending}"));
            let paths: Vec<String> = plan.parts.iter().map(|p| p.path.clone()).collect();
            let wanted: Vec<String> = [
                "description/description",
                "context/context",
                "definitions/definitions",
                "rules/rules",
                "contracts/contracts",
                "bindings/bindings",
                "usage/usage",
                "stop_conditions/stop_conditions",
                "tasks/task_001",
                "checks/checks",
                "data/data",
                "output/output",
            ]
            .iter()
            .map(|p| format!("{p}{ending}"))
            .collect();
            assert_eq!(paths, wanted);
            check_plan(engine(), &plan).expect("the canonical plan is valid");
            let text = entry(engine(), &plan, None).unwrap().text;
            let mut at = 0;
            for path in &wanted {
                let found = text[at..]
                    .find(&format!("SOURCE: PATH(\"{path}\")"))
                    .unwrap_or_else(|| panic!("{path} is listed after the one before it"));
                at += found + 1;
            }
            for part_plan in &plan.parts {
                let file =
                    part(engine(), &part_plan.doc_type, mode, &part_plan.path, None).unwrap();
                assert_valid(&part_plan.role, &file);
            }
        }
    }
}

#[test]
fn same_request_produces_deterministic_bytes() {
    let other = open();
    for role in roles(engine()) {
        for mode in MODES {
            for locale in [None, Some(lv())] {
                let a = part(engine(), &role, mode, "x.lcl", locale.as_ref());
                let b = part(&other, &role, mode, "x.lcl", locale.as_ref());
                assert_eq!(a, b, "{role} {mode:?}");
            }
        }
    }
    for mode in MODES {
        let plan = Plan::canonical(mode, ".lcl.txt");
        assert_eq!(entry(engine(), &plan, None), entry(&other, &plan, None));
    }
}

/// A localized scaffold is produced only when its locale profile spells every
/// reserved word it needs, and is then valid; otherwise it is refused by name,
/// never written with mixed spellings. The canonical fixture profiles are
/// partial, and every one of them spells a Minimal part scaffold.
#[test]
fn localized_project_scaffolds_remain_valid() {
    let refused_as_unspellable =
        |result: Result<Scaffold, ScaffoldError>, locale: &str| match result {
            Ok(scaffold) => Some(scaffold),
            Err(error) => {
                let wanted = format!("the {locale} locale profile has no spelling for ");
                assert!(error.0.starts_with(&wanted), "{error}");
                None
            }
        };
    for locale in LOCALES {
        let directive = format!("@locale {locale}\n");
        for t in document_types(engine()) {
            let role = t.role;
            let minimal = part(engine(), t.id, Mode::Minimal, "x.lcl", Some(&tag(locale)))
                .unwrap_or_else(|e| panic!("{locale} {}: {e}", t.id));
            assert!(minimal.text.starts_with(&directive), "{}", minimal.text);
            assert!(!minimal.text.contains("SPECIFICATION"), "{}", minimal.text);
            assert_valid(role, &minimal);
            let guided = part(engine(), t.id, Mode::Guided, "x.lcl", Some(&tag(locale)));
            if let Some(guided) = refused_as_unspellable(guided, locale) {
                assert_valid(role, &guided);
            }
        }
        for mode in MODES {
            let project = entry(engine(), &Plan::canonical(mode, ".lcl"), Some(&tag(locale)));
            if let Some(project) = refused_as_unspellable(project, locale) {
                assert_valid("kind.project", &project);
            }
        }
    }
    let error = part(
        engine(),
        "kind.part.task",
        Mode::Guided,
        "x.lcl",
        Some(&lv()),
    )
    .unwrap_err();
    assert!(error.0.contains("COMMENT, CONTENT, DESCRIPTION"), "{error}");
}

fn role_master(core: &str, role: &str, text: &str) -> String {
    format!(
        "{{\"format\": 1, \"id\": \"mine\", \"name\": \"Mine\", \"core\": \"{core}\", \"role\": \
         \"{role}\", \"text\": {}}}",
        Node::string(text).compact()
    )
}

/// How a project Master's check finds the role Masters it names.
type Lookup<'a> = &'a dyn Fn(&str) -> Result<Master, ScaffoldError>;

fn no_masters(id: &str) -> Result<Master, ScaffoldError> {
    Err(ScaffoldError(format!("there is no Master {id}")))
}

fn check(json: &str) -> Result<Vec<Mark>, ScaffoldError> {
    check_master(engine(), &parse_master(json)?, &no_masters)
}

fn refused(json: &str, needle: &str) {
    let error = check(json).expect_err(needle);
    assert!(error.0.contains(needle), "wanted {needle:?}, got {error}");
}

fn minimal(role: &str) -> String {
    part(engine(), role, Mode::Minimal, "mine.lcl", None)
        .unwrap()
        .text
}

#[test]
fn valid_master_is_accepted_with_its_slots() {
    let s = part(engine(), "kind.part.task", Mode::Guided, "mine.lcl", None).unwrap();
    let marks = check(&role_master("0.3.0", "kind.part.task", &s.text)).unwrap();
    assert_eq!(slot_lines(&marks), slot_lines(&s.marks));
    let filled =
        "LCL:\n    VERSION: \"0.3.0\"\n\nSPECIFICATION:\n    ID: mine.rules\n    NAME: \"House \
                  rules\"\n    VERSION: \"1.0.0\"\n    KIND: kind.part.rules\n";
    assert_eq!(
        check(&role_master("0.3.0", "kind.part.rules", filled)).unwrap(),
        []
    );
}

#[test]
fn invalid_master_is_rejected() {
    let rules = minimal("kind.part.rules");
    let good = role_master("0.3.0", "kind.part.rules", &rules);
    check(&good).expect("the unedited scaffold is a valid Master");
    let with_text = |text: &str| role_master("0.3.0", "kind.part.rules", text);
    let cases = [
        ("not json".to_string(), "one JSON object"),
        (good.replace("\"format\": 1", "\"format\": 2"), "format 1 only"),
        (good.replacen('{', "{\"colour\": \"red\", ", 1), "is not a key"),
        (good.replace("\"id\": \"mine\"", "\"id\": \"../mine\""), "not a Master id"),
        (good.replace("\"name\": \"Mine\"", "\"name\": \"\""), "a Master name"),
        (with_text(rules.trim_end_matches('\n')), "line feed"),
        (
            with_text(&rules.replace("    NAME:\n", "    NAME:\n    COLOUR:\n")),
            "COLOUR is not a field of SPECIFICATION",
        ),
        (
            with_text(&format!("{rules}\nGOAL:\n    ID: goal.mine\n    ASSERT:\n")),
            "error.",
        ),
        (
            with_text(&format!(
                "{rules}\nFORBID:\n    ID: forbid.mine\n    OPERATION: core.delete\n    TARGET:\n    \
                 PARAMETER:\n        NAME:\n"
            )),
            "a slot must be a field of a top-level block",
        ),
        (with_text(&rules.replace("    NAME:\n", "    NAME:\t\"x\"\n")), "error."),
        (
            with_text(&rules.replace("    KIND: kind.part.rules\n", "    KIND:\n")),
            "is fixed and cannot be a slot",
        ),
    ];
    for (json, needle) in cases {
        refused(&json, needle);
    }
}

#[test]
fn wrong_role_master_is_rejected() {
    let task = minimal("kind.part.task");
    refused(
        &role_master("0.3.0", "kind.part.rules", &task),
        "must declare SPECIFICATION KIND kind.part.rules",
    );
    refused(
        &role_master("0.3.0", "kind.task", &task),
        "not a project file role",
    );
    refused(
        &role_master("0.3.0", "kind.part.notes", &task),
        "not a project file role",
    );
}

#[test]
fn incompatible_version_master_is_rejected() {
    let task = minimal("kind.part.task");
    refused(
        &role_master("0.2.0", "kind.part.task", &task),
        "never reinterpreted",
    );
    let older = task.replace("VERSION: \"0.3.0\"", "VERSION: \"0.2.0\"");
    let error = check(&role_master("0.3.0", "kind.part.task", &older)).unwrap_err();
    assert!(error.0.to_lowercase().contains("version"), "{error}");
}

fn planned(path: &str, role: &str) -> PlannedPart {
    PlannedPart {
        path: path.to_string(),
        role: role.to_string(),
        doc_type: role.to_string(),
        required: true,
        master: None,
    }
}

#[test]
fn invalid_project_file_plan_is_rejected() {
    let with = |change: &dyn Fn(&mut Plan)| {
        let mut plan = Plan::canonical(Mode::Guided, ".lcl");
        change(&mut plan);
        plan
    };
    let cases = [
        (
            with(&|p| p.parts.push(planned("rules/rules.lcl", "kind.part.rules"))),
            "names rules/rules.lcl twice",
        ),
        (
            with(&|p| p.parts.push(planned("more/rules.lcl", "kind.part.rules"))),
            "would collide",
        ),
        (
            with(&|p| p.parts[0].doc_type = "contracts".to_string()),
            "a Contracts file is a kind.part.rules part, not kind.part.description",
        ),
        (
            with(&|p| p.parts[0].doc_type = "notes".to_string()),
            "not a project file role or document type",
        ),
        (
            with(&|p| p.entry = "app/main.lcl".to_string()),
            "project folder itself",
        ),
        (
            with(&|p| p.parts[0].path = "../outside.lcl".to_string()),
            "not a plan path",
        ),
        (
            with(&|p| p.parts[0].path = "/abs.lcl".to_string()),
            "not a plan path",
        ),
        (
            with(&|p| p.parts[0].path = "notes.txt".to_string()),
            "does not end with",
        ),
        (
            with(&|p| p.parts[0].path = "My-Notes.lcl".to_string()),
            "identifier",
        ),
        (
            with(&|p| p.parts[0].role = "kind.project".to_string()),
            "not a project file role",
        ),
        (
            with(&|p| p.parts[0].role = "kind.part.notes".to_string()),
            "not a project file role",
        ),
        (
            with(&|p| p.parts[0].master = Some("Bad Id".to_string())),
            "not a Master id",
        ),
    ];
    for (plan, needle) in cases {
        let error = check_plan(engine(), &plan).expect_err(needle);
        assert!(error.0.contains(needle), "wanted {needle:?}, got {error}");
    }
    for mode in MODES {
        check_plan(engine(), &Plan::canonical(mode, ".lcl"))
            .expect("the canonical plans are valid");
    }
}

#[test]
fn project_master_names_only_valid_role_masters() {
    let project =
        "{\"format\": 1, \"id\": \"web\", \"name\": \"Web\", \"core\": \"0.3.0\", \"role\": \
                   \"kind.project\", \"mode\": \"guided\", \"entry\": \"main.lcl\", \"parts\": \
                   [{\"path\": \"task.lcl\", \"role\": \"kind.part.task\", \"master\": \"mine\"}]}";
    let master = parse_master(project).unwrap();
    let task = parse_master(&role_master(
        "0.3.0",
        "kind.part.task",
        &minimal("kind.part.task"),
    ))
    .unwrap();
    let rules = parse_master(&role_master(
        "0.3.0",
        "kind.part.rules",
        &minimal("kind.part.rules"),
    ))
    .unwrap();
    let broken = parse_master(&role_master("0.3.0", "kind.part.task", "LCL:\n")).unwrap();
    let web = master.clone();
    check_master(engine(), &master, &|_| Ok(task.clone())).expect("a valid project Master");
    let cases: [(Lookup, &str); 4] = [
        (&no_masters, "there is no Master mine"),
        (&|_| Ok(rules.clone()), "is for kind.part.rules"),
        (&|_| Ok(web.clone()), "is a project Master"),
        (&|_| Ok(broken.clone()), "Master mine"),
    ];
    for (lookup, needle) in cases {
        let error = check_master(engine(), &master, lookup).expect_err(needle);
        assert!(error.0.contains(needle), "wanted {needle:?}, got {error}");
    }
    let unknown = project.replace("\"master\": \"mine\"", "\"colour\": \"red\"");
    assert!(parse_master(&unknown)
        .unwrap_err()
        .0
        .contains("not a key of a project Master part"));
    let mode = project.replace("\"guided\"", "\"verbose\"");
    assert!(parse_master(&mode).unwrap_err().0.contains("is not a mode"));
}

/// A Master of a narrower type names it with "type"; it is checked against
/// its role, and a project part of that type takes only a Master of that type.
#[test]
fn typed_masters_are_kept_to_their_type() {
    let contracts = part(engine(), "contracts", Mode::Guided, "contracts.lcl", None)
        .unwrap()
        .text;
    let typed = |role: &str, doc_type: &str| {
        role_master("0.3.0", role, &contracts).replacen(
            "\"text\"",
            &format!("\"type\": \"{doc_type}\", \"text\""),
            1,
        )
    };
    let master = parse_master(&typed("kind.part.rules", "contracts")).unwrap();
    assert_eq!(master.doc_type, "contracts");
    check_master(engine(), &master, &no_masters).expect("a valid Contracts Master");
    let untyped = parse_master(&role_master("0.3.0", "kind.part.rules", &contracts)).unwrap();
    assert_eq!(untyped.doc_type, "kind.part.rules");
    refused(
        &typed("kind.part.checks", "contracts"),
        "a Contracts Master is for kind.part.rules, not kind.part.checks",
    );
    refused(
        &typed("kind.part.rules", "notes"),
        "not a project file role or document type",
    );
    let project = "{\"format\": 1, \"id\": \"web\", \"name\": \"Web\", \"core\": \"0.3.0\", \
                   \"role\": \"kind.project\", \"mode\": \"guided\", \"entry\": \"main.lcl\", \
                   \"parts\": [{\"path\": \"contracts.lcl\", \"role\": \"kind.part.rules\", \
                   \"type\": \"contracts\", \"master\": \"mine\"}]}";
    let plan = parse_master(project).unwrap();
    check_master(engine(), &plan, &|_| Ok(master.clone())).expect("a Contracts part Master");
    let error = check_master(engine(), &plan, &|_| Ok(untyped.clone())).unwrap_err();
    assert!(
        error.0.contains("is for kind.part.rules, not contracts"),
        "{error}"
    );
}

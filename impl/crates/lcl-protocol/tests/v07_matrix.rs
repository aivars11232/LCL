//! LCL-FEATURE-04 E: the V07 cross-language matrix.
//!
//! Three programs exist in canonical English and in lv-LV, nl-NL, ru-RU and
//! zh-CN: the package's localized minimum (small), `apps/small-invoice-total`
//! (execution-bearing) and `apps/medium-release-notes` (medium: one import and
//! one granted effect). The application variants under `tests/fixtures/v07`
//! were rendered from the applications by the scratch script
//! `t4_e1_generate_v07.py`: `LCL VERSION` became `0.2.0`, reserved words outside
//! strings took the fixture profile's preferred spelling, and nothing else
//! changed. The medium root carries an explicit `@locale`; every other file is
//! detected.
//!
//! Every assertion compares canonical structures or engine records. The four
//! profiles are the package's fixture profiles: this proves the mechanism and
//! their 60 mappings each, not natural-language coverage.
//!
//! The tests after the phase F marker prove the dynamic-profile design with a
//! deterministic, provider-neutral test resolver for `lt-LT`, a locale for which
//! no static profile exists anywhere in the package.

use lcl_diagnostics::Stage;
use lcl_localization::{
    content_identity, CoverageDetector, LocaleProfileResolver, LocaleTag, MemoryResolver, Pin,
    ProviderUnavailable, UnavailableResolver,
};
use lcl_protocol::{Engine, Engines, Granted, Inputs, Reached, Report};
use lcl_resolver::{MemoryProvider, SourceId, SourceUnit};
use lcl_spec::anchor::APPROVED_PACKAGE_0_2_0;
use lcl_spec::json::{self, Json};
use lcl_spec::SpecPackage;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex};

const LOCALES: [&str; 4] = ["lv-LV", "nl-NL", "ru-RU", "zh-CN"];
const VARIANTS: [&str; 5] = ["en", "lv-LV", "nl-NL", "ru-RU", "zh-CN"];

/// The medium application declares this workspace in its own source.
const NOTES_WORKSPACE: &str = "/tmp/lcl-apps/medium-release-notes";
static WORKSPACE: Mutex<()> = Mutex::new(());

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .expect("the repository root is present")
}

fn canonical(version: &str) -> PathBuf {
    repository().join(format!("canonical/LCL_Core_{version}"))
}

fn package_fixtures() -> PathBuf {
    canonical("0.2.0").join("09_CONFORMANCE/LOCALIZATION_FIXTURES")
}

fn profile_path(locale: &str) -> PathBuf {
    package_fixtures()
        .join("profiles")
        .join(format!("{locale}.json"))
}

fn read(path: impl AsRef<Path>) -> Vec<u8> {
    let path = path.as_ref();
    std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn engines() -> Engines {
    let files: Vec<PathBuf> = LOCALES.iter().map(|locale| profile_path(locale)).collect();
    let core = Engine::open(canonical("0.1.0")).expect("the approved 0.1.0 engine");
    let localized =
        Engine::open_localized(canonical("0.2.0"), &files).expect("the approved 0.2.0 engine");
    Engines::new(core, Some(localized)).expect("engines")
}

/// One program in one language: its root first, then every file it imports.
struct Program {
    variant: &'static str,
    files: Vec<(String, Vec<u8>)>,
}

impl Program {
    fn root(&self) -> SourceUnit {
        self.unit(&self.files[0].0)
    }

    fn unit(&self, id: &str) -> SourceUnit {
        let (name, bytes) = self
            .files
            .iter()
            .find(|(name, _)| name == id)
            .expect("a unit of this program");
        SourceUnit::new(SourceId::new(name.clone()), bytes.clone())
    }

    fn provider(&self) -> MemoryProvider {
        let mut provider = MemoryProvider::new();
        for (name, bytes) in &self.files[1..] {
            provider.insert(name.clone(), bytes.clone());
        }
        provider
    }
}

/// The five language variants of one program, English first.
fn programs(name: &str) -> Vec<Program> {
    VARIANTS
        .into_iter()
        .map(|variant| {
            let files = if name == "minimum" {
                let file = if variant == "en" {
                    "canonical_en.lcl".to_string()
                } else {
                    format!("auto_{}.lcl", &variant[..2])
                };
                vec![(
                    "main.lcl".to_string(),
                    read(package_fixtures().join("sources").join(file)),
                )]
            } else {
                let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/fixtures/v07")
                    .join(name)
                    .join(variant);
                let mut names = vec!["main.lcl"];
                if name == "release_notes" {
                    names.push("rules.lcl");
                }
                names
                    .into_iter()
                    .map(|file| (file.to_string(), read(dir.join(file))))
                    .collect()
            };
            Program { variant, files }
        })
        .collect()
}

/// Each canonical token: its terminal and its canonical word or exact text.
fn canonical_tokens(engine: &Engine, unit: &SourceUnit) -> Vec<(&'static str, String)> {
    let staged = engine.stage(unit);
    assert!(
        staged.stage_failure().is_none(),
        "{} did not stage",
        unit.id()
    );
    let source = staged.source();
    staged
        .lexed()
        .tokens()
        .iter()
        .map(|token| {
            (
                token.kind.ebnf_name(),
                token.word(source).unwrap_or_default().to_string(),
            )
        })
        .collect()
}

/// `Debug` rendering with every `Span { start: _, end: _ }` removed.
fn without_spans(rendered: &str) -> String {
    const OPEN: &str = "Span { start: ";
    let mut out = String::new();
    let mut rest = rendered;
    while let Some(at) = rest.find(OPEN) {
        out.push_str(&rest[..at]);
        out.push_str("Span");
        let after = &rest[at..];
        let close = after.find('}').expect("closed span");
        rest = &after[close + 1..];
    }
    out.push_str(rest);
    out
}

/// SHA-256 of the span-free AST rendering.
fn ast_digest(engine: &Engine, unit: &SourceUnit) -> String {
    let staged = engine.stage(unit);
    let document = staged.document().expect("a parsed document");
    let rendered = without_spans(&format!("{document:?}"));
    SourceUnit::new(SourceId::new("ast"), rendered.into_bytes()).digest()
}

/// Keys whose values locate or identify bytes, not decisions: every span and
/// position (`span`, `id_span`, `id_position`, ...), the unit digests and the
/// package record.
fn is_location(key: &str) -> bool {
    ["span", "position", "units", "spec"].contains(&key)
        || key.ends_with("_span")
        || key.ends_with("_position")
}

fn render(node: &Json, out: &mut String) {
    match node {
        Json::Null => out.push_str("null"),
        Json::Bool(value) => out.push_str(if *value { "true" } else { "false" }),
        Json::Number(value) => out.push_str(&value.to_string()),
        Json::String(value) => out.push_str(&format!("{value:?}")),
        Json::Array(items) => {
            out.push('[');
            for item in items {
                render(item, out);
                out.push(',');
            }
            out.push(']');
        }
        Json::Object(fields) => {
            out.push('{');
            for (key, value) in fields {
                if !is_location(key) {
                    out.push_str(&format!("{key:?}:"));
                    render(value, out);
                    out.push(',');
                }
            }
            out.push('}');
        }
    }
}

/// The report's JSON projection without the keys that locate bytes.
fn normalized(report: &Report) -> String {
    let parsed = json::parse(&report.to_json().compact()).expect("report JSON");
    let mut out = String::new();
    render(&parsed, &mut out);
    out
}

/// Every span in a report, in projection order, with its source unit.
fn spans(report: &Report) -> Vec<(String, usize, usize)> {
    fn walk(node: &Json, source: &str, out: &mut Vec<(String, usize, usize)>) {
        match node {
            Json::Array(items) => items.iter().for_each(|item| walk(item, source, out)),
            Json::Object(fields) => {
                let source = node.get("source").and_then(Json::as_str).unwrap_or(source);
                for (key, value) in fields {
                    // An absent span (`null`) locates nothing.
                    if value == &Json::Null {
                        continue;
                    }
                    if key == "span" || key.ends_with("_span") {
                        let offset = |at: &str| {
                            value
                                .get(at)
                                .and_then(Json::as_u64)
                                .unwrap_or_else(|| panic!("{key} {at}: {value:?}"))
                                as usize
                        };
                        out.push((source.to_string(), offset("start"), offset("end")));
                    } else {
                        walk(value, source, out);
                    }
                }
            }
            _ => {}
        }
    }
    let parsed = json::parse(&report.to_json().compact()).expect("report JSON");
    let mut out = Vec::new();
    walk(&parsed, "", &mut out);
    out
}

/// English byte offset to localized byte offset, from the aligned canonical
/// tokens: every token boundary, and every offset inside a token whose bytes
/// are identical in both documents. A conflicting mapping fails.
fn offset_map(
    english: (&Engine, &SourceUnit),
    localized: (&Engine, &SourceUnit),
) -> BTreeMap<usize, usize> {
    let (a, b) = (english.0.stage(english.1), localized.0.stage(localized.1));
    let (ta, tb) = (a.lexed().tokens(), b.lexed().tokens());
    assert_eq!(ta.len(), tb.len(), "aligned token streams");
    let mut map = BTreeMap::new();
    let mut put = |from: usize, to: usize| {
        let previous = map.insert(from, to);
        assert!(
            previous.is_none() || previous == Some(to),
            "offset {from} maps twice"
        );
    };
    for (x, y) in ta.iter().zip(tb) {
        put(x.span.start, y.span.start);
        put(x.span.end, y.span.end);
        if x.span.slice(a.source()) == y.span.slice(b.source()) {
            for k in 0..x.span.end - x.span.start {
                put(x.span.start + k, y.span.start + k);
            }
        }
    }
    map
}

#[test]
fn every_variant_selects_its_locale_and_maps_each_word_through_its_profile() {
    let engines = engines();
    let cases: [(&str, &[&str]); 3] = [
        ("minimum", &["auto"]),
        ("invoice", &["auto"]),
        ("release_notes", &["explicit", "auto"]),
    ];
    for (name, methods) in cases {
        for program in programs(name) {
            let at = format!("{name}/{}", program.variant);
            let root = program.root();
            let engine = engines.engine_for(&root);
            assert_eq!(engine.spec_record().formal_version, "0.2.0", "{at}");
            let report = engine.check(&root, &program.provider());
            assert!(
                report.diagnostics.is_empty(),
                "{at}: {:?}",
                report.diagnostics
            );
            assert_eq!(report.units.len(), program.files.len(), "{at}");
            for (unit, method) in report.units.iter().zip(methods) {
                let record = unit.locale.as_ref().expect("a locale record");
                assert_eq!(record.lcl_version, "0.2.0");
                let selected = (
                    record.method.as_str(),
                    record.locale.as_deref(),
                    record.profile_identity.clone(),
                );
                if program.variant == "en" {
                    assert_eq!(selected, ("canonical", None, None), "{at}/{}", unit.id);
                } else {
                    let identity = content_identity(&read(profile_path(program.variant)));
                    assert_eq!(
                        selected,
                        (*method, Some(program.variant), Some(identity)),
                        "{at}/{}",
                        unit.id
                    );
                }
            }
            if program.variant == "en" {
                continue;
            }
            let profile = json::parse(
                &String::from_utf8(read(profile_path(program.variant))).expect("UTF-8"),
            )
            .expect("profile JSON");
            let spellings = profile.get("spellings").expect("spellings");
            for (id, _) in &program.files {
                let staged = engine.stage(&program.unit(id));
                let source = staged.source();
                let mut mapped = 0;
                for token in staged.lexed().tokens() {
                    if token.kind == lcl_lexer::TokenKind::ReservedWord {
                        let spelling = token.span.slice(source).expect("in bounds");
                        let word = token.word(source).expect("a canonical word");
                        assert_eq!(
                            spellings.get(spelling).and_then(Json::as_str),
                            Some(word),
                            "{at}/{id}: {spelling}"
                        );
                        mapped += 1;
                    }
                }
                assert!(mapped > 0, "{at}/{id}");
            }
        }
    }
}

#[test]
fn every_variant_normalizes_to_the_same_canonical_tokens_and_ast() {
    let engines = engines();
    for name in ["minimum", "invoice", "release_notes"] {
        let programs = programs(name);
        let english = &programs[0];
        let en_engine = engines.engine_for(&english.root());
        for (id, _) in &english.files {
            let tokens = canonical_tokens(en_engine, &english.unit(id));
            let digest = ast_digest(en_engine, &english.unit(id));
            for program in &programs[1..] {
                let engine = engines.engine_for(&program.root());
                let unit = program.unit(id);
                assert_eq!(
                    canonical_tokens(engine, &unit),
                    tokens,
                    "{name}/{}/{id}: canonical tokens",
                    program.variant
                );
                assert_eq!(
                    ast_digest(engine, &unit),
                    digest,
                    "{name}/{}/{id}: AST digest",
                    program.variant
                );
            }
        }
    }
}

#[test]
fn every_variant_reaches_the_same_static_decisions_on_its_own_bytes() {
    let engines = engines();
    for name in ["minimum", "invoice", "release_notes"] {
        let programs = programs(name);
        let english = &programs[0];
        let en_engine = engines.engine_for(&english.root());
        let reports = |program: &Program| {
            let engine = engines.engine_for(&program.root());
            let (root, provider) = (program.root(), program.provider());
            [
                engine.check(&root, &provider),
                engine.validate(&root, &provider, &Inputs::new()),
                engine.inspect(&root, &provider, &Inputs::new()),
            ]
        };
        let english_reports = reports(english);
        for program in &programs[1..] {
            let at = format!("{name}/{}", program.variant);
            let engine = engines.engine_for(&program.root());
            let maps: BTreeMap<String, BTreeMap<usize, usize>> = english
                .files
                .iter()
                .map(|(id, _)| {
                    let map =
                        offset_map((en_engine, &english.unit(id)), (engine, &program.unit(id)));
                    (id.clone(), map)
                })
                .collect();
            let mut mapped = 0;
            for (en, localized) in english_reports.iter().zip(reports(program)) {
                assert_eq!(
                    normalized(&localized),
                    normalized(en),
                    "{at}: {:?}",
                    en.command
                );
                let (en_spans, localized_spans) = (spans(en), spans(&localized));
                assert_eq!(en_spans.len(), localized_spans.len(), "{at}");
                for ((source, start, end), located) in en_spans.iter().zip(&localized_spans) {
                    let map = &maps[source];
                    assert_eq!(
                        &(source.clone(), map[start], map[end]),
                        located,
                        "{at}: English span {start}..{end} in {source}"
                    );
                    mapped += 1;
                }
            }
            assert!(mapped > 0, "{at}: no span was compared");
        }
    }
}

fn run(engine: &Engine, program: &Program) -> Report {
    let mut granted = Granted::none();
    if program.files.len() > 1 {
        let root = PathBuf::from(NOTES_WORKSPACE);
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("the workspace is writable");
        granted.write.push(root);
    }
    let (mut stdlib, mut host) =
        lcl_protocol::surface(engine, &granted).expect("an operation surface");
    engine.run(
        &program.root(),
        &program.provider(),
        &Inputs::new(),
        &mut stdlib,
        &mut host,
    )
}

#[test]
fn every_variant_runs_exactly_as_the_core_0_1_0_application() {
    let _serial = WORKSPACE.lock().unwrap_or_else(|e| e.into_inner());
    let engines = engines();
    for (name, app) in [
        ("invoice", "small-invoice-total"),
        ("release_notes", "medium-release-notes"),
    ] {
        let source = repository().join("apps").join(app).join("src");
        let mut files = vec![("main.lcl".to_string(), read(source.join("main.lcl")))];
        if name == "release_notes" {
            files.push(("rules.lcl".to_string(), read(source.join("rules.lcl"))));
        }
        let original = Program {
            variant: "0.1.0",
            files,
        };
        let core = engines.engine_for(&original.root());
        assert_eq!(core.spec_record().formal_version, "0.1.0");
        let baseline = run(core, &original);
        assert_eq!(
            baseline.terminal_status(),
            Some("status.succeeded"),
            "{name}: {:?}",
            baseline.diagnostics
        );
        let expected = normalized(&baseline);
        for program in programs(name) {
            let at = format!("{name}/{}", program.variant);
            let engine = engines.engine_for(&program.root());
            assert_eq!(engine.spec_record().formal_version, "0.2.0", "{at}");
            let report = run(engine, &program);
            assert_eq!(
                report.terminal_status(),
                Some("status.succeeded"),
                "{at}: {:?}",
                report.diagnostics
            );
            assert_eq!(normalized(&report), expected, "{at}");
            if name == "release_notes" {
                assert!(
                    Path::new(NOTES_WORKSPACE).join("NOTES.txt").is_file(),
                    "{at}: the granted effect did not happen"
                );
            }
        }
    }
}

#[test]
fn equivalent_invalid_variants_report_the_same_diagnostics_on_their_own_bytes() {
    let engines = engines();
    let programs: Vec<Program> = programs("invoice")
        .into_iter()
        .map(|mut program| {
            let text = String::from_utf8(program.files[0].1.clone()).expect("UTF-8");
            assert_eq!(text.matches("output.total) == 6630").count(), 2);
            program.files[0].1 = text
                .replacen("output.total) == 6630", "output.absent) == 6630", 1)
                .into_bytes();
            program
        })
        .collect();
    let english = &programs[0];
    let en_engine = engines.engine_for(&english.root());
    let en_report = en_engine.check(&english.root(), &english.provider());
    let primary = en_report
        .primary()
        .expect("the invalid program is rejected");
    for program in &programs[1..] {
        let at = program.variant;
        let engine = engines.engine_for(&program.root());
        assert_eq!(engine.spec_record().formal_version, "0.2.0", "{at}");
        let report = engine.check(&program.root(), &program.provider());
        assert_eq!(normalized(&report), normalized(&en_report), "{at}");
        let localized = report.primary().expect("rejected");
        assert_eq!(localized.id, primary.id, "{at}");
        assert_eq!(localized.stage, primary.stage, "{at}");
        let map = offset_map((en_engine, &english.root()), (engine, &program.root()));
        assert_eq!(
            (localized.span.start, localized.span.end),
            (map[&primary.span.start], map[&primary.span.end]),
            "{at}"
        );
    }
}

#[test]
fn package_fixtures_select_and_fail_closed_through_the_engine() {
    let listed = json::parse(
        &String::from_utf8(read(package_fixtures().join("expected_results.json"))).expect("UTF-8"),
    )
    .expect("expected_results.json");
    let sources = listed
        .get("sources")
        .and_then(Json::as_object)
        .expect("sources");
    assert_eq!(sources.len(), 31);
    let mut failures = Vec::new();
    for (relative, case) in sources {
        let name = relative.trim_start_matches("sources/");
        let bytes = read(package_fixtures().join(relative));
        let mut resolver = MemoryResolver::new("lcl.fixture.memory");
        for locale in case
            .get("available")
            .and_then(Json::as_array)
            .expect("available")
        {
            let locale = locale.as_str().expect("locale");
            resolver.insert(
                LocaleTag::parse(locale).expect("tag"),
                read(profile_path(locale)),
            );
        }
        let pin = case.get("pin").and_then(|pin| {
            let locale = pin.get("locale")?.as_str()?;
            let identity = pin.get("identity")?.as_str()?;
            let identity = if identity == "@profile" {
                content_identity(&read(profile_path(locale)))
            } else {
                identity.to_string()
            };
            Some(Pin {
                locale: LocaleTag::parse(locale).ok()?,
                identity,
            })
        });
        let spec = SpecPackage::open_with_anchor(canonical("0.2.0"), &APPROVED_PACKAGE_0_2_0)
            .expect("the approved 0.2.0 package");
        let engine = Engine::assemble(spec).expect("the 0.2.0 engine assembles");
        let available = case
            .get("resolver_available")
            .and_then(Json::as_bool)
            .unwrap_or(true);
        let engine = if available {
            engine.with_localization(Arc::new(resolver), Arc::new(CoverageDetector))
        } else {
            engine.with_localization(Arc::new(UnavailableResolver), Arc::new(CoverageDetector))
        }
        .expect("the localization contract");
        let engine = match pin {
            Some(pin) => engine.with_locale_pins(BTreeMap::from([(SourceId::new(name), pin)])),
            None => engine,
        };
        let report = engine.check(
            &SourceUnit::new(SourceId::new(name), bytes.clone()),
            &MemoryProvider::new(),
        );
        let mut view: Vec<(&str, String)> = Vec::new();
        if let Some(primary) = report
            .primary()
            .filter(|p| p.stage == Stage::Localization || p.id == "error.keyword.unknown")
        {
            view.push(("error", primary.id.to_string()));
            view.push(("offset", primary.span.start.to_string()));
            let stage = if primary.id.starts_with("error.localization.") {
                Stage::Localization
            } else {
                Stage::Lexical
            };
            if primary.stage != stage || primary.span.end > bytes.len() {
                failures.push(format!("{name}: {:?} at {:?}", primary.stage, primary.span));
            }
        }
        if let Some(record) = report.units.first().and_then(|unit| unit.locale.as_ref()) {
            view.push(("method", record.method.clone()));
            if let Some(locale) = &record.locale {
                view.push(("locale", locale.clone()));
            }
        }
        for (key, value) in case
            .get("expected")
            .and_then(Json::as_object)
            .expect("expected")
        {
            let want = value
                .as_str()
                .map(str::to_string)
                .or_else(|| value.as_u64().map(|n| n.to_string()))
                .expect("expected value");
            let got = view.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone());
            if got.as_deref() != Some(want.as_str()) {
                failures.push(format!("{name}: {key} expected {want}, got {got:?}"));
            }
        }
    }
    assert!(failures.is_empty(), "mismatches:\n{}", failures.join("\n"));
}

#[test]
fn an_explicit_locale_wins_over_what_detection_chooses() {
    let engines = engines();
    let engine = engines.localized().expect("the 0.2.0 engine");
    // `LCL` and `ID` are spelled identically in lv-LV and nl-NL, and both are
    // canonical English words, so detection cannot choose either locale.
    let body = "LCL:\n    ID: x\n";
    let selection = |text: &str| {
        let staged = engine.stage(&SourceUnit::new(
            SourceId::new("shared.lcl"),
            text.as_bytes(),
        ));
        let localization = staged.localization().expect("localized");
        localization.record.as_ref().map(|record| {
            (
                record.method.as_str().to_string(),
                record.locale.as_ref().map(|l| l.as_str().to_string()),
                localization.diagnostics.is_empty(),
            )
        })
    };
    let detected = selection(body);
    for locale in ["lv-LV", "nl-NL"] {
        let explicit = Some(("explicit".to_string(), Some(locale.to_string()), true));
        assert_ne!(
            detected, explicit,
            "detection alone must not produce {locale}"
        );
        assert_eq!(selection(&format!("@locale {locale}\n{body}")), explicit);
    }
}

/// Byte offset just after the `n`th LINE FEED (0 is the start of the file).
fn line_start(bytes: &[u8], n: usize) -> usize {
    if n == 0 {
        return 0;
    }
    bytes
        .iter()
        .enumerate()
        .filter(|(_, b)| **b == b'\n')
        .nth(n - 1)
        .map(|(i, _)| i + 1)
        .expect("enough lines")
}

#[test]
fn source_safety_rejections_are_the_same_on_each_variants_own_bytes() {
    let engines = engines();
    // Each injection sits outside every word: the start of the file, the end of
    // line 4 (`SPECIFICATION:` in its own language), or the indentation of line 5.
    // It returns the edited bytes and the offset of the injected bytes.
    type Inject = fn(&[u8]) -> (Vec<u8>, usize, Vec<u8>);
    let injections: [(&str, Inject); 5] = [
        ("byte_order_mark", |b| {
            let mark = "\u{FEFF}".as_bytes().to_vec();
            ([mark.as_slice(), b].concat(), 0, mark)
        }),
        ("carriage_return", |b| {
            let at = line_start(b, 4) - 1;
            ([&b[..at], b"\r", &b[at..]].concat(), at, b"\r".to_vec())
        }),
        ("trailing_space", |b| {
            let at = line_start(b, 4) - 1;
            ([&b[..at], b" ", &b[at..]].concat(), at, b" ".to_vec())
        }),
        ("bidi_override", |b| {
            let at = line_start(b, 4) - 1;
            let control = "\u{202E}".as_bytes().to_vec();
            (
                [&b[..at], control.as_slice(), &b[at..]].concat(),
                at,
                control,
            )
        }),
        ("no_break_space_indent", |b| {
            let at = line_start(b, 4);
            assert_eq!(&b[at..at + 4], b"    ", "line 5 is indented");
            let space = "\u{00A0}".as_bytes().to_vec();
            (
                [&b[..at], space.as_slice(), &b[at + 1..]].concat(),
                at,
                space,
            )
        }),
    ];
    for (name, inject) in injections {
        let mut english: Option<(String, Stage)> = None;
        for mut program in programs("invoice") {
            let at_name = format!("{name}/{}", program.variant);
            let (bytes, offset, injected) = inject(&program.files[0].1);
            program.files[0].1 = bytes.clone();
            let engine = engines.engine_for(&program.root());
            let report = engine.check(&program.root(), &program.provider());
            let primary = report
                .primary()
                .unwrap_or_else(|| panic!("{at_name}: not rejected"));
            assert_eq!(primary.span.start, offset, "{at_name}: {}", primary.id);
            assert!(
                bytes[primary.span.start..].starts_with(&injected),
                "{at_name}: the span starts at the injected bytes"
            );
            let decided = (primary.id.to_string(), primary.stage);
            match &english {
                None => english = Some(decided),
                Some(expected) => assert_eq!(&decided, expected, "{at_name}"),
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Phase F: a dynamically resolved profile, with no static profile for its
// locale.
// ---------------------------------------------------------------------------

const DYNAMIC_LOCALE: &str = "lt-LT";

/// What the test provider answers, which each test changes between calls.
const UNAVAILABLE: u8 = 0;
const FIRST: u8 = 1;
const REVISED: u8 = 2;
const NOT_JSON: u8 = 3;
const OTHER_LOCALE: u8 = 4;
const COLLISION: u8 = 5;

fn replace_once(text: &str, old: &str, new: &str) -> String {
    assert_eq!(text.matches(old).count(), 1, "anchor {old:?}");
    text.replacen(old, new, 1)
}

/// An `lt-LT` profile built at run time: the lv-LV fixture's spellings under
/// another locale and a dynamic provider. Revisions differ only in provenance
/// bytes, so they map identically under different content identities.
fn dynamic_profile(revision: u8) -> Vec<u8> {
    let text = String::from_utf8(read(profile_path("lv-LV"))).expect("UTF-8");
    let text = replace_once(&text, "\"locale\": \"lv-LV\",", "\"locale\": \"lt-LT\",");
    let text = replace_once(
        &text,
        "\"provider_class\": \"fixture\"",
        "\"provider_class\": \"dynamic_resolver\"",
    );
    replace_once(
        &text,
        "\"provider_identity\": \"lcl-0.2.0-conformance-fixture\"",
        &format!("\"provider_identity\": \"lcl-test-dynamic-resolver/{revision}\""),
    )
    .into_bytes()
}

/// A deterministic provider whose answer the test controls.
struct DynamicResolver {
    answer: AtomicU8,
}

impl DynamicResolver {
    fn answering(answer: u8) -> Arc<DynamicResolver> {
        Arc::new(DynamicResolver {
            answer: AtomicU8::new(answer),
        })
    }
}

impl LocaleProfileResolver for DynamicResolver {
    fn identity(&self) -> &str {
        "lcl.test.dynamic"
    }

    fn available_locales(&self) -> Result<Vec<LocaleTag>, ProviderUnavailable> {
        if self.answer.load(Ordering::SeqCst) == UNAVAILABLE {
            return Err(ProviderUnavailable(
                "the test provider is offline".to_string(),
            ));
        }
        Ok(vec![LocaleTag::parse(DYNAMIC_LOCALE).expect("tag")])
    }

    fn resolve(&self, locale: &LocaleTag) -> Result<Option<Vec<u8>>, ProviderUnavailable> {
        if locale.as_str() != DYNAMIC_LOCALE {
            return Ok(None);
        }
        Ok(Some(match self.answer.load(Ordering::SeqCst) {
            UNAVAILABLE => {
                return Err(ProviderUnavailable(
                    "the test provider is offline".to_string(),
                ))
            }
            FIRST => dynamic_profile(1),
            REVISED => dynamic_profile(2),
            NOT_JSON => b"<html>rate limited</html>".to_vec(),
            OTHER_LOCALE => read(profile_path("lv-LV")),
            COLLISION => replace_once(
                &String::from_utf8(dynamic_profile(1)).expect("UTF-8"),
                "\"spellings\": {",
                "\"spellings\": {\n    \"TEST\": \"TASK\",",
            )
            .into_bytes(),
            other => panic!("no answer {other}"),
        }))
    }
}

fn dynamic_engine(resolver: &Arc<DynamicResolver>) -> Engine {
    let provider: Arc<dyn LocaleProfileResolver + Send + Sync> = resolver.clone();
    let spec = SpecPackage::open_with_anchor(canonical("0.2.0"), &APPROVED_PACKAGE_0_2_0)
        .expect("the approved 0.2.0 package");
    Engine::assemble(spec)
        .expect("the 0.2.0 engine assembles")
        .with_localization(provider, Arc::new(CoverageDetector))
        .expect("the localization contract")
}

fn invoice(variant: &str) -> Program {
    programs("invoice")
        .into_iter()
        .find(|program| program.variant == variant)
        .expect("an invoice variant")
}

fn explicit(program: &Program) -> SourceUnit {
    let directive = format!("@locale {DYNAMIC_LOCALE}\n");
    SourceUnit::new(
        SourceId::new("main.lcl"),
        [directive.as_bytes(), program.files[0].1.as_slice()].concat(),
    )
}

#[test]
fn a_dynamically_resolved_profile_is_validated_selected_and_runs() {
    fn mentions(dir: &Path, needle: &[u8]) -> bool {
        std::fs::read_dir(dir)
            .expect("readable")
            .flatten()
            .any(|entry| {
                let path = entry.path();
                if path.is_dir() {
                    mentions(&path, needle)
                } else {
                    read(&path).windows(needle.len()).any(|w| w == needle)
                }
            })
    }
    assert!(
        !mentions(&canonical("0.2.0"), DYNAMIC_LOCALE.as_bytes()),
        "no static {DYNAMIC_LOCALE} profile may exist"
    );

    let resolver = DynamicResolver::answering(FIRST);
    let engine = dynamic_engine(&resolver);
    let latvian = invoice("lv-LV");
    let report = engine.check(&latvian.root(), &latvian.provider());
    assert!(report.diagnostics.is_empty(), "{:?}", report.diagnostics);
    let record = report.units[0].locale.as_ref().expect("a locale record");
    let first = content_identity(&dynamic_profile(1));
    assert_eq!(
        (
            record.method.as_str(),
            record.locale.as_deref(),
            record.profile_identity.as_deref()
        ),
        ("auto", Some(DYNAMIC_LOCALE), Some(first.as_str()))
    );

    let english = invoice("en");
    let engines = engines();
    let expected = normalized(&run(engines.engine_for(&english.root()), &english));
    let dynamic = run(&engine, &latvian);
    assert_eq!(
        dynamic.terminal_status(),
        Some("status.succeeded"),
        "{:?}",
        dynamic.diagnostics
    );
    assert_eq!(normalized(&dynamic), expected);
}

#[test]
fn a_pinned_profile_reproduces_and_a_changed_answer_is_visible() {
    let resolver = DynamicResolver::answering(FIRST);
    let latvian = invoice("lv-LV");
    let unit = latvian.root();
    let first = content_identity(&dynamic_profile(1));
    let pins = || {
        BTreeMap::from([(
            SourceId::new("main.lcl"),
            Pin {
                locale: LocaleTag::parse(DYNAMIC_LOCALE).expect("tag"),
                identity: first.clone(),
            },
        )])
    };
    let pinned = dynamic_engine(&resolver).with_locale_pins(pins());
    let unpinned = dynamic_engine(&resolver);
    let recorded = pinned.check(&unit, &latvian.provider());
    assert!(
        recorded.diagnostics.is_empty(),
        "{:?}",
        recorded.diagnostics
    );
    let record = recorded.units[0].locale.as_ref().expect("a locale record");
    assert_eq!(
        (record.method.as_str(), record.profile_identity.as_deref()),
        ("pinned", Some(first.as_str()))
    );

    // The provider's answer changes.
    resolver.answer.store(REVISED, Ordering::SeqCst);
    let revised = content_identity(&dynamic_profile(2));
    assert_ne!(revised, first);

    // Unpinned use takes the new answer under its new identity.
    let fresh = unpinned.check(&unit, &latvian.provider());
    assert!(fresh.diagnostics.is_empty(), "{:?}", fresh.diagnostics);
    assert_eq!(
        fresh.units[0]
            .locale
            .as_ref()
            .and_then(|record| record.profile_identity.as_deref()),
        Some(revised.as_str())
    );

    // The pinned evaluation does not take it silently: drift, before grammar.
    let drifted = pinned.check(&unit, &latvian.provider());
    let primary = drifted.primary().expect("drift is reported");
    assert_eq!(primary.id.to_string(), "error.localization.profile_drift");
    assert_eq!(primary.stage, Stage::Localization);
    assert_eq!(drifted.reached, Reached::Lexical);
    assert!(pinned.stage(&unit).document().is_none());

    // A content cache holding the pinned bytes, consulted in the provider's
    // place, reproduces the recorded evaluation exactly.
    let mut cache = MemoryResolver::new("lcl.test.cache");
    cache.insert(
        LocaleTag::parse(DYNAMIC_LOCALE).expect("tag"),
        dynamic_profile(1),
    );
    let spec = SpecPackage::open_with_anchor(canonical("0.2.0"), &APPROVED_PACKAGE_0_2_0)
        .expect("the approved 0.2.0 package");
    let replay = Engine::assemble(spec)
        .expect("assembles")
        .with_localization(Arc::new(cache), Arc::new(CoverageDetector))
        .expect("the localization contract")
        .with_locale_pins(pins());
    assert_eq!(replay.check(&unit, &latvian.provider()), recorded);
}

#[test]
fn an_unavailable_provider_with_no_cache_fails_closed() {
    let resolver = DynamicResolver::answering(UNAVAILABLE);
    let engine = dynamic_engine(&resolver);
    let latvian = invoice("lv-LV");
    for (name, unit) in [("auto", latvian.root()), ("explicit", explicit(&latvian))] {
        let report = engine.check(&unit, &MemoryProvider::new());
        let primary = report
            .primary()
            .unwrap_or_else(|| panic!("{name}: accepted"));
        assert_eq!(
            (primary.id.to_string(), primary.stage),
            (
                "error.localization.profile_unavailable".to_string(),
                Stage::Localization
            ),
            "{name}"
        );
        assert!(engine.stage(&unit).document().is_none(), "{name}: no AST");
    }
}

#[test]
fn malicious_provider_output_is_rejected_before_grammar() {
    let resolver = DynamicResolver::answering(FIRST);
    let engine = dynamic_engine(&resolver);
    let unit = explicit(&invoice("lv-LV"));
    assert!(engine
        .check(&unit, &MemoryProvider::new())
        .diagnostics
        .is_empty());
    for (name, answer) in [
        ("not_json", NOT_JSON),
        ("other_locale", OTHER_LOCALE),
        ("collision", COLLISION),
    ] {
        resolver.answer.store(answer, Ordering::SeqCst);
        let report = engine.check(&unit, &MemoryProvider::new());
        let primary = report
            .primary()
            .unwrap_or_else(|| panic!("{name}: accepted"));
        assert_eq!(
            (primary.id.to_string(), primary.stage),
            (
                "error.localization.profile_invalid".to_string(),
                Stage::Localization
            ),
            "{name}"
        );
        assert!(engine.stage(&unit).document().is_none(), "{name}: no AST");
    }
}

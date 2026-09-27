//! # Role scaffolds and Master templates — Core 0.3.0
//!
//! A new project file should start from the structure of its role rather than
//! from a blank page. This module makes that starting text and checks the
//! user-editable Master templates that can replace it. It decides no language
//! rule. The roles are the canonical `part_kind` domain, the blocks a role may
//! hold are the canonical `document_kind_blocks`, and a block's fields and
//! which of them are required are the canonical block schemas, all read from
//! the engine's [`Grammar`]. Whether a text is structurally legal is decided by
//! staging it through the engine, never by a second schema here.
//!
//! ## Slots
//!
//! LCL has no comment syntax: "There is no ignorable comment syntax. COMMENT is
//! an explicit data block" (`02_LEXICAL/02`). A value the user still has to
//! supply therefore cannot be a commented-out example, and a placeholder value
//! would be a fake value in a semantic field. A *slot* is a field line with
//! nothing after its colon:
//!
//! ```text
//!     NAME:
//! ```
//!
//! The lexer rejects that line (`error.indentation.empty_block`), so a file
//! with an unfilled slot cannot be checked, validated or run, and nothing
//! placeholder-shaped can reach a semantic field. A slot is a field of a
//! top-level block, required or optional as that block's schema says.
//!
//! ## What else a scaffold carries
//!
//! [`Mark`]s tell an editor which lines are slots, which are guidance and which
//! hold identifiers this module generated; every other line is ordinary
//! source. Guidance in the text itself is a `COMMENT` block, which "has no
//! operational effect" (`02_LEXICAL/12`), and appears only in [`Mode::Guided`].
//! A generated identifier is `<block>.<stem>`, where the stem is the file name
//! without its LCL ending and must itself be an identifier segment
//! (`[a-z][a-z0-9_]*`). A name that is not one is refused, never repaired.
//!
//! ## Masters
//!
//! A Master is a person's own starting text for one role and one Core
//! version, stored as JSON ([`parse_master`]) and checked by [`check_master`].
//! It is copied into a new file; nothing created from a Master refers to it
//! afterwards. A project Master names the files a new project starts with
//! ([`Plan`]); the project entry is always generated from that plan.

use crate::engine::Engine;
pub use lcl_localization::LocaleTag;
use lcl_localization::Profile;
use lcl_parser::{FieldSignature, FormSet, Grammar};
use lcl_resolver::{ResolvedUnit, SourceId, SourceUnit};
use lcl_spec::json::Json;
use std::collections::BTreeSet;
use std::fmt;

/// The Master file format this build reads.
pub const MASTER_FORMAT: u64 = 1;

/// The document kind of a project entry.
pub const PROJECT_KIND: &str = "kind.project";

/// The longest Master text accepted, in bytes.
pub const MAX_MASTER_TEXT: usize = 1 << 20;

/// The canonical closed domain whose members are the project file roles.
const PART_KIND_DOMAIN: &str = "part_kind";

/// How much a canonical scaffold contains.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Mode {
    /// Only what the canon requires of a file of the role.
    Minimal,
    /// Minimal plus the role's common sections and a guidance `COMMENT`.
    Guided,
}

impl Mode {
    pub fn parse(text: &str) -> Option<Mode> {
        match text {
            "minimal" => Some(Mode::Minimal),
            "guided" => Some(Mode::Guided),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Mode::Minimal => "minimal",
            Mode::Guided => "guided",
        }
    }
}

/// What a marked line of a scaffold is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MarkKind {
    /// A slot the file cannot do without.
    RequiredSlot,
    /// A slot that may be filled or deleted.
    OptionalSlot,
    /// Explanatory text for the person filling the file in.
    Guidance,
    /// An identifier, or a reference to one, that this module generated.
    GeneratedId,
}

impl MarkKind {
    pub fn as_str(self) -> &'static str {
        match self {
            MarkKind::RequiredSlot => "required_slot",
            MarkKind::OptionalSlot => "optional_slot",
            MarkKind::Guidance => "guidance",
            MarkKind::GeneratedId => "generated_id",
        }
    }
}

/// One marked line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mark {
    pub kind: MarkKind,
    /// One-based line number in the text.
    pub line: usize,
    /// Canonical spelling of the top-level block the line belongs to.
    pub block: String,
    /// Canonical spelling of the field on the line.
    pub field: String,
}

/// The starting text of one file and what its marked lines are.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scaffold {
    pub text: String,
    pub marks: Vec<Mark>,
}

/// Why a scaffold, Master or plan was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScaffoldError(pub String);

impl fmt::Display for ScaffoldError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ScaffoldError {}

fn refuse<T>(detail: impl Into<String>) -> Result<T, ScaffoldError> {
    Err(ScaffoldError(detail.into()))
}

/// The roles a project file can have: this engine's canonical `part_kind`
/// domain, in ascending order. Empty for a Core version without projects.
pub fn roles(engine: &Engine) -> Vec<String> {
    engine
        .grammar()
        .closed_domain_members(PART_KIND_DOMAIN)
        .map(|members| members.iter().cloned().collect())
        .unwrap_or_default()
}

fn require_role(engine: &Engine, role: &str) -> Result<(), ScaffoldError> {
    let roles = roles(engine);
    if roles.iter().any(|r| r == role) {
        return Ok(());
    }
    if roles.is_empty() {
        return refuse(format!(
            "LCL {} has no project file roles",
            engine.spec().formal_version()
        ));
    }
    refuse(format!(
        "{role} is not a project file role; the roles are {}",
        roles.join(", ")
    ))
}

/// How a Guided section fills one field.
#[derive(Clone, Copy)]
enum Fill {
    /// `<block>.<stem>`, generated.
    Id,
    /// Left for the person as a slot of this kind.
    Slot(MarkKind),
    /// `REF(<block>.<stem>)` to the named block of the same scaffold.
    Ref(&'static str),
}

const REQUIRED: Fill = Fill::Slot(MarkKind::RequiredSlot);
const OPTIONAL: Fill = Fill::Slot(MarkKind::OptionalSlot);

/// One block a Guided scaffold adds, with its fields in order.
struct Section {
    block: &'static str,
    fields: &'static [(&'static str, Fill)],
}

/// The common sections Guided mode adds for a role. This is presentation, not
/// canon: a slot marked required here may be a field the schema lists as
/// optional when the block's conditional requirement needs one of a choice
/// (`GOAL` needs `ASSERT` or `RESULT`). The parity tests prove every block legal
/// for its role, every field registered for its block, every required field
/// present, and the whole scaffold accepted by the engine once filled.
fn guided_sections(role: &str) -> Option<&'static [Section]> {
    let sections: &'static [Section] = match role {
        "kind.part.task" => &[
            Section {
                block: "GOAL",
                fields: &[("ID", Fill::Id), ("ASSERT", REQUIRED)],
            },
            Section {
                block: "ACTION",
                fields: &[("ID", Fill::Id), ("OPERATION", REQUIRED)],
            },
            Section {
                block: "SUCCESS",
                fields: &[("ID", Fill::Id), ("ALL", REQUIRED)],
            },
            Section {
                block: "TASK",
                fields: &[
                    ("ID", Fill::Id),
                    ("GOAL", Fill::Ref("GOAL")),
                    ("ACTION", Fill::Ref("ACTION")),
                    ("SUCCESS", Fill::Ref("SUCCESS")),
                ],
            },
        ],
        "kind.part.description" => &[Section {
            block: "COMMENT",
            fields: &[("CONTENT", REQUIRED)],
        }],
        "kind.part.rules" => &[
            Section {
                block: "REQUIRE",
                fields: &[("ID", Fill::Id), ("ASSERT", REQUIRED)],
            },
            Section {
                block: "FORBID",
                fields: &[
                    ("ID", Fill::Id),
                    ("OPERATION", REQUIRED),
                    ("TARGET", REQUIRED),
                ],
            },
        ],
        "kind.part.context" => &[Section {
            block: "CONTEXT",
            fields: &[
                ("ID", Fill::Id),
                ("TYPE", REQUIRED),
                ("SCOPE", REQUIRED),
                ("VALUE", REQUIRED),
            ],
        }],
        "kind.part.data" => &[
            Section {
                block: "INPUT",
                fields: &[("ID", Fill::Id), ("TYPE", REQUIRED), ("VALUE", REQUIRED)],
            },
            Section {
                block: "DATA",
                fields: &[("ID", Fill::Id), ("TYPE", REQUIRED), ("VALUE", REQUIRED)],
            },
        ],
        "kind.part.output" => &[Section {
            block: "OUTPUT",
            fields: &[("ID", Fill::Id), ("TYPE", REQUIRED), ("FORMAT", REQUIRED)],
        }],
        "kind.part.checks" => &[
            Section {
                block: "VALIDATE",
                fields: &[("ID", Fill::Id), ("ASSERT", REQUIRED)],
            },
            Section {
                block: "VERIFY",
                fields: &[("ID", Fill::Id), ("ASSERT", REQUIRED)],
            },
        ],
        "kind.part.definitions" => &[Section {
            block: "DEFINE",
            fields: &[("ID", Fill::Id), ("KIND", REQUIRED), ("MEANING", OPTIONAL)],
        }],
        _ => return None,
    };
    Some(sections)
}

/// One Guided section, for the parity tests: the block, and each field with
/// the slot kind it gets, or `None` when its value is generated.
pub type SectionShape = (&'static str, Vec<(&'static str, Option<MarkKind>)>);

/// The blocks and fields a Guided scaffold of `role` adds, for the parity
/// tests: `(block, [(field, kind)])`, where `kind` is `None` for a field whose
/// value is generated.
pub fn guided_shape(role: &str) -> Option<Vec<SectionShape>> {
    guided_sections(role).map(|sections| {
        sections
            .iter()
            .map(|section| {
                let fields = section
                    .fields
                    .iter()
                    .map(|(field, fill)| match fill {
                        Fill::Slot(kind) => (*field, Some(*kind)),
                        Fill::Id | Fill::Ref(_) => (*field, None),
                    })
                    .collect();
                (section.block, fields)
            })
            .collect()
    })
}

/// Builds a scaffold's text line by line and records its marks.
struct Writer<'a> {
    profile: Option<&'a Profile>,
    locale: String,
    missing: BTreeSet<String>,
    text: String,
    lines: usize,
    blocks: usize,
    block: &'static str,
    marks: Vec<Mark>,
}

impl<'a> Writer<'a> {
    fn new(locale: Option<&LocaleTag>, profile: Option<&'a Profile>) -> Writer<'a> {
        let mut writer = Writer {
            profile,
            locale: locale.map(|l| l.as_str().to_string()).unwrap_or_default(),
            missing: BTreeSet::new(),
            text: String::new(),
            lines: 0,
            blocks: 0,
            block: "",
            marks: Vec::new(),
        };
        if let Some(locale) = locale {
            writer.text = format!("@locale {}\n", locale.as_str());
            writer.lines = 1;
        }
        writer
    }

    /// A reserved word of the source, in the scaffold's spelling. A word the
    /// locale profile cannot spell is recorded, and [`Writer::finish`] refuses
    /// the scaffold: one file cannot mix spellings (`error.localization.mixed`).
    fn word(&mut self, canonical: &str) -> String {
        if let Some(profile) = self.profile {
            if profile.preferred(canonical).is_none() {
                self.missing.insert(canonical.to_string());
            }
        }
        render(self.profile, canonical)
    }

    /// A reserved word quoted inside a string, where any spelling is text.
    fn label(&self, canonical: &str) -> String {
        render(self.profile, canonical)
    }

    fn open(&mut self, block: &'static str) {
        if self.blocks > 0 {
            self.text.push('\n');
            self.lines += 1;
        }
        let word = self.word(block);
        self.text.push_str(&word);
        self.text.push_str(":\n");
        self.lines += 1;
        self.blocks += 1;
        self.block = block;
    }

    fn field(&mut self, field: &'static str, value: Option<&str>, mark: Option<MarkKind>) {
        let word = self.word(field);
        self.text.push_str("    ");
        self.text.push_str(&word);
        self.text.push(':');
        if let Some(value) = value {
            self.text.push(' ');
            self.text.push_str(value);
        }
        self.text.push('\n');
        self.lines += 1;
        if let Some(kind) = mark {
            self.marks.push(Mark {
                kind,
                line: self.lines,
                block: self.block.to_string(),
                field: field.to_string(),
            });
        }
    }

    fn finish(self) -> Result<Scaffold, ScaffoldError> {
        if !self.missing.is_empty() {
            let missing: Vec<&str> = self.missing.iter().map(String::as_str).collect();
            return refuse(format!(
                "the {} locale profile has no spelling for {}, which this scaffold needs, \
                 and one file cannot mix spellings",
                self.locale,
                missing.join(", ")
            ));
        }
        Ok(Scaffold {
            text: self.text,
            marks: self.marks,
        })
    }
}

fn render(profile: Option<&Profile>, canonical: &str) -> String {
    profile
        .and_then(|p| p.preferred(canonical))
        .unwrap_or(canonical)
        .to_string()
}

fn profile(engine: &Engine, locale: Option<&LocaleTag>) -> Result<Option<Profile>, ScaffoldError> {
    locale
        .map(|locale| engine.locale_profile(locale).map_err(ScaffoldError))
        .transpose()
}

/// `LCL` and `SPECIFICATION`, with the optional description slot in Guided
/// mode.
fn header(w: &mut Writer, version: &str, kind: &str, stem: &str, mode: Mode) {
    w.open("LCL");
    w.field("VERSION", Some(&format!("\"{version}\"")), None);
    w.open("SPECIFICATION");
    w.field(
        "ID",
        Some(&format!("specification.{stem}")),
        Some(MarkKind::GeneratedId),
    );
    w.field("NAME", None, Some(MarkKind::RequiredSlot));
    w.field("VERSION", None, Some(MarkKind::RequiredSlot));
    w.field("KIND", Some(kind), None);
    if mode == Mode::Guided {
        w.field("DESCRIPTION", None, Some(MarkKind::OptionalSlot));
    }
}

/// The Guided `COMMENT`: what the file is, the blocks the canon allows in it,
/// and how slots work.
fn guidance(w: &mut Writer, grammar: &Grammar, kind: &str) {
    let blocks: Vec<String> = grammar
        .document_kind_blocks(kind)
        .into_iter()
        .flatten()
        .map(|block| w.label(block))
        .collect();
    let what = match kind.strip_prefix("kind.part.") {
        Some(role) => format!("the {role} part of a project"),
        None => "the entry of a project".to_string(),
    };
    let versions = if kind == PROJECT_KIND {
        format!(
            " Every part declares the {} {} this entry declares.",
            w.label("SPECIFICATION"),
            w.label("VERSION")
        )
    } else {
        String::new()
    };
    let content = format!(
        "\"This file is {what}. The blocks allowed here are {}.{versions} \
         A field with nothing after its colon is a slot to fill in; \
         an optional slot you do not need can be deleted.\"",
        blocks.join(", ")
    );
    w.open("COMMENT");
    w.field("CONTENT", Some(&content), Some(MarkKind::Guidance));
}

/// The stem of a planned or requested file: its last path segment without
/// the LCL ending. It becomes the last segment of every generated identifier.
pub fn stem(path: &str) -> Result<String, ScaffoldError> {
    let name = path.rsplit('/').next().unwrap_or(path);
    let stem = lcl_project::SUFFIXES
        .iter()
        .find_map(|suffix| name.strip_suffix(suffix))
        .filter(|stem| !stem.is_empty());
    let Some(stem) = stem else {
        return refuse(format!(
            "{path} is not an LCL document name: it ends with {}",
            lcl_project::SUFFIXES.join(" or ")
        ));
    };
    if !is_segment(stem) {
        return refuse(format!(
            "{path}: the file name without its ending must be an LCL identifier \
             segment ([a-z][a-z0-9_]*), because the file's generated IDs end with it"
        ));
    }
    Ok(stem.to_string())
}

fn is_segment(text: &str) -> bool {
    let mut chars = text.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// The canonical scaffold of one project file of `role`, for the file at
/// `path`, in `locale`'s spelling when one is given.
pub fn part(
    engine: &Engine,
    role: &str,
    mode: Mode,
    path: &str,
    locale: Option<&LocaleTag>,
) -> Result<Scaffold, ScaffoldError> {
    require_role(engine, role)?;
    let stem = stem(path)?;
    let sections = match mode {
        Mode::Minimal => &[][..],
        Mode::Guided => match guided_sections(role) {
            Some(sections) => sections,
            None => return refuse(format!("there is no Guided scaffold for {role}")),
        },
    };
    let profile = profile(engine, locale)?;
    let mut w = Writer::new(locale, profile.as_ref());
    header(&mut w, engine.spec().formal_version(), role, &stem, mode);
    if mode == Mode::Guided {
        guidance(&mut w, engine.grammar(), role);
    }
    for section in sections {
        let own = section.block.to_ascii_lowercase();
        w.open(section.block);
        for (field, fill) in section.fields {
            match fill {
                Fill::Id => w.field(
                    field,
                    Some(&format!("{own}.{stem}")),
                    Some(MarkKind::GeneratedId),
                ),
                Fill::Slot(kind) => w.field(field, None, Some(*kind)),
                Fill::Ref(target) => {
                    let value =
                        format!("{}({}.{stem})", w.word("REF"), target.to_ascii_lowercase());
                    w.field(field, Some(&value), Some(MarkKind::GeneratedId));
                }
            }
        }
    }
    w.finish()
}

/// One file a project plan creates besides its entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedPart {
    /// Relative to the project folder, `/`-separated.
    pub path: String,
    pub role: String,
    /// `PART REQUIRED`; the file is created either way.
    pub required: bool,
    /// The role Master this file starts from, when one is named.
    pub master: Option<String>,
}

/// The files a new project starts with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    /// The mode of the entry and of every part that names no Master.
    pub mode: Mode,
    /// The entry's file name; the entry sits in the project folder itself.
    pub entry: String,
    pub parts: Vec<PlannedPart>,
}

impl Plan {
    /// The project a person gets when no project Master is chosen: in Minimal
    /// mode one task part, which the entry's required `EXECUTE` needs to name
    /// anything; in Guided mode a description, rules and a task.
    pub fn canonical(mode: Mode) -> Plan {
        let part = |path: &str, role: &str| PlannedPart {
            path: path.to_string(),
            role: role.to_string(),
            required: true,
            master: None,
        };
        let parts = match mode {
            Mode::Minimal => vec![part("task.lcl", "kind.part.task")],
            Mode::Guided => vec![
                part("description.lcl", "kind.part.description"),
                part("rules.lcl", "kind.part.rules"),
                part("task.lcl", "kind.part.task"),
            ],
        };
        Plan {
            mode,
            entry: "main.lcl".to_string(),
            parts,
        }
    }
}

/// A plan path: relative, `/`-separated segments of ASCII letters, digits,
/// `_`, `-` and `.`, no segment starting with `.`, and an LCL ending.
pub fn check_path(path: &str) -> Result<(), ScaffoldError> {
    let segment_ok = |segment: &str| {
        !segment.is_empty()
            && !segment.starts_with('.')
            && segment
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
    };
    if path.len() > 255 || !path.split('/').all(segment_ok) {
        return refuse(format!(
            "{path:?} is not a plan path: use relative '/'-separated names of letters, \
             digits, '_', '-' and '.', none starting with '.'"
        ));
    }
    if !lcl_project::is_document(path) {
        return refuse(format!(
            "{path} does not end with {}",
            lcl_project::SUFFIXES.join(" or ")
        ));
    }
    Ok(())
}

/// Check that a plan can be created: every path is a plain relative document
/// path, no two files share a path or a stem (their generated IDs would
/// collide), every role is a project file role and every Master name is a
/// Master identifier.
pub fn check_plan(engine: &Engine, plan: &Plan) -> Result<(), ScaffoldError> {
    check_path(&plan.entry)?;
    if plan.entry.contains('/') {
        return refuse(format!(
            "the entry {} must sit in the project folder itself, because every PART SOURCE \
             is resolved from it",
            plan.entry
        ));
    }
    let mut paths = BTreeSet::from([plan.entry.clone()]);
    let mut stems = BTreeSet::from([stem(&plan.entry)?]);
    for part in &plan.parts {
        check_path(&part.path)?;
        require_role(engine, &part.role)?;
        if !paths.insert(part.path.clone()) {
            return refuse(format!("the plan names {} twice", part.path));
        }
        if !stems.insert(stem(&part.path)?) {
            return refuse(format!(
                "{} has the same file name as another planned file, so their generated IDs \
                 would collide",
                part.path
            ));
        }
        if let Some(master) = &part.master {
            check_id(master)?;
        }
    }
    Ok(())
}

/// The canonical entry of a planned project: one `PART` per planned part, in
/// plan order, and the `EXECUTE` every project entry requires, whose
/// `REFERENCE` is a slot.
pub fn entry(
    engine: &Engine,
    plan: &Plan,
    locale: Option<&LocaleTag>,
) -> Result<Scaffold, ScaffoldError> {
    check_plan(engine, plan)?;
    let stem = stem(&plan.entry)?;
    let profile = profile(engine, locale)?;
    let mut w = Writer::new(locale, profile.as_ref());
    header(
        &mut w,
        engine.spec().formal_version(),
        PROJECT_KIND,
        &stem,
        plan.mode,
    );
    if plan.mode == Mode::Guided {
        guidance(&mut w, engine.grammar(), PROJECT_KIND);
    }
    for part in &plan.parts {
        w.open("PART");
        w.field(
            "ID",
            Some(&format!("part.{}", self::stem(&part.path)?)),
            Some(MarkKind::GeneratedId),
        );
        let source = format!("{}(\"{}\")", w.word("PATH"), part.path);
        w.field("SOURCE", Some(&source), None);
        w.field("KIND", Some(&part.role), None);
        if !part.required {
            let no = w.word("FALSE");
            w.field("REQUIRED", Some(&no), None);
        }
    }
    w.open("EXECUTE");
    w.field("REFERENCE", None, Some(MarkKind::RequiredSlot));
    w.finish()
}

/// What a Master holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Content {
    /// A role Master: the new file's exact text.
    Text(String),
    /// A project Master: the files a new project starts with.
    Project(Plan),
}

/// One Master template.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Master {
    /// File-name-safe identifier, `[a-z0-9][a-z0-9_-]*`, at most 64 bytes.
    pub id: String,
    /// What a person sees.
    pub name: String,
    /// The one LCL Core version the Master targets.
    pub core: String,
    /// A project file role, or `kind.project` for a project Master.
    pub role: String,
    pub content: Content,
}

/// Check a Master identifier.
pub fn check_id(id: &str) -> Result<(), ScaffoldError> {
    let mut chars = id.chars();
    let ok = id.len() <= 64
        && chars
            .next()
            .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-');
    if ok {
        Ok(())
    } else {
        refuse(format!(
            "{id:?} is not a Master id: use at most 64 of a-z, 0-9, '_' and '-', \
             starting with a letter or digit"
        ))
    }
}

fn string_member<'j>(object: &'j Json, key: &str) -> Result<&'j str, ScaffoldError> {
    match object.get(key) {
        Some(value) => value
            .as_str()
            .ok_or_else(|| ScaffoldError(format!("Master key {key:?} must be a string"))),
        None => refuse(format!("a Master needs the key {key:?}")),
    }
}

/// Read one Master file. Every key is required except a part's `required`
/// (default `true`) and `master`; an unknown key, a wrong type or another
/// format version is refused. The Master's Core version, role and content are
/// checked against an engine by [`check_master`].
///
/// ```text
/// {"format": 1, "id": "my-task", "name": "My task", "core": "0.3.0",
///  "role": "kind.part.task", "text": "LCL:\n    VERSION: \"0.3.0\"\n..."}
///
/// {"format": 1, "id": "web", "name": "Web project", "core": "0.3.0",
///  "role": "kind.project", "mode": "guided", "entry": "main.lcl",
///  "parts": [{"path": "task.lcl", "role": "kind.part.task", "master": "my-task"}]}
/// ```
pub fn parse_master(json_text: &str) -> Result<Master, ScaffoldError> {
    let json = lcl_spec::json::parse(json_text)
        .map_err(|e| ScaffoldError(format!("a Master is one JSON object: {e}")))?;
    let Some(members) = json.as_object() else {
        return refuse("a Master is one JSON object");
    };
    let role = string_member(&json, "role")?.to_string();
    let allowed: &[&str] = if role == PROJECT_KIND {
        &[
            "format", "id", "name", "core", "role", "mode", "entry", "parts",
        ]
    } else {
        &["format", "id", "name", "core", "role", "text"]
    };
    if let Some((key, _)) = members
        .iter()
        .find(|(key, _)| !allowed.contains(&key.as_str()))
    {
        return refuse(format!(
            "{key:?} is not a key of a {} Master",
            if role == PROJECT_KIND {
                "project"
            } else {
                "role"
            }
        ));
    }
    match json.get("format").map(|f| f.as_u64()) {
        Some(Some(MASTER_FORMAT)) => {}
        Some(_) => {
            return refuse(format!(
                "this build reads Master format {MASTER_FORMAT} only"
            ))
        }
        None => return refuse("a Master needs the key \"format\""),
    }
    let id = string_member(&json, "id")?.to_string();
    check_id(&id)?;
    let name = string_member(&json, "name")?.to_string();
    if name.trim().is_empty() || name.chars().count() > 200 || name.chars().any(char::is_control) {
        return refuse("a Master name is 1 to 200 characters without control characters");
    }
    let core = string_member(&json, "core")?.to_string();
    let content = if role == PROJECT_KIND {
        let mode = string_member(&json, "mode")?;
        let Some(mode) = Mode::parse(mode) else {
            return refuse(format!(
                "{mode:?} is not a mode: use \"minimal\" or \"guided\""
            ));
        };
        let entry = string_member(&json, "entry")?.to_string();
        let Some(items) = json.get("parts").and_then(Json::as_array) else {
            return refuse("a project Master needs \"parts\", a list");
        };
        let mut parts = Vec::with_capacity(items.len());
        for item in items {
            let Some(fields) = item.as_object() else {
                return refuse("each project Master part is an object");
            };
            if let Some((key, _)) = fields
                .iter()
                .find(|(key, _)| !["path", "role", "required", "master"].contains(&key.as_str()))
            {
                return refuse(format!("{key:?} is not a key of a project Master part"));
            }
            let required = match item.get("required") {
                None => true,
                Some(value) => value.as_bool().ok_or_else(|| {
                    ScaffoldError("a part's \"required\" is true or false".into())
                })?,
            };
            let master = match item.get("master") {
                None => None,
                Some(value) => Some(
                    value
                        .as_str()
                        .ok_or_else(|| ScaffoldError("a part's \"master\" is a string".into()))?
                        .to_string(),
                ),
            };
            parts.push(PlannedPart {
                path: string_member(item, "path")?.to_string(),
                role: string_member(item, "role")?.to_string(),
                required,
                master,
            });
        }
        Content::Project(Plan { mode, entry, parts })
    } else {
        Content::Text(string_member(&json, "text")?.to_string())
    };
    Ok(Master {
        id,
        name,
        core,
        role,
        content,
    })
}

/// Check a Master against `engine`: it targets exactly this engine's Core
/// version, its role is a project file role (or `kind.project` for a project
/// Master), a role Master's text passes [`check_text`], and a project Master's
/// plan passes [`check_plan`] with every named part Master found by `lookup`,
/// a role Master of that part's role, and itself valid.
pub fn check_master(
    engine: &Engine,
    master: &Master,
    lookup: &dyn Fn(&str) -> Result<Master, ScaffoldError>,
) -> Result<Vec<Mark>, ScaffoldError> {
    let version = engine.spec().formal_version();
    if master.core != version {
        return refuse(format!(
            "Master {} targets LCL {}, and this engine is LCL {version}; a Master is never \
             reinterpreted under another Core version",
            master.id, master.core
        ));
    }
    match &master.content {
        Content::Text(text) => {
            require_role(engine, &master.role)?;
            check_text(engine, &master.role, text)
                .map_err(|e| ScaffoldError(format!("Master {}: {e}", master.id)))
        }
        Content::Project(plan) => {
            check_plan(engine, plan)?;
            for part in &plan.parts {
                let Some(id) = &part.master else { continue };
                let named = lookup(id)?;
                if !matches!(named.content, Content::Text(_)) {
                    return refuse(format!("{}: Master {id} is a project Master", part.path));
                }
                if named.role != part.role {
                    return refuse(format!(
                        "{}: Master {id} is for {}, not {}",
                        part.path, named.role, part.role
                    ));
                }
                check_master(engine, &named, lookup)?;
            }
            Ok(Vec::new())
        }
    }
}

/// A slot found in a text: its zero-based line, its field word and the word of
/// the top-level block it belongs to, both as written.
struct Slot {
    line: usize,
    field: String,
    block: String,
}

fn indentation(line: &str) -> usize {
    line.len() - line.trim_start_matches(' ').len()
}

/// Every slot of `text`: a line `WORD:` with nothing after the colon and
/// nothing nested under it, directly inside a top-level block. Lines inside a
/// multiline string are content, never slots.
fn find_slots(text: &str) -> Result<Vec<Slot>, ScaffoldError> {
    let lines: Vec<&str> = text.split('\n').collect();
    let mut slots = Vec::new();
    let mut block: Option<&str> = None;
    let mut multiline: Option<usize> = None;
    for (i, line) in lines.iter().enumerate() {
        let content = line.trim();
        if let Some(opened) = multiline {
            if content == "\"\"\"" && indentation(line) == opened {
                multiline = None;
            }
            continue;
        }
        if content.is_empty() || content.starts_with('@') {
            continue;
        }
        let indent = indentation(line);
        if content
            .split_once(':')
            .is_some_and(|(_, value)| value.trim() == "\"\"\"")
        {
            multiline = Some(indent);
            continue;
        }
        let Some(word) = content.strip_suffix(':') else {
            continue;
        };
        if indent == 0 {
            block = Some(word);
            continue;
        }
        if word.is_empty() || word.contains([' ', ':', '"']) {
            continue;
        }
        let nested = lines[i + 1..]
            .iter()
            .find(|next| !next.trim().is_empty())
            .is_some_and(|next| indentation(next) > indent);
        if nested {
            continue;
        }
        let parent = lines[..i]
            .iter()
            .rev()
            .find(|previous| !previous.trim().is_empty() && indentation(previous) < indent)
            .map(|previous| indentation(previous));
        let (Some(0), Some(block)) = (parent, block) else {
            return refuse(format!(
                "line {}: a slot must be a field of a top-level block",
                i + 1
            ));
        };
        slots.push(Slot {
            line: i,
            field: word.to_string(),
            block: block.to_string(),
        });
    }
    Ok(slots)
}

/// A value of `sig`'s form, used only to stage a text whose slots are empty.
/// It is never written anywhere. A closed-domain field gets the domain's first
/// member; other fields get the first form they accept, in a fixed order,
/// among the forms whose reserved words `word` can spell.
fn probe(
    grammar: &Grammar,
    sig: &FieldSignature,
    word: &dyn Fn(&str) -> Option<String>,
) -> Option<String> {
    if let Some(domain) = sig
        .value_kind
        .strip_prefix("qualified_identifier(")
        .and_then(|rest| rest.strip_suffix(')'))
    {
        if let Some(first) = grammar
            .closed_domain_members(domain)
            .and_then(|members| members.iter().next())
        {
            return Some(first.clone());
        }
    }
    let choices = [
        (FormSet::STRING, Some("\"0.0.0\"".to_string())),
        (FormSet::INTEGER, Some("0".to_string())),
        (FormSet::BOOLEAN, word("TRUE")),
        (
            FormSet::QUALIFIED_IDENTIFIER,
            Some("probe.value".to_string()),
        ),
        (FormSet::SIMPLE_IDENTIFIER, Some("probe".to_string())),
        (
            FormSet::REFERENCE,
            word("REF").map(|r| format!("{r}(probe.value)")),
        ),
        (
            FormSet::REFERENCE_LIST,
            word("REF").map(|r| format!("[{r}(probe.value)]")),
        ),
        (FormSet::TYPE_EXPRESSION, word("STRING")),
        (FormSet::EXPRESSION, word("TRUE")),
    ];
    choices
        .into_iter()
        .find(|(form, text)| sig.forms.contains(*form) && text.is_some())
        .and_then(|(_, text)| text)
}

fn stage(engine: &Engine, text: &str) -> ResolvedUnit {
    engine.stage(&SourceUnit::new(
        SourceId::new("master.lcl"),
        text.as_bytes().to_vec(),
    ))
}

fn line_of(text: &str, offset: usize) -> usize {
    text.as_bytes()[..offset.min(text.len())]
        .iter()
        .filter(|b| **b == b'\n')
        .count()
        + 1
}

/// The first diagnostic of a staged unit, as `line N: id`.
fn first_defect(unit: &ResolvedUnit) -> Option<String> {
    if let Some(d) = unit.localization().and_then(|l| l.diagnostics.first()) {
        return Some(format!(
            "line {}: {}",
            line_of(unit.source(), d.offset),
            d.id
        ));
    }
    if let Some(d) = unit.lexed().diagnostics().first() {
        return Some(format!(
            "line {}: {}",
            d.position.line,
            d.id.as_registry_str()
        ));
    }
    if let Some(d) = unit.parsed().and_then(|p| p.diagnostics().first()) {
        return Some(format!(
            "line {}: {}",
            d.position.line,
            d.id.as_registry_str()
        ));
    }
    if let Some(failure) = unit.stage_failure() {
        return Some(format!("{failure:?}"));
    }
    if unit.version_rejected() {
        return Some("the LCL VERSION is not one this engine processes".to_string());
    }
    None
}

/// Check a role Master's text, or any file text that may still hold slots, and
/// return its slot marks.
///
/// The text must end with a line feed, and its only lexical defects may be
/// its slots. Each slot must be a registered field of its top-level block
/// that takes an inline value. The text with every slot filled by a probe of
/// the field's form must then pass this engine's localization, lexical and
/// grammar-or-schema stages with no diagnostic at all, and declare exactly
/// this engine's LCL `VERSION` and `SPECIFICATION KIND` `role`, neither of
/// them a slot.
pub fn check_text(engine: &Engine, role: &str, text: &str) -> Result<Vec<Mark>, ScaffoldError> {
    if text.len() > MAX_MASTER_TEXT {
        return refuse(format!("the text is longer than {MAX_MASTER_TEXT} bytes"));
    }
    if !text.ends_with('\n') {
        return refuse("the text must end with a line feed");
    }
    let raw = stage(engine, text);
    let profile = raw.localization().and_then(|l| l.profile.as_ref());
    let canonical = |word: &str| {
        profile
            .and_then(|p| p.canonical(word))
            .unwrap_or(word)
            .to_string()
    };
    let spell = |word: &str| match profile {
        Some(p) => p.preferred(word).map(str::to_string),
        None => Some(word.to_string()),
    };
    let grammar = engine.grammar();
    // Name every slot's field first. An unknown word is a localization defect
    // too, and the precise report is this one.
    let slots = find_slots(text)?;
    let mut named = Vec::with_capacity(slots.len());
    for slot in &slots {
        let block = canonical(&slot.block);
        let field = canonical(&slot.field);
        let at = slot.line + 1;
        if (block == "LCL" && field == "VERSION") || (block == "SPECIFICATION" && field == "KIND") {
            return refuse(format!(
                "line {at}: {block} {field} is fixed and cannot be a slot"
            ));
        }
        let Some(sig) = grammar
            .schema(&block)
            .and_then(|schema| schema.field(&field))
        else {
            return refuse(format!("line {at}: {field} is not a field of {block}"));
        };
        named.push((slot.line, at, block, field, sig));
    }
    if let Some(d) = raw.localization().and_then(|l| l.diagnostics.first()) {
        return refuse(format!("line {}: {}", line_of(text, d.offset), d.id));
    }
    let lexical = raw.lexed().diagnostics();
    if let Some(d) = lexical
        .iter()
        .find(|d| d.id.as_registry_str() != "error.indentation.empty_block")
    {
        return refuse(format!(
            "line {}: {}",
            d.position.line,
            d.id.as_registry_str()
        ));
    }
    if slots.len() != lexical.len() {
        return refuse(format!(
            "the text has {} empty declarations but {} slots: a slot is a field line of a \
             top-level block with nothing after its colon",
            lexical.len(),
            slots.len()
        ));
    }
    let mut lines: Vec<String> = text.split('\n').map(str::to_string).collect();
    let mut marks = Vec::with_capacity(named.len());
    for (line, at, block, field, sig) in named {
        let Some(value) = probe(grammar, sig, &spell) else {
            return refuse(format!(
                "line {at}: {block} {field} holds nested lines, or no form its locale can spell, \
                 so it cannot be a slot"
            ));
        };
        lines[line] = format!("{} {value}", lines[line]);
        marks.push(Mark {
            kind: if sig.required {
                MarkKind::RequiredSlot
            } else {
                MarkKind::OptionalSlot
            },
            line: at,
            block,
            field,
        });
    }
    let filled = stage(engine, &lines.join("\n"));
    if let Some(defect) = first_defect(&filled) {
        return refuse(defect);
    }
    let Some(document) = filled.document() else {
        return refuse("the text did not parse");
    };
    let value = |block: &str, field: &str| {
        document.block(block).and_then(|b| b.field(field)).map(|f| {
            let span = f.body.span();
            filled.source()[span.start..span.end].trim().to_string()
        })
    };
    let version = engine.spec().formal_version();
    if value("LCL", "VERSION") != Some(format!("\"{version}\"")) {
        return refuse(format!("the text must declare LCL VERSION \"{version}\""));
    }
    if value("SPECIFICATION", "KIND").as_deref() != Some(role) {
        return refuse(format!("the text must declare SPECIFICATION KIND {role}"));
    }
    Ok(marks)
}

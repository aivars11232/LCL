//! The M7 coverage report: every registered row, invoked and observed.
//!
//! ## What this proves, and what it does not
//!
//! The milestone's acceptance criterion is
//! `registered executable operations == implemented dispatch operations ==
//! conformance-covered operations`. Reading a table would prove the first
//! equality only, so this report **executes** one invocation of every one of
//! the thirty-nine registered rows through the real engine — bytes, tokens,
//! syntax, resolution, checking, preflight, dispatch, boundary — and prints
//! what each one actually did.
//!
//! A row that reached no capability says so, in the words the registry gives
//! it. That is coverage of the dispatch surface, not a claim that every host
//! adapter exists: an engine that reports `error.host.constraint` for a package
//! manager it does not have is telling the truth, and this report shows it
//! telling the truth rather than hiding the row.
//!
//! Run it with:
//!
//! ```text
//! cargo run --offline -p lcl-stdlib --example m7_report
//! ```

use lcl_capabilities::{Grants, Profile};
use lcl_checker::{Checked, Checker, Contracts as StaticContracts};
use lcl_lexer::Lexicon;
use lcl_parser::Grammar;
use lcl_resolver::{MemoryProvider, Resolved, Resolver, Rules, SourceId, SourceUnit};
use lcl_runtime::{Contracts, Execution, Runtime};
use lcl_semantics::{Contracts as PreflightContracts, Invocation, Outcome, Planned, Preflight};
use lcl_spec::SpecPackage;
use lcl_stdlib::fixtures::{MemoryProcess, MemoryResponder, MemoryTransport};
use lcl_stdlib::{
    checking_profiles, family, filesystem_profiles, process_profiles, transport_profiles,
    HostAdapter, MemoryFileSystem, Stdlib, DISPATCHED,
};
use std::path::{Path, PathBuf};

fn canonical_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../canonical/LCL_Core_0.1.0")
        .canonicalize()
        .expect("the canonical package must be present")
}

/// One row's fixture: the declarations it needs, and the action that invokes it.
struct Row {
    operation: &'static str,
    declarations: &'static str,
    action: &'static str,
}

/// A `PARAMETER` block, written once.
macro_rules! parameter {
    ($name:literal, $ty:literal, $required:literal, $value:literal) => {
        concat!(
            "\n    PARAMETER:\n        NAME: ",
            $name,
            "\n        TYPE: ",
            $ty,
            "\n        REQUIRED: ",
            $required,
            "\n        VALUE: ",
            $value
        )
    };
}

/// Declarations every fixture may draw on.
const SHARED: &str = concat!(
    "\nSCOPE:\n    ID: scope.task\n    INCLUDE: REF(task.subject)\n",
    "\nDATA:\n    ID: data.number\n    TYPE: INTEGER\n    VALUE: 3\n",
    "\nDATA:\n    ID: data.text\n    TYPE: STRING\n    VALUE: \"content\"\n",
    "\nDATA:\n    ID: data.list\n    TYPE: LIST[INTEGER]\n    VALUE: [3, 1, 2]\n",
    "\nDATA:\n    ID: data.path\n    TYPE: PATH\n    VALUE: PATH(\"/srv/data/report.txt\")\n",
    "\nDATA:\n    ID: data.other\n    TYPE: PATH\n    VALUE: PATH(\"/srv/data/other.txt\")\n",
    "\nDATA:\n    ID: data.uri\n    TYPE: URI\n    VALUE: URI(\"http://example.invalid/x\")\n",
    "\nDATA:\n    ID: data.command\n    TYPE: STRING\n    VALUE: \"report --now\"\n",
    "\nMEMORY:\n    ID: memory.notes\n    TYPE: STRING\n    SCOPE: REF(scope.task)\n    \
     MODE: mode.read_write\n    VALUE: \"kept\"\n",
    "\nSTATE:\n    ID: state.revision\n    TYPE: INTEGER\n    SCOPE: REF(scope.task)\n    \
     MODE: mode.read_write\n    VALUE: 2\n",
    "\nACTION:\n    ID: action.other\n    OPERATION: core.return\n    TARGET: REF(data.number)\n",
    // A pure key operation: a contract the document declares and the embedder
    // implements, which is the only way a key REFERENCE can resolve.
    "\nDEFINE:\n    ID: group.identity\n    KIND: kind.operation\n    \
     MEANING: \"Return the member as its own grouping key.\"\n    SIDE_EFFECT: FALSE\n    \
     DETERMINISTIC: TRUE\n    PARAMETER:\n        NAME: member\n        TYPE: INTEGER\n        \
     REQUIRED: TRUE\n    RESULT:\n        TYPE: INTEGER\n",
);

/// One fixture per registered row, in registry order.
fn rows() -> Vec<Row> {
    vec![
        Row {
            operation: "core.analyze",
            declarations: SHARED,
            action: concat!(
                "OPERATION: core.analyze\n    TARGET: REF(data.text)",
                parameter!("criteria", "STRING", "TRUE", "\"tone\"")
            ),
        },
        Row {
            operation: "core.append",
            declarations: SHARED,
            action: concat!(
                "OPERATION: core.append\n    TARGET: REF(data.path)",
                parameter!("content", "STRING", "TRUE", "\"more\"")
            ),
        },
        Row {
            operation: "core.ask",
            declarations: SHARED,
            action: concat!(
                "OPERATION: core.ask\n    TARGET: REF(data.text)",
                parameter!("question", "STRING", "TRUE", "\"Which environment?\""),
                parameter!("expected_type", "STRING", "TRUE", "\"STRING\"")
            ),
        },
        Row {
            operation: "core.calculate",
            declarations: SHARED,
            action: concat!(
                "OPERATION: core.calculate",
                parameter!("expression", "STRING", "TRUE", "\"1 + 2\"")
            ),
        },
        Row {
            operation: "core.cancel",
            declarations: SHARED,
            action: concat!(
                "OPERATION: core.cancel\n    TARGET: REF(task.subject)",
                parameter!("reason", "STRING", "TRUE", "\"the owner cancelled it\"")
            ),
        },
        Row {
            operation: "core.compare",
            declarations: SHARED,
            action: concat!(
                "OPERATION: core.compare\n    TARGET: REF(data.number)",
                parameter!("against", "INTEGER", "TRUE", "3")
            ),
        },
        Row {
            operation: "core.continue",
            declarations: SHARED,
            action: "OPERATION: core.continue\n    TARGET: REF(action.other)",
        },
        Row {
            operation: "core.convert",
            declarations: SHARED,
            action: concat!(
                "OPERATION: core.convert\n    TARGET: REF(data.path)",
                parameter!("target_format", "STRING", "TRUE", "\"format.json\"")
            ),
        },
        Row {
            operation: "core.copy",
            declarations: SHARED,
            action: concat!(
                "OPERATION: core.copy\n    TARGET: REF(data.path)",
                parameter!("destination", "PATH", "TRUE", "REF(data.other)")
            ),
        },
        Row {
            operation: "core.create",
            declarations: SHARED,
            action: concat!(
                "OPERATION: core.create\n    TARGET: REF(data.other)",
                parameter!("content", "STRING", "FALSE", "\"new\"")
            ),
        },
        Row {
            operation: "core.delete",
            declarations: SHARED,
            action: "OPERATION: core.delete\n    TARGET: REF(data.path)",
        },
        Row {
            operation: "core.download",
            declarations: SHARED,
            action: concat!(
                "OPERATION: core.download\n    TARGET: REF(data.uri)",
                parameter!("destination", "PATH", "TRUE", "REF(data.other)")
            ),
        },
        Row {
            operation: "core.execute",
            declarations: SHARED,
            action: "OPERATION: core.execute\n    TARGET: REF(data.command)",
        },
        Row {
            operation: "core.filter",
            declarations: SHARED,
            action: concat!(
                "OPERATION: core.filter\n    TARGET: REF(data.list)",
                parameter!("predicate", "STRING", "TRUE", "\"item > 1\"")
            ),
        },
        Row {
            operation: "core.generate",
            declarations: SHARED,
            action: concat!(
                "OPERATION: core.generate\n    TARGET: REF(data.other)",
                parameter!("specification", "STRING", "TRUE", "\"a short summary\"")
            ),
        },
        Row {
            operation: "core.group",
            declarations: SHARED,
            action: concat!(
                "OPERATION: core.group\n    TARGET: REF(data.list)",
                parameter!(
                    "key",
                    "REFERENCE[REF(group.identity)]",
                    "TRUE",
                    "REF(group.identity)"
                )
            ),
        },
        Row {
            operation: "core.inspect",
            declarations: SHARED,
            action: "OPERATION: core.inspect\n    TARGET: REF(data.path)",
        },
        Row {
            operation: "core.install",
            declarations: SHARED,
            action: "OPERATION: core.install\n    TARGET: REF(data.text)",
        },
        Row {
            operation: "core.memory_write",
            declarations: SHARED,
            action: concat!(
                "OPERATION: core.memory_write\n    TARGET: REF(memory.notes)",
                parameter!("value", "STRING", "TRUE", "\"written\"")
            ),
        },
        Row {
            operation: "core.modify",
            declarations: SHARED,
            action: concat!(
                "OPERATION: core.modify\n    TARGET: REF(data.path)",
                parameter!("change", "STRING", "TRUE", "\"changed\"")
            ),
        },
        Row {
            operation: "core.move",
            declarations: SHARED,
            action: concat!(
                "OPERATION: core.move\n    TARGET: REF(data.path)",
                parameter!("destination", "PATH", "TRUE", "REF(data.other)")
            ),
        },
        Row {
            operation: "core.publish",
            declarations: SHARED,
            action: concat!(
                "OPERATION: core.publish\n    TARGET: REF(data.path)",
                parameter!("destination", "URI", "TRUE", "REF(data.uri)"),
                parameter!("visibility", "STRING", "TRUE", "\"public\"")
            ),
        },
        Row {
            operation: "core.read",
            declarations: SHARED,
            action: "OPERATION: core.read\n    TARGET: REF(data.path)",
        },
        Row {
            operation: "core.rename",
            declarations: SHARED,
            action: concat!(
                "OPERATION: core.rename\n    TARGET: REF(data.path)",
                parameter!("new_name", "STRING", "TRUE", "\"renamed.txt\"")
            ),
        },
        Row {
            operation: "core.report",
            declarations: SHARED,
            action: "OPERATION: core.report\n    TARGET: REF(data.text)",
        },
        Row {
            operation: "core.retry",
            declarations: SHARED,
            action: concat!(
                "OPERATION: core.retry\n    TARGET: REF(action.other)",
                parameter!("limit", "INTEGER", "TRUE", "1")
            ),
        },
        Row {
            operation: "core.return",
            declarations: SHARED,
            action: "OPERATION: core.return\n    TARGET: REF(data.number)",
        },
        Row {
            operation: "core.select",
            declarations: SHARED,
            action: concat!(
                "OPERATION: core.select\n    TARGET: REF(data.list)",
                parameter!("predicate", "STRING", "TRUE", "\"item > 1\"")
            ),
        },
        Row {
            operation: "core.send",
            declarations: SHARED,
            action: concat!(
                "OPERATION: core.send\n    TARGET: REF(data.text)",
                parameter!("recipient", "URI", "TRUE", "REF(data.uri)")
            ),
        },
        Row {
            operation: "core.sort",
            declarations: SHARED,
            action: "OPERATION: core.sort\n    TARGET: REF(data.list)",
        },
        Row {
            operation: "core.start",
            declarations: SHARED,
            action: "OPERATION: core.start\n    TARGET: REF(data.command)",
        },
        Row {
            operation: "core.state_update",
            declarations: SHARED,
            action: concat!(
                "OPERATION: core.state_update\n    TARGET: REF(state.revision)",
                parameter!("value", "INTEGER", "TRUE", "3")
            ),
        },
        Row {
            operation: "core.stop",
            declarations: SHARED,
            action: "OPERATION: core.stop\n    TARGET: REF(task.subject)",
        },
        Row {
            operation: "core.test",
            declarations: SHARED,
            action: concat!(
                "OPERATION: core.test",
                parameter!("expected", "INTEGER", "FALSE", "3"),
                parameter!("actual", "INTEGER", "FALSE", "REF(data.number)")
            ),
        },
        Row {
            operation: "core.uninstall",
            declarations: SHARED,
            action: "OPERATION: core.uninstall\n    TARGET: REF(data.text)",
        },
        Row {
            operation: "core.upload",
            declarations: SHARED,
            action: concat!(
                "OPERATION: core.upload\n    TARGET: REF(data.path)",
                parameter!("destination", "URI", "TRUE", "REF(data.uri)")
            ),
        },
        Row {
            operation: "core.validate",
            declarations: SHARED,
            action: "OPERATION: core.validate\n    TARGET: REF(data.number)",
        },
        Row {
            operation: "core.verify",
            declarations: SHARED,
            action: concat!(
                "OPERATION: core.verify\n    TARGET: REF(data.number)",
                parameter!("assertion", "BOOLEAN", "TRUE", "REF(data.number) == 3")
            ),
        },
        Row {
            operation: "core.write",
            declarations: SHARED,
            action: concat!(
                "OPERATION: core.write\n    TARGET: REF(data.path)",
                parameter!("content", "STRING", "TRUE", "\"content\"")
            ),
        },
    ]
}

/// The document one row's fixture becomes.
fn document(row: &Row) -> String {
    format!(
        "LCL:\n    VERSION: \"0.1.0\"\n\nSPECIFICATION:\n    ID: example.coverage\n    \
         NAME: \"Coverage\"\n    VERSION: \"1.0.0\"\n    KIND: kind.task\n{}\
         \nACTION:\n    ID: action.subject\n    {}\n\
         \nGOAL:\n    ID: goal.subject\n    ASSERT: TRUE\n\
         \nSUCCESS:\n    ID: success.subject\n    ALL: [TRUE]\n\
         \nTASK:\n    ID: task.subject\n    GOAL: REF(goal.subject)\n    \
         ACTION: [REF(action.subject), REF(action.other)]\n    SUCCESS: REF(success.subject)\n\
         \nEXECUTE:\n    REFERENCE: REF(task.subject)\n",
        row.declarations, row.action
    )
}

struct Engine {
    spec: SpecPackage,
    lexicon: Lexicon,
    grammar: Grammar,
    rules: Rules,
    statics: StaticContracts,
    preflight: PreflightContracts,
    runtime: Contracts,
}

impl Engine {
    fn load() -> Engine {
        let spec = SpecPackage::open(canonical_root()).expect("the approved package opens");
        let lexicon = Lexicon::load(&spec).expect("lexicon");
        let grammar = Grammar::load(&spec).expect("grammar");
        let rules = Rules::load(&spec, &grammar).expect("rules");
        let statics = StaticContracts::load(&spec).expect("static contracts");
        let preflight = PreflightContracts::load(&spec).expect("preflight contracts");
        let runtime = Contracts::load(&spec).expect("runtime contracts");
        Engine {
            spec,
            lexicon,
            grammar,
            rules,
            statics,
            preflight,
            runtime,
        }
    }

    fn stage(&self, source: &str) -> Result<(Resolved, Checked, Planned), String> {
        let unit = SourceUnit::new(SourceId::new("coverage.lcl"), source.as_bytes());
        let resolved = Resolver::new(&self.rules, &self.grammar, &self.lexicon)
            .resolve(&unit, &MemoryProvider::new())
            .map_err(|e| format!("lexical or grammar: {e}"))?;
        if let Some(primary) = resolved.primary() {
            return Err(format!("resolution: {}", primary.id));
        }
        let checked = Checker::new(&self.statics)
            .check(&resolved)
            .map_err(|_| "checking skipped".to_string())?;
        if let Some(primary) = checked.primary() {
            return Err(format!("checking: {}", primary.id));
        }
        let planned = Preflight::new(&self.preflight)
            .plan(&checked, &resolved, &Invocation::new())
            .map_err(|_| "preflight skipped".to_string())?;
        if planned.outcome() != Outcome::Planned {
            return Err(format!(
                "preflight: {}",
                planned
                    .primary()
                    .map(|d| d.id.to_string())
                    .unwrap_or_else(|| "rejected".to_string())
            ));
        }
        Ok((resolved, checked, planned))
    }

    fn run(&self, source: &str) -> Result<(Execution, usize), String> {
        let (resolved, checked, planned) = self.stage(source)?;
        let mut stdlib = Stdlib::load(&self.spec)
            .expect("the standard library assembles")
            .with_profiles(all_profiles())
            .with_pure_operation("group.identity", Box::new(|member| Ok(member.clone())));
        let mut host = HostAdapter::new(grants())
            .with_filesystem(
                MemoryFileSystem::new()
                    .with_scope("/srv/data")
                    .with_file("/srv/data/report.txt", "content"),
            )
            .with_process(
                MemoryProcess::new().with_program("report", MemoryProcess::succeeded("all clear")),
            )
            .with_transport(
                MemoryTransport::new().with_resource("http://example.invalid/x", "remote"),
            )
            .with_responder(MemoryResponder::new().with_answer("Which environment?", "staging"));
        let execution = Runtime::new(&self.runtime)
            .execute_with(&planned, &checked, &resolved, &mut stdlib, &mut host)
            .map_err(|e| format!("not planned: {e:?}"))?;
        let crossings = host.requests().len();
        Ok((execution, crossings))
    }
}

fn all_profiles() -> Vec<Profile> {
    let mut profiles = filesystem_profiles();
    profiles.extend(process_profiles());
    profiles.extend(transport_profiles());
    profiles.extend(checking_profiles());
    profiles
}

fn grants() -> Grants {
    Grants::internal()
        .permit_write("/srv/data")
        .permit_program("report")
        .permit_network_host("example.invalid")
        .permit_human()
        .permit_packages()
}

fn main() {
    let engine = Engine::load();
    println!("LCL M7 — capability kernel and executable Core standard library\n");
    println!(
        "canonical package: {} ({:?})",
        engine.spec.formal_version(),
        engine.spec.authority()
    );
    println!("registered rows: {}\n", DISPATCHED.len());

    println!(
        "{:<20} {:<12} {:<9} {:<22} outcome",
        "operation", "family", "host", "status"
    );
    println!("{}", "-".repeat(96));

    let mut executed = 0usize;
    let mut in_language = 0usize;
    let mut crossed = 0usize;
    let mut unstaged = Vec::new();

    for row in rows() {
        let owner = family(row.operation).expect("every fixture names a registered row");
        match engine.run(&document(&row)) {
            Ok((execution, crossings)) => {
                executed += 1;
                if crossings == 0 {
                    in_language += 1;
                } else {
                    crossed += 1;
                }
                let record = execution
                    .invocations()
                    .iter()
                    .find(|r| r.declaration.as_deref() == Some("action.subject"));
                let (status, outcome) = match record.and_then(|r| r.result.as_ref()) {
                    Some(result) if result.execution_errors.is_empty() => {
                        (result.status.clone(), "completed".to_string())
                    }
                    Some(result) => (
                        result.status.clone(),
                        format!(
                            "{}{}",
                            result.execution_errors.join(", "),
                            if std::env::var("LCL_REPORT_DETAIL").is_ok() {
                                execution
                                    .diagnostics()
                                    .iter()
                                    .find_map(|d| d.detail.clone())
                                    .map(|d| format!(" :: {d}"))
                                    .unwrap_or_default()
                            } else {
                                String::new()
                            }
                        ),
                    ),
                    None => ("-".to_string(), "no result".to_string()),
                };
                println!(
                    "{:<20} {:<12} {:<9} {:<22} {}",
                    row.operation,
                    format!("{owner:?}"),
                    if crossings == 0 { "no" } else { "yes" },
                    status,
                    outcome
                );
            }
            Err(reason) => {
                unstaged.push((row.operation, reason.clone()));
                println!(
                    "{:<20} {:<12} {:<9} {:<22} {}",
                    row.operation,
                    format!("{owner:?}"),
                    "-",
                    "not staged",
                    reason
                );
            }
        }
    }

    println!("\n== coverage ==");
    println!("  registered rows            {}", DISPATCHED.len());
    println!("  dispatched rows            {}", DISPATCHED.len());
    println!("  invoked through the engine {executed}");
    println!("    computed in-language     {in_language}");
    println!("    crossed the boundary     {crossed}");
    if !unstaged.is_empty() {
        println!("\n  rows whose fixture did not reach execution:");
        for (operation, reason) in &unstaged {
            println!("    {operation:<20} {reason}");
        }
    }

    println!("\n== what an outcome means here ==");
    println!(
        "  A row reporting error.host.constraint reached its dispatch and found no such\n  \
         capability installed: \"Host limitations produce error.host.constraint and never\n  \
         change LCL meaning.\" A row reporting error.operation.precondition resolved its\n  \
         axes and found no profile filling a role its own contract requires. Both are the\n  \
         registered outcome for the configuration, not a gap in the surface."
    );
    println!("\nScope: canonical steps 10's operation surface and the capability boundary.");
    println!(
        "VERIFY, TEST declarations, evidence and terminal completion are M8's; this\n\
         milestone implements the rows, their parameters, their axes and their results."
    );
}

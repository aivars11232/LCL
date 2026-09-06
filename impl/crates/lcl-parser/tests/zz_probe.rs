mod common;
use common::*;
use std::path::PathBuf;

fn files(sub: &str) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(canonical_root().join(sub)).unwrap()
        .filter_map(Result::ok).map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "lcl")).collect();
    v.sort(); v
}

#[test]
fn matrix_probe() {
    println!("===== VALID (must parse clean) =====");
    for p in files("08_EXAMPLES/VALID") {
        let parsed = parse_bytes(&std::fs::read(&p).unwrap());
        let n = p.file_name().unwrap().to_string_lossy();
        let d: Vec<String> = parsed.diagnostics().iter().map(|x| format!("{}@{}", x.id, x.span.start)).collect();
        println!("{} {:48} {:?}", if d.is_empty() {"PASS"} else {"FAIL"}, n, d);
    }
    println!("===== INVALID =====");
    for p in files("08_EXAMPLES/INVALID") {
        let lexed = lex_bytes(&std::fs::read(&p).unwrap());
        let n = p.file_name().unwrap().to_string_lossy();
        if lexed.primary().is_some() {
            println!("SKIP {:48} lexical: {}", n, lexed.primary().unwrap().id);
            continue;
        }
        let parsed = lcl_parser::Parser::new(grammar()).parse(&lexed).unwrap();
        let d: Vec<String> = parsed.diagnostics().iter().map(|x| format!("{}@{}", x.id, x.span.start)).collect();
        println!("     {:48} {:?}", n, d);
    }
}

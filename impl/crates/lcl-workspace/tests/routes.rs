//! The route table, driven over a real socket against a real project.

mod common;

use common::{get_json, send, serve_examples};
use lcl_spec::json::Json;

fn text(value: &Json) -> &str {
    value.as_str().expect("a string")
}

#[test]
fn the_session_names_the_package_every_result_is_judged_against() {
    let (_scratch, running) = serve_examples("session");
    let session = get_json(&running, "/api/session", &[]);

    assert_eq!(text(session.get("protocol").unwrap()), "lcl.engine/1");
    let spec = session.get("spec").unwrap();
    assert_eq!(text(spec.get("formal_version").unwrap()), "0.1.0");
    // The engine renders its own authority verdict; the UI does not restyle it.
    assert_eq!(text(spec.get("authority").unwrap()), "authoritative");
    assert_eq!(text(spec.get("identity_digest").unwrap()).len(), 64);
}

#[test]
fn the_tree_lists_every_document_and_nothing_else() {
    let (scratch, running) = serve_examples("tree");
    scratch.put("notes.txt", "not a document");
    scratch.put("sub/nested.lcl", &common::example("01_MINIMAL_TASK.lcl"));

    let reply = get_json(&running, "/api/documents", &[]);
    let entries = reply.get("entries").unwrap().as_array().expect("an array");
    let ids: Vec<&str> = entries.iter().map(|e| text(e.get("id").unwrap())).collect();

    assert!(ids.contains(&"01_MINIMAL_TASK.lcl"));
    assert!(ids.contains(&"sub/nested.lcl"));
    assert!(ids.contains(&"sub"));
    assert!(!ids.iter().any(|id| id.ends_with(".txt")));
    assert!(!ids.contains(&"lcl.project.json"));
}

#[test]
fn a_document_reads_back_exactly_the_bytes_on_disk() {
    let (_scratch, running) = serve_examples("read");
    for name in common::valid_examples() {
        let reply = get_json(&running, "/api/document", &[("id", &name)]);
        assert_eq!(
            text(reply.get("text").unwrap()),
            common::example(&name),
            "{name}: the route changed the bytes"
        );
        assert_eq!(text(reply.get("digest").unwrap()).len(), 64);
    }
}

#[test]
fn saving_writes_the_exact_bytes_and_reading_them_back_agrees() {
    let (scratch, running) = serve_examples("save");
    let name = "01_MINIMAL_TASK.lcl";
    let edited = common::example(name).replace("\"Double one integer\"", "\"Doubled\"");

    let reply = send(
        running.address,
        "PUT",
        &format!("/api/document?t={}&id={name}", running.token),
        &[],
        edited.as_bytes(),
    );
    assert_eq!(reply.status, 200, "{}", reply.body);

    let on_disk = std::fs::read_to_string(scratch.join(name)).expect("read");
    assert_eq!(on_disk, edited);

    let reread = get_json(&running, "/api/document", &[("id", name)]);
    assert_eq!(text(reread.get("text").unwrap()), edited);
}

#[test]
fn a_save_that_would_break_the_encoding_rule_is_refused_with_the_reason() {
    let (scratch, running) = serve_examples("refuse-save");
    let name = "01_MINIMAL_TASK.lcl";
    let before = std::fs::read(scratch.join(name)).expect("read");

    let crlf = common::example(name).replace('\n', "\r\n");
    let reply = send(
        running.address,
        "PUT",
        &format!("/api/document?t={}&id={name}", running.token),
        &[],
        crlf.as_bytes(),
    );
    assert_eq!(reply.status, 422);
    assert!(reply.body.contains("carriage return"), "{}", reply.body);
    assert_eq!(std::fs::read(scratch.join(name)).expect("read"), before);
}

#[test]
fn a_document_outside_the_project_is_not_reachable_through_a_route() {
    let (_scratch, running) = serve_examples("route-containment");
    for id in [
        "../escape.lcl",
        "%2Fetc%2Fpasswd",
        "sub%2F..%2F..%2Fescape.lcl",
    ] {
        let reply = send(
            running.address,
            "GET",
            &format!("/api/document?t={}&id={id}", running.token),
            &[],
            b"",
        );
        assert_eq!(
            reply.status, 404,
            "{id} must not be readable: {}",
            reply.body
        );
    }
}

#[test]
fn the_frontend_is_served_and_asks_for_nothing_off_this_machine() {
    let (_scratch, running) = serve_examples("frontend");
    for (path, kind) in [
        ("/", "text/html"),
        ("/app.css", "text/css"),
        ("/app.js", "text/javascript"),
    ] {
        let reply = send(
            running.address,
            "GET",
            &format!("{path}?t={}", running.token),
            &[],
            b"",
        );
        assert_eq!(reply.status, 200, "{path}");
        assert!(reply.header("Content-Type").unwrap().starts_with(kind));
        // Nothing may be fetched from another origin: the content security
        // policy would block it, so a reference to one is a bug, not a
        // fallback.
        for offsite in ["http://", "https://", "//cdn", "@import url("] {
            assert!(
                !reply.body.contains(offsite),
                "{path} refers to {offsite}, which the CSP will block"
            );
        }
    }
}

#[test]
fn an_unknown_route_is_a_refusal_rather_than_a_guess() {
    let (_scratch, running) = serve_examples("unknown");
    let reply = send(
        running.address,
        "GET",
        &format!("/api/does-not-exist?t={}", running.token),
        &[],
        b"",
    );
    assert_eq!(reply.status, 404);
}

#[test]
fn every_asset_the_page_references_is_fetchable_the_way_a_browser_fetches_it() {
    // Regression. The page was served correctly and then sat blank, because
    // the browser requests the stylesheet and the script by itself and those
    // requests carry no token unless the page puts one on them. A headless
    // Firefox found it; this is the test that keeps it found.
    let (_scratch, running) = serve_examples("assets");
    let page = send(
        running.address,
        "GET",
        &format!("/?t={}", running.token),
        &[],
        b"",
    );
    assert_eq!(page.status, 200);
    assert!(
        !page.body.contains("{{TOKEN}}"),
        "the page must be stamped, not served with its placeholder"
    );

    // Pull every local URL out of the page and fetch it exactly as written.
    let mut referenced = Vec::new();
    for marker in ["href=\"", "src=\""] {
        let mut rest = page.body.as_str();
        while let Some(at) = rest.find(marker) {
            rest = &rest[at + marker.len()..];
            let Some(end) = rest.find('"') else { break };
            let url = &rest[..end];
            if url.starts_with('/') {
                referenced.push(url.to_string());
            }
        }
    }
    assert!(
        referenced.len() >= 2,
        "the page must reference its stylesheet and its script: {referenced:?}"
    );

    for url in referenced {
        let reply = send(running.address, "GET", &url, &[], b"");
        assert_eq!(
            reply.status, 200,
            "a browser fetching {url} with no extra headers must be served"
        );
        assert!(!reply.body.is_empty(), "{url} came back empty");
    }
}

#[test]
fn the_frontend_declares_no_function_twice() {
    // Regression, and a nasty one: a later `function f() {}` silently replaces
    // an earlier one, because declarations hoist and the last wins. A stub
    // left behind from an earlier phase disabled live analysis entirely, with
    // no error anywhere — the page simply stopped making requests.
    //
    // Checked here rather than trusted, because nothing else would notice.
    let source = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/app.js"),
    )
    .expect("the frontend script is readable");

    let mut names = Vec::new();
    for line in source.lines() {
        let rest = line
            .strip_prefix("async function ")
            .or_else(|| line.strip_prefix("function "));
        if let Some(rest) = rest {
            if let Some(name) = rest.split('(').next() {
                names.push(name.trim().to_string());
            }
        }
    }
    assert!(names.len() > 20, "the script should declare many functions");

    let mut seen = std::collections::BTreeSet::new();
    let mut duplicated = Vec::new();
    for name in &names {
        if !seen.insert(name.clone()) {
            duplicated.push(name.clone());
        }
    }
    assert!(
        duplicated.is_empty(),
        "these functions are declared more than once, so only the last one \
         runs: {duplicated:?}"
    );
}

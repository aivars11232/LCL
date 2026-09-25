//! The route table, driven over a real socket against a real project.

mod common;

use common::{get_json, send, send_binary, serve_examples};
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

// ---------------------------------------------------------------------------
// The product's own mark
// ---------------------------------------------------------------------------
//
// B1. The header showed the letters `LCL` in a monospace font and the page
// declared no favicon at all, so a browser asked for `/favicon.ico`, was
// refused by the token gate, and the tab stayed blank.
//
// What these cases refuse to accept as evidence: a file existing on disk, a
// route answering 200, or the HTML naming a path. An image is delivered when
// the exact bytes arrive under a type a decoder will accept, so that is what
// is compared.

/// The committed derivative behind one served route.
fn derived(name: &str) -> Vec<u8> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("assets/brand")
        .join(name);
    std::fs::read(&path).unwrap_or_else(|e| panic!("{} is not readable: {e}", path.display()))
}

/// Width, height and colour type from a PNG's own header.
///
/// Read from the bytes rather than from a library, because the question is
/// whether *these* bytes are a decodable image. `IHDR` is fixed-position: an
/// eight-byte signature, a four-byte length, the chunk type, then the
/// dimensions as big-endian `u32`s, then bit depth and colour type.
fn png_header(bytes: &[u8]) -> (u32, u32, u8) {
    assert_eq!(
        &bytes[..8],
        b"\x89PNG\r\n\x1a\n",
        "the reply does not begin with the PNG signature"
    );
    assert_eq!(&bytes[12..16], b"IHDR", "the first chunk must be IHDR");
    let number =
        |at: usize| u32::from_be_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]);
    (number(16), number(20), bytes[25])
}

#[test]
fn the_header_mark_is_served_as_the_exact_image_on_disk() {
    let (_scratch, running) = serve_examples("brand-mark");
    let reply = send_binary(
        running.address,
        &format!("/brand/lcl-mark.png?t={}", running.token),
    );

    assert_eq!(reply.status, 200);
    assert_eq!(
        reply.content_type, "image/png",
        "an image needs an image content type, not a text one"
    );
    assert_eq!(
        reply.body,
        derived("lcl-mark.png"),
        "the served bytes are not the committed derivative"
    );

    let (width, height, colour) = png_header(&reply.body);
    assert_eq!((width, height), (97, 96));
    // Colour type 6 is truecolour with alpha. The supplied artwork is
    // transparent, and a derivative that had lost its alpha would still decode.
    assert_eq!(colour, 6, "the mark must keep its alpha channel");
}

#[test]
fn the_favicon_is_served_as_a_square_transparent_image() {
    let (_scratch, running) = serve_examples("brand-icon");
    let reply = send_binary(
        running.address,
        &format!("/brand/lcl-icon-32.png?t={}", running.token),
    );

    assert_eq!(reply.status, 200);
    assert_eq!(reply.content_type, "image/png");
    assert_eq!(reply.body, derived("lcl-icon-32.png"));
    let (width, height, colour) = png_header(&reply.body);
    assert_eq!((width, height), (32, 32));
    assert_eq!(colour, 6);
}

#[test]
fn the_page_references_both_images_with_the_token_the_gate_needs() {
    let (_scratch, running) = serve_examples("brand-page");
    let page = send(
        running.address,
        "GET",
        &format!("/?t={}", running.token),
        &[],
        b"",
    );
    assert_eq!(page.status, 200);

    // A reference without the token is a reference the browser cannot follow,
    // which is exactly how the first attempt at this failed.
    for reference in [
        format!("/brand/lcl-icon-32.png?t={}", running.token),
        format!("/brand/lcl-mark.png?t={}", running.token),
    ] {
        assert!(
            page.body.contains(&reference),
            "the page does not reference {reference}"
        );
    }
    assert!(
        page.body.contains("rel=\"icon\""),
        "the page must declare a real favicon rather than leaving the browser \
         to guess at /favicon.ico"
    );
    // The mark carries an accessible name, because it replaced readable text.
    assert!(
        page.body.contains("alt=\"LCL\""),
        "the header image must keep the name the letters used to give it"
    );
}

#[test]
fn an_image_is_refused_without_the_token_like_everything_else() {
    let (_scratch, running) = serve_examples("brand-gate");
    for path in ["/brand/lcl-mark.png", "/brand/lcl-icon-32.png"] {
        let reply = send(running.address, "GET", path, &[], b"");
        assert_eq!(reply.status, 403, "{path} answered without a session token");
    }
    // And a guessed favicon path is not a hole in the route table.
    let reply = send(
        running.address,
        "GET",
        &format!("/favicon.ico?t={}", running.token),
        &[],
        b"",
    );
    assert_eq!(
        reply.status, 404,
        "no unauthenticated or undeclared favicon route exists"
    );
}

#[test]
fn an_image_is_refused_from_another_origin() {
    let (_scratch, running) = serve_examples("brand-origin");
    let reply = send(
        running.address,
        "GET",
        &format!("/brand/lcl-mark.png?t={}", running.token),
        &[("Origin", "http://evil.example")],
        b"",
    );
    assert_eq!(
        reply.status, 403,
        "an image route must pass the same origin check as every other route"
    );
}

// ---------------------------------------------------------------------------
// Creating a document
// ---------------------------------------------------------------------------

/// POST one create request and return the parsed reply.
fn create(running: &common::Running, id: &str, body: &str) -> common::Reply {
    send(
        running.address,
        "POST",
        &format!("/api/document?id={id}&t={}", running.token),
        &[],
        body.as_bytes(),
    )
}

const SEED: &str = "LCL:\n    VERSION: \"0.1.0\"\n\nSPECIFICATION:\n    ID: example.new\n    \
                    NAME: \"New document\"\n    VERSION: \"1.0.0\"\n    KIND: kind.task\n    \
                    DOMAIN: \"general\"\n";

#[test]
fn a_new_document_gets_lcl_unless_an_ending_was_chosen() {
    // `.lcl` is the native default for a name without an ending; an explicitly
    // chosen `.lcl` or `.lcl.txt` is kept as written, and nothing is stacked.
    let (scratch, running) = serve_examples("create-default");
    scratch.put("notes.txt", "ordinary text stays ordinary\n");
    for (written, expected) in [
        ("plain", "plain.lcl"),
        ("classic.lcl", "classic.lcl"),
        ("modern.lcl.txt", "modern.lcl.txt"),
        ("nested/deep", "nested/deep.lcl"),
        ("nested/chosen.lcl", "nested/chosen.lcl"),
        ("nested/shared.lcl.txt", "nested/shared.lcl.txt"),
        ("notes.txt", "notes.txt.lcl"),
    ] {
        let reply = create(&running, written, SEED);
        assert_eq!(reply.status, 200, "{written}: {}", reply.body);
        let parsed = lcl_spec::json::parse(&reply.body).expect("a reply is JSON");
        assert_eq!(
            text(parsed.get("id").unwrap()),
            expected,
            "{written} was created under the wrong name"
        );
        assert_eq!(text(parsed.get("requested").unwrap()), written);
        assert!(
            scratch.join(expected).is_file(),
            "{expected} is not on disk"
        );
    }
    // No stacked or converted twin was created beside what was asked for.
    for absent in [
        "plain.lcl.txt",
        "classic.lcl.txt",
        "modern.lcl.txt.lcl",
        "modern.lcl",
        "nested/chosen.lcl.txt",
        "nested/shared.lcl",
        "notes.txt.lcl.txt",
    ] {
        assert!(!scratch.join(absent).exists(), "{absent} must not exist");
    }
    assert_eq!(
        std::fs::read_to_string(scratch.join("notes.txt")).unwrap(),
        "ordinary text stays ordinary\n"
    );
}

#[test]
fn a_name_that_is_only_a_suffix_is_refused() {
    let (_scratch, running) = serve_examples("create-bare");
    for written in [".lcl", ".lcl.txt", "%20%20"] {
        let reply = create(&running, written, SEED);
        assert_eq!(reply.status, 400, "{written:?}: {}", reply.body);
    }
}

#[test]
fn creating_over_an_existing_document_is_refused_rather_than_overwriting_it() {
    let (scratch, running) = serve_examples("create-conflict");
    assert_eq!(create(&running, "once", SEED).status, 200);
    let original = std::fs::read_to_string(scratch.join("once.lcl")).expect("readable");

    let again = create(&running, "once", "LCL:\n");
    assert_eq!(again.status, 409, "{}", again.body);
    assert_eq!(
        std::fs::read_to_string(scratch.join("once.lcl")).expect("readable"),
        original,
        "a refused creation must leave the document untouched"
    );

    // The same document, named with its ending, is the same document.
    let by_full_name = create(&running, "once.lcl", SEED);
    assert_eq!(
        by_full_name.status, 409,
        "`once` defaults to `once.lcl`, which already exists: {}",
        by_full_name.body
    );

    // An explicitly chosen `.lcl.txt` is a different file name, so it does not
    // collide with `once.lcl`, and creating it leaves `once.lcl` untouched.
    let text_form = create(&running, "once.lcl.txt", "LCL:\n");
    assert_eq!(text_form.status, 200, "{}", text_form.body);
    assert!(scratch.join("once.lcl.txt").is_file());
    assert_eq!(
        std::fs::read_to_string(scratch.join("once.lcl")).expect("readable"),
        original,
        "creating once.lcl.txt must not touch once.lcl"
    );
    assert_eq!(
        create(&running, "once.lcl.txt", SEED).status,
        409,
        "and once.lcl.txt, now existing, is refused in turn"
    );

    // Neither is_file precheck recognizes these occupied names. The atomic
    // publication error must still become a conflict and preserve the entry.
    let missing = scratch.join("missing.txt");
    let link = scratch.join("link.lcl");
    std::os::unix::fs::symlink(&missing, &link).unwrap();
    let refused = create(&running, "link", SEED);
    assert_eq!(refused.status, 409, "{}", refused.body);
    assert_eq!(std::fs::read_link(link).unwrap(), missing);
    assert!(!missing.exists());

    let directory = scratch.join("occupied.lcl");
    std::fs::create_dir(&directory).unwrap();
    std::fs::write(directory.join("user.txt"), b"preserved").unwrap();
    let refused = create(&running, "occupied", SEED);
    assert_eq!(refused.status, 409, "{}", refused.body);
    assert_eq!(
        std::fs::read(directory.join("user.txt")).unwrap(),
        b"preserved"
    );
}

#[test]
fn saving_writes_the_exact_name_it_was_given() {
    // `PUT` is not `POST`. Saving an open `.lcl` document must never apply the
    // creation default, or every save would rename what it opened.
    let (scratch, running) = serve_examples("save-exact");
    let name = "01_MINIMAL_TASK.lcl";
    let edited = common::example(name).replace("\"Double one integer\"", "\"Edited\"");
    let reply = send(
        running.address,
        "PUT",
        &format!("/api/document?id={name}&t={}", running.token),
        &[],
        edited.as_bytes(),
    );
    assert_eq!(reply.status, 200, "{}", reply.body);
    let parsed = lcl_spec::json::parse(&reply.body).expect("a reply is JSON");
    assert_eq!(text(parsed.get("id").unwrap()), name);
    assert!(scratch.join(name).is_file());
    assert!(
        !scratch.join("01_MINIMAL_TASK.lcl.txt").exists(),
        "a save created a second document under another name"
    );

    // An open `.lcl.txt` document is saved under exactly that name too.
    let text_name = "shared.lcl.txt";
    scratch.put(text_name, &edited);
    let reply = send(
        running.address,
        "PUT",
        &format!("/api/document?id={text_name}&t={}", running.token),
        &[],
        edited.as_bytes(),
    );
    assert_eq!(reply.status, 200, "{}", reply.body);
    let parsed = lcl_spec::json::parse(&reply.body).expect("a reply is JSON");
    assert_eq!(text(parsed.get("id").unwrap()), text_name);
    assert!(scratch.join(text_name).is_file());
    for renamed in ["shared.lcl", "shared.lcl.txt.lcl"] {
        assert!(
            !scratch.join(renamed).exists(),
            "saving {text_name} created {renamed}"
        );
    }
}

#[test]
fn a_created_document_is_checked_and_run_like_any_other() {
    // The ending is not a way past the engine. A `.lcl.txt` document goes
    // through the same route, the same engine and the same contracts.
    let (_scratch, running) = serve_examples("create-check");
    let created = create(&running, "fresh.lcl.txt", SEED);
    assert_eq!(created.status, 200);

    let checked = send(
        running.address,
        "POST",
        &format!("/api/check?id=fresh.lcl.txt&t={}", running.token),
        &[],
        SEED.as_bytes(),
    );
    assert_eq!(checked.status, 200, "{}", checked.body);
    let report = lcl_spec::json::parse(&checked.body).expect("a report");
    assert!(
        report.get("reached").is_some(),
        "a created document must produce an engine report: {}",
        checked.body
    );

    // And an invalid one under the same ending is still refused.
    let invalid = send(
        running.address,
        "POST",
        &format!("/api/check?id=fresh.lcl.txt&t={}", running.token),
        &[],
        b"LCL:\n    VERSION: \"0.1.0\"\n\nNOT_A_BLOCK:\n    ID: x\n",
    );
    assert_eq!(invalid.status, 200, "{}", invalid.body);
    let report = lcl_spec::json::parse(&invalid.body).expect("a report");
    let diagnostics = report
        .get("diagnostics")
        .and_then(lcl_spec::json::Json::as_array)
        .expect("diagnostics");
    assert!(
        !diagnostics.is_empty(),
        "ending a name in .txt must not relax validation: {}",
        invalid.body
    );
}

/// PRETEST-04 F15: the frontend never hands text to the HTML parser. A startup
/// failure carries a server or transport message, which is text, and a message
/// holding markup must not become markup.
#[test]
fn the_frontend_never_assigns_markup() {
    let source = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/app.js"),
    )
    .expect("the frontend script is readable");
    let mut sinks = Vec::new();
    for (number, line) in source.lines().enumerate() {
        let code = line.trim_start();
        if code.starts_with('*') || code.starts_with("//") || code.starts_with("/*") {
            continue;
        }
        for sink in [
            "innerHTML",
            "outerHTML",
            "insertAdjacentHTML",
            "document.write",
        ] {
            if code.contains(sink) {
                sinks.push(format!("{}: {}", number + 1, code));
            }
        }
    }
    assert!(sinks.is_empty(), "markup sinks in app.js: {sinks:#?}");
}

#[test]
fn revoking_an_android_device_takes_only_a_device_id() {
    // Refused before any program is looked for or run, installed or not.
    let (_scratch, running) = serve_examples("remote-revoke");
    for id in ["-rf", "..%2Fdevices.json", "a%20b", "ABC", ""] {
        let reply = send(
            running.address,
            "POST",
            &format!("/api/remote/revoke?t={}&id={id}", running.token),
            &[],
            b"",
        );
        assert_eq!(reply.status, 400, "{id:?}: {}", reply.body);
    }
    let missing = send(
        running.address,
        "POST",
        &format!("/api/remote/revoke?t={}", running.token),
        &[],
        b"",
    );
    assert_eq!(missing.status, 400);
}

#[test]
fn approving_or_denying_a_pairing_request_takes_only_a_request_id() {
    // Refused before any program is looked for or run, installed or not.
    let (_scratch, running) = serve_examples("remote-decide");
    for route in ["approve", "deny"] {
        for id in ["-rf", "--json", "..%2Fpairing.json", "a%20b", "ABC", ""] {
            let reply = send(
                running.address,
                "POST",
                &format!("/api/remote/{route}?t={}&id={id}", running.token),
                &[],
                b"",
            );
            assert_eq!(reply.status, 400, "{route} {id:?}: {}", reply.body);
        }
        let missing = send(
            running.address,
            "POST",
            &format!("/api/remote/{route}?t={}", running.token),
            &[],
            b"",
        );
        assert_eq!(missing.status, 400, "{route} without an id");
    }
}

#[test]
fn android_device_routes_need_the_session_token() {
    let (_scratch, running) = serve_examples("remote-token");
    for (method, path) in [
        ("GET", "/api/remote/devices"),
        ("POST", "/api/remote/pair"),
        ("POST", "/api/remote/revoke?id=00"),
        ("GET", "/api/remote/pending"),
        ("POST", "/api/remote/approve?id=00"),
        ("POST", "/api/remote/deny?id=00"),
    ] {
        let reply = send(running.address, method, path, &[], b"");
        assert_eq!(
            reply.status, 403,
            "{method} {path} without the token: {}",
            reply.body
        );
    }
}

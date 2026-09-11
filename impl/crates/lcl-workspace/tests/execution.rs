//! Running a document through the workspace, and watching it happen.

mod common;

use common::{example, send, serve_examples, valid_examples, Running};
use lcl_spec::json::Json;
use std::io::{Read, Write};
use std::net::TcpStream;

fn text(value: &Json) -> &str {
    value.as_str().expect("a string")
}

/// Start a run and return its id.
fn start(running: &Running, id: &str, body: &str, extra: &str) -> String {
    let reply = send(
        running.address,
        "POST",
        &format!("/api/run?t={}&id={id}{extra}", running.token),
        &[],
        body.as_bytes(),
    );
    assert_eq!(reply.status, 200, "run refused: {}", reply.body);
    let parsed = lcl_spec::json::parse(&reply.body).expect("JSON");
    text(parsed.get("run").unwrap()).to_string()
}

/// Read one run's event stream to its end, as `(event, payload)` pairs.
///
/// A real socket reading a real `text/event-stream`, framed the way the format
/// requires, because that is what a browser will do.
fn stream(running: &Running, run: &str) -> Vec<(String, String)> {
    let mut socket = TcpStream::connect(running.address).expect("connect");
    let request = format!(
        "GET /api/events?t={}&run={run} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nContent-Length: 0\r\n\r\n",
        running.token,
        running.address.port()
    );
    socket.write_all(request.as_bytes()).expect("sent");
    socket.flush().expect("flushed");
    socket
        .set_read_timeout(Some(std::time::Duration::from_secs(30)))
        .expect("timeout");

    let mut raw = String::new();
    socket.read_to_string(&mut raw).expect("stream read");
    parse_events(&raw)
}

fn parse_events(raw: &str) -> Vec<(String, String)> {
    let body = raw.split_once("\r\n\r\n").map(|(_, b)| b).unwrap_or("");
    let mut out = Vec::new();
    for frame in body.split("\n\n") {
        let mut name = String::new();
        let mut data = Vec::new();
        for line in frame.lines() {
            if let Some(rest) = line.strip_prefix("event: ") {
                name = rest.to_string();
            } else if let Some(rest) = line.strip_prefix("data: ") {
                data.push(rest.to_string());
            }
        }
        if !name.is_empty() {
            out.push((name, data.join("\n")));
        }
    }
    out
}

/// The raw bytes of the `report` event, for a byte-for-byte comparison.
fn report_payload(events: &[(String, String)]) -> String {
    events
        .iter()
        .find(|(name, _)| name == "report")
        .map(|(_, payload)| payload.clone())
        .unwrap_or_else(|| panic!("the run produced no report; events: {events:?}"))
}

fn report_of(events: &[(String, String)]) -> Json {
    let payload = events
        .iter()
        .find(|(name, _)| name == "report")
        .map(|(_, payload)| payload.clone())
        .unwrap_or_else(|| panic!("the run produced no report; events: {events:?}"));
    lcl_spec::json::parse(&payload).expect("the report is JSON")
}

#[test]
fn every_valid_example_runs_to_exactly_one_terminal_status() {
    let (_scratch, running) = serve_examples("run-all");
    for name in valid_examples() {
        let run = start(&running, &name, &example(&name), "");
        let events = stream(&running, &run);
        let report = report_of(&events);

        assert_eq!(text(report.get("command").unwrap()), "run");
        let completion = report.get("completion");
        if let Some(completion) = completion {
            let status = text(completion.get("terminal_status").unwrap());
            assert!(!status.is_empty(), "{name}: an empty terminal status");
            assert!(
                !text(completion.get("reason").unwrap()).is_empty(),
                "{name}: a terminal status with no stated reason"
            );
        } else {
            // No completion means the run stopped before step 13, and the
            // report must say where and why rather than going quiet.
            assert!(
                !report
                    .get("diagnostics")
                    .unwrap()
                    .as_array()
                    .unwrap()
                    .is_empty(),
                "{name}: no completion and no diagnostic is not a truthful report"
            );
        }
    }
}

#[test]
fn the_stream_ends_and_carries_the_events_the_run_observed() {
    let (_scratch, running) = serve_examples("stream");
    let name = "01_MINIMAL_TASK.lcl";
    let run = start(&running, name, &example(name), "");
    let events = stream(&running, &run);

    let names: Vec<&str> = events.iter().map(|(n, _)| n.as_str()).collect();
    assert!(names.contains(&"report"), "events: {names:?}");
    assert_eq!(names.last(), Some(&"end"), "the stream must close itself");
    assert!(
        names.contains(&"operation"),
        "a run that invokes an operation must say so: {names:?}"
    );
}

/// A document whose ACTION writes one file, so a run needs a real capability.
///
/// The same shape `lcl-cli`'s capability suite uses: a `kind.task` document
/// needs exactly one `EXECUTE` root, and the write needs an `ALLOW` to
/// authorize it in the language before any host grant is even consulted.
/// Both gates, which is the point.
fn writing_document(target: &std::path::Path) -> String {
    format!(
        "LCL:\n    VERSION: \"0.1.0\"\n\
         \n\
         SPECIFICATION:\n    ID: example.write\n    NAME: \"Write one file\"\n    \
         VERSION: \"1.0.0\"\n    KIND: kind.task\n\
         \n\
         DATA:\n    ID: data.content\n    TYPE: STRING\n    VALUE: \"written by lcl\"\n\
         \n\
         OUTPUT:\n    ID: output.written\n    TYPE: PATH\n    FORMAT: format.plain_text\n\
         \n\
         GOAL:\n    ID: goal.write\n    ASSERT: TRUE\n\
         \n\
         ALLOW:\n    ID: allow.write\n    OPERATION: core.write\n    \
         TARGET: PATH({target:?})\n    AUTHORITY: 900\n\
         \n\
         ACTION:\n    ID: action.write\n    OPERATION: core.write\n    \
         TARGET: PATH({target:?})\n    PARAMETER:\n        NAME: content\n        \
         TYPE: STRING\n        REQUIRED: TRUE\n        VALUE: REF(data.content)\n    \
         PARAMETER:\n        NAME: create_if_missing\n        TYPE: BOOLEAN\n        \
         REQUIRED: FALSE\n        VALUE: TRUE\n    \
         OUTPUT: REF(output.written)\n\
         \n\
         SUCCESS:\n    ID: success.write\n    ALL: [REF(output.written)]\n\
         \n\
         TASK:\n    ID: task.write\n    GOAL: REF(goal.write)\n    \
         ACTION: REF(action.write)\n    OUTPUT: REF(output.written)\n    \
         SUCCESS: REF(success.write)\n\
         \n\
         EXECUTE:\n    REFERENCE: REF(task.write)\n",
        target = target.display().to_string()
    )
}

/// Percent-encode one grant value for a query string.
fn encode(raw: &str) -> String {
    let mut out = String::new();
    for byte in raw.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

#[test]
fn a_run_that_needs_a_capability_nobody_granted_says_so_rather_than_doing_it() {
    // The default. Nothing is granted unless a grant names it, so the write
    // does not happen and the report says why. The document's own ALLOW
    // authorized it; the host offered nothing to carry it out, which is the
    // two gates of contract 5.7 behaving independently.
    let (scratch, running) = serve_examples("ungranted");
    let target = scratch.join("written-by-lcl.txt");
    let source = writing_document(&target);

    let run = start(&running, "write.lcl", &source, "");
    let events = stream(&running, &run);
    let report = report_of(&events);

    let ids: Vec<&str> = report
        .get("diagnostics")
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .map(|d| text(d.get("id").unwrap()))
        .collect();
    assert!(
        ids.iter().any(|id| id.starts_with("error.host")
            || id.starts_with("error.permission")
            || id.starts_with("error.operation.precondition")),
        "an ungranted effect must fail for a host reason: {ids:?}"
    );
    assert!(
        !target.exists(),
        "nothing was granted, so nothing may be written"
    );
}

#[test]
fn a_granted_capability_actually_performs_the_effect() {
    // The other half, and the one that proves a grant is real rather than
    // decorative: the file is on disk afterwards, with the bytes the document
    // said.
    let (scratch, running) = serve_examples("granted");
    let target = scratch.join("written-by-lcl.txt");
    let source = writing_document(&target);

    let grant = format!("&allow_write={}", encode(&target.display().to_string()));
    let run = start(&running, "write.lcl", &source, &grant);
    let events = stream(&running, &run);
    let report = report_of(&events);

    let ids: Vec<String> = report
        .get("diagnostics")
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .map(|d| text(d.get("id").unwrap()).to_string())
        .collect();
    assert!(
        target.exists(),
        "the grant named this path, so the effect must happen. diagnostics: {ids:?}"
    );
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "written by lcl");
}

#[test]
fn a_run_reports_the_supplied_data_it_was_given() {
    let (_scratch, running) = serve_examples("inputs");
    let name = "01_MINIMAL_TASK.lcl";
    let run = start(&running, name, &example(name), "&input=input.value%3D9");
    let events = stream(&running, &run);
    let report = report_of(&events);

    let inputs = report.get("inputs").unwrap().as_array().unwrap();
    assert_eq!(inputs.len(), 1);
    assert_eq!(text(inputs[0].get("id").unwrap()), "input.value");
}

#[test]
fn two_runs_of_the_same_document_produce_the_same_record() {
    // Determinism, observed rather than argued. Same bytes, same inputs, same
    // grants: the same report, byte for byte, including the event order.
    let (_scratch, running) = serve_examples("deterministic");
    let name = "05_CONDITION_AND_ITERATION.lcl";
    let source = example(name);

    let first = report_payload(&stream(&running, &start(&running, name, &source, "")));
    let second = report_payload(&stream(&running, &start(&running, name, &source, "")));
    assert_eq!(first, second, "two identical runs disagreed");
}

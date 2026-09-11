//! Breaking at the capability boundary, and the gate consent cannot open.

mod common;

use common::{send, serve_examples, Running, Scratch};
use lcl_spec::json::Json;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::Path;

fn text(value: &Json) -> &str {
    value.as_str().expect("a string")
}

/// A document whose ACTION writes one file, authorized by its own ALLOW.
fn writing_document(target: &Path) -> String {
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

/// The same document, with the write prohibited by the language itself.
///
/// `FORBID` and not "no `ALLOW`". Removing the `ALLOW` does not make an effect
/// unauthorized: `block_schemas_v0.1.0.json#/schemas/ALLOW` says an `ALLOW` is
/// "Permission only; does not require execution", and the absence of a
/// permission is not a prohibition. `#/schemas/FORBID` is the prohibition —
/// "Hard prohibition", which an `ALLOW` "never defeats by itself" — so that is
/// what a test of the language's own gate has to use.
fn forbidden_document(target: &Path) -> String {
    let forbid = format!(
        "FORBID:\n    ID: forbid.write\n    OPERATION: core.write\n    \
         TARGET: PATH({target:?})\n    AUTHORITY: 1000\n\n",
        target = target.display().to_string()
    );
    writing_document(target).replacen("ACTION:", &format!("{forbid}ACTION:"), 1)
}

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

/// An open event stream that can be read one frame at a time.
struct Stream {
    socket: TcpStream,
    buffer: String,
    started: bool,
}

impl Stream {
    fn open(running: &Running, run: &str) -> Stream {
        let mut socket = TcpStream::connect(running.address).expect("connect");
        let request = format!(
            "GET /api/events?t={}&run={run} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\n\
             Content-Length: 0\r\n\r\n",
            running.token,
            running.address.port()
        );
        socket.write_all(request.as_bytes()).expect("sent");
        socket.flush().expect("flushed");
        socket
            .set_read_timeout(Some(std::time::Duration::from_secs(20)))
            .expect("timeout");
        Stream {
            socket,
            buffer: String::new(),
            started: false,
        }
    }

    /// Read frames until one named `name` arrives, and return its payload.
    fn wait_for(&mut self, name: &str) -> Option<String> {
        loop {
            if let Some(found) = self.take(name) {
                return Some(found);
            }
            let mut chunk = [0u8; 4096];
            match self.socket.read(&mut chunk) {
                Ok(0) => return self.take(name),
                Ok(n) => self.buffer.push_str(&String::from_utf8_lossy(&chunk[..n])),
                Err(_) => return self.take(name),
            }
        }
    }

    /// Pull the first complete frame with this name out of the buffer.
    fn take(&mut self, name: &str) -> Option<String> {
        if !self.started {
            let at = self.buffer.find("\r\n\r\n")?;
            self.buffer = self.buffer[at + 4..].to_string();
            self.started = true;
        }
        let mut consumed = 0usize;
        let frames: Vec<String> = self.buffer.split("\n\n").map(str::to_string).collect();
        for (i, frame) in frames.iter().enumerate() {
            if i + 1 == frames.len() {
                break; // possibly incomplete
            }
            consumed += frame.len() + 2;
            let mut event = String::new();
            let mut data = Vec::new();
            for line in frame.lines() {
                if let Some(rest) = line.strip_prefix("event: ") {
                    event = rest.to_string();
                } else if let Some(rest) = line.strip_prefix("data: ") {
                    data.push(rest.to_string());
                }
            }
            if event == name {
                self.buffer = self.buffer[consumed..].to_string();
                return Some(data.join("\n"));
            }
        }
        None
    }
}

/// Answer a pause on its own connection, the way the browser does.
fn answer(running: &Running, run: &str, sequence: u64, answer: &str) -> u16 {
    send(
        running.address,
        "POST",
        &format!(
            "/api/answer?t={}&run={run}&sequence={sequence}&answer={answer}",
            running.token
        ),
        &[],
        b"",
    )
    .status
}

#[test]
fn a_run_pauses_at_the_effect_boundary_and_says_what_it_is_about_to_do() {
    let (scratch, running) = serve_examples("pause");
    let target = scratch.join("written.txt");
    let grant = format!("&allow_write={}", encode(&target.display().to_string()));
    let run = start(
        &running,
        "write.lcl",
        &writing_document(&target),
        &format!("{grant}&break_effects=1"),
    );

    let mut stream = Stream::open(&running, &run);
    let paused = stream.wait_for("paused").expect("the run pauses");
    let pause = lcl_spec::json::parse(&paused).expect("JSON");

    assert_eq!(text(pause.get("kind").unwrap()), "effect");
    assert_eq!(text(pause.get("operation").unwrap()), "core.write");
    // Everything an operator needs in order to judge it.
    assert!(pause.get("target").unwrap().as_str().is_some());
    assert!(!pause
        .get("parameters")
        .unwrap()
        .as_array()
        .unwrap()
        .is_empty());
    assert_eq!(text(pause.get("source").unwrap()), "write.lcl");
    assert!(pause.get("span").unwrap().get("start").is_some());

    let authorization = pause.get("authorization").unwrap();
    assert_eq!(text(authorization.get("operation").unwrap()), "core.write");
    assert!(
        !authorization
            .get("permitted_by")
            .unwrap()
            .as_array()
            .unwrap()
            .is_empty(),
        "the pause must name the rule that authorized it"
    );

    // While it is paused, nothing has happened yet.
    assert!(!target.exists(), "a paused effect has not happened");

    let sequence = pause.get("sequence").unwrap().as_u64().unwrap();
    assert_eq!(answer(&running, &run, sequence, "continue"), 200);
    stream.wait_for("end");
    assert!(target.exists(), "allowing the effect performs it");
}

#[test]
fn denying_a_paused_effect_refuses_it_and_the_engine_decides_what_that_means() {
    let (scratch, running) = serve_examples("deny");
    let target = scratch.join("written.txt");
    let grant = format!("&allow_write={}", encode(&target.display().to_string()));
    let run = start(
        &running,
        "write.lcl",
        &writing_document(&target),
        &format!("{grant}&break_effects=1"),
    );

    let mut stream = Stream::open(&running, &run);
    let paused = stream.wait_for("paused").expect("the run pauses");
    let sequence = lcl_spec::json::parse(&paused)
        .unwrap()
        .get("sequence")
        .unwrap()
        .as_u64()
        .unwrap();
    assert_eq!(answer(&running, &run, sequence, "deny"), 200);

    let report = stream.wait_for("report").expect("a report");
    let report = lcl_spec::json::parse(&report).unwrap();

    assert!(!target.exists(), "a denied effect must not happen");
    let ids: Vec<&str> = report
        .get("diagnostics")
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .map(|d| text(d.get("id").unwrap()))
        .collect();
    assert!(
        ids.contains(&"error.permission.denied"),
        "a host refusal has a registered meaning: {ids:?}"
    );
}

#[test]
fn consent_cannot_open_a_gate_the_language_kept_shut() {
    // The acceptance criterion, executed.
    //
    // The host grant is given. The operator would say yes. And it still cannot
    // happen, because the document forbids it: the engine refuses at preflight,
    // before step 10, so the host is never consulted and there is no pause to
    // answer. Contract 5.7 in one test — two gates, and the UI is only ever the
    // second one.
    let (scratch, running) = serve_examples("no-bypass");
    let target = scratch.join("never-written.txt");
    let grant = format!("&allow_write={}", encode(&target.display().to_string()));

    let run = start(
        &running,
        "write.lcl",
        &forbidden_document(&target),
        &format!("{grant}&break_effects=1"),
    );

    let mut stream = Stream::open(&running, &run);
    let report = stream.wait_for("report").expect("a report");
    let report = lcl_spec::json::parse(&report).unwrap();

    assert!(
        !target.exists(),
        "a forbidden effect must not happen, grant or no grant"
    );
    assert_eq!(
        text(report.get("reached").unwrap()),
        "preflight",
        "the refusal must come before execution, not from the host"
    );

    let ids: Vec<&str> = report
        .get("diagnostics")
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .map(|d| text(d.get("id").unwrap()))
        .collect();
    assert!(
        ids.contains(&"error.permission.denied"),
        "the language's own refusal, with its registered identifier: {ids:?}"
    );

    // And the operator was never asked, because there was nothing to ask
    // about. A consent prompt for a forbidden effect would be the UI offering
    // to do what the document prohibits.
    assert!(
        stream.take("paused").is_none(),
        "the operator must not be asked to permit what LCL forbids"
    );
}

#[test]
fn cancelling_stops_the_run_and_leaves_the_project_intact() {
    let (scratch, running) = serve_examples("cancel");
    let target = scratch.join("written.txt");
    let grant = format!("&allow_write={}", encode(&target.display().to_string()));
    let run = start(
        &running,
        "write.lcl",
        &writing_document(&target),
        &format!("{grant}&break_effects=1"),
    );

    let mut stream = Stream::open(&running, &run);
    let paused = stream.wait_for("paused").expect("the run pauses");
    let sequence = lcl_spec::json::parse(&paused)
        .unwrap()
        .get("sequence")
        .unwrap()
        .as_u64()
        .unwrap();
    assert_eq!(answer(&running, &run, sequence, "cancel"), 200);
    stream.wait_for("end");

    assert!(
        !target.exists(),
        "a cancelled run performs no further effect"
    );

    // The project is untouched and still usable: every document reads back and
    // a second run works. A cancel that corrupted state would show up here.
    for name in common::valid_examples() {
        let reply = send(
            running.address,
            "GET",
            &format!("/api/document?t={}&id={name}", running.token),
            &[],
            b"",
        );
        assert_eq!(reply.status, 200, "{name} must still be readable");
    }
    let again = start(
        &running,
        "01_MINIMAL_TASK.lcl",
        &common::example("01_MINIMAL_TASK.lcl"),
        "",
    );
    let mut second = Stream::open(&running, &again);
    assert!(
        second.wait_for("report").is_some(),
        "the engine must still run after a cancelled run"
    );
}

#[test]
fn a_stale_answer_cannot_pre_authorise_the_next_effect() {
    // A browser holding an old sequence number must not be able to answer a
    // pause it was never shown.
    let (scratch, running) = serve_examples("stale");
    let target = scratch.join("written.txt");
    let grant = format!("&allow_write={}", encode(&target.display().to_string()));
    let run = start(
        &running,
        "write.lcl",
        &writing_document(&target),
        &format!("{grant}&break_effects=1"),
    );

    let mut stream = Stream::open(&running, &run);
    let paused = stream.wait_for("paused").expect("the run pauses");
    let sequence = lcl_spec::json::parse(&paused)
        .unwrap()
        .get("sequence")
        .unwrap()
        .as_u64()
        .unwrap();

    assert_eq!(
        answer(&running, &run, sequence + 99, "continue"),
        409,
        "an answer for a different pause is refused"
    );
    assert!(!target.exists(), "the run is still waiting");

    assert_eq!(answer(&running, &run, sequence, "continue"), 200);
    stream.wait_for("end");
}

#[test]
fn a_run_without_breakpoints_never_pauses() {
    let (scratch, running) = serve_examples("no-breaks");
    let target = scratch.join("written.txt");
    let grant = format!("&allow_write={}", encode(&target.display().to_string()));
    let run = start(&running, "write.lcl", &writing_document(&target), &grant);

    let mut stream = Stream::open(&running, &run);
    assert!(stream.wait_for("report").is_some());
    assert!(
        stream.take("paused").is_none(),
        "nothing asked for a pause, so nothing may pause"
    );
    assert!(target.exists());
}

#[test]
fn answering_a_run_that_does_not_exist_is_refused() {
    let (_scratch, running) = serve_examples("missing-run");
    assert_eq!(answer(&running, "run-does-not-exist", 1, "continue"), 404);
}

#[test]
fn a_denied_effect_leaves_a_pre_existing_file_exactly_as_it_was() {
    let scratch = Scratch::new("deny-preserve");
    let target = scratch.join("existing.txt");
    std::fs::write(&target, "original contents\n").expect("seed");

    let (_project, running) = serve_examples("deny-preserve-project");
    let grant = format!("&allow_write={}", encode(&target.display().to_string()));
    let run = start(
        &running,
        "write.lcl",
        &writing_document(&target),
        &format!("{grant}&break_effects=1"),
    );

    let mut stream = Stream::open(&running, &run);
    let paused = stream.wait_for("paused").expect("the run pauses");
    let sequence = lcl_spec::json::parse(&paused)
        .unwrap()
        .get("sequence")
        .unwrap()
        .as_u64()
        .unwrap();
    assert_eq!(answer(&running, &run, sequence, "deny"), 200);
    stream.wait_for("end");

    assert_eq!(
        std::fs::read_to_string(&target).unwrap(),
        "original contents\n",
        "a denied write must not have touched the file"
    );
}

//! Real authenticated HTTP writers, with an owned product process per server.

mod common;

use common::Scratch;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::Barrier;
use std::time::{Duration, Instant};

struct Server {
    child: Child,
    address: SocketAddr,
    token: String,
}

impl Server {
    fn start(root: &Path, log_name: &str) -> Self {
        let log_path = root.join(log_name);
        let log = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&log_path)
            .unwrap();
        let child = Command::new(env!("CARGO_BIN_EXE_lcl-workspace"))
            .arg(root)
            .arg("--spec")
            .arg(common::canonical_root())
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("TMPDIR", std::env::temp_dir())
            .current_dir(root)
            .stdin(Stdio::null())
            .stdout(log.try_clone().unwrap())
            .stderr(log)
            .spawn()
            .unwrap();
        // Establish the cleanup guard before any startup assertion can fail.
        let mut server = Self {
            child,
            address: "127.0.0.1:0".parse().unwrap(),
            token: String::new(),
        };
        let started = Instant::now();
        loop {
            let output = std::fs::read_to_string(&log_path).unwrap();
            for line in output.lines() {
                if let Some(url) = line.trim().strip_prefix("open     http://") {
                    if let Some((address, token)) = url.split_once("/?t=") {
                        if !token.is_empty() {
                            server.address = address.parse().unwrap();
                            server.token = token.to_string();
                            return server;
                        }
                    }
                }
            }
            assert!(
                server.child.try_wait().unwrap().is_none(),
                "server exited: {output}"
            );
            assert!(
                started.elapsed() < Duration::from_secs(10),
                "server startup exceeded ten seconds: {output}"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// Deliver a valid authenticated request except for its final body byte.
    /// Both sockets are prepared before either route can receive a full body.
    fn prepare(&self, method: &str, id: &str, body: &[u8]) -> TcpStream {
        assert!(!body.is_empty());
        let mut stream = TcpStream::connect_timeout(&self.address, Duration::from_secs(3)).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        write!(stream,
            "{method} /api/document?id={id}&t={} HTTP/1.1\r\nHost: {}\r\nOrigin: http://{}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            self.token, self.address, self.address, body.len()).unwrap();
        stream.write_all(&body[..body.len() - 1]).unwrap();
        stream.flush().unwrap();
        stream
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill(); // this exact test-created process, never a name pattern
        let started = Instant::now();
        loop {
            match self.child.try_wait() {
                Ok(Some(_)) => return, // reaped, including every connection thread
                Ok(None) if started.elapsed() < Duration::from_secs(2) => {
                    std::thread::sleep(Duration::from_millis(5));
                }
                other => {
                    let detail = format!(
                        "fixture server {} cleanup incomplete: {other:?}",
                        self.child.id()
                    );
                    if std::thread::panicking() {
                        eprintln!("{detail}");
                        return;
                    }
                    panic!("{detail}");
                }
            }
        }
    }
}

#[derive(Debug)]
struct Reply {
    status: u16,
    body: String,
}

fn finish(mut stream: TcpStream, last: u8, barrier: &Barrier) -> Reply {
    barrier.wait();
    stream.write_all(&[last]).unwrap();
    let mut raw = Vec::new();
    let mut chunk = [0; 4096];
    loop {
        let count = stream.read(&mut chunk).unwrap();
        if count == 0 {
            break;
        }
        raw.extend_from_slice(&chunk[..count]);
        assert!(
            raw.len() <= 64 * 1024,
            "unexpectedly large fixture response"
        );
    }
    let text = String::from_utf8(raw).unwrap();
    let (head, body) = text.split_once("\r\n\r\n").unwrap();
    Reply {
        status: head.split_whitespace().nth(1).unwrap().parse().unwrap(),
        body: body.to_string(),
    }
}

fn together(left: TcpStream, right: TcpStream, a: u8, b: u8) -> (Reply, Reply) {
    let barrier = Barrier::new(2);
    std::thread::scope(|scope| {
        let left = scope.spawn(|| finish(left, a, &barrier));
        let right = scope.spawn(|| finish(right, b, &barrier));
        (left.join().unwrap(), right.join().unwrap())
    })
}

fn payload(byte: u8) -> Vec<u8> {
    let mut bytes = vec![byte; 128 * 1024];
    bytes.push(b'\n');
    bytes
}

fn no_temporaries(root: &Path) {
    let leftovers: Vec<_> = std::fs::read_dir(root)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .filter(|name| name.to_string_lossy().ends_with(".tmp"))
        .collect();
    assert!(leftovers.is_empty(), "temporary debris: {leftovers:?}");
}

fn create_pair(root: &Path, left: &Server, right: &Server, names: [&str; 2], expected: &str) {
    let a = payload(b'a');
    let b = payload(b'b');
    let first = left.prepare("POST", names[0], &a);
    let second = right.prepare("POST", names[1], &b);
    assert!(
        !root.join(expected).exists(),
        "an incomplete request published a document"
    );
    let (one, two) = together(first, second, b'\n', b'\n');
    let mut statuses = [one.status, two.status];
    statuses.sort();
    assert_eq!(statuses, [200, 409], "{one:?}; {two:?}");
    let (winner, reply) = if one.status == 200 {
        (&a, &one)
    } else {
        (&b, &two)
    };
    assert_eq!(std::fs::read(root.join(expected)).unwrap(), *winner);
    let response = lcl_spec::json::parse(&reply.body).unwrap();
    assert_eq!(response.get("id").unwrap().as_str(), Some(expected));
    assert_eq!(
        response.get("digest").unwrap().as_str(),
        Some(lcl_spec::sha256::hex_digest(winner).as_str())
    );
    no_temporaries(root);
}

#[test]
fn authenticated_same_and_normalized_names_have_one_complete_winner() {
    let scratch = Scratch::new("concurrent-create");
    scratch.put("notes.txt", "ordinary user data\n");
    let server = Server::start(&scratch.path, "server.log");
    create_pair(
        &scratch.path,
        &server,
        &server,
        ["same", "same"],
        "same.lcl",
    );
    create_pair(
        &scratch.path,
        &server,
        &server,
        ["alias", "alias.lcl"],
        "alias.lcl",
    );
    // An explicitly chosen `.lcl.txt` is its own name (it no longer shares a
    // file with `full`), so its race is between two requests for that name.
    create_pair(
        &scratch.path,
        &server,
        &server,
        ["full.lcl.txt", "full.lcl.txt"],
        "full.lcl.txt",
    );
    assert_eq!(
        std::fs::read(scratch.join("notes.txt")).unwrap(),
        b"ordinary user data\n"
    );
}

#[test]
fn separate_workspace_processes_cannot_both_create_the_same_document() {
    let scratch = Scratch::new("two-server-create");
    let left = Server::start(&scratch.path, "left.log");
    let right = Server::start(&scratch.path, "right.log");
    create_pair(
        &scratch.path,
        &left,
        &right,
        ["shared", "shared.lcl"],
        "shared.lcl",
    );
}

#[test]
fn overlapping_saves_preserve_whole_payloads_and_the_legacy_name() {
    let scratch = Scratch::new("concurrent-save");
    scratch.put("classic.lcl", "original\n");
    let server = Server::start(&scratch.path, "server.log");
    let a = payload(b'a');
    let b = payload(b'b');
    let left = server.prepare("PUT", "classic.lcl", &a);
    let right = server.prepare("PUT", "classic.lcl", &b);
    let (one, two) = together(left, right, b'\n', b'\n');
    // Replacing saves are ordered by acceptance (UI-03). When the save accepted
    // later publishes first, the overtaken one is refused with 409 instead of
    // replacing newer content. Both outcomes are legitimate: two 200s, or
    // exactly one 409 that names the supersession and published nothing.
    let observed = std::fs::read(scratch.join("classic.lcl")).unwrap();
    assert!(
        observed == a || observed == b,
        "save mixed or truncated payloads"
    );
    let replies = [(&one, &a), (&two, &b)];
    let accepted: Vec<_> = replies
        .iter()
        .filter(|(reply, _)| reply.status == 200)
        .collect();
    assert!(
        !accepted.is_empty(),
        "both overlapping saves were refused: {one:?}; {two:?}"
    );
    for (reply, bytes) in &replies {
        match reply.status {
            200 => {
                let response = lcl_spec::json::parse(&reply.body).unwrap();
                assert_eq!(
                    response.get("digest").unwrap().as_str(),
                    Some(lcl_spec::sha256::hex_digest(bytes).as_str())
                );
            }
            409 => {
                assert!(
                    reply
                        .body
                        .contains("classic.lcl was saved again while this write was in flight"),
                    "a refused save must name the supersession: {reply:?}"
                );
                assert_eq!(
                    observed, *accepted[0].1,
                    "an overtaken save must not replace the accepted content"
                );
            }
            other => panic!("unexpected status {other}: {one:?}; {two:?}"),
        }
    }
    assert!(!scratch.join("classic.lcl.txt").exists());
    no_temporaries(&scratch.path);
}

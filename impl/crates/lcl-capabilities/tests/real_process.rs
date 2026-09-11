//! `RealProcess` against real programs that write more than a pipe holds.
//!
//! Regression, post-Task-20 finding F3. The adapter piped both streams and then
//! waited for exit before reading either. A pipe holds one buffer -- 65,536
//! bytes on Linux -- so a program that printed more than that blocked in
//! `write`, never exited, and was killed as a timeout. The command had done
//! nothing wrong; the supervisor was waiting for something it was preventing.
//!
//! The cap had the same shape of defect one layer along: it was applied to a
//! `Vec` that had already been filled, so `max_stream_bytes` bounded what the
//! caller saw and not what the process allocated.
//!
//! Nothing here is mocked. A fake host would only prove that the fake agrees
//! with itself, and the thing under test is what happens between two real file
//! descriptors.

use lcl_capabilities::bounds::{Bounds, Deadline};
use lcl_capabilities::grant::Grants;
use lcl_capabilities::process::{Command, Process, ProcessError, RealProcess};
use std::collections::BTreeMap;

/// Comfortably more than a pipe buffer, and enough that a blocked write is the
/// only way the old adapter could have behaved.
const PAST_PIPE_BUFFER: usize = 1024 * 1024;

/// A shell, granted. Every case runs one `sh -c` script.
fn shell(script: &str) -> Command {
    Command {
        program: "/bin/sh".to_string(),
        arguments: vec!["-c".to_string(), script.to_string()],
        working_directory: None,
        environment: BTreeMap::new(),
    }
}

fn adapter() -> RealProcess {
    RealProcess::new(Grants::none().permit_program("/bin/sh"))
}

/// A script printing `bytes` bytes to `stream`, in one `head` read.
///
/// `/dev/zero` through `tr` is deterministic, needs no temporary file, and
/// produces the volume in one process rather than in a shell loop.
fn flood(stream: &str, bytes: usize) -> String {
    format!("head -c {bytes} /dev/zero | tr '\\0' 'x' >&{stream}")
}

/// A generous deadline. The point of these cases is that a healthy command
/// completes, so the bound must be one no healthy command could reach.
fn deadline() -> Bounds {
    Bounds::new().with_deadline(Some(Deadline::from_nanos(30_000_000_000)))
}

#[test]
fn a_program_writing_past_the_pipe_buffer_on_stdout_completes() {
    let mut process = adapter();
    let completion = process
        .run(&shell(&flood("1", PAST_PIPE_BUFFER)), &deadline())
        .expect("a command that merely prints a lot is not a timeout");
    assert_eq!(completion.exit_code, Some(0));
    assert_eq!(completion.stdout.len(), PAST_PIPE_BUFFER);
    assert!(completion.stderr.is_empty());
    assert!(!completion.truncated);
}

#[test]
fn a_program_writing_past_the_pipe_buffer_on_stderr_completes() {
    let mut process = adapter();
    let completion = process
        .run(&shell(&flood("2", PAST_PIPE_BUFFER)), &deadline())
        .expect("stderr is drained too, not only stdout");
    assert_eq!(completion.exit_code, Some(0));
    assert!(completion.stdout.is_empty());
    assert_eq!(completion.stderr.len(), PAST_PIPE_BUFFER);
}

#[test]
fn a_program_flooding_both_streams_at_once_completes() {
    // Both pipes full at the same time. Draining one stream and then the other
    // would deadlock here even though each drain is individually correct.
    let mut process = adapter();
    let script = format!(
        "{} & {} ; wait",
        flood("1", PAST_PIPE_BUFFER),
        flood("2", PAST_PIPE_BUFFER)
    );
    let completion = process
        .run(&shell(&script), &deadline())
        .expect("both streams are drained concurrently");
    assert_eq!(completion.exit_code, Some(0));
    assert_eq!(completion.stdout.len(), PAST_PIPE_BUFFER);
    assert_eq!(completion.stderr.len(), PAST_PIPE_BUFFER);
}

#[test]
fn the_stream_bound_holds_while_the_output_is_being_read() {
    // The program writes two megabytes; the bound retains eight kilobytes. The
    // command must still complete, which it can only do if the excess is being
    // read and dropped rather than left in the pipe.
    let cap = 8 * 1024;
    let mut process = adapter();
    let bounds = deadline().with_max_stream_bytes(cap as u64);
    let completion = process
        .run(&shell(&flood("1", 2 * 1024 * 1024)), &bounds)
        .expect("a bounded capture is not a timeout");
    assert_eq!(completion.exit_code, Some(0));
    assert_eq!(completion.stdout.len(), cap);
    assert!(
        completion.truncated,
        "a stream cut at its bound must say so rather than look complete"
    );
}

#[test]
fn a_genuinely_slow_program_still_times_out() {
    // The repair must not have turned the deadline off. This one writes
    // nothing and sleeps past its bound.
    let mut process = adapter();
    let bounds = Bounds::new().with_deadline(Some(Deadline::from_nanos(200_000_000)));
    let outcome = process.run(&shell("sleep 30"), &bounds);
    match outcome {
        Err(ProcessError::Bounded(cancelled)) => {
            assert!(
                cancelled.reason.contains("timeout"),
                "a timeout says which bound elapsed: {}",
                cancelled.reason
            );
        }
        other => panic!("a program past its deadline is a bounded failure, got {other:?}"),
    }
}

#[test]
fn a_program_that_floods_and_then_hangs_still_times_out() {
    // The interesting combination: enough output to fill a pipe, then a sleep
    // past the deadline. The old adapter reported a timeout here for the wrong
    // reason. The repaired one must report it for the right one, and must not
    // wait for the flood to be collected before doing so.
    let mut process = adapter();
    let bounds = Bounds::new().with_deadline(Some(Deadline::from_nanos(500_000_000)));
    let script = format!("{}; sleep 30", flood("1", PAST_PIPE_BUFFER));
    let started = std::time::Instant::now();
    let outcome = process.run(&shell(&script), &bounds);
    assert!(
        matches!(outcome, Err(ProcessError::Bounded(_))),
        "a program that outlives its deadline is bounded, got {outcome:?}"
    );
    assert!(
        started.elapsed() < std::time::Duration::from_secs(10),
        "the deadline was enforced late, after {:?}",
        started.elapsed()
    );
}

#[test]
fn a_child_that_writes_nothing_is_reaped_and_reports_its_code() {
    let mut process = adapter();
    let completion = process
        .run(&shell("exit 3"), &deadline())
        .expect("an ordinary non-zero exit is a completion, not an error");
    assert_eq!(completion.exit_code, Some(3));
    assert!(completion.stdout.is_empty());
    assert!(completion.stderr.is_empty());
    assert!(completion.started && completion.completed);
}

#[test]
fn many_flooding_children_in_sequence_leave_nothing_behind() {
    // A leak shows up as an accumulation. Twenty children, each printing past
    // the pipe buffer, must all be reaped: a zombie would still be a child of
    // this process, and `wait` would find it.
    let mut process = adapter();
    for round in 0..20 {
        let completion = process
            .run(&shell(&flood("1", 256 * 1024)), &deadline())
            .unwrap_or_else(|e| panic!("round {round} failed: {e}"));
        assert_eq!(completion.exit_code, Some(0));
        assert_eq!(completion.stdout.len(), 256 * 1024);
    }
    assert_eq!(
        zombie_children(),
        0,
        "every child must be reaped on the path that collected its output"
    );
}

/// How many of this process's children are zombies right now.
///
/// Read from `/proc`, because the question is about the operating system's
/// process table rather than about anything this crate records. A child that
/// exited and was never waited for stays there as a `Z` whose parent is us.
fn zombie_children() -> usize {
    let me = std::process::id();
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return 0;
    };
    let mut zombies = 0;
    for entry in entries.flatten() {
        let Ok(stat) = std::fs::read_to_string(entry.path().join("stat")) else {
            continue;
        };
        // `pid (comm) state ppid ...`. The command name may hold spaces and
        // parentheses, so the fields are read after the last closing one.
        let Some((_, after)) = stat.rsplit_once(')') else {
            continue;
        };
        let mut fields = after.split_whitespace();
        let state = fields.next().unwrap_or_default();
        let parent: u32 = fields.next().and_then(|p| p.parse().ok()).unwrap_or(0);
        if state == "Z" && parent == me {
            zombies += 1;
        }
    }
    zombies
}

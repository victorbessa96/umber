//! The JSONL agent wire's panic-hunt (C2-20): `umber-cli agent` reads
//! one JSON step per line from stdin, the one surface an external program
//! drives directly. Garbage lines (noise, truncated JSON, wrong shapes,
//! hostile values) must each answer with an `"ok": false` envelope on its
//! own line, and the process must stay alive for the next step. A panic
//! would kill the agent and every step after it.

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

fn garbage(seed: u64, len: usize) -> Vec<u8> {
    let mut x = seed | 1;
    (0..len)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            (x & 0xff) as u8
        })
        .collect()
}

/// The hostile-but-JSON class: real step names with wrong-typed,
/// out-of-range or absurd arguments. None names a real mesh file, so no
/// step reaches the GPU.
fn shaped_cases() -> Vec<String> {
    let mut out: Vec<String> = [
        r#"{}"#,
        r#"[]"#,
        r#"null"#,
        r#"42"#,
        r#""load""#,
        r#"{"load":null}"#,
        r#"{"load":{"mesh":42}}"#,
        r#"{"load":{"mesh":"/definitely/not/here.obj"}}"#,
        r#"{"inspect":{"mesh":""}}"#,
        r#"{"bake":{"size":-1}}"#,
        r#"{"bake":{"size":18446744073709551615}}"#,
        r#"{"bake":{"size":1e308}}"#,
        r#"{"export":{"preset":"\u0000"}}"#,
        r#"{"paint":{"uv":[NaN,1]}}"#,
        r#"{"paint":{"uv":[1e39,-1e39],"radius":-5}}"#,
        r#"{"load":{"mesh":"x"},"bake":{}}"#,
        r#"{"no_such_step":{}}"#,
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    // Deep nesting: a stack-depth probe (serde_json's recursion limit
    // must answer, not overflow).
    out.push(format!("{}{}", "[".repeat(10_000), "]".repeat(10_000)));
    out.push(format!(
        r#"{{"load":{{"mesh":"{}"}}}}"#,
        "a".repeat(100_000)
    ));
    out
}

#[test]
fn the_agent_answers_every_garbage_line_and_stays_alive() {
    let mut lines: Vec<String> = shaped_cases();
    for seed in 1..=150u64 {
        // Lossy UTF-8 with newlines removed: one garbage step per line.
        let text = String::from_utf8_lossy(&garbage(seed, (seed % 400) as usize + 1))
            .replace(['\n', '\r'], " ");
        lines.push(text);
    }
    // Truncations of a real step: every prefix is a malformed line.
    let step = r#"{"inspect":{"mesh":"/no/such/mesh.obj"}}"#;
    for cut in 1..step.len() {
        lines.push(step[..cut].to_string());
    }

    let mut child = Command::new(env!("CARGO_BIN_EXE_umber-cli"))
        .arg("agent")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn umber-cli agent");
    let mut stdin = child.stdin.take().expect("stdin");
    let stdout = child.stdout.take().expect("stdout");
    let sent = lines.clone();
    // Write on a thread so a full stdout pipe can never deadlock us.
    let writer = std::thread::spawn(move || {
        for line in &sent {
            if line.trim().is_empty() {
                continue;
            }
            writeln!(stdin, "{line}").expect("agent alive to read");
        }
        // stdin drops here: EOF ends the session.
    });
    let replies: Vec<String> = BufReader::new(stdout)
        .lines()
        .map_while(Result::ok)
        .collect();
    writer.join().expect("writer finished");
    let status = child.wait().expect("agent exits");

    let expected = lines.iter().filter(|l| !l.trim().is_empty()).count();
    assert!(status.success(), "the agent survived to EOF: {status:?}");
    assert_eq!(
        replies.len(),
        expected,
        "one reply per step, none lost to a crash"
    );
    for (i, reply) in replies.iter().enumerate() {
        let value: serde_json::Value = serde_json::from_str(reply)
            .unwrap_or_else(|e| panic!("reply {i} is not JSON ({e}): {reply}"));
        assert_eq!(
            value["ok"], false,
            "garbage step {i} must fail typed: {reply}"
        );
        assert!(
            value["error"].is_string(),
            "reply {i} names its error: {reply}"
        );
        assert_eq!(value["index"], i, "replies stay in order");
    }
}

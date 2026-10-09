use serde_json::{Value, json};
use std::{
    fs,
    io::{Read, Write},
    net::TcpListener,
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::Duration,
};

static NEXT_FILE_ID: AtomicU64 = AtomicU64::new(0);

fn run_notifier(event: Value, response: &str) -> (Output, String, Value) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let webhook = format!("http://{}/discord", listener.local_addr().unwrap());
    let response = response.to_owned();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let request = read_request(&mut stream);
        write!(
            stream,
            "HTTP/1.1 {response}\r\nContent-Length: 2\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{{}}"
        )
        .unwrap();
        request
    });

    let event_path = std::env::temp_dir().join(format!(
        "hook-me-up-{}-{}.json",
        std::process::id(),
        NEXT_FILE_ID.fetch_add(1, Ordering::Relaxed)
    ));
    fs::write(&event_path, serde_json::to_vec(&event).unwrap()).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_hook-me-up"))
        .env("GITHUB_EVENT_PATH", &event_path)
        .env("DISCORD_WEBHOOK_URL", webhook)
        .output()
        .unwrap();
    fs::remove_file(event_path).unwrap();

    let request = server.join().unwrap();
    let (headers, body) = request.split_once("\r\n\r\n").unwrap();
    (output, headers.into(), serde_json::from_str(body).unwrap())
}

fn read_request(stream: &mut impl Read) -> String {
    let mut request = Vec::new();
    let mut content_length: Option<usize> = None;

    loop {
        let mut buffer = [0; 1024];
        let read = stream.read(&mut buffer).unwrap();
        assert!(read > 0, "connection closed before the request completed");
        request.extend_from_slice(&buffer[..read]);

        if let Some(header_end) = request.windows(4).position(|part| part == b"\r\n\r\n") {
            let header_end = header_end + 4;
            let length = *content_length.get_or_insert_with(|| {
                String::from_utf8_lossy(&request[..header_end])
                    .lines()
                    .find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        name.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse().unwrap())
                    })
                    .expect("request must have a content length")
            });

            if request.len() >= header_end + length {
                return String::from_utf8(request).unwrap();
            }
        }
    }
}

#[test]
fn posts_opened_issue_from_event_file() {
    let (output, headers, body) = run_notifier(
        json!({
            "action": "opened",
            "issue": {
                "number": 7,
                "title": "Broken login",
                "html_url": "https://github.com/example/repo/issues/7",
                "body": "## Problem\nUsers cannot log in.",
                "user": { "login": "reporter" }
            }
        }),
        "200 OK",
    );

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(headers.starts_with("POST /discord?wait=true HTTP/1.1"));
    assert_eq!(body["embeds"][0]["title"], "ISSUE OPENED · #7 Broken login");
    assert_eq!(body["embeds"][0]["description"], "Users cannot log in.");
    assert_eq!(body["allowed_mentions"], json!({ "parse": [] }));
}

#[test]
fn posts_merged_pull_request_from_event_file() {
    let (output, _, body) = run_notifier(
        json!({
            "action": "closed",
            "pull_request": {
                "number": 42,
                "title": "Ship issue notifications",
                "html_url": "https://github.com/example/repo/pull/42",
                "user": { "login": "author" },
                "merged": true
            }
        }),
        "200 OK",
    );

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        body["embeds"][0]["title"],
        "MERGED · #42 Ship issue notifications"
    );
    assert_eq!(body["embeds"][0]["footer"]["text"], "PR by author");
}

#[test]
fn fails_when_discord_rejects_the_message() {
    let (output, _, _) = run_notifier(
        json!({
            "action": "reopened",
            "issue": {
                "number": 7,
                "title": "Broken login",
                "html_url": "https://github.com/example/repo/issues/7",
                "user": { "login": "reporter" }
            }
        }),
        "500 Internal Server Error",
    );

    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("Discord returned HTTP 500"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

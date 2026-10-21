//! The two multipart limits `server.maxRequestBodySize` does not cover.
//!
//! The streamed parser capped the whole body and nothing else, so a body well
//! under the size limit could still carry an unbounded number of parts — each
//! one a form key or an upload temp file — and a `filename=` of any length. A
//! body with more parts than `server.maxMultipartParts` is now refused with
//! 413, like the size cap, and a part whose filename is over 255 characters
//! with 400. A body inside both limits must still arrive whole.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Child, Command};
use std::time::Duration;

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/multipart_limits")
}

struct Server {
    child: Child,
    port: u16,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Held from picking a port until the server is accepting on it: free_port()
/// binds :0 and releases it, so two tests starting at once can be handed the
/// same port (see request_body_limit.rs).
static START: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn start_server() -> Server {
    let _serialised = START.lock().unwrap_or_else(|e| e.into_inner());
    let port = free_port();
    let child = Command::new(env!("CARGO_BIN_EXE_rustcfml"))
        .arg("--serve")
        .arg(fixtures_dir())
        .arg("--port")
        .arg(port.to_string())
        .arg("--cfconfig")
        .arg(fixtures_dir().join(".cfconfig.json"))
        .spawn()
        .expect("spawn rustcfml --serve");
    let mut server = Server { child, port };
    for _ in 0..600 {
        // A socket whose local and peer addresses match connected to ITSELF (Linux
        // TCP simultaneous open on an ephemeral-range port), not to a server.
        if TcpStream::connect(("127.0.0.1", port))
            .is_ok_and(|s| s.local_addr().ok() != s.peer_addr().ok())
        {
            return server;
        }
        if let Ok(Some(status)) = server.child.try_wait() {
            panic!("rustcfml --serve exited before accepting connections: {status}");
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("rustcfml --serve not accepting connections on port {port} after 30s");
}

/// POST a multipart body of `text_parts` plain fields followed by one file part
/// named `file_name`. Returns the full raw response.
fn post_parts(port: u16, text_parts: usize, file_name: &str) -> String {
    let boundary = "----------------------------rustcfmltestboundary";
    let mut body: Vec<u8> = Vec::new();
    for i in 0..text_parts {
        body.extend_from_slice(
            format!("--{boundary}\r\nContent-Disposition: form-data; name=\"f{i}\"\r\n\r\nv{i}\r\n")
                .as_bytes(),
        );
    }
    body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"upload\"; \
             filename=\"{file_name}\"\r\nContent-Type: application/octet-stream\r\n\r\nAAAA\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());

    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(120)))
        .unwrap();
    let head = format!(
        "POST /up.cfm HTTP/1.0\r\nHost: 127.0.0.1:{port}\r\n\
         Content-Type: multipart/form-data; boundary={boundary}\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(head.as_bytes()).unwrap();
    stream.write_all(&body).unwrap();
    stream.flush().unwrap();
    let mut out = Vec::new();
    stream.read_to_end(&mut out).expect("read response");
    String::from_utf8_lossy(&out).to_string()
}

fn status_line(resp: &str) -> &str {
    resp.lines().next().unwrap_or("")
}

#[test]
fn part_count_and_filename_length_are_capped() {
    let server = start_server();

    // Inside both limits (4 text parts + 1 file = 5, the configured cap).
    let ok = post_parts(server.port, 4, "small.bin");
    assert!(
        status_line(&ok).contains("200"),
        "an in-limit body must be accepted: {ok}"
    );
    assert!(
        ok.contains("formKeyCount=5"),
        "every part must reach the form scope: {ok}"
    );

    // One part over the cap: 413, like the body-size cap.
    let too_many = post_parts(server.port, 5, "small.bin");
    assert!(
        status_line(&too_many).contains("413"),
        "a body over server.maxMultipartParts must be refused with 413: {too_many}"
    );
    assert!(
        too_many.contains("maxMultipartParts"),
        "the 413 body must name the setting: {too_many}"
    );

    // A filename a filesystem could not take: 400.
    let long_name = format!("{}.bin", "n".repeat(300));
    let long = post_parts(server.port, 0, &long_name);
    assert!(
        status_line(&long).contains("400"),
        "a 300-character filename must be refused with 400: {long}"
    );

    // Exactly 255 characters is still a legal name.
    let edge_name = format!("{}.bin", "e".repeat(251));
    let edge = post_parts(server.port, 0, &edge_name);
    assert!(
        status_line(&edge).contains("200") && edge.contains("formKeyCount=1"),
        "a 255-character filename is within the limit: {edge}"
    );
}

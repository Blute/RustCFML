//! A PostgreSQL datasource keeps working after the idle-pool sweep has dropped
//! its pool (`RUSTCFML_DB_POOL_IDLE_TTL`, docs/memory.md).
//!
//! The sweep's contract is that an idle pool is closed and "the NEXT use of the
//! datasource simply reconnects". On a server whose PostgreSQL pool has been
//! idle past the TTL, the first sweep instead panics in the driver
//! ("Cannot start a runtime from within a runtime", postgres
//! `connection.rs`), and from then on every query on any pooled datasource
//! fails (`PoisonError` in `get_postgres_pool`) or never answers, until the
//! process restarts. With the default TTL that is any server left quiet for
//! ten minutes.
//!
//! A Rust integration test rather than a CFML suite because the sweep only
//! runs in `--serve` (it is the server's 60 s housekeeping tick), and the TTL
//! is read from the process environment. `RUSTCFML_DB_POOL_IDLE_TTL=1` makes
//! the first tick drop the pool, so the test takes a little over a minute.
//!
//! Live test, gated on RUSTCFML_TEST_PG_URL like the CFML PostgreSQL suites:
//!   docker run --rm -d -p 5432:5432 -e POSTGRES_PASSWORD=postgres postgres:16
//!   RUSTCFML_TEST_PG_URL=postgresql://postgres:postgres@127.0.0.1:5432/postgres \
//!     cargo test -p rustcfml-cli --test datasource_pool_idle_sweep

use std::fs::File;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

fn webroot() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/pool_idle_sweep_app")
}

struct Server {
    child: Child,
    port: u16,
    stderr_path: PathBuf,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_file(&self.stderr_path);
    }
}

impl Server {
    fn stderr(&self) -> String {
        std::fs::read_to_string(&self.stderr_path).unwrap_or_default()
    }
}

fn start_server(idle_ttl_secs: &str) -> Server {
    let port = free_port();
    let stderr_path = std::env::temp_dir().join(format!("rustcfml-pool-idle-sweep-{port}.log"));
    let stderr = File::create(&stderr_path).expect("create stderr log");
    let child = Command::new(env!("CARGO_BIN_EXE_rustcfml"))
        .arg("--serve")
        .arg(webroot())
        .arg("--port")
        .arg(port.to_string())
        .env("RUSTCFML_DB_POOL_IDLE_TTL", idle_ttl_secs)
        .stderr(Stdio::from(stderr))
        .spawn()
        .expect("spawn rustcfml --serve");
    for _ in 0..200 {
        // A socket whose local and peer addresses match connected to ITSELF (Linux
        // TCP simultaneous open on an ephemeral-range port), not to a server.
        if TcpStream::connect(("127.0.0.1", port))
            .is_ok_and(|s| s.local_addr().ok() != s.peer_addr().ok())
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Server { child, port, stderr_path }
}

/// GET with a bounded read: a server that never answers is a failure here,
/// not a hung test run. Errors come back as text so the assertion can show them.
fn http_get(port: u16, path: &str, timeout: Duration) -> String {
    let mut stream = match TcpStream::connect(("127.0.0.1", port)) {
        Ok(s) => s,
        Err(e) => return format!("<connect failed: {e}>"),
    };
    stream.set_read_timeout(Some(timeout)).unwrap();
    write!(
        stream,
        "GET {path} HTTP/1.0\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n"
    )
    .unwrap();
    let mut out = String::new();
    match stream.read_to_string(&mut out) {
        Ok(_) => out,
        Err(e) => format!("{out}<read failed after {timeout:?}: {e}>"),
    }
}

#[test]
fn postgres_datasource_survives_the_idle_pool_sweep() {
    let Ok(_) = std::env::var("RUSTCFML_TEST_PG_URL") else {
        eprintln!("skipped - set RUSTCFML_TEST_PG_URL to enable");
        return;
    };

    let server = start_server("1");
    let started = Instant::now();

    // --- the datasource works and its pool is registered -------------------
    let first = http_get(server.port, "/query.cfm", Duration::from_secs(30));
    assert!(
        first.contains("QUERY-OK 1"),
        "the first query must succeed (is RUSTCFML_TEST_PG_URL reachable?); got:\n{first}"
    );

    // --- let the first housekeeping tick (60 s after start) drop the pool --
    let past_first_tick = Duration::from_secs(70);
    if let Some(rest) = past_first_tick.checked_sub(started.elapsed()) {
        std::thread::sleep(rest);
    }

    // --- the next use reconnects --------------------------------------------
    let after = http_get(server.port, "/query.cfm", Duration::from_secs(15));
    let log = server.stderr();
    assert!(
        after.contains("QUERY-OK 1"),
        "a query after the idle pool was swept must reconnect and succeed; got:\n{after}\n\
         --- server stderr ---\n{log}"
    );
    assert!(
        !log.contains("panicked"),
        "dropping an idle PostgreSQL pool must not panic; server stderr:\n{log}"
    );

    // --- and the server keeps serving the datasource ------------------------
    let again = http_get(server.port, "/query.cfm", Duration::from_secs(15));
    assert!(
        again.contains("QUERY-OK 1"),
        "the datasource must keep working after the sweep; got:\n{again}"
    );
}

//! End-to-end integration tests for the HTTP backend.
//!
//! These tests start a minimal TCP test server and exercise the full pipeline:
//! programmatic `TestPlan` → HttpBackend → send request → evaluate assertions → report.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use tast::plan::types::{InputEntry, PlanStep, StepEntry};
use tast::runner::backend::TestBackend;
use tast::runner::backends::http::{HttpBackend, HttpConfig};
use tast::runner::context::RunContext;
use tast::runner::result::{StepErrorKind, StepStatus};

// ── Test Server ─────────────────────────────────────────────

struct TestServer {
    addr: String,
    shutdown_tx: mpsc::Sender<()>,
    handle: Option<thread::JoinHandle<()>>,
}

impl TestServer {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind to ephemeral port");
        let addr = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
        let (shutdown_tx, shutdown_rx) = mpsc::channel::<()>();

        listener
            .set_nonblocking(true)
            .expect("set listener nonblocking");

        let handle = thread::spawn(move || {
            loop {
                // Check for shutdown signal.
                if shutdown_rx.try_recv().is_ok() {
                    break;
                }

                match listener.accept() {
                    Ok((mut stream, _)) => {
                        // Read the request.
                        let mut reader = BufReader::new(stream.try_clone().unwrap());
                        let mut request_line = String::new();
                        let _ = reader.read_line(&mut request_line);

                        // Read headers to find Content-Length.
                        let mut content_length = 0usize;
                        loop {
                            let mut header = String::new();
                            let _ = reader.read_line(&mut header);
                            if header.trim().is_empty() {
                                break;
                            }
                            if let Some(val) = header.strip_prefix("Content-Length: ") {
                                content_length = val.trim().parse().unwrap_or(0);
                            }
                        }

                        // Read body if present.
                        let mut _body = vec![0u8; content_length];
                        if content_length > 0 {
                            let _ = std::io::Read::read_exact(&mut reader, &mut _body);
                        }

                        let response = route(&request_line);
                        let _ = stream.write_all(response.as_bytes());
                        let _ = stream.flush();
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(_) => break,
                }
            }
        });

        Self {
            addr,
            shutdown_tx,
            handle: Some(handle),
        }
    }

    fn base_url(&self) -> &str {
        &self.addr
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        let _ = self.shutdown_tx.send(());
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn route(request_line: &str) -> String {
    let parts: Vec<&str> = request_line.split_whitespace().collect();
    let method = parts.first().copied().unwrap_or("");
    let path = parts.get(1).copied().unwrap_or("");

    match (method, path) {
        ("GET", "/api/users") => http_response(
            200,
            r#"{"users":[{"id":"1","name":"Alice"},{"id":"2","name":"Bob"}]}"#,
        ),
        ("POST", "/api/users") => http_response(201, r#"{"id":"new-1","name":"Created"}"#),
        ("GET", "/api/error") => http_response(500, r#"{"error":"internal"}"#),
        ("GET", "/api/delay") => {
            thread::sleep(Duration::from_secs(5));
            http_response(200, r#"{"delayed":true}"#)
        }
        ("GET", p) if p.starts_with("/api/users/") => {
            let id = p.strip_prefix("/api/users/").unwrap_or("unknown");
            http_response(200, &format!(r#"{{"id":"{id}","name":"Looked Up"}}"#))
        }
        _ => http_response(404, r#"{"error":"not found"}"#),
    }
}

fn http_response(status: u16, body: &str) -> String {
    let reason = match status {
        200 => "OK",
        201 => "Created",
        404 => "Not Found",
        500 => "Internal Server Error",
        _ => "Unknown",
    };
    format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

// ── Helpers ─────────────────────────────────────────────────

fn make_http_step(
    order: usize,
    node: &str,
    action_text: &str,
    assertions: Vec<StepEntry>,
) -> PlanStep {
    PlanStep {
        order,
        node: node.to_string(),
        description: Some(format!("HTTP step: {node}")),
        tags: vec![],
        depends_on: vec![],
        preconditions: vec![],
        actions: vec![StepEntry {
            step_type: "when".to_string(),
            text: action_text.to_string(),
            data: vec![],
            parameters: vec![],
        }],
        assertions,
        inputs: vec![],
        outputs: vec![],
        config: HashMap::new(),
    }
}

fn assertion(text: &str) -> StepEntry {
    StepEntry {
        step_type: "then".to_string(),
        text: text.to_string(),
        data: vec![],
        parameters: vec![],
    }
}

fn http_backend(base_url: &str) -> HttpBackend {
    HttpBackend::new(HttpConfig {
        base_url: base_url.to_string(),
        timeout: Duration::from_secs(5),
        ..Default::default()
    })
}

fn empty_harness() -> tast::runner::backend::GeneratedHarness {
    tast::runner::backend::GeneratedHarness {
        files: vec![],
        entry_point: std::path::PathBuf::new(),
        metadata: HashMap::new(),
    }
}

// ── E2: HTTP Backend E2E Tests ──────────────────────────────

#[test]
fn http_e2e_get_request_passes() {
    let server = TestServer::start();
    let backend = http_backend(server.base_url());
    let harness = empty_harness();
    let mut context = RunContext::new("/tmp");

    let step = make_http_step(
        1,
        "GetUsers",
        "GET /api/users",
        vec![assertion("the API returns 200")],
    );

    let result = backend.execute_step(&step, &harness, &mut context).unwrap();
    assert_eq!(result.status, StepStatus::Passed);
    assert!(result.assertions[0].passed);
}

#[test]
fn http_e2e_post_request_with_body() {
    let server = TestServer::start();
    let backend = http_backend(server.base_url());
    let harness = empty_harness();
    let mut context = RunContext::new("/tmp");

    let mut step = make_http_step(
        1,
        "CreateUser",
        "POST /api/users",
        vec![assertion("the API returns 201")],
    );
    step.actions[0].data = vec![("name".to_string(), "NewUser".to_string())];

    let result = backend.execute_step(&step, &harness, &mut context).unwrap();
    assert_eq!(result.status, StepStatus::Passed);
    assert!(result.assertions[0].passed);
}

#[test]
fn http_e2e_status_assertion_fails() {
    let server = TestServer::start();
    let backend = http_backend(server.base_url());
    let harness = empty_harness();
    let mut context = RunContext::new("/tmp");

    let step = make_http_step(
        1,
        "GetError",
        "GET /api/error",
        vec![assertion("the API returns 200")],
    );

    let result = backend.execute_step(&step, &harness, &mut context).unwrap();
    assert_eq!(result.status, StepStatus::Failed);
    assert!(!result.assertions[0].passed);
    assert_eq!(
        result.error.as_ref().unwrap().kind,
        StepErrorKind::AssertionFailed
    );
    // The message should mention the actual 500 status.
    assert!(
        result.error.as_ref().unwrap().message.contains("500"),
        "error should mention actual status, got: {}",
        result.error.as_ref().unwrap().message
    );
}

#[test]
fn http_e2e_response_data_extraction() {
    let server = TestServer::start();
    let backend = http_backend(server.base_url());
    let harness = empty_harness();
    let mut context = RunContext::new("/tmp");

    let mut step = make_http_step(
        1,
        "GetUsers",
        "GET /api/users",
        vec![assertion("the API returns 200")],
    );
    step.outputs = vec!["users".to_string()];

    let result = backend.execute_step(&step, &harness, &mut context).unwrap();
    assert_eq!(result.status, StepStatus::Passed);
    assert!(
        result.outputs.contains_key("users"),
        "outputs should contain 'users', got: {:?}",
        result.outputs
    );
}

#[test]
fn http_e2e_data_passing_between_steps() {
    let server = TestServer::start();
    let backend = http_backend(server.base_url());
    let harness = empty_harness();
    let mut context = RunContext::new("/tmp");

    // Step 1: POST to create a user, extract the "id" field.
    let mut step1 = make_http_step(
        1,
        "CreateUser",
        "POST /api/users",
        vec![assertion("the API returns 201")],
    );
    step1.outputs = vec!["id".to_string()];

    let result1 = backend
        .execute_step(&step1, &harness, &mut context)
        .unwrap();
    assert_eq!(result1.status, StepStatus::Passed);
    assert!(result1.outputs.contains_key("id"));

    // Step 2: GET /api/users/{id} — uses the id from step 1.
    let mut step2 = make_http_step(
        2,
        "GetUser",
        "GET /api/users/{id}",
        vec![assertion("the API returns 200")],
    );
    step2.inputs.push(InputEntry {
        field: "id".to_string(),
        from: "CreateUser".to_string(),
    });

    let result2 = backend
        .execute_step(&step2, &harness, &mut context)
        .unwrap();
    assert_eq!(result2.status, StepStatus::Passed);
    // Verify the URL was constructed with the resolved id.
    assert!(
        result2.stdout.contains("Looked Up"),
        "response should be from the user-specific endpoint"
    );
}

#[test]
fn http_e2e_timeout_handling() {
    let server = TestServer::start();
    let backend = HttpBackend::new(HttpConfig {
        base_url: server.base_url().to_string(),
        timeout: Duration::from_secs(1),
        ..Default::default()
    });
    let harness = empty_harness();
    let mut context = RunContext::new("/tmp");

    let step = make_http_step(
        1,
        "SlowRequest",
        "GET /api/delay",
        vec![assertion("the API returns 200")],
    );

    // The /api/delay endpoint sleeps 5s, but timeout is 1s → should error.
    let result = backend.execute_step(&step, &harness, &mut context);
    // This should be an error from ureq due to timeout.
    assert!(
        result.is_err(),
        "request to /api/delay with 1s timeout should fail"
    );
}

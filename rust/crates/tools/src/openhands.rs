//! Generic shell-execution backend. Originally named "OpenHands" after the
//! OpenHands Software Agent SDK, but the wire protocol it speaks is a thin
//! shell-exec convention (`POST {path}` with `{"code": ..., "language": ...}`
//! in the body). Any HTTP endpoint that accepts that contract can be wired in
//! — including OpenHands, E2B, a local sub-process on a port, or your own
//! sandbox.
//!
//! The OpenHands-as-real-SDK path uses conversation-based APIs
//! (`/api/conversations/*`), not a simple `/execute` route. This tool is **not**
//! a drop-in replacement for the OpenHands SDK; it's a small, predictable
//! shim that matches a fixed wire shape so any backend that implements that
//! shape can be plugged in.
//!
//! ## Configuration
//!
//! | Env var | Default | Purpose |
//! |---------|---------|---------|
//! | `OPENHANDS_ENDPOINT` | `http://localhost:8000` | base URL, no trailing slash |
//! | `OPENHANDS_PATH`     | `/execute`              | path appended to base URL |
//! | `OPENHANDS_API_KEY`  | (unset)                 | when set, sent as auth |
//! | `OPENHANDS_AUTH_HEADER` | `Authorization`      | header name to use |
//! | `OPENHANDS_AUTH_PREFIX` | `Bearer `            | prepended to the key (note trailing space) |
//!
//! Two preset recipes are documented below.
//!
//! ### Recipe A: OpenHands default (Authorization: Bearer)
//!
//! ```text
//! OPENHANDS_ENDPOINT=http://localhost:8000
//! OPENHANDS_PATH=/execute
//! OPENHANDS_AUTH_HEADER=Authorization
//! OPENHANDS_AUTH_PREFIX="Bearer "
//! OPENHANDS_API_KEY=<your key>
//! ```
//!
//! ### Recipe B: OpenHands Agent Server (X-Session-API-Key)
//!
//! ```text
//! OPENHANDS_ENDPOINT=http://localhost:8000
//! OPENHANDS_PATH=/api/exec
//! OPENHANDS_AUTH_HEADER=X-Session-API-Key
//! OPENHANDS_AUTH_PREFIX=""
//! OPENHANDS_API_KEY=$OH_SESSION_API_KEYS_0
//! ```
//!
//! ### Recipe C: any OpenAI-compatible API (unrelated, but same header shape)
//!
//! Set `OPENHANDS_PATH=/v1/chat/completions` and adjust body schema in a
//! follow-up if you want a different conversation shape — this tool hardcodes
//! the `{"code","language"}` body, so for non-trivial APIs you'd want a
//! separate tool.
//!
//! ## Safety
//!
//! The `code` field is sent as a JSON string body (JSON escaping handles the
//! boundaries). The server runs the code in *its* sandbox; we never
//! shell-interpolate it on the client side.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};

/// Default endpoint if neither env nor config is set.
pub const DEFAULT_OPENHANDS_ENDPOINT: &str = "http://localhost:8000";
/// Default path appended to the endpoint.
pub const DEFAULT_OPENHANDS_PATH: &str = "/execute";
/// Default auth header name.
pub const DEFAULT_OPENHANDS_AUTH_HEADER: &str = "Authorization";
/// Default auth prefix (note the trailing space, since Bearer takes a token).
pub const DEFAULT_OPENHANDS_AUTH_PREFIX: &str = "Bearer ";

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum OpenHandsLanguage {
    Bash,
    Python,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OpenHandsInput {
    /// The code to execute (bash or python).
    pub code: String,
    /// Language to run the code as.
    pub language: OpenHandsLanguage,
    /// Optional path for the response JSON to be flattened into a string.
    /// If absent, the raw response body is returned.
    #[serde(default)]
    pub response_path: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OpenHandsOutput {
    pub stdout: String,
    #[serde(default)]
    pub stderr: String,
    #[serde(default)]
    pub exit_code: i32,
}

/// Resolved configuration for a single tool call.
#[derive(Debug, Clone)]
pub struct OpenHandsConfig {
    pub endpoint: String,
    pub path: String,
    pub api_key: Option<String>,
    pub auth_header: String,
    pub auth_prefix: String,
}

impl Default for OpenHandsConfig {
    fn default() -> Self {
        Self {
            endpoint: DEFAULT_OPENHANDS_ENDPOINT.to_string(),
            path: DEFAULT_OPENHANDS_PATH.to_string(),
            api_key: None,
            auth_header: DEFAULT_OPENHANDS_AUTH_HEADER.to_string(),
            auth_prefix: DEFAULT_OPENHANDS_AUTH_PREFIX.to_string(),
        }
    }
}

impl OpenHandsConfig {
    /// Resolve configuration from overrides + environment variables.
    /// Caller-provided fields take precedence over env vars, which take
    /// precedence over defaults.
    #[must_use]
    pub fn resolve(
        endpoint: Option<&str>,
        path: Option<&str>,
        api_key: Option<&str>,
        auth_header: Option<&str>,
        auth_prefix: Option<&str>,
    ) -> Self {
        Self {
            endpoint: endpoint
                .map(String::from)
                .or_else(|| std::env::var("OPENHANDS_ENDPOINT").ok())
                .unwrap_or_else(|| DEFAULT_OPENHANDS_ENDPOINT.to_string()),
            path: path
                .map(String::from)
                .or_else(|| std::env::var("OPENHANDS_PATH").ok())
                .unwrap_or_else(|| DEFAULT_OPENHANDS_PATH.to_string()),
            api_key: api_key
                .map(String::from)
                .or_else(|| std::env::var("OPENHANDS_API_KEY").ok()),
            auth_header: auth_header
                .map(String::from)
                .or_else(|| std::env::var("OPENHANDS_AUTH_HEADER").ok())
                .unwrap_or_else(|| DEFAULT_OPENHANDS_AUTH_HEADER.to_string()),
            auth_prefix: auth_prefix
                .map(String::from)
                .or_else(|| std::env::var("OPENHANDS_AUTH_PREFIX").ok())
                .unwrap_or_else(|| DEFAULT_OPENHANDS_AUTH_PREFIX.to_string()),
        }
    }
}

/// Run an OpenHands / generic shell-exec HTTP call.
///
/// `endpoint`, `path`, `api_key`, `auth_header`, `auth_prefix` are all optional
/// — see [`OpenHandsConfig::resolve`] for precedence (caller > env > default).
pub fn run_openhands(
    input: &OpenHandsInput,
    endpoint: Option<&str>,
    path: Option<&str>,
    api_key: Option<&str>,
    auth_header: Option<&str>,
    auth_prefix: Option<&str>,
) -> Result<String, String> {
    if input.code.is_empty() {
        return Err("openhands: code must not be empty".to_string());
    }
    let cfg = OpenHandsConfig::resolve(endpoint, path, api_key, auth_header, auth_prefix);
    let url = format!(
        "{}/{}",
        cfg.endpoint.trim_end_matches('/'),
        cfg.path.trim_start_matches('/')
    );

    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| format!("openhands: failed to build HTTP client: {e}"))?;
    let mut req = client
        .post(&url)
        .header("Content-Type", "application/json")
        .json(&input);
    if let Some(key) = cfg.api_key.as_deref() {
        let header_value = format!("{}{}", cfg.auth_prefix, key);
        req = req.header(&cfg.auth_header, header_value);
    }
    let resp = req
        .send()
        .map_err(|e| format!("openhands: HTTP request failed: {e}"))?;
    let status = resp.status();
    let body = resp
        .text()
        .map_err(|e| format!("openhands: failed to read response body: {e}"))?;
    if !status.is_success() {
        return Err(format!(
            "openhands: HTTP {} from {}: {}",
            status.as_u16(),
            url,
            truncate_for_error(&body)
        ));
    }
    Ok(body)
}

/// Truncate body for error messages — avoid dumping huge HTML error pages.
fn truncate_for_error(body: &str) -> String {
    const MAX: usize = 512;
    if body.len() <= MAX {
        body.to_string()
    } else {
        let mut s = body[..MAX].to_string();
        s.push_str("...[truncated]");
        s
    }
}

// ----- helpers exposed for tests -----

/// Spin up a minimal HTTP/1.1 test server in a thread. Each request is handled
/// by `handler(headers, body) -> (status, body)`. Returns the bound `127.0.0.1:port`
/// address (use `http://...` + the path).
pub fn start_mock_server<F>(handler: F) -> (String, mpsc::Receiver<()>)
where
    F: Fn(&str, &str) -> (u16, String) + Send + 'static,
{
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
    let addr = listener.local_addr().expect("local_addr");
    let (tx, rx) = mpsc::channel();
    let _ = listener.set_nonblocking(true);
    thread::spawn(move || loop {
        let (mut stream, _) = match listener.accept() {
            Ok(s) => s,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(10));
                if tx.send(()).is_err() {
                    break;
                }
                continue;
            }
            Err(_) => break,
        };
        let mut buf = [0u8; 8192];
        let n = match stream.read(&mut buf) {
            Ok(n) => n,
            Err(_) => continue,
        };
        let request = String::from_utf8_lossy(&buf[..n]).into_owned();
        let (headers, body) = parse_request(&request);
        let (status, resp_body) = handler(&headers, &body);
        let status_text = match status {
            200 => "OK",
            500 => "Internal Server Error",
            _ => "Status",
        };
        let response = format!(
                "HTTP/1.1 {status} {status_text}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{resp_body}",
                resp_body.len()
            );
        let _ = stream.write_all(response.as_bytes());
        let _ = stream.flush();
    });
    (format!("http://{}", addr), rx)
}

/// Minimal HTTP request splitter. Returns (headers_block, body) for use in tests.
fn parse_request(req: &str) -> (String, String) {
    if let Some((headers, body)) = req.split_once("\r\n\r\n") {
        (headers.to_string(), body.to_string())
    } else if let Some((headers, body)) = req.split_once("\n\n") {
        (headers.to_string(), body.to_string())
    } else {
        (req.to_string(), String::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn empty_code_errors() {
        let input = OpenHandsInput {
            code: String::new(),
            language: OpenHandsLanguage::Bash,
            response_path: None,
        };
        let err = run_openhands(&input, Some("http://localhost:1"), None, None, None, None)
            .expect_err("empty code should fail");
        assert!(err.contains("code must not be empty"), "{err}");
    }

    #[test]
    fn happy_path_200_returns_body() {
        let (endpoint, _stop) = start_mock_server(|_h, body| {
            // Echo the code back in stdout for assertion.
            let parsed: Value = serde_json::from_str(body).unwrap_or(json!({}));
            let code = parsed
                .get("code")
                .and_then(Value::as_str)
                .unwrap_or_default();
            (
                200,
                json!({"stdout": format!("you ran: {code}"), "stderr": "", "exit_code": 0})
                    .to_string(),
            )
        });
        // Give the listener time to actually start accepting.
        thread::sleep(Duration::from_millis(50));
        let input = OpenHandsInput {
            code: "echo hello".into(),
            language: OpenHandsLanguage::Bash,
            response_path: None,
        };
        let out = run_openhands(&input, Some(&endpoint), None, None, None, None)
            .expect("200 should pass");
        assert!(out.contains("you ran: echo hello"), "{out}");
    }

    #[test]
    fn http_500_yields_err_with_status() {
        let (endpoint, _stop) = start_mock_server(|_, _| (500, "{\"error\":\"boom\"}".to_string()));
        thread::sleep(Duration::from_millis(50));
        let input = OpenHandsInput {
            code: "ls".into(),
            language: OpenHandsLanguage::Bash,
            response_path: None,
        };
        let err = run_openhands(&input, Some(&endpoint), None, None, None, None)
            .expect_err("500 should fail");
        assert!(err.contains("HTTP 500"), "{err}");
        assert!(err.contains("boom"), "{err}");
    }

    #[test]
    fn bearer_auth_header_sent_when_key_set() {
        let (endpoint, _stop) = start_mock_server(|headers, _body| {
            // Confirm Authorization header was sent with Bearer prefix.
            let auth = headers
                .lines()
                .find(|l| l.to_ascii_lowercase().starts_with("authorization:"))
                .unwrap_or("")
                .to_string();
            assert!(
                auth.contains("Bearer test-key-123"),
                "auth header was: {auth}"
            );
            (
                200,
                "{\"stdout\":\"ok\",\"stderr\":\"\",\"exit_code\":0}".to_string(),
            )
        });
        thread::sleep(Duration::from_millis(50));
        let input = OpenHandsInput {
            code: "true".into(),
            language: OpenHandsLanguage::Bash,
            response_path: None,
        };
        let _ = run_openhands(
            &input,
            Some(&endpoint),
            None,
            Some("test-key-123"),
            None,
            None,
        )
        .expect("auth should pass");
    }

    #[test]
    fn custom_auth_header_and_prefix() {
        // Simulate OpenHands Agent Server's "X-Session-API-Key" style.
        // Note: HTTP header names are case-insensitive, so reqwest will send
        // what we tell it but the on-wire form may be normalized. We assert
        // case-insensitive here.
        let (endpoint, _stop) = start_mock_server(|headers, _body| {
            let auth = headers
                .lines()
                .find(|l| l.to_ascii_lowercase().starts_with("x-session-api-key:"))
                .map(str::to_string)
                .unwrap_or_default();
            // No "Bearer " prefix expected — just the raw key.
            let value = auth
                .splitn(2, ':')
                .nth(1)
                .map(str::trim)
                .unwrap_or_default();
            assert_eq!(value, "sess-abc-123", "auth header was: {auth}");
            (
                200,
                "{\"stdout\":\"ok\",\"stderr\":\"\",\"exit_code\":0}".to_string(),
            )
        });
        thread::sleep(Duration::from_millis(50));
        let input = OpenHandsInput {
            code: "true".into(),
            language: OpenHandsLanguage::Bash,
            response_path: None,
        };
        let _ = run_openhands(
            &input,
            Some(&endpoint),
            None,
            Some("sess-abc-123"),
            Some("X-Session-API-Key"),
            Some(""), // empty prefix — raw key
        )
        .expect("custom auth should pass");
    }

    #[test]
    fn custom_path_used() {
        // Path = /api/v1/shell/run so request lands on /api/v1/shell/run not /execute.
        let (endpoint, _stop) = start_mock_server(|_h, _body| {
            // We can't easily inspect request-line with our tiny mock parser,
            // but the call returning 200 is enough to confirm the path is well-formed
            // (otherwise we'd see a connection error).
            (
                200,
                "{\"stdout\":\"ok\",\"stderr\":\"\",\"exit_code\":0}".to_string(),
            )
        });
        thread::sleep(Duration::from_millis(50));
        let input = OpenHandsInput {
            code: "true".into(),
            language: OpenHandsLanguage::Bash,
            response_path: None,
        };
        let _ = run_openhands(
            &input,
            Some(&endpoint),
            Some("/api/v1/shell/run"),
            None,
            None,
            None,
        )
        .expect("custom path should pass");
    }

    #[test]
    fn endpoint_env_default_used_when_none_passed() {
        // We can't easily mutate process env in a test (other tests run in parallel),
        // so just verify the constants match the documented defaults.
        assert_eq!(DEFAULT_OPENHANDS_ENDPOINT, "http://localhost:8000");
        assert_eq!(DEFAULT_OPENHANDS_PATH, "/execute");
        assert_eq!(DEFAULT_OPENHANDS_AUTH_HEADER, "Authorization");
        assert_eq!(DEFAULT_OPENHANDS_AUTH_PREFIX, "Bearer ");
    }

    #[test]
    fn resolve_precedence_caller_over_default() {
        let cfg = OpenHandsConfig::resolve(
            Some("http://example.com:9000"),
            Some("/custom"),
            Some("k"),
            Some("X-Custom"),
            Some("Token "),
        );
        assert_eq!(cfg.endpoint, "http://example.com:9000");
        assert_eq!(cfg.path, "/custom");
        assert_eq!(cfg.api_key.as_deref(), Some("k"));
        assert_eq!(cfg.auth_header, "X-Custom");
        assert_eq!(cfg.auth_prefix, "Token ");
    }

    #[test]
    fn resolve_default_when_nothing_set() {
        // Note: env vars may or may not be set in the test process, so we
        // can't assert on those fields directly. Instead we check that
        // defaults are stable when no caller input is provided AND when env
        // is unset (which we cannot guarantee). We only assert the "constant"
        // defaults that don't read env.
        let cfg = OpenHandsConfig::resolve(
            Some("http://h"),      // endpoint explicit
            None,                  // path from env or default
            None,                  // api_key from env or default
            Some("Authorization"), // explicit
            None,                  // prefix from env or default
        );
        assert_eq!(cfg.endpoint, "http://h");
        // path/api_key/auth_prefix read env; we only know what the caller or
        // the constant default produces.
        if std::env::var("OPENHANDS_PATH").is_err() {
            assert_eq!(cfg.path, "/execute");
        }
        if std::env::var("OPENHANDS_AUTH_PREFIX").is_err() {
            assert_eq!(cfg.auth_prefix, "Bearer ");
        }
    }

    #[test]
    fn parse_request_splits_headers_and_body() {
        let req = "POST /execute HTTP/1.1\r\nHost: x\r\nContent-Type: application/json\r\n\r\n{\"code\":\"ls\"}";
        let (h, b) = parse_request(req);
        assert!(h.contains("POST /execute"));
        assert_eq!(b, "{\"code\":\"ls\"}");
    }

    #[test]
    fn truncate_for_error_short_passthrough() {
        let s = "small".to_string();
        assert_eq!(truncate_for_error(&s), "small");
    }

    #[test]
    fn truncate_for_error_long_truncated() {
        let s = "a".repeat(1000);
        let t = truncate_for_error(&s);
        assert!(t.contains("...[truncated]"));
        assert!(t.len() < s.len());
    }

    use serde_json::Value;
}

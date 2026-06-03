//! mmx-cli subprocess wrapper.
//!
//! mmx-cli is MiniMax's official CLI for chat, web search, vision, and
//! media generation. This module wraps the subset we use as Tauri
//! commands so the webview can call them through `invoke()`:
//!
//!   * `mmx_detect`            — find `mmx` on PATH, report version
//!   * `mmx_search`            — web search via `mmx search query`
//!   * `mmx_vision_describe`   — image understanding via `mmx vision describe`
//!   * `mmx_text_chat`         — text chat via `mmx text chat`
//!
//! Why `tokio::process::Command` (not `tauri-plugin-shell`): mmx-cli is
//! a *user-installed* CLI on PATH, not a sidecar binary. The shell
//! plugin's scope model is built for fixed absolute paths or sidecars;
//// scoping a user-installed PATH binary cleanly is awkward. A
//! hardcoded Rust command that always and only spawns `mmx` gives the
//! same security property (the webview can't pivot through it to run
//! arbitrary commands) without pulling in another plugin or editing
//! `capabilities/default.json`.
//!
//! All commands are stateless one-shot: each `invoke()` spawns mmx,
//! waits for it to finish, captures stdout, and returns. mmx-cli's
//! commands complete in seconds and don't need a long-lived stream
//! like `claude -p` or `codex exec --json`. If we ever need streaming
//! (e.g. `mmx text chat --stream` for live token output) we can add a
//! spawn/kill/state pair modeled on `claude_cli.rs`.
//!
//! Logging: every command emits a single line on stderr with the
//! command name, the args hash (not the args themselves — search
//! queries and prompts can carry user PII), elapsed ms, and exit
//! status. This satisfies the structured-traceability rule for any
//! command that crosses the process boundary.

use std::path::PathBuf;
use std::process::Stdio;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::time::timeout;

const DEFAULT_TIMEOUT_SECS: u64 = 60;
const DETECT_TIMEOUT_SECS: u64 = 5;

fn find_mmx_command() -> Result<PathBuf, String> {
    which::which("mmx").map_err(|_| "`mmx` not found on PATH (run `npm i -g mmx-cli`)".to_string())
}

/// Result of probing for `mmx` on PATH. Returned (not just thrown) so
/// the frontend can show a "mmx not installed" pill with actionable
/// remediation rather than a raw error string.
#[derive(Serialize)]
pub struct DetectResult {
    pub installed: bool,
    pub version: Option<String>,
    pub path: Option<String>,
    pub error: Option<String>,
}

/// `mmx search query` returns a JSON object of the form:
///   { "organic": [ { "title": ..., "link": ..., "snippet": ..., "date": ... }, ... ], ... }
/// We only surface the fields the rest of the app needs. mmx's schema
/// is the only one we promise to keep working — when (if) they add
/// `images` / `videos` blocks, widen this struct and the deserializer
/// stays forward-compatible because every field is optional.
#[derive(Deserialize, Debug)]
struct MmxSearchEnvelope {
    organic: Option<Vec<MmxSearchHitRaw>>,
}

#[derive(Deserialize, Debug)]
struct MmxSearchHitRaw {
    title: Option<String>,
    link: Option<String>,
    snippet: Option<String>,
    date: Option<String>,
}

/// Normalized search result. Matches the shape `WebSearchResult` (TS)
/// expects, so the webview can drop these straight into the existing
/// search pipeline without re-mapping.
#[derive(Serialize, Debug)]
pub struct SearchHit {
    pub title: String,
    pub url: String,
    pub snippet: String,
    pub source: String,
    pub date: Option<String>,
}

/// One role/content pair sent to `mmx text chat --message`. The
/// `mmx-cli` skill documents `--message` as a repeatable flag with
/// `role:` prefix; we mirror that on the wire and just translate from
/// this struct.
#[derive(Deserialize, Debug)]
pub struct ChatMessage {
    /// "system" | "user" | "assistant"
    pub role: String,
    pub content: String,
}

/// Locate `mmx` on PATH and confirm it's runnable by calling
/// `mmx --version` with a short timeout. Cheap; safe to call when
/// the settings panel mounts.
#[tauri::command]
pub async fn mmx_detect() -> Result<DetectResult, String> {
    let path = match find_mmx_command() {
        Ok(p) => p,
        Err(error) => {
            return Ok(DetectResult {
                installed: false,
                version: None,
                path: None,
                error: Some(error),
            })
        }
    };

    let started = Instant::now();
    let output = match timeout(
        Duration::from_secs(DETECT_TIMEOUT_SECS),
        Command::new(&path).arg("--version").output(),
    )
    .await
    {
        Ok(Ok(out)) => out,
        Ok(Err(e)) => {
            eprintln!(
                "[mmx_detect] spawn failed path={} err={}",
                path.display(),
                e
            );
            return Ok(DetectResult {
                installed: false,
                version: None,
                path: Some(path.display().to_string()),
                error: Some(format!("spawn failed: {e}")),
            });
        }
        Err(_) => {
            eprintln!("[mmx_detect] timeout path={}", path.display());
            return Ok(DetectResult {
                installed: false,
                version: None,
                path: Some(path.display().to_string()),
                error: Some(format!(
                    "--version timed out after {}s",
                    DETECT_TIMEOUT_SECS
                )),
            });
        }
    };

    let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let status = output.status;
    eprintln!(
        "[mmx_detect] ok path={} version={} status={} elapsed_ms={}",
        path.display(),
        version,
        status,
        started.elapsed().as_millis()
    );

    Ok(DetectResult {
        installed: status.success(),
        version: Some(version),
        path: Some(path.display().to_string()),
        error: if status.success() {
            None
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            Some(if stderr.is_empty() {
                format!("exit {status}")
            } else {
                stderr
            })
        },
    })
}

/// Run a web search through `mmx search query`. Returns a flat list of
/// `SearchHit` ready to feed into the existing deep-research pipeline.
#[tauri::command]
pub async fn mmx_search(
    query: String,
    max_results: Option<u32>,
    timeout_secs: Option<u64>,
) -> Result<Vec<SearchHit>, String> {
    let path = find_mmx_command()?;
    let mut cmd = Command::new(&path);
    cmd.arg("search")
        .arg("query")
        .arg("--q")
        .arg(&query)
        .arg("--output")
        .arg("json")
        .arg("--quiet");
    if let Some(n) = max_results {
        cmd.arg("--n").arg(n.to_string());
    }

    let started = Instant::now();
    let query_hash = hash_short(&query);
    let timeout_dur = Duration::from_secs(timeout_secs.unwrap_or(DEFAULT_TIMEOUT_SECS));
    let output = run_capture(&mut cmd, timeout_dur).await.map_err(|e| {
        eprintln!("[mmx_search] failed query_hash={query_hash} err={e}");
        e
    })?;

    let parsed: MmxSearchEnvelope = serde_json::from_slice(&output.stdout).map_err(|e| {
        let snippet = String::from_utf8_lossy(&output.stdout)
            .chars()
            .take(200)
            .collect::<String>();
        format!("mmx search returned non-JSON: {e}; first 200 bytes: {snippet}")
    })?;

    let hits: Vec<SearchHit> = parsed
        .organic
        .unwrap_or_default()
        .into_iter()
        .filter_map(|h| {
            let url = h.link?;
            Some(SearchHit {
                title: h.title.unwrap_or_default(),
                url: url.clone(),
                snippet: h.snippet.unwrap_or_default(),
                source: extract_source(&url),
                date: h.date,
            })
        })
        .collect();

    eprintln!(
        "[mmx_search] ok query_hash={query_hash} hits={} elapsed_ms={}",
        hits.len(),
        started.elapsed().as_millis()
    );

    Ok(hits)
}

/// Describe an image using `mmx vision describe`. `image_path` may be
/// a local filesystem path or an http(s) URL — mmx-cli base64-encodes
/// paths and fetches URLs server-side. Returns the model's reply as a
/// plain string; the frontend is responsible for any rendering.
#[tauri::command]
pub async fn mmx_vision_describe(
    image_path: String,
    prompt: Option<String>,
    timeout_secs: Option<u64>,
) -> Result<String, String> {
    let path = find_mmx_command()?;
    let mut cmd = Command::new(&path);
    cmd.arg("vision")
        .arg("describe")
        .arg("--image")
        .arg(&image_path)
        .arg("--output")
        .arg("json")
        .arg("--quiet");
    if let Some(p) = prompt.as_deref() {
        if !p.is_empty() {
            cmd.arg("--prompt").arg(p);
        }
    }

    let started = Instant::now();
    let prompt_hash = hash_short(prompt.as_deref().unwrap_or(""));
    let timeout_dur = Duration::from_secs(timeout_secs.unwrap_or(DEFAULT_TIMEOUT_SECS));
    let output = run_capture(&mut cmd, timeout_dur).await.map_err(|e| {
        eprintln!("[mmx_vision_describe] failed prompt_hash={prompt_hash} err={e}");
        e
    })?;

    // mmx-cli's text/chat resources with `--output json --quiet` print
    // the assistant text directly. vision describe does the same — the
    // JSON envelope is only on the wire when the caller wants the full
    // response object. Accept either: parse JSON if it looks like one,
    // otherwise treat stdout as the literal text.
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let text = if stdout.starts_with('{') {
        extract_content_field(&stdout).unwrap_or(stdout)
    } else {
        stdout
    };

    eprintln!(
        "[mmx_vision_describe] ok prompt_hash={prompt_hash} bytes={} elapsed_ms={}",
        text.len(),
        started.elapsed().as_millis()
    );

    Ok(text)
}

/// Send a chat completion through `mmx text chat`. `messages` is the
/// full conversation; we translate each one into a `--message` flag
/// with `role:` prefix (mmx-cli's wire convention).
#[tauri::command]
pub async fn mmx_text_chat(
    messages: Vec<ChatMessage>,
    model: Option<String>,
    timeout_secs: Option<u64>,
) -> Result<String, String> {
    if messages.is_empty() {
        return Err("mmx_text_chat: messages must not be empty".to_string());
    }

    let path = find_mmx_command()?;
    let mut cmd = Command::new(&path);
    cmd.arg("text")
        .arg("chat")
        .arg("--output")
        .arg("json")
        .arg("--quiet");
    for m in &messages {
        // `--message role:content` is mmx-cli's multi-turn wire format.
        // Roles are pinned to the user/system/assistant triple; anything
        // else would 4xx on the server side.
        let role = match m.role.as_str() {
            "system" | "user" | "assistant" => m.role.as_str(),
            other => return Err(format!("mmx_text_chat: invalid role '{other}'")),
        };
        cmd.arg("--message").arg(format!("{role}:{}", m.content));
    }
    if let Some(m) = model.as_deref() {
        if !m.is_empty() {
            cmd.arg("--model").arg(m);
        }
    }

    let started = Instant::now();
    let msg_count = messages.len();
    let timeout_dur = Duration::from_secs(timeout_secs.unwrap_or(DEFAULT_TIMEOUT_SECS));
    let output = run_capture(&mut cmd, timeout_dur).await.map_err(|e| {
        eprintln!("[mmx_text_chat] failed msgs={msg_count} err={e}");
        e
    })?;

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let text = if stdout.starts_with('{') {
        extract_content_field(&stdout).unwrap_or(stdout)
    } else {
        stdout
    };

    eprintln!(
        "[mmx_text_chat] ok msgs={msg_count} bytes={} elapsed_ms={}",
        text.len(),
        started.elapsed().as_millis()
    );

    Ok(text)
}

// ----- internal helpers -----------------------------------------------------

struct CapturedOutput {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    status: std::process::ExitStatus,
}

async fn run_capture(cmd: &mut Command, timeout_dur: Duration) -> Result<CapturedOutput, String> {
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let mut child = cmd.spawn().map_err(|e| format!("spawn mmx failed: {e}"))?;

    // Read stdout/stderr concurrently so a chatty child can't block on
    // a full pipe buffer. tokio's processkill_on_drop already handles
    // cleanup if we drop the child.
    let mut stdout_pipe = child.stdout.take();
    let mut stderr_pipe = child.stderr.take();

    let stdout_fut = async move {
        let mut buf = Vec::new();
        if let Some(mut s) = stdout_pipe.take() {
            let _ = s.read_to_end(&mut buf).await;
        }
        buf
    };
    let stderr_fut = async move {
        let mut buf = Vec::new();
        if let Some(mut s) = stderr_pipe.take() {
            let _ = s.read_to_end(&mut buf).await;
        }
        buf
    };
    let wait_fut = child.wait();

    let timed = timeout(timeout_dur, async {
        let (status, stdout, stderr) = tokio::join!(wait_fut, stdout_fut, stderr_fut);
        let status = status.map_err(|e| format!("wait mmx failed: {e}"))?;
        Ok::<_, String>(CapturedOutput { stdout, stderr, status })
    })
    .await;

    match timed {
        Ok(Ok(captured)) => {
            if !captured.status.success() {
                let code = captured.status.code();
                let stderr = String::from_utf8_lossy(&captured.stderr).trim().to_string();
                // mmx-cli's documented exit codes (skill: mmx-cli):
                //   3 = auth, 4 = quota, 5 = timeout, 10 = content filter
                // We surface the numeric code so the frontend can
                // decide whether to prompt for re-auth (3) or warn
                // about quota (4).
                return Err(format!(
                    "mmx exited with code {code:?}: {}",
                    if stderr.is_empty() {
                        "<no stderr>"
                    } else {
                        stderr.as_str()
                    }
                ));
            }
            Ok(captured)
        }
        Ok(Err(e)) => Err(e),
        Err(_) => Err(format!(
            "mmx timed out after {}s",
            timeout_dur.as_secs()
        )),
    }
}

/// Pull `content` out of mmx's full JSON response object (the shape
/// returned when the caller asks for the full envelope, e.g.
/// `{ "content": "..." , "model": "..." , "usage": {...} }`).
fn extract_content_field(stdout: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(stdout).ok()?;
    if let Some(s) = v.get("content").and_then(|c| c.as_str()) {
        return Some(s.to_string());
    }
    if let Some(s) = v.get("text").and_then(|c| c.as_str()) {
        return Some(s.to_string());
    }
    None
}

fn extract_source(url: &str) -> String {
    // Cheap best-effort: take the registrable host. We don't pull in
    // url crate just for this; webview already knows the source host
    // from the URL itself.
    let trimmed = url
        .trim_start_matches("https://")
        .trim_start_matches("http://");
    trimmed
        .split('/')
        .next()
        .unwrap_or(trimmed)
        .to_string()
}

/// Short, non-cryptographic hash for log correlation. Keeps PII out
/// of logs while still letting us grep "did this query succeed".
fn hash_short(s: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}

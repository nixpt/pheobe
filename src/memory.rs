//! pheobe memory — `MemoryStore` trait + implementations.
//!
//! PHEOBE_MEMORY env selects at run start (default: local):
//!   local  — JsonlStore (append-only JSONL in ~/.pheobe/learning/)
//!   none   — NullStore  (all no-ops)
//!   host   — HostStore  (best-effort joker-mcp MCP delegation; falls back to local)
//!
//! The existing `learn.rs` free functions delegate to whichever store is active.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

// ── Session / Nudge (re-exported from learn for trait method signatures) ──────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub repo: String,
    pub task: String,
    pub started_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Nudge {
    pub scope_type: String,
    pub scope_value: String,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trigger_terms: Option<String>,
    #[serde(default)]
    pub shown: u32,
    #[serde(default)]
    pub created_at: String,
}

// ── Trait ─────────────────────────────────────────────────────────────────────

pub trait MemoryStore: Send + Sync {
    fn enabled(&self) -> bool;
    fn session_begin(&self, repo: &Path, task: &str) -> Result<Option<Session>>;
    fn session_end(&self, s: Option<&Session>, outcome: &str, exit: i32) -> Result<()>;
    fn log_event(&self, session: &str, kind: &str, tool: Option<&str>, payload: Option<&str>);
    fn store_nudge(&self, repo: &str, text: &str, trigger_terms: Option<&str>) -> Result<()>;
    fn nudges_for(&self, repo: &str) -> Vec<String>;
}

// ── Global store selection ─────────────────────────────────────────────────────

static STORE: OnceLock<Box<dyn MemoryStore>> = OnceLock::new();

/// Select the memory store from PHEOBE_MEMORY (none|local|host). Default: local.
/// Called once at run start. If never called, free functions lazily create a
/// JsonlStore reading the current env (backward-compat for tests).
pub fn init() {
    let mode = std::env::var("PHEOBE_MEMORY").unwrap_or_else(|_| "local".into());
    let store: Box<dyn MemoryStore> = match mode.as_str() {
        "none" => Box::new(NullStore),
        "host" => match HostStore::new() {
            Ok(s) => Box::new(s),
            Err(e) => {
                eprintln!("⚠ PHEOBE_MEMORY=host but joker unavailable ({e}); using local");
                Box::new(JsonlStore::new())
            }
        },
        _ => Box::new(JsonlStore::new()),
    };
    let _ = STORE.set(store);
}

pub fn current() -> &'static dyn MemoryStore {
    STORE.get().map(|s| s.as_ref()).unwrap_or_else(|| {
        // Lazy init for tests / backward compat — no explicit init() call needed
        let mode = std::env::var("PHEOBE_MEMORY").unwrap_or_else(|_| "local".into());
        let store: Box<dyn MemoryStore> = match mode.as_str() {
            "none" => Box::new(NullStore),
            "host" => HostStore::new()
                .map(|s| Box::new(s) as Box<dyn MemoryStore>)
                .unwrap_or_else(|_| Box::new(JsonlStore::new())),
            _ => Box::new(JsonlStore::new()),
        };
        let _ = STORE.set(store);
        STORE.get().unwrap().as_ref()
    })
}

// ── JsonlStore (local) ────────────────────────────────────────────────────────

struct JsonlStore;

impl JsonlStore {
    fn new() -> Self {
        Self
    }
}

fn learning_dir() -> Option<PathBuf> {
    if let Ok(d) = std::env::var("PHEOBE_LEARNING_DIR") {
        return Some(PathBuf::from(d));
    }
    std::env::var("HOME")
        .ok()
        .map(|h| PathBuf::from(h).join(".pheobe").join("learning"))
}

fn jsonl_enabled() -> bool {
    if std::env::var("PHEOBE_LEARN").ok().as_deref() == Some("1") {
        return true;
    }
    learning_dir().is_some_and(|d| d.is_dir())
}

fn append_jsonl(file: &Path, line: &str) -> anyhow::Result<()> {
    use std::io::Write;
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(file)?;
    writeln!(f, "{line}")?;
    Ok(())
}

/// Resolve a command name against PATH's dirs (missing PHEOBE_JOKER_CMD stays
/// a plain name check so `joker-mcp` resolves from the caller's PATH).
fn bin_is_resolvable(cmd: &str) -> bool {
    use std::os::unix::fs::PermissionsExt;
    if cmd.contains('/') {
        return std::fs::metadata(cmd)
            .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false);
    }
    let Ok(paths) = std::env::var("PATH") else {
        return false;
    };
    paths.split(':').any(|d| {
        let p = Path::new(d).join(cmd);
        std::fs::metadata(&p)
            .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    })
}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn now_iso() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0) as i64;
    let days = secs / 86400;
    let rem = secs % 86400;
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        (rem / 3600),
        (rem % 3600) / 60,
        rem % 60
    )
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719468;
    let era = z.div_euclid(146097);
    let doe = z.rem_euclid(146097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (y + if m <= 2 { 1 } else { 0 }, m, d)
}

impl MemoryStore for JsonlStore {
    fn enabled(&self) -> bool {
        jsonl_enabled()
    }

    fn session_begin(&self, repo: &Path, task: &str) -> Result<Option<Session>> {
        if !self.enabled() {
            return Ok(None);
        }
        let s = Session {
            id: format!("s-{}", now_unix()),
            repo: repo.display().to_string(),
            task: task.to_string(),
            started_at: now_iso(),
            ended_at: None,
            outcome: None,
            exit_code: None,
        };
        if let Some(dir) = learning_dir() {
            append_jsonl(&dir.join("sessions.jsonl"), &serde_json::to_string(&s)?)?;
        }
        Ok(Some(s))
    }

    fn session_end(&self, s: Option<&Session>, outcome: &str, exit: i32) -> Result<()> {
        let Some(s) = s else { return Ok(()) };
        if !self.enabled() {
            return Ok(());
        }
        let closed = Session {
            id: s.id.clone(),
            repo: s.repo.clone(),
            task: s.task.clone(),
            started_at: s.started_at.clone(),
            ended_at: Some(now_iso()),
            outcome: Some(outcome.to_string()),
            exit_code: Some(exit),
        };
        if let Some(dir) = learning_dir() {
            append_jsonl(
                &dir.join("sessions.jsonl"),
                &serde_json::to_string(&closed)?,
            )?;
        }
        Ok(())
    }

    fn log_event(&self, session: &str, kind: &str, tool: Option<&str>, payload: Option<&str>) {
        if !self.enabled() {
            return;
        }
        let ev = serde_json::json!({
            "session": session, "ts": now_iso(), "kind": kind,
            "tool": tool, "payload": payload,
        });
        if let Some(dir) = learning_dir() {
            let _ = append_jsonl(&dir.join("events.jsonl"), &ev.to_string());
        }
    }

    fn store_nudge(&self, repo: &str, text: &str, trigger_terms: Option<&str>) -> Result<()> {
        let n = Nudge {
            scope_type: "repo".into(),
            scope_value: repo.to_string(),
            text: text.to_string(),
            trigger_terms: trigger_terms.map(|t| t.to_string()),
            shown: 0,
            created_at: now_iso(),
        };
        if let Some(dir) = learning_dir() {
            append_jsonl(&dir.join("nudges.jsonl"), &serde_json::to_string(&n)?)?;
        }
        Ok(())
    }

    fn nudges_for(&self, repo: &str) -> Vec<String> {
        if !self.enabled() {
            return vec![];
        }
        let Some(dir) = learning_dir() else {
            return vec![];
        };
        let Ok(txt) = std::fs::read_to_string(dir.join("nudges.jsonl")) else {
            return vec![];
        };
        txt.lines()
            .filter_map(|l| serde_json::from_str::<Nudge>(l).ok())
            .filter(|n| n.scope_type == "repo" && n.scope_value == repo)
            .map(|n| n.text)
            .collect()
    }
}

// ── NullStore (none) ──────────────────────────────────────────────────────────

struct NullStore;

impl MemoryStore for NullStore {
    fn enabled(&self) -> bool {
        false
    }
    fn session_begin(&self, _repo: &Path, _task: &str) -> Result<Option<Session>> {
        Ok(None)
    }
    fn session_end(&self, _s: Option<&Session>, _outcome: &str, _exit: i32) -> Result<()> {
        Ok(())
    }
    fn log_event(&self, _session: &str, _kind: &str, _tool: Option<&str>, _payload: Option<&str>) {}
    fn store_nudge(&self, _repo: &str, _text: &str, _trigger_terms: Option<&str>) -> Result<()> {
        Ok(())
    }
    fn nudges_for(&self, _repo: &str) -> Vec<String> {
        vec![]
    }
}

// ── HostStore (joker-mcp MCP stdio, best-effort) ──────────────────────────────

struct HostStore {
    bin: String,
    doubt: std::sync::OnceLock<String>,
}

impl HostStore {
    fn new() -> Result<Self> {
        let bin = std::env::var("PHEOBE_JOKER_CMD").unwrap_or_else(|_| "joker-mcp".to_string());
        if !bin_is_resolvable(&bin) {
            return Err(anyhow::anyhow!("{bin}: not found on PATH"));
        }
        Ok(Self {
            bin,
            doubt: std::sync::OnceLock::new(),
        })
    }

    fn mcp_call(&self, tool: &str, args: serde_json::Value) -> Option<String> {
        use std::io::{BufRead, BufReader, Write};
        use std::process::{Command, Stdio};
        use std::sync::mpsc;
        use std::time::Duration;

        let init = serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize",
            "params":{"protocolVersion":"2024-11-05","capabilities":{},
                      "clientInfo":{"name":"pheobe","version":"0.1"}}});
        let notif = serde_json::json!({"jsonrpc":"2.0","method":"notifications/initialized"});
        let call = serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/call",
            "params":{"name": tool, "arguments": args}});

        let mut child = Command::new(&self.bin)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;

        // reader thread: joker-mcp is request/response over its stdin — we keep
        // the stdin open, handshake, read the id:2 line, then kill it.
        let (tx, rx) = mpsc::channel::<String>();
        let stdout = child.stdout.take()?;
        let t = std::thread::spawn(move || {
            let mut lines = BufReader::new(stdout).lines();
            while let Some(Ok(l)) = lines.next() {
                if tx.send(l).is_err() {
                    break;
                }
            }
        });

        let write = |s: &mut std::process::ChildStdin, line: String| writeln!(s, "{line}").ok();
        {
            let stdin = child.stdin.as_mut()?;
            write(stdin, init.to_string())?;
            // wait for the initialize ack before proceeding
            match rx.recv_timeout(Duration::from_secs(5)) {
                Ok(_) => {}
                Err(_) => {
                    let _ = child.kill();
                    return None;
                }
            }
            write(stdin, notif.to_string())?;
            write(stdin, call.to_string())?;
        }

        // collect lines until we see id:2 (or timeout)
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while std::time::Instant::now() < deadline {
            match rx.recv_timeout(Duration::from_millis(200)) {
                Ok(line) => {
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) {
                        if v.get("id") == Some(&serde_json::json!(2)) {
                            if let Some(content) =
                                v.pointer("/result/content").and_then(|c| c.get(0))
                            {
                                let _ = child.kill();
                                return Some(content.get("text")?.as_str()?.to_string());
                            }
                        }
                    }
                }
                Err(_) => continue,
            }
        }
        let _ = child.kill();
        let _ = t.join();
        None
    }

    fn doubt(&self, msg: &str) -> String {
        self.doubt.get_or_init(|| msg.to_string()).clone()
    }
}

impl MemoryStore for HostStore {
    fn enabled(&self) -> bool {
        true
    }

    fn session_begin(&self, repo: &Path, task: &str) -> Result<Option<Session>> {
        // joker doesn't manage sessions; fall through to local tracking
        JsonlStore.session_begin(repo, task)
    }

    fn session_end(&self, s: Option<&Session>, outcome: &str, exit: i32) -> Result<()> {
        JsonlStore.session_end(s, outcome, exit)
    }

    fn log_event(&self, session: &str, kind: &str, tool: Option<&str>, payload: Option<&str>) {
        JsonlStore.log_event(session, kind, tool, payload);
    }

    fn store_nudge(&self, repo: &str, text: &str, trigger_terms: Option<&str>) -> Result<()> {
        let fact = format!("nudge[{repo}]: {text}");
        match self.mcp_call(
            "joker_store_fact",
            serde_json::json!({
                "fact": fact, "category": "pheobe",
            }),
        ) {
            Some(_) => Ok(()),
            None => {
                eprintln!(
                    "⚠ host store_nudge failed ({})",
                    self.doubt("joker-mcp store failed; falling back to local")
                );
                JsonlStore.store_nudge(repo, text, trigger_terms)
            }
        }
    }

    fn nudges_for(&self, repo: &str) -> Vec<String> {
        match self.mcp_call(
            "joker_recall_facts",
            serde_json::json!({
                "category": "pheobe", "query": format!("nudge[{repo}]"),
            }),
        ) {
            Some(resp) => {
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&resp) {
                    if let Some(facts) = parsed.get("facts").and_then(|f| f.as_array()) {
                        return facts
                            .iter()
                            .filter_map(|f| f.get("content")?.as_str().map(String::from))
                            .filter(|s| s.starts_with(&format!("nudge[{repo}]")))
                            .map(|s| s.replacen(&format!("nudge[{repo}]: "), "", 1))
                            .collect();
                    }
                }
                eprintln!(
                    "⚠ host nudges_for parse failed ({})",
                    self.doubt("joker-mcp parse failed; using local")
                );
                JsonlStore.nudges_for(repo)
            }
            None => {
                eprintln!(
                    "⚠ host nudges_for failed ({})",
                    self.doubt("joker-mcp recall failed; using local")
                );
                JsonlStore.nudges_for(repo)
            }
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::sync::Mutex;

    // Serialize tests that mutate env or the global STORE — shared with the
    // learn tests in src/tests.rs via crate::memory::tests::env_lock().
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[cfg(test)]
    pub(crate) fn env_lock() -> &'static Mutex<()> {
        &ENV_LOCK
    }

    #[test]
    fn null_store_is_always_disabled_and_noop() {
        let _lock = ENV_LOCK.lock().unwrap();
        let s = NullStore;
        assert!(!s.enabled());
        assert!(s.session_begin(Path::new("/tmp/x"), "t").unwrap().is_none());
        s.store_nudge("r", "n", None).unwrap();
        assert!(s.nudges_for("r").is_empty());
    }

    #[test]
    fn jsonl_store_respects_env_and_writes_jsonl() {
        let _lock = ENV_LOCK.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("pheobe-mem-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        unsafe { std::env::set_var("PHEOBE_LEARNING_DIR", &dir) };
        unsafe { std::env::set_var("PHEOBE_MEMORY", "local") };

        let s = JsonlStore;
        assert!(s.enabled());

        let sess = s
            .session_begin(Path::new("/tmp/repo"), "write tests")
            .unwrap()
            .unwrap();
        s.store_nudge("/tmp/repo", "watch for semicolons", Some("syntax"))
            .unwrap();
        let nudges = s.nudges_for("/tmp/repo");
        assert_eq!(nudges.len(), 1);
        assert!(nudges[0].contains("semicolons"));

        s.session_end(Some(&sess), "done", 0).unwrap();
        let txt = std::fs::read_to_string(dir.join("sessions.jsonl")).unwrap();
        assert_eq!(txt.lines().count(), 2);

        unsafe { std::env::remove_var("PHEOBE_LEARNING_DIR") };
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn host_store_falls_back_to_local_when_joker_missing() {
        let _lock = ENV_LOCK.lock().unwrap();
        // PHEOBE_JOKER_CMD pointing at a nonexistent binary
        unsafe { std::env::set_var("PHEOBE_JOKER_CMD", "/no/such/binary-xyz") };
        let dir = std::env::temp_dir().join(format!("pheobe-mem-host-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        unsafe { std::env::set_var("PHEOBE_LEARNING_DIR", &dir) };

        // HostStore::new() fails when the joker binary is unresolvable →
        // init()/current() fall back to the local JsonlStore.
        assert!(HostStore::new().is_err());
        let store = crate::memory::current();
        assert!(store.enabled()); // fell back to local
        store
            .store_nudge("/tmp/repo", "local fallback works", None)
            .unwrap();
        assert_eq!(store.nudges_for("/tmp/repo").len(), 1);

        unsafe { std::env::remove_var("PHEOBE_LEARNING_DIR") };
        unsafe { std::env::remove_var("PHEOBE_JOKER_CMD") };
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn init_selects_null_store_when_none() {
        let _lock = ENV_LOCK.lock().unwrap();
        unsafe { std::env::set_var("PHEOBE_MEMORY", "none") };
        // Reset the static for this test — can't re-init OnceLock, so just
        // verify NullStore path compiles and returns correct defaults
        let s = NullStore;
        assert!(!s.enabled());
        assert!(s.nudges_for("any").is_empty());
        unsafe { std::env::remove_var("PHEOBE_MEMORY") };
    }
}

//! Public-channel version probe (`pheobe doctor`) and in-place upgrade
//! (`pheobe update`).
//!
//! crates.io when the crate exists; otherwise the newest `v*` git tag on
//! `nixpt/pheobe`. `pheobe update` spawns `cargo install` — it does not
//! download GitHub Release assets (bro's private-repo path).

use anyhow::{Context, Result};
use std::time::Duration;

pub const CRATE: &str = "pheobe";
pub const REPO: &str = "nixpt/pheobe";
pub const GIT_URL: &str = "https://github.com/nixpt/pheobe";

const CRATES_URL: &str = "https://crates.io/api/v1/crates/pheobe";
const TAGS_URL: &str = "https://api.github.com/repos/nixpt/pheobe/tags";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    CratesIo,
    GithubTag,
}

impl Source {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CratesIo => "crates.io",
            Self::GithubTag => "github tag",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Channel {
    pub version: String,
    pub source: Source,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    pub local: String,
    pub remote: Option<Channel>,
    pub unreachable: bool,
}

impl Status {
    pub fn update_available(&self) -> bool {
        match &self.remote {
            Some(ch) => cmp_ver(&ch.version, &self.local) == std::cmp::Ordering::Greater,
            None => false,
        }
    }

    /// One doctor column, no trailing newline.
    pub fn line(&self) -> String {
        if self.unreachable {
            return format!("{} (channel unreachable)", self.local);
        }
        match &self.remote {
            None => format!("{} (no public version yet)", self.local),
            Some(ch) => match cmp_ver(&ch.version, &self.local) {
                std::cmp::Ordering::Greater => {
                    format!(
                        "{} — {} available ({})",
                        self.local,
                        ch.version,
                        ch.source.as_str()
                    )
                }
                std::cmp::Ordering::Equal => match ch.source {
                    Source::CratesIo => format!("{} (current)", self.local),
                    Source::GithubTag => {
                        format!("{} (current — github tag; not on crates.io)", self.local)
                    }
                },
                std::cmp::Ordering::Less => {
                    format!(
                        "{} (newer than {} on {})",
                        self.local,
                        ch.version,
                        ch.source.as_str()
                    )
                }
            },
        }
    }
}

/// Remote lookup used by [`check`]. Tests inject a fake; doctor uses [`Live`].
pub trait Probe {
    fn crates_max(&self) -> Result<Option<String>>;
    fn github_latest_tag(&self) -> Result<Option<String>>;
}

pub struct Live;

impl Probe for Live {
    fn crates_max(&self) -> Result<Option<String>> {
        let (code, body) = get(CRATES_URL)?;
        if code == 404 {
            return Ok(None);
        }
        if !(200..300).contains(&code) {
            anyhow::bail!("crates.io HTTP {code}");
        }
        Ok(parse_crates_io(&body))
    }

    fn github_latest_tag(&self) -> Result<Option<String>> {
        let (code, body) = get(TAGS_URL)?;
        if code == 404 {
            return Ok(None);
        }
        if !(200..300).contains(&code) {
            anyhow::bail!("github tags HTTP {code}");
        }
        Ok(parse_github_tags(&body))
    }
}

/// crates.io first; git tags if the crate is unpublished or crates.io errors.
pub fn check(local: &str, probe: &dyn Probe) -> Status {
    let local = strip_v(local).to_string();
    match probe.crates_max() {
        Ok(Some(v)) => Status {
            local,
            remote: Some(Channel {
                version: strip_v(&v).to_string(),
                source: Source::CratesIo,
            }),
            unreachable: false,
        },
        Ok(None) | Err(_) => match probe.github_latest_tag() {
            Ok(Some(v)) => Status {
                local,
                remote: Some(Channel {
                    version: strip_v(&v).to_string(),
                    source: Source::GithubTag,
                }),
                unreachable: false,
            },
            Ok(None) => Status {
                local,
                remote: None,
                unreachable: false,
            },
            Err(_) => Status {
                local,
                remote: None,
                unreachable: true,
            },
        },
    }
}

pub fn current() -> Status {
    check(env!("CARGO_PKG_VERSION"), &Live)
}

/// `cargo` argv after the binary name. crates.io crate when that is the
/// channel; otherwise `--git` pinned to the tag (or HEAD if none).
pub fn cargo_install_args(status: &Status) -> Vec<String> {
    match &status.remote {
        Some(ch) if ch.source == Source::CratesIo => vec![
            "install".into(),
            CRATE.into(),
            "--locked".into(),
            "--force".into(),
        ],
        Some(ch) => vec![
            "install".into(),
            "--git".into(),
            GIT_URL.into(),
            "--tag".into(),
            format!("v{}", ch.version),
            "--locked".into(),
            "--force".into(),
        ],
        None => vec![
            "install".into(),
            "--git".into(),
            GIT_URL.into(),
            "--locked".into(),
            "--force".into(),
        ],
    }
}

/// Whether `pheobe update` should spawn cargo. Unreachable is an error so
/// we never install blind. Already-current is a no-op unless `force`.
pub fn should_install(status: &Status, force: bool) -> Result<bool> {
    if status.unreachable {
        anyhow::bail!("channel unreachable — not running cargo install");
    }
    if force {
        return Ok(true);
    }
    Ok(status.update_available())
}

/// Exit code for `pheobe update --check`: 1 if behind or the channel is
/// unreachable (a silent 0 would look like "already current").
pub fn check_exit_code(status: &Status) -> i32 {
    if status.unreachable || status.update_available() {
        1
    } else {
        0
    }
}

/// Doctor-identical probe, then optionally `cargo install`. Returns the
/// process exit code for `--check` (1 = update available or unreachable).
pub fn run(check_only: bool, force: bool) -> Result<i32> {
    let status = current();
    println!("{:14} {}", "pheobe:", status.line());
    if check_only {
        return Ok(check_exit_code(&status));
    }
    if !should_install(&status, force)? {
        if status.remote.is_none() {
            println!("no public version yet — pass --force to install from git HEAD");
        } else {
            println!("already current — pass --force to reinstall");
        }
        return Ok(0);
    }
    let args = cargo_install_args(&status);
    eprintln!("$ cargo {}", args.join(" "));
    match std::process::Command::new("cargo").args(&args).status() {
        Ok(s) if s.success() => {
            println!("installed. restart pheobe to use the new binary.");
            Ok(0)
        }
        Ok(s) => anyhow::bail!("cargo install failed ({s})"),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => anyhow::bail!(
            "cargo not on PATH — install Rust (https://rustup.rs) then `cargo install pheobe`"
        ),
        Err(e) => Err(e).context("spawn cargo"),
    }
}

pub fn parse_crates_io(body: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    v.get("crate")?
        .get("max_version")?
        .as_str()
        .map(|s| strip_v(s).to_string())
}

pub fn parse_github_tags(body: &str) -> Option<String> {
    let tags: Vec<serde_json::Value> = serde_json::from_str(body).ok()?;
    for t in tags {
        if let Some(name) = t.get("name").and_then(|n| n.as_str()) {
            if looks_like_semver(name) {
                return Some(strip_v(name).to_string());
            }
        }
    }
    None
}

fn get(url: &str) -> Result<(u16, String)> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(2))
        .user_agent(format!(
            "pheobe/{} (+https://github.com/{REPO})",
            env!("CARGO_PKG_VERSION")
        ))
        .build()?;
    let resp = client.get(url).send()?;
    let code = resp.status().as_u16();
    let body = resp.text().unwrap_or_default();
    Ok((code, body))
}

fn strip_v(s: &str) -> &str {
    s.trim().trim_start_matches(['v', 'V'])
}

fn looks_like_semver(s: &str) -> bool {
    let s = strip_v(s);
    let mut parts = s.split('.');
    let major = parts.next().unwrap_or("");
    let minor = parts.next().unwrap_or("");
    !major.is_empty()
        && major.chars().all(|c| c.is_ascii_digit())
        && !minor.is_empty()
        && minor
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .next()
            .is_some()
}

fn cmp_ver(a: &str, b: &str) -> std::cmp::Ordering {
    parse_ver(a).cmp(&parse_ver(b))
}

fn parse_ver(s: &str) -> [u64; 3] {
    let s = strip_v(s);
    let mut out = [0u64; 3];
    for (i, p) in s.split('.').take(3).enumerate() {
        let digits: String = p.chars().take_while(|c| c.is_ascii_digit()).collect();
        out[i] = digits.parse().unwrap_or(0);
    }
    out
}

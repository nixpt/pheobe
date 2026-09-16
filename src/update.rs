//! Public-channel version probe for `pheobe doctor`.
//!
//! crates.io when the crate exists; otherwise the newest `v*` git tag on
//! `nixpt/pheobe`. Tags exist today; GitHub Release objects may not
//! (`/releases/latest` 404s). Network failure never fails doctor.

use anyhow::Result;
use std::time::Duration;

pub const CRATE: &str = "pheobe";
pub const REPO: &str = "nixpt/pheobe";

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

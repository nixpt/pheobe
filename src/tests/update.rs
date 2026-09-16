//! PHEOBE-31: doctor version probe (fake channel, no live net).

use crate::update::{check, parse_crates_io, parse_github_tags, Probe, Source, Status};
use anyhow::Result;

struct Fake {
    crates: Result<Option<String>, anyhow::Error>,
    github: Result<Option<String>, anyhow::Error>,
}

impl Probe for Fake {
    fn crates_max(&self) -> Result<Option<String>> {
        match &self.crates {
            Ok(v) => Ok(v.clone()),
            Err(e) => Err(anyhow::anyhow!("{e}")),
        }
    }
    fn github_latest_tag(&self) -> Result<Option<String>> {
        match &self.github {
            Ok(v) => Ok(v.clone()),
            Err(e) => Err(anyhow::anyhow!("{e}")),
        }
    }
}

fn fake(crates: Result<Option<&str>, &str>, github: Result<Option<&str>, &str>) -> Fake {
    Fake {
        crates: crates
            .map(|o| o.map(|s| s.to_string()))
            .map_err(|e| anyhow::anyhow!("{e}")),
        github: github
            .map(|o| o.map(|s| s.to_string()))
            .map_err(|e| anyhow::anyhow!("{e}")),
    }
}

#[test]
fn crates_io_newer_is_an_update() {
    let s = check("0.2.0", &fake(Ok(Some("0.3.0")), Ok(Some("0.2.0"))));
    assert!(s.update_available());
    assert_eq!(s.remote.as_ref().unwrap().source, Source::CratesIo);
    assert_eq!(s.line(), "0.2.0 — 0.3.0 available (crates.io)");
}

#[test]
fn unpublished_crate_falls_back_to_github_tag() {
    let s = check("0.2.0", &fake(Ok(None), Ok(Some("v0.2.0"))));
    assert!(!s.update_available());
    assert_eq!(s.remote.as_ref().unwrap().source, Source::GithubTag);
    assert_eq!(s.line(), "0.2.0 (current — github tag; not on crates.io)");
}

#[test]
fn github_tag_newer_when_crates_missing() {
    let s = check("0.1.0", &fake(Ok(None), Ok(Some("v0.2.0"))));
    assert!(s.update_available());
    assert_eq!(s.line(), "0.1.0 — 0.2.0 available (github tag)");
}

#[test]
fn crates_io_error_still_tries_github() {
    let s = check("0.2.0", &fake(Err("timeout"), Ok(Some("0.2.0"))));
    assert!(!s.update_available());
    assert_eq!(s.remote.as_ref().unwrap().source, Source::GithubTag);
}

#[test]
fn both_channels_down_is_unreachable_not_a_failure() {
    let s = check("0.2.0", &fake(Err("down"), Err("down")));
    assert!(s.unreachable);
    assert_eq!(s.line(), "0.2.0 (channel unreachable)");
}

#[test]
fn empty_channels_are_not_unreachable() {
    let s = check("0.1.0", &fake(Ok(None), Ok(None)));
    assert!(!s.unreachable);
    assert_eq!(s.line(), "0.1.0 (no public version yet)");
}

#[test]
fn local_ahead_of_crates() {
    let s = check("0.2.0", &fake(Ok(Some("0.1.0")), Ok(None)));
    assert!(!s.update_available());
    assert_eq!(s.line(), "0.2.0 (newer than 0.1.0 on crates.io)");
}

#[test]
fn crates_current() {
    let s = check("0.2.0", &fake(Ok(Some("0.2.0")), Ok(Some("9.9.9"))));
    assert_eq!(s.line(), "0.2.0 (current)");
}

#[test]
fn parse_crates_io_max_version() {
    let body = r#"{"crate":{"id":"pheobe","max_version":"0.2.0","newest_version":"0.2.0"}}"#;
    assert_eq!(parse_crates_io(body).as_deref(), Some("0.2.0"));
    assert!(parse_crates_io(r#"{"errors":[{"detail":"Not Found"}]}"#).is_none());
}

#[test]
fn parse_github_tags_skips_non_semver() {
    let body = r#"[{"name":"nightly"},{"name":"v0.2.0"},{"name":"v0.1.0"}]"#;
    assert_eq!(parse_github_tags(body).as_deref(), Some("0.2.0"));
}

#[test]
fn status_line_never_panics_on_empty_local() {
    let s = Status {
        local: "0.0.0".into(),
        remote: None,
        unreachable: true,
    };
    assert!(s.line().contains("unreachable"));
}

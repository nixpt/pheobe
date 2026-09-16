//! pheobe ctx — the knowledge drive (borrowed from jokersquad's ctx tool).
//! Entries: version-controlled markdown with front-matter, kinded internal /
//! external / reference, stale after 90 days. `brief` emits the prompt block
//! with the standing preamble: where it conflicts with what you "know", it is
//! right and you are wrong.

use anyhow::Result;

use std::path::{Path, PathBuf};

pub const STALE_DAYS: i64 = 90;

#[derive(Debug, Clone, serde::Serialize)]
pub struct Entry {
    pub slug: String,
    pub name: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_verified: Option<String>,
    pub stale: bool,
    pub path: PathBuf,
}

/// Drive roots, in order: `$PHEOBE_KNOWLEDGE_DIR` (or `~/.pheobe/knowledge`),
/// then `$PHEOBE_CTX_EXTRA` (a squad box points this at `.squad/research` —
/// same format, one shared corpus), then repo-local `<repo>/.pheobe/knowledge`.
pub fn drive_roots(repo: Option<&Path>) -> Vec<PathBuf> {
    let mut v = vec![];
    match std::env::var("PHEOBE_KNOWLEDGE_DIR") {
        Ok(dir) => v.push(PathBuf::from(dir)),
        Err(_) => {
            if let Ok(home) = std::env::var("HOME") {
                v.push(PathBuf::from(home).join(".pheobe").join("knowledge"));
            }
        }
    }
    if let Ok(extra) = std::env::var("PHEOBE_CTX_EXTRA") {
        v.push(PathBuf::from(extra));
    }
    if let Some(r) = repo {
        v.push(r.join(".pheobe").join("knowledge"));
    }
    v
}

/// The prompt-injectable brief block.
pub fn brief(entries: &[Entry]) -> String {
    let mut out = String::new();
    out.push_str("## RESEARCH DRIVE — things your training data does NOT contain\n\n");
    out.push_str(
        "You are working on subjects that post-date or fall outside your training. What\n\
         follows is curated, verified fact — not your recollection. Where it conflicts\n\
         with what you 'know', THIS IS RIGHT AND YOU ARE WRONG. If you need a detail that\n\
         isn't here, read the source. Do not guess an API into existence.\n\n",
    );
    for e in entries {
        out.push_str(&format!(
            "### {}  ({})\nversion: {}   last verified: {}{}\n\n",
            e.name,
            e.kind,
            e.version.clone().unwrap_or_else(|| "?".into()),
            e.last_verified.clone().unwrap_or_else(|| "never".into()),
            if e.stale {
                format!(
                    "   ⚠ NOT re-verified in {}d+ — treat specifics as possibly stale",
                    STALE_DAYS
                )
            } else {
                String::new()
            }
        ));
    }
    out
}

/// Parse one front-matter block. Stdlib-only, mirrors ctx's hand-parser.
fn parse_entry(path: &Path) -> Option<Entry> {
    let txt = std::fs::read_to_string(path).ok()?;
    if !txt.starts_with("---") {
        return None;
    }
    let end = txt.find("\n---")?;
    let head = &txt[3..end];
    let mut slug = None;
    let mut name = None;
    let mut kind = None;
    let mut version = None;
    let mut last_verified = None;
    for line in head.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('-') {
            continue;
        }
        let (k, v) = line.split_once(':')?;
        let v = v.trim().trim_matches('"');
        match k {
            "slug" => slug = Some(v.to_string()),
            "name" => name = Some(v.to_string()),
            "kind" => kind = Some(v.to_string()),
            "version" => version = Some(v.to_string()),
            "last_verified" => last_verified = Some(v.to_string()),
            _ => {}
        }
    }
    let slug = slug.or_else(|| {
        path
            .file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string())
    })?;
    let name = name.unwrap_or_else(|| slug.clone());
    let stale = last_verified
        .as_deref()
        .and_then(days_since)
        .map(|d| d >= STALE_DAYS)
        .unwrap_or(true);
    Some(Entry {
        slug,
        name,
        kind: kind.unwrap_or_else(|| "external".into()),
        version,
        last_verified,
        stale,
        path: path.to_path_buf(),
    })
}

/// Days since an ISO date (YYYY-MM-DD) — approximate, staleness-grade only.
fn days_since(iso: &str) -> Option<i64> {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs() as i64;
    let today = secs / 86400;
    let y: i64 = iso.get(0..4)?.parse().ok()?;
    let m: i64 = iso.get(5..7)?.parse().ok()?;
    let d: i64 = iso.get(8..10)?.parse().ok()?;
    // epoch days of Y-M-D via the standard civil-days formula
    let yy = if m <= 2 { y - 1 } else { y };
    let era = (if yy >= 0 { yy } else { yy - 399 }) / 400;
    let yoe = yy - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let entry_day = era * 146097 + doe - 719468;
    Some(today - entry_day)
}

/// Scan a directory's `*.md` entries (sorted for determinism).
pub fn scan(dir: &Path) -> Result<Vec<Entry>> {
    if !dir.is_dir() {
        return Ok(vec![]);
    }
    let mut out = vec![];
    for f in std::fs::read_dir(dir)? {
        let f = f?;
        let p = f.path();
        let keep = p
            .extension()
            .is_some_and(|e| e == "md")
            && p.file_name()
                .and_then(|n| n.to_str())
                .map(|n| !n.starts_with('_') && n != "README.md")
                .unwrap_or(false);
        if keep {
            if let Some(e) = parse_entry(&p) {
                out.push(e);
            }
        }
    }
    out.sort_by(|a, b| a.slug.cmp(&b.slug));
    Ok(out)
}

/// All entries across all drives, in drive order.
pub fn load_all(repo: Option<&Path>) -> Result<Vec<Entry>> {
    let mut out = vec![];
    for dir in drive_roots(repo) {
        out.extend(scan(&dir)?);
    }
    Ok(out)
}

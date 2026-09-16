//! Format-on-write — bro's `tools/format.rs` table (BRO-93) ported to
//! pheobe's sync std-only shape. After `write`/`edit` touch a file, run the
//! file's formatter if installed so edits land in canonical style instead of
//! trusting the model to format. Best-effort by design: a missing binary is
//! a silent skip, a formatter error is a doubt note in the tool result —
//! never a tool failure. Kill-switch: `PHEOBE_FORMAT=off`.

use std::path::Path;
use std::process::Command;

/// (name, extensions, argv template with $FILE). First extension match wins.
/// Prettier is repo-local by design: it's a project dependency, resolved to
/// `<worktree>/node_modules/.bin/prettier` at run time.
const FORMATTERS: &[(&str, &[&str], &[&str])] = &[
    ("rustfmt", &["rs"], &["rustfmt", "$FILE"]),
    ("gofmt", &["go"], &["gofmt", "-w", "$FILE"]),
    ("black", &["py"], &["black", "-q", "$FILE"]),
    (
        "prettier",
        &["ts", "tsx", "js", "jsx", "mjs", "cjs"],
        &["prettier", "--write", "$FILE"],
    ),
];

/// Pure resolver: formatter name + concrete argv for a path.
pub fn formatter_for(wt: &Path, path: &Path) -> Option<(&'static str, Vec<String>)> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    for (name, exts, argv) in FORMATTERS {
        if exts.contains(&ext.as_str()) {
            let out: Vec<String> = argv
                .iter()
                .map(|a| match *a {
                    "$FILE" => path.display().to_string(),
                    "prettier" if *name == "prettier" => wt
                        .join("node_modules")
                        .join(".bin")
                        .join("prettier")
                        .display()
                        .to_string(),
                    _ => (*a).to_string(),
                })
                .collect();
            return Some((name, out));
        }
    }
    None
}

fn enabled() -> bool {
    !std::env::var("PHEOBE_FORMAT")
        .map(|v| v.trim().eq_ignore_ascii_case("off"))
        .unwrap_or(false)
}

/// Best-effort format of a just-written/edited file. `Some(note)` carries the
/// tool-result tail: a formatted note, or a doubt note on formatter failure.
/// Missing binary / disabled → `None`. Never fails the tool call.
pub fn format_on_write(wt: &Path, path: &Path) -> Option<String> {
    if !enabled() {
        return None;
    }
    let (name, argv) = formatter_for(wt, path)?;
    let out = match Command::new(&argv[0])
        .args(&argv[1..])
        .current_dir(wt)
        .output()
    {
        Err(_) => return None, // ENOENT — formatter not installed, silent skip
        Ok(o) => o,
    };
    if out.status.success() {
        return Some(format!(" (formatted with {name})"));
    }
    let stderr = String::from_utf8_lossy(&out.stderr);
    let first = stderr.lines().next().unwrap_or("").trim();
    Some(format!(
        " (doubt: formatter {name} failed on {} — file left as written: {})",
        path.display(),
        crate::llm::truncate(first, 160)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formatter_table_claims_by_extension() {
        let wt = Path::new("/tmp");
        let (n, argv) = formatter_for(wt, Path::new("src/lib.rs")).unwrap();
        assert_eq!(n, "rustfmt");
        assert_eq!(argv, vec!["rustfmt".to_string(), "src/lib.rs".to_string()]);

        let (n, argv) = formatter_for(wt, Path::new("pkg/app.go")).unwrap();
        assert_eq!(n, "gofmt");
        assert_eq!(
            argv,
            vec![
                "gofmt".to_string(),
                "-w".to_string(),
                "pkg/app.go".to_string()
            ]
        );

        let (n, _) = formatter_for(wt, Path::new("m/x.py")).unwrap();
        assert_eq!(n, "black");

        let (n, argv) = formatter_for(wt, Path::new("web/app.ts")).unwrap();
        assert_eq!(n, "prettier");
        assert!(
            argv[0].ends_with("node_modules/.bin/prettier"),
            "got: {argv:?}"
        );

        assert!(formatter_for(wt, Path::new("data.bin")).is_none());
        assert!(formatter_for(wt, Path::new("noext")).is_none());
    }

    #[test]
    fn table_has_no_duplicate_extension_claims() {
        let mut seen = std::collections::BTreeSet::new();
        for (_, exts, _) in FORMATTERS {
            for e in *exts {
                assert!(seen.insert(*e), "duplicate formatter for {e}");
            }
        }
    }

    #[test]
    fn off_switch_and_doubt_note() {
        let dir = std::env::temp_dir().join(format!("pheobe-fmt-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        // off-switch (BRO-93 precedent: BRO_FORMAT=off)
        unsafe { std::env::set_var("PHEOBE_FORMAT", "off") };
        assert!(format_on_write(&dir, Path::new("src/lib.rs")).is_none());
        unsafe { std::env::remove_var("PHEOBE_FORMAT") };

        // unmapped extension → silent skip even when enabled
        assert!(format_on_write(&dir, Path::new("data.bin")).is_none());

        // formatter failure = doubt note, never a tool failure: fake prettier
        std::fs::create_dir_all(dir.join("node_modules/.bin")).unwrap();
        crate::tests::write_shim(
            &dir.join("node_modules/.bin"),
            "prettier",
            "echo 'boom' >&2\nexit 3\n",
        );
        let note = format_on_write(&dir, Path::new("web/app.js")).unwrap();
        assert!(
            note.contains("doubt") && note.contains("prettier") && note.contains("boom"),
            "got: {note}"
        );

        std::fs::remove_dir_all(&dir).ok();
    }
}

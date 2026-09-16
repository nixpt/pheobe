//! Knowledge drive mechanics (PHEOBE-29): the seed corpus ships in the
//! binary, `ctx seed` lands it, and the brief carries the bodies that match
//! the repo — a passport is useless as a title alone.
use crate::knowledge;
use std::path::PathBuf;

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("pheobe-know-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn seed_list_matches_knowledge_dir() {
    let mut on_disk: Vec<String> =
        std::fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/knowledge"))
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| n.ends_with(".md") && n != "README.md")
            .collect();
    on_disk.sort();
    let mut embedded: Vec<String> = knowledge::SEED.iter().map(|(n, _)| n.to_string()).collect();
    embedded.sort();
    assert_eq!(
        embedded, on_disk,
        "add new knowledge/*.md files to knowledge::SEED"
    );
}

#[test]
fn seed_writes_then_keeps_unless_forced() {
    let dir = scratch("seed");
    let (w, k) = knowledge::seed(&dir, false).unwrap();
    assert_eq!((w, k), (knowledge::SEED.len(), 0));
    std::fs::write(
        dir.join("rust.md"),
        "---\nslug: rust\nname: mine\n---\nedited\n",
    )
    .unwrap();
    let (w, k) = knowledge::seed(&dir, false).unwrap();
    assert_eq!(
        (w, k),
        (0, knowledge::SEED.len()),
        "a user's edits are kept"
    );
    assert!(std::fs::read_to_string(dir.join("rust.md"))
        .unwrap()
        .contains("edited"));
    let (w, _) = knowledge::seed(&dir, true).unwrap();
    assert_eq!(w, knowledge::SEED.len());
    assert!(!std::fs::read_to_string(dir.join("rust.md"))
        .unwrap()
        .contains("edited"));
    let entries = knowledge::scan(&dir).unwrap();
    assert_eq!(
        entries.len(),
        knowledge::SEED.len(),
        "every seeded file parses"
    );
    assert!(
        entries.iter().all(|e| !e.tags.is_empty()),
        "every seed entry carries tags"
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn repo_signals_read_the_root() {
    let repo = scratch("signals");
    assert!(knowledge::repo_signals(&repo).is_empty());
    std::fs::write(repo.join("Cargo.toml"), "[package]\n").unwrap();
    std::fs::write(repo.join("build.sh"), "#!/bin/sh\n").unwrap();
    let s = knowledge::repo_signals(&repo);
    assert!(s.contains(&"rust") && s.contains(&"shell"), "{s:?}");
    assert!(!s.contains(&"python"));
    std::fs::remove_dir_all(&repo).ok();
}

#[test]
fn brief_injects_matching_bodies_and_paths_for_the_rest() {
    let drive = scratch("brief-drive");
    knowledge::seed(&drive, false).unwrap();
    let repo = scratch("brief-repo");
    std::fs::write(repo.join("Cargo.toml"), "[package]\n").unwrap();
    let entries = knowledge::scan(&drive).unwrap();
    let b = knowledge::brief(&entries, Some(&repo));
    // the rust passport's body is in the prompt …
    assert!(b.contains("### Rust"), "{b}");
    assert!(
        b.contains("Use the lockfile as version truth"),
        "rust body injected"
    );
    // … the python one is header + path only …
    assert!(b.contains("### Python"));
    assert!(
        !b.contains("Use the venv, don't create a second one"),
        "python body must not be injected for a rust repo"
    );
    assert!(b.contains(&format!("source: {}", drive.join("python.md").display())));
    // … and with no repo, nothing but headers.
    let none = knowledge::brief(&entries, None);
    assert!(!none.contains("Use the lockfile as version truth"));
    std::fs::remove_dir_all(&drive).ok();
    std::fs::remove_dir_all(&repo).ok();
}

//! pheobe — CLI surface. v0.1: the loop is scaffolded with the mechanical
//! stages live (intake, orient assembly, verify gate, worktree, report); the
//! model turn is the one unwired piece, marked clearly.

use anyhow::{bail, Result};
use clap::{Parser, Subcommand};
use pheobe::{knowledge, plan, report, task, verify, worktree};
use std::path::{Path, PathBuf};

#[derive(Parser, Debug)]
#[command(name = "pheobe", about = "A coding workhorse with no face.", version)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Run the loop on a task (self mode). JSON report on stdout.
    Run {
        /// Task file (JSON) or `-` for stdin
        task_file: String,
        /// Override the branch name
        #[arg(long)]
        branch: Option<String>,
    },
    /// Run the done_when + allowlist gate only (host mode's exit gate)
    Verify {
        /// Task file (JSON) or `-` for stdin
        task_file: String,
        /// Working copy to verify (defaults to cwd)
        #[arg(long)]
        worktree: Option<String>,
    },
    /// Knowledge drive: brief the prompt for a repo
    Ctx {
        #[command(subcommand)]
        cmd: CtxCmd,
    },
    /// Print/validate an adoption kit
    Adopt {
        #[command(subcommand)]
        cmd: AdoptCmd,
    },
    /// Check the environment: endpoint, worktree primitives, optional tools
    Doctor,
}

#[derive(Subcommand, Debug)]
enum CtxCmd {
    /// List drive entries
    List,
    /// The prompt-injectable brief block for a repo
    Brief {
        /// Emit entries relevant to this repo
        #[arg(long)]
        for_repo: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
enum AdoptCmd {
    /// Print the claude-code subagent kit (self mode)
    Claude,
    /// Print the opencode agent kit (host mode)
    Opencode,
    /// Print the codex bash-tool invocation (self mode)
    Codex,
}

fn main() {
    if let Err(e) = run_cmd() {
        eprintln!("pheobe: {e:#}");
        std::process::exit(1);
    }
}

fn run_cmd() -> Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Run { task_file, branch } => cmd_run(&task_file, branch),
        Cmd::Verify { task_file, worktree } => cmd_verify(&task_file, worktree),
        Cmd::Ctx { cmd } => cmd_ctx(cmd),
        Cmd::Adopt { cmd } => cmd_adopt(cmd),
        Cmd::Doctor => cmd_doctor(),
    }
}

fn cmd_run(task_file: &str, branch: Option<String>) -> Result<()> {
    let task = task::load(task_file)?;
    let repo = task.resolve_repo()?;
    let branch = branch.or(task.branch.clone()).unwrap_or_else(|| format!("pheobe/{}", task_slug(&task.task)));
    #[allow(unused_variables)]
    let task_id = task_slug(&task.task);

    if task.worktree {
        if Path::new(&repo).join(".git").is_dir() {
            bail!(
                "refusing to cook in the primary source checkout — pass --worktree or set \
                 worktree:true (the source checkout is shared: mom's kitchen rule)"
            );
        }
    }
    let (wt, branch) = worktree::provision(&repo, &branch)?;
    eprintln!("🍳 worktree: {}  branch: {branch}", wt.display());

    // orient: knowledge drive brief (repo-local + global drives)
    let entries = knowledge::load_all(Some(&wt))?;
    let _brief = knowledge::brief(&entries);

    // plan file seeded with the task; steps filled by the model turn
    plan::save(&wt, &plan::Plan { task: task.task.clone(), steps: vec![] })?;

    // THE MODEL TURN IS THE ONE UNWIRED PIECE (v0.2): uno feature / OpenAI-shaped
    // endpoint. Until wired, the mechanical stages still run — and the run
    // reports honestly instead of pretending.
    let report = report::HandoffReport {
        ok: false,
        task: task.task.clone(),
        branch: Some(branch),
        worktree: Some(wt.display().to_string()),
        commits: vec![],
        tests: None,
        summary: None,
        next_steps: vec!["wire the model endpoint (PHEOBE_BASE_URL/PHEOBE_MODEL) and rerun".into()],
        doubts: vec![],
        blocked: Some("model loop not wired in v0.1 — mechanical stages live, turn loop pending".into()),
        usage: None,
    };
    report::emit(&report)
}

fn cmd_verify(task_file: &str, worktree: Option<String>) -> Result<()> {
    let task = task::load(task_file)?;
    let cwd = worktree.map(PathBuf::from).unwrap_or_else(|| std::env::current_dir().unwrap());
    let ev = verify::run_done_when(&task, &cwd)?;
    let violations = worktree::check_allowlist(&cwd, &task.paths_allow)?;
    if !ev.passed || !violations.is_empty() {
        for v in violations {
            eprintln!("  ✋ outside paths_allow: {v}");
        }
        bail!("verify failed");
    }
    println!("✅ verify passed: {}", ev.ran);
    Ok(())
}

fn cmd_ctx(cmd: CtxCmd) -> Result<()> {
    match cmd {
        CtxCmd::Brief { for_repo } => {
            let repo = for_repo.as_deref().map(Path::new);
            let entries = knowledge::load_all(repo)?;
            if entries.is_empty() {
                eprintln!("research drive is empty — seed ~/.pheobe/knowledge/*.md");
                return Ok(());
            }
            print!("{}", knowledge::brief(&entries));
        }
        CtxCmd::List => {
            let entries = knowledge::load_all(None)?;
            for e in &entries {
                println!(
                    "{:16} {:9} {:32} {}",
                    e.slug,
                    e.kind,
                    e.name,
                    if e.stale { "STALE" } else { "fresh" }
                );
            }
        }
    }
    Ok(())
}

fn cmd_adopt(cmd: AdoptCmd) -> Result<()> {
    match cmd {
        AdoptCmd::Claude => print!("{}", include_str!("../adopt/claude/self-agent.md")),
        AdoptCmd::Opencode => print!("{}", include_str!("../adopt/opencode/host-agent.md")),
        AdoptCmd::Codex => print!("{}", include_str!("../adopt/codex/README.md")),
    }
    Ok(())
}

fn cmd_doctor() -> Result<()> {
    let endpoint = std::env::var("PHEOBE_BASE_URL").is_ok() && std::env::var("PHEOBE_MODEL").is_ok();
    println!("endpoint:     {}", if endpoint { "configured (PHEOBE_BASE_URL + PHEOBE_MODEL)" } else { "NOT configured" });
    for (name, hint) in [
        ("kitchen", "worktree primitive (preferred)"),
        ("buckets", "worktree primitive (fallback)"),
        ("polydex", "structural code intelligence (optional)"),
        ("code-atlas", "structural edit protocol (optional)"),
        ("dejavue", "repo memory (optional)"),
    ] {
        let found = std::process::Command::new("sh")
            .args(["-c", &format!("command -v {name} >/dev/null 2>&1")])
            .status()
            .is_ok_and(|s| s.success());
        println!("{:14} {}", format!("{name}:"), if found { hint_ok(hint) } else { "missing".to_string() });
    }
    Ok(())
}

fn hint_ok(hint: &str) -> String {
    format!("ok ({hint})")
}

fn task_slug(t: &str) -> String {
    t.chars()
        .take(24)
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_lowercase()
}

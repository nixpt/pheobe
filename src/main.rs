//! pheobe — CLI surface. v0.1: the loop is scaffolded with the mechanical
//! stages live (intake, orient assembly, verify gate, worktree, report); the
//! model turn is the one unwired piece, marked clearly.

use anyhow::{bail, Result};
use clap::{Parser, Subcommand};
use pheobe::{agent, checkpoint, knowledge, learn, llm, plan, report, task, verify, worktree};
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
    /// Closed-loop learning store (borrowed from joker's learning module)
    Learn {
        #[command(subcommand)]
        cmd: LearnCmd,
    },
    /// Print/validate an adoption kit
    Adopt {
        #[command(subcommand)]
        cmd: AdoptCmd,
    },
    /// Named working-state snapshots (git-stash plumbing; create never touches the tree)
    Check {
        #[command(subcommand)]
        cmd: CheckCmd,
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

#[derive(Subcommand, Debug)]
enum CheckCmd {
    /// Snapshot the current dirty state under a name
    Create { name: String },
    /// List checkpoints
    List,
    /// Apply a checkpoint's snapshot back onto the working tree
    Restore { name: String },
    /// Keep only the newest N checkpoints
    Prune {
        #[arg(long, default_value_t = 5)]
        keep: usize,
    },
}

#[derive(Subcommand, Debug)]
enum LearnCmd {
    /// Store a learned nudge scoped to a repo
    Nudge {
        /// Repo path this gotcha applies to
        #[arg(long)]
        repo: String,
        /// The lesson / gotcha text
        text: String,
        /// Optional trigger terms (comma-separated) that should resurface it
        #[arg(long)]
        terms: Option<String>,
    },
    /// List stored nudges for a repo
    Nudges {
        #[arg(long)]
        repo: String,
    },
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
        Cmd::Learn { cmd } => cmd_learn(cmd),
        Cmd::Adopt { cmd } => cmd_adopt(cmd),
        Cmd::Check { cmd } => cmd_check(cmd),
        Cmd::Doctor => cmd_doctor(),
    }
}

fn cmd_run(task_file: &str, branch: Option<String>) -> Result<()> {
    let task = task::load(task_file)?;
    let repo = task.resolve_repo()?;
    let branch = branch.or(task.branch.clone()).unwrap_or_else(|| format!("pheobe/{}", task_slug(&task.task)));
    let task_id = task_slug(&task.task);

    if !task.worktree && Path::new(&repo).join(".git").is_dir() {
        bail!(
            "refusing to cook in the primary source checkout — set worktree:true (default) \
             or point repo at a worktree (the source checkout is shared: mom's kitchen rule)"
        );
    }
    let (wt, branch) = worktree::provision(&repo, &branch)?;
    eprintln!("🍳 worktree: {}  branch: {branch}", wt.display());
    let repo_str = repo.display().to_string();
    let session = learn::begin_session(&repo, &task.task)?;

    // orient: knowledge drive brief (repo-local + global drives) + learned nudges
    let entries = knowledge::load_all(Some(&wt))?;
    let brief = knowledge::brief(&entries);
    let nudges = learn::nudges_for(&repo_str);
    if !nudges.is_empty() {
        eprintln!("📚 {} learned nudge(s) for this repo", nudges.len());
    }

    // plan file seeded; the model refines it through the loop
    plan::save(&wt, &plan::Plan { task: task.task.clone(), steps: vec![] })?;

    // the model turn (PHEOBE_BASE_URL / PHEOBE_MODEL / PHEOBE_API_KEY)
    let provider = llm::OpenAi::from_env()?;
    let ttl = task.ttl.as_deref().map(pheobe::aging::parse_ttl).transpose()?;
    let cfg = agent::LoopCfg {
        max_turns: task.budget.as_ref().map(|b| b.max_iterations * 8).unwrap_or(60),
        ttl,
        max_usd: task.budget.as_ref().and_then(|b| b.max_usd),
        usd_per_mtok: std::env::var("PHEOBE_USD_PER_MTOK").ok().and_then(|v| v.parse().ok()),
    };
    let outcome = agent::run(&provider, &task, &wt, &task_id, &brief, &nudges, &cfg)?;

    // mechanical gates run after the loop and have the final word over the model
    let mut commits = vec![];
    let dirty = worktree::status_dirty(&wt)?;
    if outcome.ok && dirty {
        let (violations, byproducts) = worktree::check_allowlist(&wt, &task.paths_allow)?;
        if !byproducts.is_empty() {
            eprintln!("📝 bash-run byproducts (uncommitted, staged out): {}", byproducts.join(", "));
        }
        if !violations.is_empty() {
            let _ = learn::end_session(session.as_ref(), "allowlist_violation", 2);
            let rep = report::HandoffReport::failure(&task.task, &format!("paths outside paths_allow: {}", violations.join(", ")));
            report::emit(&rep)?;
            bail!("run blocked by allowlist violations");
        }
        let sha = worktree::commit(&wt, &task_id, outcome.summary.as_deref().unwrap_or(&task.task), &task.paths_allow)?;
        commits.push(sha);
    }

    let tests = if outcome.ok || dirty {
        Some(verify::run_done_when(&task, &wt)?)
    } else {
        None
    };
    let ok = outcome.ok && tests.as_ref().map(|t| t.passed).unwrap_or(true);
    if !ok {
        eprintln!("❌ done_when failed");
    }
    let _ = learn::end_session(session.as_ref(), if ok { "done" } else { "failed" }, if ok { 0 } else { 1 });

    let rep = report::HandoffReport {
        ok,
        task: task.task.clone(),
        branch: Some(branch),
        worktree: Some(wt.display().to_string()),
        commits,
        tests,
        summary: outcome.summary,
        next_steps: outcome.next_steps,
        doubts: outcome.doubts,
        blocked: if ok { None } else { outcome.blocked.or(Some("done_when failed".into())) },
        usage: Some(report::Usage { turns: outcome.usage.turns, usd: None }),
    };
    if task.push && ok {
        worktree::push(&wt, rep.branch.clone().unwrap_or_default().as_str())?;
    }
    report::emit(&rep)
}

fn cmd_verify(task_file: &str, worktree: Option<String>) -> Result<()> {
    let task = task::load(task_file)?;
    let cwd = worktree.map(PathBuf::from).unwrap_or_else(|| std::env::current_dir().unwrap());
    let ev = verify::run_done_when(&task, &cwd)?;
    let (violations, _byproducts) = worktree::check_allowlist(&cwd, &task.paths_allow)?;
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

fn cmd_learn(cmd: LearnCmd) -> Result<()> {
    match cmd {
        LearnCmd::Nudge { repo, text, terms } => {
            learn::store_nudge(&repo, &text, terms.as_deref())?;
            println!("📚 nudge stored for {repo}");
        }
        LearnCmd::Nudges { repo } => {
            let nudges = learn::nudges_for(&repo);
            if nudges.is_empty() {
                println!("no nudges for {repo}");
            } else {
                for n in nudges {
                    println!("  · {n}");
                }
            }
        }
    }
    Ok(())
}

fn cmd_check(cmd: CheckCmd) -> Result<()> {
    let wt = std::env::current_dir()?;
    match cmd {
        CheckCmd::Create { name } => println!("checkpoint: {}", checkpoint::create(&wt, &name)?),
        CheckCmd::List => {
            let cps = checkpoint::list(&wt)?;
            if cps.is_empty() {
                println!("no checkpoints in {}", wt.join(".pheobe").join("checkpoints.json").display());
                return Ok(());
            }
            println!("{:<20} {:<12} {:<12} {:<12}", "NAME", "TS", "HEAD", "STASH");
            for c in cps {
                println!(
                    "{:<20} {:<12} {:<12} {:<12}",
                    c.name,
                    c.ts,
                    c.head.chars().take(8).collect::<String>(),
                    c.stash
                        .as_deref()
                        .map(|s| s.chars().take(8).collect::<String>())
                        .unwrap_or_else(|| "none".into())
                );
            }
        }
        CheckCmd::Restore { name } => println!("checkpoint: {}", checkpoint::restore(&wt, &name)?),
        CheckCmd::Prune { keep } => println!("checkpoint: {}", checkpoint::prune(&wt, keep)?),
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

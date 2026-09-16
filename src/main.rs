//! pheobe — CLI surface. v0.1: the loop is scaffolded with the mechanical
//! stages live (intake, orient assembly, verify gate, worktree, report); the
//! model turn is the one unwired piece, marked clearly.

use anyhow::{bail, Result};
use clap::{Parser, Subcommand};
use pheobe::{checkpoint, host, knowledge, learn, report, run, task, verify, worktree};
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
        /// Emit the report as JSON on stdout. This is already the only output
        /// mode; the flag exists so the adopt kits' `pheobe run … --json`
        /// is a valid invocation.
        #[arg(long)]
        json: bool,
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
        cmd: Option<AdoptCmd>,
    },
    /// Host-mode supervisor: provision a worktree, then gate the exit
    Host {
        #[command(subcommand)]
        cmd: HostCmd,
    },
    /// Named working-state snapshots (git-stash plumbing; create never touches the tree)
    Check {
        #[command(subcommand)]
        cmd: CheckCmd,
    },
    /// Check the environment: endpoint, worktree primitives, optional tools
    Doctor,
    /// ACP (Agent Client Protocol) server over stdio (PHEOBE-17). A dispatch
    /// prompt carrying a pheobe task JSON starts a run; progress streams as
    /// session/update notifications and the report JSON returns as the final
    /// session message. The round-trip peer is `bro synapse dispatch`.
    Acp {
        /// Explicit stdio transport (the default) — accepted for CLI parity
        /// with ACP peers like `bro acp --stdio`.
        #[arg(long)]
        stdio: bool,
    },
}

#[derive(Subcommand, Debug)]
enum HostCmd {
    /// Intake + provision a worktree; JSON {ok, worktree, branch, task} on stdout
    Setup {
        /// Task file (JSON) or `-` for stdin
        task_file: String,
        /// Override the branch name
        #[arg(long)]
        branch: Option<String>,
    },
    /// done_when + paths_allow in the worktree; JSON {ok, tests, violations}; exit 0/1
    Finish {
        /// Task file (JSON) or `-` for stdin
        task_file: String,
        /// Working copy to gate (defaults to cwd)
        #[arg(long)]
        worktree: Option<String>,
    },
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
    /// Print the claude Agent SDK subagent snippet (self mode)
    ClaudeSdk,
    /// Print the opencode self-mode agent kit (dispatcher)
    OpencodeSelf,
    /// Print the opencode host-mode agent kit
    Opencode,
    /// Print the codex bash-tool invocation + Python SDK snippet (self mode)
    Codex,
    /// Print the cursor host-mode kit (SDK + IDE agent + CLI sandbox map)
    Cursor,
    /// Print the kimi code def + provider config kit (self mode)
    Kimi,
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
        Cmd::Run {
            task_file, branch, ..
        } => cmd_run(&task_file, branch),
        Cmd::Verify {
            task_file,
            worktree,
        } => cmd_verify(&task_file, worktree),
        Cmd::Ctx { cmd } => cmd_ctx(cmd),
        Cmd::Learn { cmd } => cmd_learn(cmd),
        Cmd::Adopt { cmd } => cmd_adopt(cmd),
        Cmd::Check { cmd } => cmd_check(cmd),
        Cmd::Doctor => cmd_doctor(),
        Cmd::Acp { stdio: _ } => cmd_acp(),
        Cmd::Host { cmd } => cmd_host(cmd),
    }
}

fn cmd_acp() -> Result<()> {
    learn::init(); // PHEOBE_MEMORY=none|local|host selects the store (PHEOBE-16)
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(pheobe::acp::serve_stdio())
}

fn cmd_run(task_file: &str, branch: Option<String>) -> Result<()> {
    learn::init(); // PHEOBE_MEMORY=none|local|host selects the store (PHEOBE-16)
    let task = task::load(task_file)?;
    let rep = run::run_task(&task, branch.as_deref(), &|stage| eprintln!("{stage}"))?;
    report::emit(&rep)
}

fn cmd_verify(task_file: &str, worktree: Option<String>) -> Result<()> {
    let task = task::load(task_file)?;
    let cwd = worktree
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap());
    let ev = verify::run_done_when(&task, &cwd)?;
    let (violations, _byproducts) = worktree::check_allowlist(&cwd, &task.paths_allow)?;
    if !ev.passed || !violations.is_empty() {
        if !ev.passed {
            // issue 08: the host is a model reading this — name the command,
            // the exit, and what it printed, or it has to re-run it by hand
            eprintln!("✗ done_when failed: {}", ev.ran);
            for line in ev
                .output_excerpt
                .as_deref()
                .unwrap_or("")
                .lines()
                .filter(|l| !l.trim().is_empty())
            {
                eprintln!("  {line}");
            }
        }
        for v in violations {
            eprintln!("  ✋ outside paths_allow: {v}");
        }
        bail!("verify failed");
    }
    println!("✅ verify passed: {}", ev.ran);
    Ok(())
}

fn cmd_host(cmd: HostCmd) -> Result<()> {
    match cmd {
        HostCmd::Setup { task_file, branch } => {
            let task = task::load(&task_file)?;
            let setup = host::setup(&task, branch.as_deref())?;
            println!("{}", serde_json::to_string_pretty(&setup)?);
            Ok(())
        }
        HostCmd::Finish {
            task_file,
            worktree,
        } => {
            let task = task::load(&task_file)?;
            let wt = host::resolve_worktree(worktree);
            let finish = host::finish(&task, &wt)?;
            println!("{}", serde_json::to_string_pretty(&finish)?);
            if !finish.ok {
                std::process::exit(1);
            }
            Ok(())
        }
    }
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

fn cmd_adopt(cmd: Option<AdoptCmd>) -> Result<()> {
    match cmd {
        None => print!("{}", include_str!("../adopt/README.md")),
        Some(AdoptCmd::Claude) => print!("{}", include_str!("../adopt/claude/self-agent.md")),
        Some(AdoptCmd::ClaudeSdk) => print!("{}", include_str!("../adopt/claude/sdk-snippet.md")),
        Some(AdoptCmd::OpencodeSelf) => {
            print!("{}", include_str!("../adopt/opencode/self-agent.md"));
            print!(
                "{}",
                include_str!("../adopt/opencode/opencode.jsonc-snippet.md")
            );
        }
        Some(AdoptCmd::Opencode) => print!("{}", include_str!("../adopt/opencode/pheobe-host.md")),
        Some(AdoptCmd::Codex) => {
            print!("{}", include_str!("../adopt/codex/README.md"));
            print!("{}", include_str!("../adopt/codex/sdk-snippet.md"));
        }
        Some(AdoptCmd::Cursor) => {
            print!("{}", include_str!("../adopt/cursor/def.md"));
            print!("{}", include_str!("../adopt/cursor/pheobe-host.md"));
        }
        Some(AdoptCmd::Kimi) => print!("{}", include_str!("../adopt/kimi/def.md")),
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
                println!(
                    "no checkpoints in {}",
                    wt.join(".pheobe").join("checkpoints.json").display()
                );
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
    let endpoint =
        std::env::var("PHEOBE_BASE_URL").is_ok() && std::env::var("PHEOBE_MODEL").is_ok();
    println!(
        "endpoint:     {}",
        if endpoint {
            "configured (PHEOBE_BASE_URL + PHEOBE_MODEL)"
        } else {
            "NOT configured"
        }
    );
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
        println!(
            "{:14} {}",
            format!("{name}:"),
            if found {
                hint_ok(hint)
            } else {
                "missing".to_string()
            }
        );
    }
    Ok(())
}

fn hint_ok(hint: &str) -> String {
    format!("ok ({hint})")
}

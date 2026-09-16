//! The turn loop — intake is done, this is orient→plan→implement→verify→
//! iterate→handoff as plain tool-calls. Termination: the `handoff` terminal
//! tool, max_turns, or an error. The mechanical gates (done_when, allowlist,
//! commit) run *after* the loop and have the final word over the model's
//! claims — the machine checks everything mechanical.

use crate::aging::{self, Ladder, State};
use crate::llm::{Msg, Provider, ToolCall, Usage};
use crate::tools::{self, ToolCtx};
use crate::task::Task;
use anyhow::Result;
use serde_json::Value;
use std::path::Path;
use std::time::Instant;

pub struct RunOutcome {
    pub ok: bool,
    pub summary: Option<String>,
    pub next_steps: Vec<String>,
    pub doubts: Vec<String>,
    pub blocked: Option<String>,
    pub usage: Usage,
    pub history: Vec<Msg>,
}

pub struct LoopCfg {
    pub max_turns: u32,
    /// Hard lifespan (mayfly ladder); a run past it is a task-design failure.
    pub ttl: Option<std::time::Duration>,
    /// USD budget; needs a price signal to be enforceable.
    pub max_usd: Option<f64>,
    /// Cost estimate input: dollars per million tokens (from PHEOBE_USD_PER_MTOK).
    pub usd_per_mtok: Option<f64>,
}

impl Default for LoopCfg {
    fn default() -> Self {
        LoopCfg { max_turns: 60, ttl: None, max_usd: None, usd_per_mtok: None }
    }
}

pub fn run(
    provider: &dyn Provider,
    task: &Task,
    wt: &Path,
    task_id: &str,
    brief: &str,
    nudges: &[String],
    cfg: &LoopCfg,
) -> Result<RunOutcome> {
    let ctx = ToolCtx { wt, task, task_id };
    let barn = tools::barn(&ctx);
    let schemas = tools::schemas(&barn);

    let mut system = String::new();
    system.push_str(include_str!("../persona/pheobe.md"));
    system.push_str("\n\n---\n\n## Report contract (the handoff tool takes exactly this)\n");
    system.push_str(r#"{"ok":bool,"summary":"what changed","next_steps":[actions for the parent],"doubts":[unverified assumptions],"blocked":"reason if blocked"}"#);
    system.push_str("\n\n## Task\n");
    system.push_str(&format!(
        "task: {}\ndone_when: `{}`\npaths_allow: {:?}\ntask_id: {task_id}\n",
        task.task,
        match &task.done_when {
            crate::task::DoneWhen::Command { run, expect_exit } => format!("{run} [expect exit {expect_exit}]"),
        },
        task.paths_allow
    ));
    if !brief.trim().is_empty() {
        system.push_str("\n\n");
        system.push_str(brief);
    }
    if !nudges.is_empty() {
        system.push_str("\n\n## Learned nudges for this repo (earned by past runs)\n");
        for n in nudges {
            system.push_str(&format!("- {n}\n"));
        }
    }
    system.push_str(
        "\n\n## Rules\n\
         - Work only inside the worktree and paths_allow. \"While I'm here\" is a bug.\n\
         - Plan first with plan_tracker; update it as you go. Record every\n  \
         unverified assumption as a doubt — never argue one away.\n\
         - Prefer the edit that removes a special case. Minimal diff.\n\
         - Verify with the verify tool; trust failing output over confidence.\n\
         - Two failed repairs on one failure = the plan is wrong; go back to planning.\n\
         - END ONLY by calling handoff. When done_when passes, hand off.\n",
    );

    let mut history = vec![Msg::system(system), Msg::user(format!("Begin. task_id={task_id}"))];
    let ladder = Ladder::new(cfg.ttl);
    let started = Instant::now();
    let mut turns = 0;
    let mut total_tokens = 0u64;
    let mut delivered_warn = false;
    let mut delivered_narrow = false;
    let mut hard_stop: Option<String> = None;
    let mut handoff: Option<Value> = None;

    while turns < cfg.max_turns {
        // aging ladder: hard stop past 100%; one inject per transition
        match ladder.state_at(started.elapsed()) {
            State::Expired => {
                hard_stop = Some(format!(
                    "ttl_exceeded — the deadline hit without done_when (task-design failure, \
                     not a time problem): ttl={:?}",
                    cfg.ttl
                ));
                break;
            }
            State::Warn if !delivered_warn => {
                delivered_warn = true;
                history.push(Msg::user(aging::WARN_INJECT));
            }
            State::Narrow if !delivered_narrow => {
                delivered_narrow = true;
                history.push(Msg::user(aging::NARROW_INJECT));
            }
            _ => {}
        }
        // USD budget (token-estimated when a price signal exists)
        if let (Some(max_usd), Some(rate)) = (cfg.max_usd, cfg.usd_per_mtok) {
            let usd = total_tokens as f64 * rate / 1_000_000.0;
            if usd >= max_usd {
                hard_stop = Some(format!(
                    "budget_exceeded — ~${usd:.2} of ${max_usd:.2} spent ({total_tokens} tokens)"
                ));
                break;
            }
        }

        let (msg, tokens) = provider.chat(&history, &schemas)?;
        total_tokens += tokens.unwrap_or(0);
        turns += 1;
        crate::learn::log_event(&format!("t{turns}"), "turn", None, msg.content.as_deref());

        if msg.tool_calls.is_empty() {
            history.push(msg);
            break;
        }
        history.push(msg.clone());
        for call in &msg.tool_calls {
            let result = dispatch_logged(&barn, &ctx, call);
            history.push(Msg::tool_result(&call.id, result));
        }
        if msg.tool_calls.iter().any(|c| c.function.name == "handoff") {
            for c in &msg.tool_calls {
                if c.function.name == "handoff" {
                    handoff = Some(serde_json::from_str(&c.function.arguments).unwrap_or_else(|_| {
                        serde_json::json!({"ok": false, "summary": "handoff arguments unparseable", "blocked": "handoff JSON malformed"})
                    }));
                }
            }
            break;
        }
    }

    let handoff = match handoff {
        Some(h) => h,
        None if hard_stop.is_some() => serde_json::json!({
            "ok": false,
            "summary": "",
            "blocked": hard_stop.clone(),
            "doubts": [],
            "next_steps": ["re-scope the task: shorter, tighter done_when, or a bigger budget"],
        }),
        None => serde_json::json!({
            "ok": false,
            "summary": history.iter().filter_map(|m| m.content.as_deref()).last().unwrap_or("").to_string(),
            "blocked": format!("max_turns ({}) reached without handoff", cfg.max_turns),
            "doubts": [],
            "next_steps": [],
        }),
    };

    Ok(RunOutcome {
        ok: handoff["ok"].as_bool().unwrap_or(false),
        summary: handoff["summary"].as_str().map(|s| s.to_string()),
        next_steps: str_vec(&handoff, "next_steps"),
        doubts: str_vec(&handoff, "doubts"),
        blocked: handoff["blocked"].as_str().map(|s| s.to_string()),
        usage: Usage { turns, total_tokens: Some(total_tokens) },
        history,
    })
}

fn dispatch_logged(
    barn: &[crate::tools::Tool],
    ctx: &ToolCtx<'_>,
    call: &ToolCall,
) -> String {
    let args: Value = serde_json::from_str(&call.function.arguments).unwrap_or(Value::Null);
    let name = call.function.name.as_str();
    match crate::tools::dispatch(barn, ctx, name, &args) {
        Ok(out) => out,
        Err(e) => format!("tool error: {e:#}"),
    }
}

fn str_vec(v: &Value, key: &str) -> Vec<String> {
    v[key]
        .as_array()
        .map(|a| a.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect())
        .unwrap_or_default()
}

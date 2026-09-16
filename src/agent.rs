//! The turn loop — intake is done, this is orient→plan→implement→verify→
//! iterate→handoff as plain tool-calls. Termination: the `handoff` terminal
//! tool, max_turns, or an error. The mechanical gates (done_when, allowlist,
//! commit) run *after* the loop and have the final word over the model's
//! claims — the machine checks everything mechanical.

use crate::aging::{self, Ladder, State};
use crate::llm::{Msg, Provider, ToolCall, Usage};
use crate::tools::{self, ToolCtx};
use crate::worker::{Worker, WorkerOutcome};
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
    let system = build_prompt(task, task_id, brief, nudges);
    let ctx = ToolCtx { wt, task, task_id };
    let barn = tools::barn(&ctx);
    let schemas = tools::schemas(&barn);

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
            "summary": history.iter().filter_map(|m| m.content.as_deref()).next_back().unwrap_or("").to_string(),
            "blocked": format!("max_turns ({}) reached without handoff", cfg.max_turns),
            "doubts": [],
            "next_steps": [],
        }),
    };

    Ok(outcome_from_handoff(&handoff, turns, Some(total_tokens), history))
}

/// The system prompt both dispatch paths share: the persona, the report
/// contract, the task block, the brief, learned nudges, the rules.
pub fn build_prompt(task: &Task, task_id: &str, brief: &str, nudges: &[String]) -> String {
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
    system
}

/// The Worker path (PHEOBE-9): one prompt out (the same system prompt the
/// loop builds), one whole run back. The aging ladder and budget guards
/// wrap the worker call — deadline check before AND after; the per-turn
/// loop is skipped entirely. Mechanical gates (allowlist, commit, done_when)
/// still run in cmd_run afterwards and have the final word.
pub fn run_worker(
    worker: &dyn Worker,
    task: &Task,
    wt: &Path,
    task_id: &str,
    brief: &str,
    nudges: &[String],
    cfg: &LoopCfg,
) -> Result<RunOutcome> {
    let ladder = Ladder::new(cfg.ttl);
    let started = Instant::now();
    let prompt = build_prompt(task, task_id, brief, nudges);

    let ttl_block = |why: &str| {
        serde_json::json!({
            "ok": false,
            "summary": "",
            "blocked": format!(
                "ttl_exceeded — the deadline hit without done_when (task-design failure, \
                 not a time problem): ttl={:?}{why}",
                cfg.ttl
            ),
            "doubts": [],
            "next_steps": ["re-scope the task: shorter, tighter done_when, or a bigger budget"],
        })
    };
    let budget_block = |spent: f64, tokens: u64| {
        serde_json::json!({
            "ok": false,
            "summary": "",
            "blocked": format!("budget_exceeded — ~${spent:.2} spent ({tokens} tokens)"),
            "doubts": [],
            "next_steps": ["re-scope the task: shorter, tighter done_when, or a bigger budget"],
        })
    };

    // deadline check BEFORE the worker call
    if let State::Expired = ladder.state_at(started.elapsed()) {
        return Ok(outcome_from_handoff(&ttl_block(""), 0, None, vec![Msg::system(prompt.as_str())]));
    }
    // budget check BEFORE the call (nothing spent yet; a zero budget blocks)
    if let (Some(max_usd), Some(_rate)) = (cfg.max_usd, cfg.usd_per_mtok) {
        if 0.0 >= max_usd {
            return Ok(outcome_from_handoff(&budget_block(0.0, 0), 0, None, vec![Msg::system(prompt.as_str())]));
        }
    }

    let res = worker.run(&format!("{prompt}\n\nBegin. task_id={task_id}"), wt);

    // deadline check AFTER the worker call — the engine's internal loop has
    // no visibility into pheobe's ladder, so an engine that overruns its ttl
    // gets its result ignored and the run reports blocked.
    if let State::Expired = ladder.state_at(started.elapsed()) {
        return Ok(outcome_from_handoff(
            &ttl_block(" — the engine's whole run landed past the deadline"),
            1,
            None,
            vec![Msg::system(prompt.as_str())],
        ));
    }

    let wo = res?;
    // budget guard AFTER: a single worker call that blows the budget is cut
    if let (Some(max_usd), Some(rate)) = (cfg.max_usd, cfg.usd_per_mtok) {
        let tokens = wo.tokens.unwrap_or(0);
        let spent = wo.usd.unwrap_or(tokens as f64 * rate / 1_000_000.0);
        if spent >= max_usd {
            return Ok(outcome_from_handoff(
                &budget_block(spent, tokens),
                1,
                Some(tokens),
                vec![Msg::system(prompt.as_str())],
            ));
        }
    }

    let handoff = normalize_worker_outcome(&wo);
    Ok(outcome_from_handoff(
        &handoff,
        1,
        wo.tokens,
        vec![Msg::system(prompt.as_str()), Msg::assistant(wo.final_text.as_str(), vec![])],
    ))
}

/// Report normalization (PHEOBE-9): if the engine's json_tail parses, merge
/// ONLY summary/next_steps/doubts from it — branch/commits/tests stay
/// mechanical (filled in cmd_run). If only final_text, summary = final_text.
/// (ok/blocked are part of the handoff contract the prompt asks for, so an
/// explicit ok:false or blocked in the tail is honored; branch/commits/tests
/// keys in the tail are ignored.)
fn normalize_worker_outcome(wo: &WorkerOutcome) -> serde_json::Value {
    let mut handoff = serde_json::json!({
        "ok": true,
        "summary": wo.final_text,
        "doubts": [],
        "next_steps": [],
    });
    if let Some(tail) = &wo.json_tail {
        if let Some(ok) = tail.get("ok").and_then(|b| b.as_bool()) {
            handoff["ok"] = serde_json::Value::Bool(ok);
        }
        if let Some(b) = tail.get("blocked").and_then(|b| b.as_str()) {
            handoff["blocked"] = serde_json::Value::String(b.to_string());
            handoff["ok"] = serde_json::Value::Bool(false);
        }
        if let Some(s) = tail.get("summary").and_then(|s| s.as_str()) {
            handoff["summary"] = serde_json::Value::String(s.to_string());
        }
        if let Some(ns) = tail.get("next_steps") {
            if ns.is_array() {
                handoff["next_steps"] = ns.clone();
            }
        }
        if let Some(d) = tail.get("doubts") {
            if d.is_array() {
                handoff["doubts"] = d.clone();
            }
        }
    }
    handoff
}

fn outcome_from_handoff(
    handoff: &serde_json::Value,
    turns: u32,
    tokens: Option<u64>,
    history: Vec<Msg>,
) -> RunOutcome {
    RunOutcome {
        ok: handoff["ok"].as_bool().unwrap_or(false),
        summary: handoff["summary"].as_str().map(|s| s.to_string()),
        next_steps: str_vec(handoff, "next_steps"),
        doubts: str_vec(handoff, "doubts"),
        blocked: handoff["blocked"].as_str().map(|s| s.to_string()),
        usage: Usage { turns, total_tokens: tokens },
        history,
    }
}

fn dispatch_logged(
    barn: &[crate::tools::Tool],
    ctx: &ToolCtx<'_>,
    call: &ToolCall,
) -> String {    let args: Value = serde_json::from_str(&call.function.arguments).unwrap_or(Value::Null);
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

//! The aging ladder + budgets (PHEOBE-2): mayfly-style ttl parsing, the
//! warn/narrow/expired table, and the loop's hard stops (expired ttl, USD
//! budget).

use super::scripted::{script_call, ScriptedProvider};
use crate::agent::LoopCfg;
use crate::aging::{parse_ttl, Ladder, State};
use crate::llm::Msg;
use crate::task::Task;
use std::time::Duration;

#[test]
fn ttl_parses_mayfly_style_durations() {
    assert_eq!(parse_ttl("45m").unwrap(), Duration::from_secs(2700));
    assert_eq!(parse_ttl("2h").unwrap(), Duration::from_secs(7200));
    assert_eq!(parse_ttl("90s").unwrap(), Duration::from_secs(90));
    assert_eq!(parse_ttl("1d").unwrap(), Duration::from_secs(86400));
    assert_eq!(parse_ttl("0s").unwrap(), Duration::ZERO);
    assert!(parse_ttl("45x").is_err());
    assert!(parse_ttl("nope").is_err());
}

#[test]
fn ladder_states_match_the_mayfly_table() {
    let ladder = Ladder::new(Some(Duration::from_secs(100)));
    assert_eq!(ladder.state_at(Duration::from_secs(10)), State::Alive);
    assert_eq!(ladder.state_at(Duration::from_secs(55)), State::Warn);
    assert_eq!(ladder.state_at(Duration::from_secs(80)), State::Narrow);
    assert_eq!(ladder.state_at(Duration::from_secs(100)), State::Expired);
    // no ttl = eternal worker (the loop's other guards still apply)
    assert_eq!(
        Ladder::new(None).state_at(Duration::from_secs(9999)),
        State::Alive
    );
    // injectables exist for the live transitions only
    assert!(Ladder::inject(State::Warn).unwrap().contains("Narrow"));
    assert!(Ladder::inject(State::Narrow)
        .unwrap()
        .contains("final push"));
    assert!(Ladder::inject(State::Expired).is_none());
}

#[test]
fn loop_hard_stops_on_expired_ttl() {
    let root = std::env::temp_dir().join(format!("pheobe-ttl-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let task: Task =
        serde_json::from_str(r#"{"task":"x","done_when":{"type":"command","run":"true"}}"#)
            .unwrap();
    let provider = ScriptedProvider {
        // endless turns — the ladder must cut them off
        turns: std::sync::Mutex::new(
            (0..50)
                .map(|i| {
                    Msg::assistant(
                        "",
                        vec![script_call(
                            &format!("c{i}"),
                            "bash",
                            r#"{"command":"true"}"#,
                        )],
                    )
                })
                .collect(),
        ),
        seen_tool_calls: std::sync::Mutex::new(vec![]),
    };
    let cfg = LoopCfg {
        ttl: Some(Duration::ZERO),
        ..Default::default()
    };
    let out = crate::agent::run(&provider, &task, &root, "ttl", "", &[], &cfg).unwrap();
    assert!(!out.ok);
    let blocked = out.blocked.unwrap();
    assert!(blocked.contains("ttl_exceeded"), "got: {blocked}");
    assert!(blocked.contains("task-design failure"));
    assert_eq!(out.usage.turns, 0); // cut off before any turn ran
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn loop_stops_on_usd_budget() {
    let root = std::env::temp_dir().join(format!("pheobe-usd-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let task: Task =
        serde_json::from_str(r#"{"task":"x","done_when":{"type":"command","run":"true"}}"#)
            .unwrap();
    let provider = ScriptedProvider {
        turns: std::sync::Mutex::new(
            (0..50)
                .map(|i| {
                    Msg::assistant(
                        "",
                        vec![script_call(
                            &format!("c{i}"),
                            "bash",
                            r#"{"command":"true"}"#,
                        )],
                    )
                })
                .collect(),
        ),
        seen_tool_calls: std::sync::Mutex::new(vec![]),
    };
    // mock reports 42 tokens/turn; rate $2500/Mtok → $0.105/turn → capped ~turn 20
    let cfg = LoopCfg {
        max_usd: Some(2.0),
        usd_per_mtok: Some(2500.0),
        ..Default::default()
    };
    let out = crate::agent::run(&provider, &task, &root, "usd", "", &[], &cfg).unwrap();
    assert!(!out.ok);
    let blocked = out.blocked.unwrap();
    assert!(blocked.contains("budget_exceeded"), "got: {blocked}");
    assert!(
        out.usage.turns <= 25,
        "budget must cut the run early, got {} turns",
        out.usage.turns
    );
    std::fs::remove_dir_all(&root).ok();
}

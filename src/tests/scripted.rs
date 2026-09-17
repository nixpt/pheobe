//! The scripted in-process provider (loop tests without an endpoint) and
//! the tool-call literal helper — shared by the agent_loop and aging suites.

use crate::llm::{Msg, Provider, ToolCall, ToolFn};

pub(super) struct ScriptedProvider {
    pub(super) turns: std::sync::Mutex<Vec<Msg>>,
    #[allow(dead_code)] // written per-turn below; read only by tests that want it
    pub(super) seen_tool_calls: std::sync::Mutex<Vec<String>>,
}

impl Provider for ScriptedProvider {
    fn chat(
        &self,
        _messages: &[Msg],
        _tools: &[crate::llm::ToolSchema],
    ) -> anyhow::Result<(Msg, Option<u64>)> {
        let mut turns = self.turns.lock().unwrap();
        let next = if turns.is_empty() {
            Msg::assistant("done", vec![])
        } else {
            turns.remove(0)
        };
        Ok((next, Some(42)))
    }
}

pub(super) fn script_call(id: &str, name: &str, args: &str) -> ToolCall {
    ToolCall {
        id: id.into(),
        kind: "function".into(),
        function: ToolFn {
            name: name.into(),
            arguments: args.into(),
        },
    }
}

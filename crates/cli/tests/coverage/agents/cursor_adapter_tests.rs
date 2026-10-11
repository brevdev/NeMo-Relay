// SPDX-FileCopyrightText: Copyright (c) 2026, NVIDIA CORPORATION & AFFILIATES. All rights reserved.
// SPDX-License-Identifier: Apache-2.0

use super::*;

fn adapt_one(payload: Value) -> AdapterOutcome {
    adapt(payload, &HeaderMap::new())
}

#[test]
fn a_response_hook_becomes_a_model_step_with_its_usage() {
    let outcome = adapt_one(json!({
        "hook_event_name": "afterAgentResponse",
        "conversation_id": "conv-1",
        "generation_id": "gen-1",
        "model": "composer-2",
        "text": "done",
        "input_tokens": 1200,
        "output_tokens": 40,
        "cache_read_tokens": 1000,
        "cache_write_tokens": 0
    }));
    let [NormalizedEvent::LlmObserved(event)] = outcome.events.as_slice() else {
        panic!("expected one observed model step, got {:?}", outcome.events);
    };
    assert_eq!(event.session_id, "conv-1");
    assert_eq!(event.agent_kind, AgentKind::Cursor);
    assert_eq!(event.model.as_deref(), Some("composer-2"));
    assert_eq!(event.generation_id.as_deref(), Some("gen-1"));
    assert_eq!(event.text.as_deref(), Some("done"));
    assert_eq!(event.reasoning, None);
    assert_eq!(
        event.usage,
        Some(json!({
            "input_tokens": 1200,
            "output_tokens": 40,
            "cache_read_input_tokens": 1000,
            "cache_creation_input_tokens": 0
        }))
    );
    assert_eq!(outcome.response, json!({ "continue": true }));
}

#[test]
fn a_thought_hook_carries_reasoning_and_no_response() {
    let outcome = adapt_one(json!({
        "hook_event_name": "afterAgentThought",
        "conversation_id": "conv-1",
        "model": "composer-2",
        "text": "check the tests first",
        "duration_ms": 900
    }));
    let [NormalizedEvent::LlmObserved(event)] = outcome.events.as_slice() else {
        panic!("expected one observed event, got {:?}", outcome.events);
    };
    assert_eq!(event.text, None);
    assert_eq!(event.reasoning.as_deref(), Some("check the tests first"));
    assert_eq!(event.usage, None);
}

#[test]
fn tool_hooks_open_and_close_a_tool_span_and_are_always_allowed() {
    let start = adapt_one(json!({
        "hook_event_name": "preToolUse",
        "conversation_id": "conv-1",
        "tool_name": "Shell",
        "tool_use_id": "call-1",
        "tool_input": { "command": "ls" }
    }));
    assert!(
        matches!(start.events.as_slice(), [NormalizedEvent::ToolStarted(event)]
        if event.tool_call_id == "call-1" && event.tool_name == "Shell")
    );
    assert_eq!(
        start.response,
        json!({ "continue": true, "permission": "allow" })
    );

    let end = adapt_one(json!({
        "hook_event_name": "postToolUse",
        "conversation_id": "conv-1",
        "tool_name": "Shell",
        "tool_use_id": "call-1",
        "tool_input": { "command": "ls" },
        "tool_output": "README.md"
    }));
    assert!(
        matches!(end.events.as_slice(), [NormalizedEvent::ToolEnded(event)]
        if event.tool_call_id == "call-1" && event.result == json!("README.md"))
    );
}

#[test]
fn shell_hooks_are_marks_not_second_tool_spans() {
    // The same execution is reported by preToolUse/postToolUse; a second span would double it.
    let outcome = adapt_one(json!({
        "hook_event_name": "beforeShellExecution",
        "conversation_id": "conv-1",
        "command": "ls"
    }));
    assert!(matches!(
        outcome.events.as_slice(),
        [NormalizedEvent::HookMark(_)]
    ));
    assert_eq!(
        outcome.response,
        json!({ "continue": true, "permission": "allow" })
    );
}

#[test]
fn stop_closes_the_turn_and_a_prompt_opens_one() {
    let stop = adapt_one(
        json!({ "hook_event_name": "stop", "conversation_id": "conv-1", "status": "completed" }),
    );
    assert!(
        stop.events
            .iter()
            .any(|event| matches!(event, NormalizedEvent::TurnEnded(_)))
    );
    let prompt = adapt_one(
        json!({ "hook_event_name": "beforeSubmitPrompt", "conversation_id": "conv-1", "prompt": "fix it" }),
    );
    assert!(
        prompt
            .events
            .iter()
            .any(|event| matches!(event, NormalizedEvent::PromptSubmitted(_)))
    );
}

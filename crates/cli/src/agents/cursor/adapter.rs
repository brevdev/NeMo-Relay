// SPDX-FileCopyrightText: Copyright (c) 2026, NVIDIA CORPORATION & AFFILIATES. All rights reserved.
// SPDX-License-Identifier: Apache-2.0

//! Cursor CLI hook adapter.
//!
//! Cursor sends its model traffic through Cursor's own backend, which the gateway cannot proxy,
//! so everything Relay records comes from Cursor's hooks. `afterAgentResponse` and
//! `afterAgentThought` carry each model step (response text, thinking, model and token counts)
//! and become [`NormalizedEvent::LlmObserved`]. `preToolUse`/`postToolUse` carry tool calls
//! with their inputs and outputs. The shell and MCP hooks report the same executions as
//! `preToolUse`/`postToolUse`, so they are kept as marks rather than opened as second spans.

use axum::http::HeaderMap;
use serde_json::{Map, Value, json};

use crate::agents::shared::adapters::{
    AdapterOutcome, AgentPayloadExtractor, CURSOR_PAYLOAD_EXTRACTOR, ClassificationRules, classify,
    fallback_session_id, normalize_name,
};
use crate::events::{AgentKind, LlmObservedEvent, NormalizedEvent};

pub(crate) fn adapt(payload: Value, headers: &HeaderMap) -> AdapterOutcome {
    let response = response_for(&payload);
    if let Some(event) = observed_llm(&payload, headers) {
        return AdapterOutcome {
            events: vec![NormalizedEvent::LlmObserved(event)],
            response,
            permission: None,
        };
    }
    let events = classify(
        &payload,
        headers,
        &CURSOR_PAYLOAD_EXTRACTOR,
        &ClassificationRules {
            kind: AgentKind::Cursor,
            agent_start: &["sessionStart"],
            agent_end: &["sessionEnd"],
            subagent_start: &["subagentStart"],
            subagent_end: &["subagentStop"],
            tool_start: &["preToolUse"],
            tool_end: &["postToolUse", "postToolUseFailure"],
            turn_start: &[],
            turn_end: &["stop"],
            compaction: &["preCompact"],
        },
    );
    AdapterOutcome {
        events,
        response,
        permission: None,
    }
}

/// Cursor reads a `permission` from the hooks that can block an action, and `continue` from the
/// rest. Relay observes Cursor; it never blocks, so every hook is allowed to proceed.
fn response_for(payload: &Value) -> Value {
    let name = CURSOR_PAYLOAD_EXTRACTOR
        .event_name(payload)
        .map(|name| normalize_name(&name))
        .unwrap_or_default();
    if matches!(
        name.as_str(),
        "pretooluse" | "beforeshellexecution" | "beforemcpexecution" | "beforereadfile"
    ) {
        json!({ "continue": true, "permission": "allow" })
    } else {
        json!({ "continue": true })
    }
}

/// A model step from `afterAgentResponse`, or the thinking ahead of one from `afterAgentThought`.
fn observed_llm(payload: &Value, headers: &HeaderMap) -> Option<LlmObservedEvent> {
    let event_name = CURSOR_PAYLOAD_EXTRACTOR.event_name(payload)?;
    let thought = match normalize_name(&event_name).as_str() {
        "afteragentresponse" => false,
        "afteragentthought" => true,
        _ => return None,
    };
    let string = |key: &str| payload.get(key).and_then(Value::as_str).map(str::to_owned);
    let text = string("text");
    let session_id = CURSOR_PAYLOAD_EXTRACTOR
        .session_id(payload, headers)
        .unwrap_or_else(fallback_session_id);
    Some(LlmObservedEvent {
        metadata: CURSOR_PAYLOAD_EXTRACTOR.metadata(
            payload,
            headers,
            AgentKind::Cursor,
            &event_name,
        ),
        session_id,
        agent_kind: AgentKind::Cursor,
        event_name,
        model: string("model").filter(|model| !model.is_empty()),
        generation_id: string("generation_id").filter(|id| !id.is_empty()),
        reasoning: if thought { text.clone() } else { None },
        text: if thought {
            None
        } else {
            Some(text.unwrap_or_default())
        },
        usage: if thought { None } else { usage(payload) },
    })
}

/// Cursor's token counts, renamed to the keys ATIF reads.
fn usage(payload: &Value) -> Option<Value> {
    let mut usage = Map::new();
    for (from, to) in [
        ("input_tokens", "input_tokens"),
        ("output_tokens", "output_tokens"),
        ("cache_read_tokens", "cache_read_input_tokens"),
        ("cache_write_tokens", "cache_creation_input_tokens"),
    ] {
        if let Some(count) = payload.get(from).filter(|value| value.is_u64()) {
            usage.insert(to.into(), count.clone());
        }
    }
    (!usage.is_empty()).then_some(Value::Object(usage))
}

#[cfg(test)]
#[path = "../../../tests/coverage/agents/cursor_adapter_tests.rs"]
mod tests;

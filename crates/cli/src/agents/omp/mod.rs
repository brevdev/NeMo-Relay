// SPDX-FileCopyrightText: Copyright (c) 2026, NVIDIA CORPORATION & AFFILIATES. All rights reserved.
// SPDX-License-Identifier: Apache-2.0

//! omp (oh-my-pi) coding-agent identity and compatibility policy.
//!
//! omp is a fork of pi that keeps pi's extension API, so it loads the same NeMo Relay
//! extension (see `agents::pi`), installed into its own agent directory. The extension
//! detects that it runs inside omp and posts to `/hooks/omp`. omp has no `model_select` or
//! `agent_settled` hook, so those two are absent from the set below; the extension
//! re-selects the session's model itself after redirecting its provider.

use semver::Version;

use super::AgentDescriptor;

pub(super) const DESCRIPTOR: AgentDescriptor = AgentDescriptor {
    argument: "omp",
    install_argument: "omp",
    label: "omp",
    executable: "omp",
    hook_path: "/hooks/omp",
    version_product: "omp",
    minimum_version: (18, 8, 0),
    verified_through: Some((18, 8)),
    hook_events: &[
        "session_start",
        "session_shutdown",
        "session_before_compact",
        "session_compact",
        "agent_start",
        "agent_end",
        "turn_start",
        "turn_end",
        "tool_call",
        "tool_execution_end",
        "tool_arguments_transformed",
        "user_bash",
        "user_bash_end",
        "model_redirect",
    ],
};

/// `omp --version` prints `omp/<semver>`.
pub(super) fn parse_version(raw: &str) -> Option<Version> {
    Version::parse(raw.trim().strip_prefix("omp/")?).ok()
}

#[cfg(test)]
#[path = "../../../tests/coverage/agents/omp_tests.rs"]
mod tests;

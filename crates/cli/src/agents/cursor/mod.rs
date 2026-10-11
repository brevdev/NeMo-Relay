// SPDX-FileCopyrightText: Copyright (c) 2026, NVIDIA CORPORATION & AFFILIATES. All rights reserved.
// SPDX-License-Identifier: Apache-2.0

//! Cursor CLI (`cursor-agent`) identity, compatibility policy and hook install.
//!
//! Cursor is observed through its hooks only. Its model requests go through Cursor's own
//! backend, so there is no gateway redirect: model steps come from `afterAgentResponse` and
//! `afterAgentThought`, which carry the response, thinking, model and token counts but not the
//! request Cursor's servers built. Hooks are installed into Cursor's user `hooks.json` and
//! forward to the shared gateway that `nemo-relay gateway` keeps running.

use semver::Version;

use super::AgentDescriptor;

pub(crate) mod install;

pub(super) const DESCRIPTOR: AgentDescriptor = AgentDescriptor {
    argument: "cursor",
    install_argument: "cursor",
    label: "Cursor",
    executable: "cursor-agent",
    hook_path: "/hooks/cursor",
    version_product: "Cursor CLI",
    minimum_version: (2026, 10, 1),
    verified_through: Some((2026, 10)),
    hook_events: &[
        "sessionStart",
        "sessionEnd",
        "beforeSubmitPrompt",
        "afterAgentThought",
        "afterAgentResponse",
        "preToolUse",
        "postToolUse",
        "postToolUseFailure",
        "beforeShellExecution",
        "afterShellExecution",
        "beforeMCPExecution",
        "afterMCPExecution",
        "afterFileEdit",
        "preCompact",
        "stop",
        "subagentStart",
        "subagentStop",
    ],
};

/// `cursor-agent --version` prints a dated build such as `2026.10.01-e373342`; the suffix is a
/// commit, not a pre-release, so only the date is compared.
pub(super) fn parse_version(raw: &str) -> Option<Version> {
    let date = raw.trim().split(['-', ' ']).next()?;
    let mut parts = date.split('.').map(|part| part.parse::<u64>().ok());
    let (year, month, day) = (parts.next()??, parts.next()??, parts.next()??);
    if parts.next().is_some() {
        return None;
    }
    Some(Version::new(year, month, day))
}

#[cfg(test)]
#[path = "../../../tests/coverage/agents/cursor_tests.rs"]
mod tests;

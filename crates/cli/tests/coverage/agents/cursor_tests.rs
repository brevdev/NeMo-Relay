// SPDX-FileCopyrightText: Copyright (c) 2026, NVIDIA CORPORATION & AFFILIATES. All rights reserved.
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn parses_the_dated_build_and_ignores_the_commit_suffix() {
    // `cursor-agent --version` prints `2026.10.01-e373342`; the suffix is a commit, not a
    // pre-release, so it must not make the build compare as older than its date.
    assert_eq!(
        parse_version("2026.10.01-e373342"),
        Some(Version::new(2026, 10, 1))
    );
    assert_eq!(
        parse_version("  2026.10.01-e373342\n"),
        Some(Version::new(2026, 10, 1))
    );
    assert_eq!(parse_version("2026.11.03"), Some(Version::new(2026, 11, 3)));
}

#[test]
fn rejects_output_that_is_not_a_dated_build() {
    assert_eq!(parse_version(""), None);
    assert_eq!(parse_version("cursor-agent"), None);
    assert_eq!(parse_version("2026.10"), None);
    assert_eq!(parse_version("2026.10.01.4"), None);
}

#[test]
fn descriptor_routes_cursor_agent_to_the_cursor_hook_endpoint() {
    assert_eq!(DESCRIPTOR.hook_path, "/hooks/cursor");
    assert_eq!(DESCRIPTOR.executable, "cursor-agent");
    for event in [
        "afterAgentResponse",
        "afterAgentThought",
        "preToolUse",
        "postToolUse",
        "stop",
    ] {
        assert!(DESCRIPTOR.hook_events.contains(&event), "{event}");
    }
}

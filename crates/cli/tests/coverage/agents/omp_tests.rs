// SPDX-FileCopyrightText: Copyright (c) 2026, NVIDIA CORPORATION & AFFILIATES. All rights reserved.
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn parses_product_prefixed_version_output() {
    // `omp --version` prints `omp/18.8.7`, unlike pi's bare semver.
    assert_eq!(parse_version("omp/18.8.7"), Some(Version::new(18, 8, 7)));
    assert_eq!(
        parse_version("  omp/18.8.7\n"),
        Some(Version::new(18, 8, 7))
    );
}

#[test]
fn rejects_unprefixed_or_empty_version_output() {
    assert_eq!(parse_version("18.8.7"), None);
    assert_eq!(parse_version("omp 18.8.7"), None);
    assert_eq!(parse_version(""), None);
}

#[test]
fn descriptor_routes_to_the_omp_hook_endpoint() {
    assert_eq!(DESCRIPTOR.hook_path, "/hooks/omp");
    assert_eq!(DESCRIPTOR.executable, "omp");
}

#[test]
fn hook_events_omit_the_two_pi_hooks_omp_lacks() {
    // omp's extension API has no `model_select` or `agent_settled`; the extension never posts
    // `model_select` anyway, and `agent_settled` cannot fire under omp.
    assert!(DESCRIPTOR.hook_events.contains(&"tool_call"));
    assert!(DESCRIPTOR.hook_events.contains(&"model_redirect"));
    assert!(!DESCRIPTOR.hook_events.contains(&"agent_settled"));
}

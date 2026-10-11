// SPDX-FileCopyrightText: Copyright (c) 2026, NVIDIA CORPORATION & AFFILIATES. All rights reserved.
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::test_support::EnvScope;

const OURS: &str = "/usr/local/bin/nemo-relay hook-forward cursor --forward-only --fail-open";

#[test]
fn adding_keeps_other_entries_and_is_idempotent() {
    let mut hooks = json!({
        "version": 1,
        "hooks": { "afterFileEdit": [{ "command": "./format.sh" }] }
    });
    let added = add_entries(&mut hooks, OURS).unwrap();
    assert_eq!(added, DESCRIPTOR.hook_events.len());
    let edits = hooks["hooks"]["afterFileEdit"].as_array().unwrap();
    assert_eq!(edits[0], json!({ "command": "./format.sh" }));
    assert_eq!(edits[1], json!({ "command": OURS }));
    assert_eq!(add_entries(&mut hooks, OURS).unwrap(), 0);
}

#[test]
fn removing_takes_only_relay_entries_and_drops_emptied_events() {
    let mut hooks =
        json!({ "version": 1, "hooks": { "afterFileEdit": [{ "command": "./format.sh" }] } });
    add_entries(&mut hooks, OURS).unwrap();
    assert_eq!(
        remove_entries(&mut hooks, OURS),
        DESCRIPTOR.hook_events.len()
    );
    assert_eq!(
        hooks,
        json!({ "version": 1, "hooks": { "afterFileEdit": [{ "command": "./format.sh" }] } })
    );
}

#[test]
fn refuses_a_hooks_file_it_cannot_safely_edit() {
    assert!(add_entries(&mut json!([]), OURS).is_err());
    assert!(add_entries(&mut json!({ "hooks": [] }), OURS).is_err());
    assert!(add_entries(&mut json!({ "hooks": { "stop": {} } }), OURS).is_err());
}

#[test]
fn install_then_uninstall_round_trips_the_users_hooks_file() {
    let home = tempfile::tempdir().unwrap();
    let _scope = EnvScope::set(&[("HOME", Some(home.path().as_os_str()))]);
    let cursor = home.path().join(".cursor");
    std::fs::create_dir_all(&cursor).unwrap();
    let original =
        json!({ "version": 1, "hooks": { "stop": [{ "command": "notify-send done" }] } });
    std::fs::write(cursor.join("hooks.json"), original.to_string()).unwrap();
    let servers = json!({ "mcpServers": { "github": { "command": "gh-mcp" } } });
    std::fs::write(cursor.join("mcp.json"), servers.to_string()).unwrap();

    let request = InstallRequest {
        install_dir: None,
        force: false,
        dry_run: false,
        skip_doctor: true,
    };
    assert_eq!(install(request).unwrap(), ExitCode::SUCCESS);
    assert!(is_installed());
    let installed: Value =
        serde_json::from_str(&std::fs::read_to_string(cursor.join("hooks.json")).unwrap()).unwrap();
    assert_eq!(installed["hooks"]["stop"].as_array().unwrap().len(), 2);
    let mcp: Value =
        serde_json::from_str(&std::fs::read_to_string(cursor.join("mcp.json")).unwrap()).unwrap();
    assert_eq!(mcp["mcpServers"]["github"], json!({ "command": "gh-mcp" }));
    assert_eq!(mcp["mcpServers"]["nemo-relay"]["args"], json!(["mcp"]));

    let removal = UninstallRequest {
        install_dir: None,
        force: false,
        dry_run: false,
    };
    assert_eq!(uninstall(removal).unwrap(), ExitCode::SUCCESS);
    assert!(!is_installed());
    let restored: Value =
        serde_json::from_str(&std::fs::read_to_string(cursor.join("hooks.json")).unwrap()).unwrap();
    assert_eq!(restored, original);
    let mcp: Value =
        serde_json::from_str(&std::fs::read_to_string(cursor.join("mcp.json")).unwrap()).unwrap();
    assert_eq!(mcp, servers);
}

#[test]
fn will_not_replace_an_mcp_server_of_the_same_name_it_did_not_write() {
    let mut mcp = json!({ "mcpServers": { "nemo-relay": { "command": "someone-else" } } });
    assert!(add_mcp_server(&mut mcp, mcp_server("/usr/local/bin/nemo-relay"), None).is_err());
}

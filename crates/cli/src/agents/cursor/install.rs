// SPDX-FileCopyrightText: Copyright (c) 2026, NVIDIA CORPORATION & AFFILIATES. All rights reserved.
// SPDX-License-Identifier: Apache-2.0

//! `nemo-relay install cursor`: Relay's entries in Cursor's user `hooks.json`.
//!
//! Cursor has no plugin marketplace; it runs the commands listed per event in `hooks.json`. The
//! install adds one entry per Relay-observed event that forwards the payload to the shared
//! gateway, and keeps every other entry as it was. Installed hooks only forward to the
//! authenticated gateway that `nemo-relay mcp` starts, so the install also registers that MCP
//! server in Cursor's `mcp.json`, as the Claude Code and Codex plugins do: Cursor starts it with
//! the session. It records what it added, so uninstall removes those entries and nothing else.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use serde_json::{Value, json};

use super::DESCRIPTOR;
use crate::error::CliError;
use crate::installation::{InstallRequest, UninstallRequest};

const HOOKS_FILE: &str = "hooks.json";
const MCP_FILE: &str = "mcp.json";
const MCP_SERVER: &str = "nemo-relay";
const STATE_FILE: &str = ".nemo-relay-install.json";

/// `~/.cursor`, where the Cursor CLI reads user hooks.
fn cursor_dir() -> Option<PathBuf> {
    Some(
        crate::agents::shared::host::home_dir()
            .ok()?
            .join(".cursor"),
    )
}

fn relay_binary() -> Result<String, CliError> {
    std::env::current_exe()
        .map(|path| path.display().to_string())
        .map_err(|error| CliError::Install(format!("cannot locate the nemo-relay binary: {error}")))
}

/// The MCP server that keeps the shared gateway running while Cursor runs.
fn mcp_server(relay: &str) -> Value {
    json!({
        "command": relay,
        "args": ["mcp"],
        "env": { "NEMO_RELAY_GATEWAY_BIND": crate::bootstrap::DEFAULT_BIND }
    })
}

/// The command Cursor runs for each event: this binary forwarding to the shared gateway.
fn hook_command() -> Result<String, CliError> {
    let relay = relay_binary()?;
    let quoted = if relay.contains([' ', '"', '\'']) {
        format!("\"{}\"", relay.replace('"', "\\\""))
    } else {
        relay
    };
    Ok(format!(
        "{quoted} hook-forward cursor --forward-only --fail-open"
    ))
}

fn read_json(path: &Path) -> Result<Option<Value>, CliError> {
    match std::fs::read_to_string(path) {
        Ok(raw) => serde_json::from_str(&raw).map(Some).map_err(|error| {
            CliError::Install(format!("{} is not valid JSON: {error}", path.display()))
        }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(CliError::Install(format!(
            "cannot read {}: {error}",
            path.display()
        ))),
    }
}

fn write_json(path: &Path, value: &Value) -> Result<(), CliError> {
    let parent = path.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent).map_err(|error| {
        CliError::Install(format!("cannot create {}: {error}", parent.display()))
    })?;
    let temp = path.with_extension("json.nemo-relay-tmp");
    let body = serde_json::to_string_pretty(value).expect("JSON values serialize") + "\n";
    std::fs::write(&temp, body)
        .and_then(|()| std::fs::rename(&temp, path))
        .map_err(|error| CliError::Install(format!("cannot write {}: {error}", path.display())))
}

/// Adds `command` to every Relay event in `hooks`, returning how many entries were added.
fn add_entries(hooks: &mut Value, command: &str) -> Result<usize, CliError> {
    let object = hooks
        .as_object_mut()
        .ok_or_else(|| CliError::Install("Cursor's hooks.json must be a JSON object".into()))?;
    object.entry("version").or_insert(json!(1));
    let events = object
        .entry("hooks")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(|| {
            CliError::Install("`hooks` in Cursor's hooks.json must be an object".into())
        })?;
    let mut added = 0;
    for event in DESCRIPTOR.hook_events {
        let entries = events
            .entry(*event)
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .ok_or_else(|| {
                CliError::Install(format!(
                    "`hooks.{event}` in Cursor's hooks.json must be an array"
                ))
            })?;
        if !entries
            .iter()
            .any(|entry| entry.get("command").and_then(Value::as_str) == Some(command))
        {
            entries.push(json!({ "command": command }));
            added += 1;
        }
    }
    Ok(added)
}

/// Removes every entry running `command`, dropping event lists it leaves empty.
fn remove_entries(hooks: &mut Value, command: &str) -> usize {
    let Some(events) = hooks.get_mut("hooks").and_then(Value::as_object_mut) else {
        return 0;
    };
    let mut removed = 0;
    for entries in events.values_mut().filter_map(Value::as_array_mut) {
        let before = entries.len();
        entries.retain(|entry| entry.get("command").and_then(Value::as_str) != Some(command));
        removed += before - entries.len();
    }
    events.retain(|_, entries| entries.as_array().is_none_or(|entries| !entries.is_empty()));
    removed
}

/// Registers Relay's MCP server, refusing to replace a server of the same name it did not write.
fn add_mcp_server(
    mcp: &mut Value,
    server: Value,
    previous: Option<&Value>,
) -> Result<(), CliError> {
    let servers = mcp
        .as_object_mut()
        .ok_or_else(|| CliError::Install("Cursor's mcp.json must be a JSON object".into()))?
        .entry("mcpServers")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(|| {
            CliError::Install("`mcpServers` in Cursor's mcp.json must be an object".into())
        })?;
    if let Some(existing) = servers.get(MCP_SERVER)
        && Some(existing) != previous
        && existing != &server
    {
        return Err(CliError::Install(format!(
            "Cursor's mcp.json already has a `{MCP_SERVER}` server that NeMo Relay did not write; \
             remove it and run the install again"
        )));
    }
    servers.insert(MCP_SERVER.into(), server);
    Ok(())
}

fn remove_mcp_server(mcp: &mut Value, recorded: &Value) {
    if let Some(servers) = mcp.get_mut("mcpServers").and_then(Value::as_object_mut)
        && servers.get(MCP_SERVER) == Some(recorded)
    {
        servers.remove(MCP_SERVER);
    }
}

fn recorded_command(dir: &Path) -> Result<Option<String>, CliError> {
    Ok(read_json(&dir.join(STATE_FILE))?.and_then(|state| {
        state
            .get("command")
            .and_then(Value::as_str)
            .map(str::to_owned)
    }))
}

pub(crate) fn is_installed() -> bool {
    cursor_dir().is_some_and(|dir| matches!(recorded_command(&dir), Ok(Some(_))))
}

pub(crate) fn install(request: InstallRequest) -> Result<ExitCode, CliError> {
    if request.install_dir.is_some() {
        return Err(CliError::Install(
            "`--install-dir` does not apply to Cursor: Cursor reads hooks from ~/.cursor/hooks.json".into(),
        ));
    }
    let dir = cursor_dir()
        .ok_or_else(|| CliError::Install("cannot resolve the home directory".into()))?;
    let path = dir.join(HOOKS_FILE);
    let mcp_path = dir.join(MCP_FILE);
    let command = hook_command()?;
    let server = mcp_server(&relay_binary()?);
    let state = read_json(&dir.join(STATE_FILE))?;
    let mut hooks = read_json(&path)?.unwrap_or_else(|| json!({ "version": 1, "hooks": {} }));
    let mut mcp = read_json(&mcp_path)?.unwrap_or_else(|| json!({ "mcpServers": {} }));
    // A previous install from another binary path would leave its entries pointing elsewhere.
    if let Some(previous) = recorded_command(&dir)?
        && previous != command
    {
        remove_entries(&mut hooks, &previous);
    }
    let added = add_entries(&mut hooks, &command)?;
    add_mcp_server(
        &mut mcp,
        server.clone(),
        state.as_ref().and_then(|state| state.get("mcp_server")),
    )?;
    if request.dry_run {
        println!(
            "Plan: add {added} NeMo Relay hook entries to {} and the `{MCP_SERVER}` server to {}",
            path.display(),
            mcp_path.display()
        );
        return Ok(ExitCode::SUCCESS);
    }
    write_json(&path, &hooks)?;
    write_json(&mcp_path, &mcp)?;
    write_json(
        &dir.join(STATE_FILE),
        &json!({
            "schema": 1,
            "relay_version": env!("CARGO_PKG_VERSION"),
            "command": command,
            "mcp_server": server
        }),
    )?;
    println!(
        "Installed NeMo Relay hooks for Cursor in {} ({added} entries added)",
        path.display()
    );
    println!(
        "  Cursor starts the `{MCP_SERVER}` MCP server, which keeps the shared gateway running; \
         a headless `cursor-agent -p` run needs `--approve-mcps` (or the server approved once)."
    );
    Ok(ExitCode::SUCCESS)
}

pub(crate) fn uninstall(request: UninstallRequest) -> Result<ExitCode, CliError> {
    if request.install_dir.is_some() {
        return Err(CliError::Install(
            "`--install-dir` does not apply to Cursor: Cursor reads hooks from ~/.cursor/hooks.json".into(),
        ));
    }
    let dir = cursor_dir()
        .ok_or_else(|| CliError::Install("cannot resolve the home directory".into()))?;
    let Some(command) = recorded_command(&dir)? else {
        return Err(CliError::Install(format!(
            "no NeMo Relay-managed Cursor hooks were found in {}",
            dir.display()
        )));
    };
    let path = dir.join(HOOKS_FILE);
    let mut hooks = read_json(&path)?.unwrap_or_else(|| json!({}));
    let removed = remove_entries(&mut hooks, &command);
    if request.dry_run {
        println!(
            "Plan: remove {removed} NeMo Relay hook entries from {}",
            path.display()
        );
        return Ok(ExitCode::SUCCESS);
    }
    write_json(&path, &hooks)?;
    let mcp_path = dir.join(MCP_FILE);
    if let (Some(mut mcp), Some(server)) = (
        read_json(&mcp_path)?,
        read_json(&dir.join(STATE_FILE))?.and_then(|state| state.get("mcp_server").cloned()),
    ) {
        remove_mcp_server(&mut mcp, &server);
        write_json(&mcp_path, &mcp)?;
    }
    std::fs::remove_file(dir.join(STATE_FILE))
        .map_err(|error| CliError::Install(format!("cannot remove the install record: {error}")))?;
    println!(
        "Removed {removed} NeMo Relay hook entries from {}",
        path.display()
    );
    Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
#[path = "../../../tests/coverage/agents/cursor_install_tests.rs"]
mod tests;

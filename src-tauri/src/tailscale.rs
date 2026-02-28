use crate::checker::{parse_update_output, RESTART_PACKAGES};
use serde::Serialize;
use std::collections::HashSet;
use tokio::process::Command;

const SSH_OPTS: &[&str] = &[
    "-o",
    "ServerAliveInterval=5",
    "-o",
    "ServerAliveCountMax=2",
    "-o",
    "BatchMode=yes",
    "-o",
    "StrictHostKeyChecking=no",
];

#[derive(Debug, Clone, Serialize)]
pub struct HostResult {
    pub hostname: String,
    pub updates: Vec<crate::checker::UpdateInfo>,
    pub needs_restart: bool,
    pub restart_packages: Vec<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RemoteCheckResult {
    pub hosts: Vec<HostResult>,
}

/// Get all unique tag names (without 'tag:' prefix) from Tailscale peers.
pub async fn discover_all_tags() -> Vec<String> {
    let output = match Command::new("tailscale")
        .args(["status", "--json"])
        .output()
        .await
    {
        Ok(o) if o.status.success() => o,
        _ => return Vec::new(),
    };

    let Ok(data) = serde_json::from_slice::<serde_json::Value>(&output.stdout) else {
        return Vec::new();
    };

    let mut tags = HashSet::new();
    if let Some(peers) = data.get("Peer").and_then(|p| p.as_object()) {
        for peer in peers.values() {
            if let Some(peer_tags) = peer.get("Tags").and_then(|t| t.as_array()) {
                for tag in peer_tags {
                    if let Some(t) = tag.as_str() {
                        let name = t.strip_prefix("tag:").unwrap_or(t);
                        tags.insert(name.to_string());
                    }
                }
            }
        }
    }

    let mut sorted: Vec<String> = tags.into_iter().collect();
    sorted.sort();
    sorted
}

/// Get online Tailscale peers whose tags contain ALL specified tags.
async fn discover_peers(tags: &[String]) -> Vec<String> {
    let output = match Command::new("tailscale")
        .args(["status", "--json"])
        .output()
        .await
    {
        Ok(o) if o.status.success() => o,
        _ => return Vec::new(),
    };

    let Ok(data) = serde_json::from_slice::<serde_json::Value>(&output.stdout) else {
        return Vec::new();
    };

    let required_tags: Vec<String> = tags.iter().map(|t| format!("tag:{t}")).collect();
    let mut hostnames = Vec::new();

    if let Some(peers) = data.get("Peer").and_then(|p| p.as_object()) {
        for peer in peers.values() {
            let online = peer
                .get("Online")
                .and_then(|o| o.as_bool())
                .unwrap_or(false);
            if !online {
                continue;
            }

            let peer_tags: Vec<String> = peer
                .get("Tags")
                .and_then(|t| t.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();

            if required_tags.iter().all(|t| peer_tags.contains(t)) {
                if let Some(hostname) = peer.get("HostName").and_then(|h| h.as_str()) {
                    if !hostname.is_empty() {
                        hostnames.push(hostname.to_string());
                    }
                }
            }
        }
    }

    hostnames.sort();
    hostnames
}

/// SSH into a host and run checkupdates.
async fn check_host(hostname: &str, timeout: u32) -> HostResult {
    let restart_set: HashSet<&str> = RESTART_PACKAGES.iter().copied().collect();

    let mut args: Vec<String> = vec![
        "-o".into(),
        format!("ConnectTimeout={timeout}"),
    ];
    for opt in SSH_OPTS {
        args.push(opt.to_string());
    }
    args.push(hostname.to_string());
    args.push("checkupdates".to_string());

    let total_timeout = std::time::Duration::from_secs((timeout + 30) as u64);

    let result = tokio::time::timeout(total_timeout, async {
        Command::new("ssh").args(&args).output().await
    })
    .await;

    match result {
        Ok(Ok(output)) => {
            // checkupdates: exit 0 = updates, exit 2 = no updates
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                if !stdout.trim().is_empty() {
                    let updates = parse_update_output(&stdout);
                    let restart_pkgs: Vec<String> = updates
                        .iter()
                        .filter(|u| restart_set.contains(u.package.as_str()))
                        .map(|u| u.package.clone())
                        .collect();
                    return HostResult {
                        hostname: hostname.to_string(),
                        needs_restart: !restart_pkgs.is_empty(),
                        restart_packages: restart_pkgs,
                        updates,
                        error: None,
                    };
                }
            }
            HostResult {
                hostname: hostname.to_string(),
                updates: Vec::new(),
                needs_restart: false,
                restart_packages: Vec::new(),
                error: None,
            }
        }
        Ok(Err(e)) => HostResult {
            hostname: hostname.to_string(),
            updates: Vec::new(),
            needs_restart: false,
            restart_packages: Vec::new(),
            error: Some(format!("SSH error: {e}")),
        },
        Err(_) => HostResult {
            hostname: hostname.to_string(),
            updates: Vec::new(),
            needs_restart: false,
            restart_packages: Vec::new(),
            error: Some("Connection timed out".to_string()),
        },
    }
}

/// Check all matching remote hosts concurrently (up to 8 at a time).
pub async fn check_remote(tags: &[String], timeout: u32) -> Result<RemoteCheckResult, String> {
    let hostnames = discover_peers(tags).await;

    if hostnames.is_empty() {
        return Ok(RemoteCheckResult { hosts: Vec::new() });
    }

    let mut set = tokio::task::JoinSet::new();
    for hostname in hostnames {
        let t = timeout;
        set.spawn(async move { check_host(&hostname, t).await });
    }

    let mut results = Vec::new();
    while let Some(res) = set.join_next().await {
        match res {
            Ok(host_result) => results.push(host_result),
            Err(e) => {
                log::error!("Task join error: {e}");
            }
        }
    }

    results.sort_by(|a, b| a.hostname.cmp(&b.hostname));
    Ok(RemoteCheckResult { hosts: results })
}

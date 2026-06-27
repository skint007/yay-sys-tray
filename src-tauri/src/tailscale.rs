use crate::checker::{
    kernel_package_for, package_requires_restart, package_url, parse_si_repositories,
    parse_update_output, UpdateInfo, KERNEL_PACKAGES,
};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use tauri::{AppHandle, Emitter};
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
pub async fn discover_peers(tags: &[String]) -> Vec<String> {
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

/// Build the SSH target (`user@host` or `host`).
pub fn ssh_target(hostname: &str, ssh_user: &str) -> String {
    if ssh_user.is_empty() {
        hostname.to_string()
    } else {
        format!("{ssh_user}@{hostname}")
    }
}

/// Run a command on a host over SSH (with a hard wall-clock timeout).
async fn ssh_run(target: &str, command: &str, timeout: u32) -> std::io::Result<std::process::Output> {
    let mut args: Vec<String> = vec!["-o".into(), format!("ConnectTimeout={timeout}")];
    for opt in SSH_OPTS {
        args.push(opt.to_string());
    }
    args.push(target.to_string());
    args.push(command.to_string());

    let total_timeout = std::time::Duration::from_secs((timeout + 30) as u64);
    match tokio::time::timeout(total_timeout, Command::new("ssh").args(&args).output()).await {
        Ok(res) => res,
        Err(_) => Err(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "Connection timed out",
        )),
    }
}

/// Determine the remote running kernel package — only queried when there are
/// kernel updates (the flavor is irrelevant otherwise).
async fn remote_kernel_package(target: &str, updates: &[UpdateInfo], timeout: u32) -> Option<String> {
    if !updates.iter().any(|u| KERNEL_PACKAGES.contains(&u.package.as_str())) {
        return None;
    }
    let out = ssh_run(target, "uname -r", timeout).await.ok()?;
    if out.status.success() {
        let release = String::from_utf8_lossy(&out.stdout).trim().to_string();
        Some(kernel_package_for(&release).to_string())
    } else {
        None
    }
}

/// Fetch each remote package's source repository via `pacman -Si` over SSH, so
/// remote update cards show the repo badge (core/extra/...) like local ones do.
async fn remote_repositories(
    target: &str,
    packages: &[String],
    timeout: u32,
) -> HashMap<String, (String, String)> {
    if packages.is_empty() {
        return HashMap::new();
    }
    let command = format!("pacman -Si {}", packages.join(" "));
    match ssh_run(target, &command, timeout).await {
        Ok(out) if out.status.success() => {
            parse_si_repositories(&String::from_utf8_lossy(&out.stdout))
        }
        _ => HashMap::new(),
    }
}

/// SSH into a host and run checkupdates.
async fn check_host(hostname: &str, timeout: u32, ssh_user: &str) -> HostResult {
    let target = ssh_target(hostname, ssh_user);
    let empty = |error: Option<String>| HostResult {
        hostname: hostname.to_string(),
        updates: Vec::new(),
        needs_restart: false,
        restart_packages: Vec::new(),
        error,
    };

    match ssh_run(&target, "checkupdates", timeout).await {
        Ok(output) => {
            // checkupdates: exit 0 = updates, exit 2 = no updates
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                if !stdout.trim().is_empty() {
                    let mut updates = parse_update_output(&stdout);
                    let running_pkg = remote_kernel_package(&target, &updates, timeout).await;

                    // Enrich with source repo + package URL (checkupdates omits these).
                    let pkg_names: Vec<String> =
                        updates.iter().map(|u| u.package.clone()).collect();
                    let repos = remote_repositories(&target, &pkg_names, timeout).await;
                    for u in &mut updates {
                        if let Some((repo, arch)) = repos.get(&u.package) {
                            u.repository = repo.clone();
                            u.url = package_url(repo, arch, &u.package);
                        }
                    }

                    let restart_pkgs: Vec<String> = updates
                        .iter()
                        .filter(|u| package_requires_restart(&u.package, running_pkg.as_deref()))
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
            empty(None)
        }
        Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {
            empty(Some("Connection timed out".to_string()))
        }
        Err(e) => empty(Some(format!("SSH error: {e}"))),
    }
}

/// Check the given remote hosts concurrently, emitting per-host scan progress
/// (`check-host-start` / `check-host-done`) so the Updates window can show a
/// live "checking" view.
pub async fn check_hosts(
    app_handle: &AppHandle,
    hostnames: Vec<String>,
    timeout: u32,
    ssh_user: &str,
) -> Vec<HostResult> {
    if hostnames.is_empty() {
        return Vec::new();
    }

    let mut set = tokio::task::JoinSet::new();
    for hostname in hostnames {
        let _ = app_handle.emit("check-host-start", &hostname);
        let t = timeout;
        let user = ssh_user.to_string();
        let handle = app_handle.clone();
        set.spawn(async move {
            let result = check_host(&hostname, t, &user).await;
            let _ = handle.emit(
                "check-host-done",
                serde_json::json!({
                    "key": result.hostname,
                    "count": result.updates.len(),
                    "needs_restart": result.needs_restart,
                    "error": result.error.is_some(),
                }),
            );
            result
        });
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
    results
}

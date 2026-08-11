use crate::aur::QmOutcome;
use crate::checker::{
    kernel_package_for, maybe_kernel_pkg, package_requires_restart, package_url,
    parse_package_descriptions, parse_si_repositories, parse_update_output, UpdateInfo,
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
    /// Why this host's AUR check failed, if it did. Its repo updates are still
    /// listed alongside; mirrors `CheckResult::aur_error` for the local check so
    /// an AUR problem never reads as "up to date".
    pub aur_error: Option<String>,
    /// The host has AUR updates but no yay to apply them with. Set only when the
    /// probe actually reached the host and the shell said the command is
    /// missing — a probe that couldn't run leaves this false, since claiming
    /// "not applicable" on a guess is worse than the update path saying so.
    pub aur_helper_missing: bool,
}

impl HostResult {
    /// Pending AUR updates this host cannot apply, because it has no AUR helper.
    /// Still listed in the window; just not work the user can do from here.
    pub fn blocked_aur_count(&self) -> usize {
        if !self.aur_helper_missing {
            return 0;
        }
        self.updates.iter().filter(|u| u.is_aur()).count()
    }

    /// Updates that can actually be applied on this host. This is the number the
    /// tray badge and every count-of-work label use: an ARM box whose only
    /// pending updates need an AUR helper it can't install is not work anyone
    /// can act on, and counting it would keep the badge lit forever.
    pub fn actionable_count(&self) -> usize {
        self.updates.len() - self.blocked_aur_count()
    }
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
pub async fn ssh_run(target: &str, command: &str, timeout: u32) -> std::io::Result<std::process::Output> {
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
    if !updates.iter().any(|u| maybe_kernel_pkg(&u.package)) {
        return None;
    }
    // Prefer the exact package from the running kernel's modules `pkgbase` file
    // (handles linux-cachyos, AUR kernels, etc.); fall back to sniffing the
    // release string if that file can't be read.
    if let Ok(out) = ssh_run(target, "cat \"/usr/lib/modules/$(uname -r)/pkgbase\"", timeout).await {
        if out.status.success() {
            let name = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !name.is_empty() {
                return Some(name);
            }
        }
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

/// Fetch each remote package's description from its installed package record,
/// matching the metadata shown for local updates.
async fn remote_descriptions(
    target: &str,
    packages: &[String],
    timeout: u32,
) -> HashMap<String, String> {
    if packages.is_empty() {
        return HashMap::new();
    }
    let command = format!("pacman -Qi {}", packages.join(" "));
    match ssh_run(target, &command, timeout).await {
        // Parsed regardless of exit status, matching the local path: `pacman
        // -Qi` exits non-zero if any single name is unknown while still
        // printing the records it did find. Requiring success would blank every
        // description because of one package removed on the host mid-check.
        Ok(out) => parse_package_descriptions(&String::from_utf8_lossy(&out.stdout)),
        Err(_) => HashMap::new(),
    }
}

/// Find a host's AUR updates without needing yay — or an AUR route — on the
/// host itself. Only the package list comes over SSH; this machine does the
/// AUR query and the version comparison.
async fn remote_aur_updates(target: &str, timeout: u32) -> Result<Vec<UpdateInfo>, String> {
    let output = ssh_run(target, "pacman -Qm", timeout)
        .await
        .map_err(|e| format!("pacman -Qm over SSH failed: {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // SSH reports its own failures as 255, which pacman never uses. Catch that
    // before the pacman exit codes so a dropped connection isn't read as one of
    // pacman's own statuses.
    if output.status.code() == Some(255) {
        return Err("SSH connection failed during AUR check".to_string());
    }

    let installed = match crate::aur::interpret_qm_output(output.status.code(), &stdout) {
        QmOutcome::Packages(packages) => packages,
        QmOutcome::Failed(reason) => {
            return Err(match stderr.trim() {
                "" => reason,
                detail => format!("{reason}: {detail}"),
            })
        }
        // Same ambiguity as the local check: pacman uses this status both for
        // "no foreign packages" and for a database it couldn't read, so confirm
        // before believing it.
        QmOutcome::NoMatches => match ssh_run(target, "pacman -Qq", timeout).await {
            Ok(probe) if probe.status.success() => Vec::new(),
            // ssh reports every connection failure as 255, so this is the
            // common way the probe misses — not a pacman problem at all.
            // Blaming the database here would name the wrong cause.
            Ok(probe) if probe.status.code() == Some(255) => {
                return Err("SSH connection failed during AUR check".to_string())
            }
            // The probe reached the host and pacman still couldn't list
            // anything, so the empty result was hiding an unreadable database.
            // Its own stderr is the relevant one, not the earlier command's.
            Ok(probe) => {
                let detail = String::from_utf8_lossy(&probe.stderr).trim().to_string();
                return Err(if detail.is_empty() {
                    "pacman could not read the package database".to_string()
                } else {
                    format!("pacman could not read the package database: {detail}")
                });
            }
            Err(e) => return Err(format!("could not confirm the package list: {e}")),
        },
    };

    crate::aur::updates_for_installed(installed).await
}

/// Ask whether the host is missing the AUR helper its updates would need.
///
/// The same `command -v yay` the update commands in `terminal.rs` branch on, so
/// the answer shown in the window is the one the terminal will reach. Only asked
/// when the host has AUR updates: for everyone else the answer changes nothing,
/// and it saves the round-trip.
///
/// Anything other than a clean "no such command" answers false. ssh reports its
/// own failures as 255, and a host we couldn't ask is not a host we know lacks
/// yay — mislabelling real work as "not applicable" would hide it.
async fn remote_aur_helper_missing(target: &str, timeout: u32, ask: bool) -> bool {
    if !ask {
        return false;
    }
    match ssh_run(target, "command -v yay >/dev/null 2>&1", timeout).await {
        Ok(out) => !out.status.success() && out.status.code() != Some(255),
        Err(_) => false,
    }
}

/// SSH into a host and run checkupdates.
async fn check_host(hostname: &str, timeout: u32, ssh_user: &str) -> HostResult {
    let target = ssh_target(hostname, ssh_user);
    let empty = |error: Option<String>, aur_error: Option<String>| HostResult {
        hostname: hostname.to_string(),
        updates: Vec::new(),
        needs_restart: false,
        restart_packages: Vec::new(),
        error,
        aur_error,
        aur_helper_missing: false,
    };

    let output = match ssh_run(&target, "checkupdates", timeout).await {
        Ok(output) => output,
        Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {
            return empty(Some("Connection timed out".to_string()), None)
        }
        Err(e) => return empty(Some(format!("SSH error: {e}")), None),
    };

    // checkupdates exit codes: 0 = updates available, 2 = no updates, and
    // anything else is a real failure (1 = checkupdates error, 255 = ssh
    // could not connect/authenticate). Mirror the local checker's handling
    // so a dead or misconfigured host reports an error rather than silently
    // showing as "up to date".
    let mut updates = match output.status.code() {
        Some(0) => parse_update_output(&String::from_utf8_lossy(&output.stdout)),
        Some(2) => Vec::new(),
        Some(255) => return empty(Some("SSH connection failed".to_string()), None),
        _ => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let msg = stderr.trim();
            return empty(
                Some(if msg.is_empty() {
                    "checkupdates failed".to_string()
                } else {
                    format!("checkupdates error: {msg}")
                }),
                None,
            );
        }
    };

    // Only repo packages may go to `pacman -Si` below: it fails outright on a
    // name absent from the sync databases, which would drop the repo badge from
    // every package on the host, not just the AUR ones.
    let repo_names: Vec<String> = updates.iter().map(|u| u.package.clone()).collect();

    // The AUR half runs even when checkupdates found nothing. That is precisely
    // the case this exists for — a host with no repo updates would otherwise
    // report "up to date" while sitting on AUR updates.
    let aur_error = match remote_aur_updates(&target, timeout).await {
        Ok(aur_updates) => {
            updates.extend(aur_updates);
            None
        }
        Err(e) => {
            log::warn!("AUR check failed for {hostname}: {e}");
            Some(e)
        }
    };

    if updates.is_empty() {
        return empty(None, aur_error);
    }

    let running_pkg = remote_kernel_package(&target, &updates, timeout).await;

    // Enrich with metadata that checkupdates omits. Run the remote queries
    // together so neither descriptions nor the helper probe adds another SSH
    // round-trip to the critical path. Descriptions cover AUR packages too —
    // they are installed, so `pacman -Qi` resolves them.
    let all_names: Vec<String> = updates.iter().map(|u| u.package.clone()).collect();
    let has_aur_updates = updates.iter().any(|u| u.is_aur());
    let (repos, descriptions, aur_helper_missing) = tokio::join!(
        remote_repositories(&target, &repo_names, timeout),
        remote_descriptions(&target, &all_names, timeout),
        remote_aur_helper_missing(&target, timeout, has_aur_updates),
    );
    for u in &mut updates {
        // AUR rows carry their own repository and URL already, and are absent
        // from this map, so they pass through untouched.
        if let Some((repo, arch)) = repos.get(&u.package) {
            u.repository = repo.clone();
            u.url = package_url(repo, arch, &u.package);
        }
        if let Some(description) = descriptions.get(&u.package) {
            u.description = description.clone();
        }
    }

    // An update that can't be applied can't be a reason to reboot: an AUR kernel
    // on a host with no helper would otherwise offer "Update & Restart" and
    // reboot the box for an upgrade that never ran.
    let restart_pkgs: Vec<String> = updates
        .iter()
        .filter(|u| !(aur_helper_missing && u.is_aur()))
        .filter(|u| package_requires_restart(&u.package, running_pkg.as_deref()))
        .map(|u| u.package.clone())
        .collect();

    HostResult {
        hostname: hostname.to_string(),
        needs_restart: !restart_pkgs.is_empty(),
        restart_packages: restart_pkgs,
        updates,
        error: None,
        aur_error,
        aur_helper_missing,
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
                    // The actionable number, so the live scan readout adds up to
                    // the total the tray badge lands on when the scan finishes.
                    "key": result.hostname,
                    "count": result.actionable_count(),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn upd(package: &str, repository: &str) -> UpdateInfo {
        UpdateInfo {
            package: package.to_string(),
            old_version: "1.0-1".to_string(),
            new_version: "1.1-1".to_string(),
            description: String::new(),
            repository: repository.to_string(),
            url: String::new(),
        }
    }

    fn host(updates: Vec<UpdateInfo>, aur_helper_missing: bool) -> HostResult {
        HostResult {
            hostname: "alarm-box".to_string(),
            updates,
            needs_restart: false,
            restart_packages: Vec::new(),
            error: None,
            aur_error: None,
            aur_helper_missing,
        }
    }

    #[test]
    fn every_update_counts_when_the_host_has_a_helper() {
        let h = host(vec![upd("linux", "core"), upd("oh-my-posh-bin", "aur")], false);
        assert_eq!(h.blocked_aur_count(), 0);
        assert_eq!(h.actionable_count(), 2);
    }

    #[test]
    fn aur_updates_stop_counting_without_a_helper() {
        // The repo half is still work the user can do from here; the AUR half is
        // listed but must not keep the tray badge lit.
        let h = host(vec![upd("linux", "core"), upd("oh-my-posh-bin", "aur")], true);
        assert_eq!(h.blocked_aur_count(), 1);
        assert_eq!(h.actionable_count(), 1);
    }

    #[test]
    fn a_host_with_only_blocked_updates_has_nothing_actionable() {
        let h = host(vec![upd("oh-my-posh-bin", "aur")], true);
        assert_eq!(h.actionable_count(), 0);
        // Still listed — the window has to be able to explain why.
        assert_eq!(h.updates.len(), 1);
    }
}

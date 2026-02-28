use serde::Serialize;
use std::collections::{HashMap, HashSet};
use tokio::process::Command;

/// Packages that require a system restart when updated.
pub static RESTART_PACKAGES: &[&str] = &[
    "linux",
    "linux-lts",
    "linux-zen",
    "linux-hardened",
    "systemd",
    "glibc",
    "nvidia",
    "nvidia-lts",
];

#[derive(Debug, Clone, Serialize)]
pub struct UpdateInfo {
    pub package: String,
    pub old_version: String,
    pub new_version: String,
    pub description: String,
    pub repository: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RebootInfo {
    pub needed: bool,
    pub running_kernel: String,
    pub installed_kernel: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CheckResult {
    pub updates: Vec<UpdateInfo>,
    pub needs_restart: bool,
    pub restart_packages: Vec<String>,
    pub reboot_info: Option<RebootInfo>,
}

/// Parse "package old_version -> new_version" lines into UpdateInfo list.
pub fn parse_update_output(output: &str) -> Vec<UpdateInfo> {
    output
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || !line.contains(" -> ") {
                return None;
            }
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 4 && parts[parts.len() - 2] == "->" {
                Some(UpdateInfo {
                    package: parts[0].to_string(),
                    old_version: parts[1].to_string(),
                    new_version: parts[parts.len() - 1].to_string(),
                    description: String::new(),
                    repository: String::new(),
                    url: String::new(),
                })
            } else {
                None
            }
        })
        .collect()
}

/// Fetch package descriptions from the local pacman database.
async fn fetch_descriptions(packages: &[String]) -> HashMap<String, String> {
    if packages.is_empty() {
        return HashMap::new();
    }
    let mut args = vec!["-Qi"];
    args.extend(packages.iter().map(|s| s.as_str()));

    let output = match Command::new("pacman").args(&args).output().await {
        Ok(o) => o,
        Err(_) => return HashMap::new(),
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut descriptions = HashMap::new();
    let mut name = String::new();

    for line in stdout.lines() {
        if let Some(rest) = line.strip_prefix("Name") {
            if let Some(val) = rest.split_once(':') {
                name = val.1.trim().to_string();
            }
        } else if let Some(rest) = line.strip_prefix("Description") {
            if !name.is_empty() {
                if let Some(val) = rest.split_once(':') {
                    descriptions.insert(name.clone(), val.1.trim().to_string());
                }
            }
        }
    }
    descriptions
}

/// Fetch repository name and URL from the pacman sync database.
async fn fetch_repositories(packages: &[String]) -> HashMap<String, (String, String)> {
    if packages.is_empty() {
        return HashMap::new();
    }
    let mut args = vec!["-Si"];
    args.extend(packages.iter().map(|s| s.as_str()));

    let output = match Command::new("pacman").args(&args).output().await {
        Ok(o) => o,
        Err(_) => return HashMap::new(),
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut repos = HashMap::new();
    let mut current_repo = String::new();
    let mut current_name = String::new();

    for line in stdout.lines() {
        if let Some(rest) = line.strip_prefix("Repository") {
            if let Some(val) = rest.split_once(':') {
                current_repo = val.1.trim().to_string();
            }
        } else if let Some(rest) = line.strip_prefix("Name") {
            if let Some(val) = rest.split_once(':') {
                current_name = val.1.trim().to_string();
            }
        } else if let Some(rest) = line.strip_prefix("Architecture") {
            if !current_name.is_empty() {
                if let Some(val) = rest.split_once(':') {
                    let arch = val.1.trim().to_string();
                    repos.insert(current_name.clone(), (current_repo.clone(), arch));
                    current_name.clear();
                }
            }
        }
    }
    repos
}

/// Check if a reboot is needed by looking for the running kernel's modules.
async fn check_reboot_needed() -> RebootInfo {
    let running = Command::new("uname")
        .arg("-r")
        .output()
        .await
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();

    let modules_exist = std::path::Path::new(&format!("/lib/modules/{running}")).is_dir();

    // Detect which kernel package corresponds to the running kernel
    let pkg = if running.contains("-zen") {
        "linux-zen"
    } else if running.contains("-hardened") {
        "linux-hardened"
    } else if running.contains("-lts") {
        "linux-lts"
    } else {
        "linux"
    };

    let installed = Command::new("pacman")
        .args(["-Q", pkg])
        .output()
        .await
        .ok()
        .filter(|o| o.status.success())
        .map(|o| {
            let s = String::from_utf8_lossy(&o.stdout);
            s.split_whitespace()
                .nth(1)
                .unwrap_or("")
                .to_string()
        })
        .unwrap_or_default();

    RebootInfo {
        needed: !modules_exist,
        running_kernel: running,
        installed_kernel: installed,
    }
}

/// Run a full local update check.
pub async fn check_updates() -> Result<CheckResult, String> {
    let restart_set: HashSet<&str> = RESTART_PACKAGES.iter().copied().collect();
    let mut updates = Vec::new();
    let mut repo_packages = Vec::new();

    // checkupdates syncs a temp database copy, so results are always fresh
    let repo = Command::new("checkupdates")
        .output()
        .await
        .map_err(|e| format!("Failed to run checkupdates: {e}"))?;

    // checkupdates: exit 0 = updates, exit 2 = no updates, exit 1 = error
    match repo.status.code() {
        Some(1) => {
            let stderr = String::from_utf8_lossy(&repo.stderr);
            return Err(format!("checkupdates error: {}", stderr.trim()));
        }
        Some(0) => {
            let stdout = String::from_utf8_lossy(&repo.stdout);
            repo_packages = parse_update_output(&stdout);
            updates.extend(repo_packages.clone());
        }
        _ => {} // exit 2 = no updates, or signal
    }

    // Check AUR packages separately via yay
    if let Ok(aur) = Command::new("yay").arg("-Qua").output().await {
        if aur.status.success() {
            let stdout = String::from_utf8_lossy(&aur.stdout);
            if !stdout.trim().is_empty() {
                let mut aur_packages = parse_update_output(&stdout);
                for u in &mut aur_packages {
                    u.repository = "aur".to_string();
                    u.url = format!("https://aur.archlinux.org/packages/{}", u.package);
                }
                updates.extend(aur_packages);
            }
        }
    }

    // Fetch descriptions for all packages
    let pkg_names: Vec<String> = updates.iter().map(|u| u.package.clone()).collect();
    let descs = fetch_descriptions(&pkg_names).await;
    for u in &mut updates {
        if let Some(desc) = descs.get(&u.package) {
            u.description = desc.clone();
        }
    }

    // Fetch repositories for repo packages
    if !repo_packages.is_empty() {
        let repo_names: Vec<String> = repo_packages.iter().map(|u| u.package.clone()).collect();
        let repos = fetch_repositories(&repo_names).await;
        for u in &mut updates {
            if let Some((repo, arch)) = repos.get(&u.package) {
                u.repository = repo.clone();
                u.url = format!(
                    "https://archlinux.org/packages/{}/{}/{}/",
                    repo, arch, u.package
                );
            }
        }
    }

    let restart_pkgs: Vec<String> = updates
        .iter()
        .filter(|u| restart_set.contains(u.package.as_str()))
        .map(|u| u.package.clone())
        .collect();

    let reboot_info = check_reboot_needed().await;

    Ok(CheckResult {
        needs_restart: !restart_pkgs.is_empty(),
        restart_packages: restart_pkgs,
        updates,
        reboot_info: Some(reboot_info),
    })
}

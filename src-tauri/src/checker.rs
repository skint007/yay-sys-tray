use serde::Serialize;
use std::collections::HashMap;
use tokio::process::Command;

/// Well-known kernel packages, used only as a conservative fallback when the
/// running kernel's owning package can't be determined (see
/// [`running_kernel_package`]). The real restart decision is an exact match
/// against the running kernel package, so non-standard kernels (linux-cachyos,
/// AUR kernels) are handled without needing to appear here.
pub static KERNEL_PACKAGES: &[&str] = &["linux", "linux-lts", "linux-zen", "linux-hardened"];

/// Packages that always require a system restart when updated. Includes the
/// nvidia driver variants (nvidia-open is the current Arch default) since a
/// driver update swaps the loaded kernel module.
pub static ALWAYS_RESTART_PACKAGES: &[&str] = &[
    "systemd",
    "glibc",
    "nvidia",
    "nvidia-lts",
    "nvidia-open",
    "nvidia-open-dkms",
    "nvidia-dkms",
];

/// Map a `uname -r` release string to its kernel package name. Only a fallback
/// for when the exact package can't be read from the modules `pkgbase` file.
pub fn kernel_package_for(release: &str) -> &'static str {
    if release.contains("-zen") {
        "linux-zen"
    } else if release.contains("-hardened") {
        "linux-hardened"
    } else if release.contains("-lts") {
        "linux-lts"
    } else {
        "linux"
    }
}

/// The package that owns the currently-running kernel. Arch writes the package
/// base name into `/usr/lib/modules/<release>/pkgbase`, which is exact even for
/// non-standard kernels (linux-zen, linux-cachyos, AUR kernels). Falls back to
/// sniffing the release string only when that file can't be read.
pub fn running_kernel_package(release: &str) -> String {
    let pkgbase = std::path::Path::new("/usr/lib/modules")
        .join(release)
        .join("pkgbase");
    std::fs::read_to_string(&pkgbase)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| kernel_package_for(release).to_string())
}

/// Could this package be a bootable kernel (as opposed to linux-firmware,
/// linux-headers, …)? Used only to decide whether to bother querying a remote
/// host's running kernel; the restart decision itself is an exact-name match.
pub fn maybe_kernel_pkg(name: &str) -> bool {
    name == "linux"
        || (name.starts_with("linux-")
            && !name.contains("headers")
            && name != "linux-firmware"
            && name != "linux-docs")
}

/// Whether updating `package` requires a restart. Updating the exact package
/// that owns the running kernel does; a different kernel flavor does not. When
/// the running kernel is unknown, any recognized kernel package is treated
/// conservatively as needing a restart.
pub fn package_requires_restart(package: &str, running_kernel_pkg: Option<&str>) -> bool {
    match running_kernel_pkg {
        Some(running) if package == running => return true,
        None if KERNEL_PACKAGES.contains(&package) => return true,
        _ => {}
    }
    ALWAYS_RESTART_PACKAGES.contains(&package)
}


#[derive(Debug, Clone, Serialize)]
pub struct UpdateInfo {
    pub package: String,
    pub old_version: String,
    pub new_version: String,
    pub description: String,
    pub repository: String,
    pub url: String,
}

impl UpdateInfo {
    /// Whether this update comes from the AUR rather than a sync repository.
    /// Plain pacman can neither install nor upgrade these, so every caller that
    /// builds a pacman-only command line has to ask.
    pub fn is_aur(&self) -> bool {
        self.repository == crate::aur::AUR_REPO
    }
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
    /// Why the AUR half of the check failed, if it did. Repo updates are still
    /// reported when this is set — the UI shows it as a warning so an
    /// unreachable AUR never reads as "no AUR updates".
    pub aur_error: Option<String>,
}

/// Pacman versions always carry a numeric component and a `-pkgrel` suffix;
/// requiring both keeps stray status/warning lines from parsing as updates
/// (fake package names would otherwise flow into shell commands downstream).
fn looks_like_version(s: &str) -> bool {
    s.contains('-') && s.chars().any(|c| c.is_ascii_digit())
}

/// Valid pacman/AUR package names use only `[A-Za-z0-9@._+-]` (per libalpm), so
/// any token with other characters isn't a real package. Rejecting it here is
/// what keeps shell metacharacters out of the `yay -S`/`pacman -S` command
/// strings built downstream (some of which run through `bash -c`/ssh shells).
pub fn looks_like_package_name(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '@' | '.' | '_' | '+' | '-'))
}

/// Parse "package old_version -> new_version" lines into UpdateInfo list.
/// Used for `checkupdates`, locally and over SSH; AUR packages go through the
/// `aur` module instead. The arrow is located by search rather than by fixed
/// position so trailing annotations, such as pacman's "[ignored]", don't drop
/// the line.
pub fn parse_update_output(output: &str) -> Vec<UpdateInfo> {
    output
        .lines()
        .filter_map(|line| {
            let parts: Vec<&str> = line.split_whitespace().collect();
            let arrow = parts.iter().position(|&p| p == "->")?;
            if arrow < 2 || arrow + 1 >= parts.len() {
                return None;
            }
            let old_version = parts[arrow - 1];
            let new_version = parts[arrow + 1];
            if !looks_like_version(old_version) || !looks_like_version(new_version) {
                return None;
            }
            if !looks_like_package_name(parts[0]) {
                return None;
            }
            Some(UpdateInfo {
                package: parts[0].to_string(),
                old_version: old_version.to_string(),
                new_version: new_version.to_string(),
                description: String::new(),
                repository: String::new(),
                url: String::new(),
            })
        })
        .collect()
}

/// Parse `pacman -Qi`/`pacman -Si` output into a package -> description map.
/// Shared by local and remote checks so update cards expose the same metadata.
pub fn parse_package_descriptions(stdout: &str) -> HashMap<String, String> {
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

    parse_package_descriptions(&String::from_utf8_lossy(&output.stdout))
}

/// Parse `pacman -Si` output into a map of package -> (repository, architecture).
/// Shared by the local check and the remote (SSH) check so both surface the repo.
pub fn parse_si_repositories(stdout: &str) -> HashMap<String, (String, String)> {
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

/// Repos hosted on archlinux.org — only these get a package page URL. Packages
/// from custom/self-hosted repos (e.g. a personal pacman repo) have no
/// archlinux.org page, so linking there would 404. Must stay in sync with
/// OFFICIAL_REPOS in src/lib/repo.ts, which orders the repo groups in the UI.
static OFFICIAL_REPOS: &[&str] = &[
    "core",
    "extra",
    "multilib",
    "core-testing",
    "extra-testing",
    "multilib-testing",
    "core-staging",
    "extra-staging",
    "multilib-staging",
    "kde-unstable",
    "gnome-unstable",
];

/// Build the archlinux.org package page URL for an official repo package, or
/// an empty string for custom-repo packages (the UI hides the link).
pub fn package_url(repo: &str, arch: &str, package: &str) -> String {
    if !OFFICIAL_REPOS.contains(&repo) {
        return String::new();
    }
    format!("https://archlinux.org/packages/{repo}/{arch}/{package}/")
}

/// Fetch repository name and URL from the local pacman sync database.
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
    parse_si_repositories(&stdout)
}

/// Check if a reboot is needed by looking for the running kernel's modules.
async fn check_reboot_needed(running: &str) -> RebootInfo {
    let modules_exist = std::path::Path::new(&format!("/lib/modules/{running}")).is_dir();

    // Detect which kernel package corresponds to the running kernel
    let pkg = running_kernel_package(running);

    let installed = Command::new("pacman")
        .args(["-Q", &pkg])
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
        running_kernel: running.to_string(),
        installed_kernel: installed,
    }
}

/// Run a full local update check.
pub async fn check_updates() -> Result<CheckResult, String> {
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

    // AUR packages are checked against the AUR's RPC API rather than by
    // parsing `yay -Qua` — see the `aur` module for why. A failure here is
    // recorded and surfaced, not swallowed, so the repo updates above still
    // reach the user while the AUR problem stays visible.
    let aur_error = match crate::aur::check_aur_updates().await {
        Ok(aur_packages) => {
            updates.extend(aur_packages);
            None
        }
        Err(e) => {
            log::warn!("AUR update check failed: {e}");
            Some(e)
        }
    };

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
                u.url = package_url(repo, arch, &u.package);
            }
        }
    }

    // Determine the running kernel's exact owning package once (reused for the
    // reboot check).
    let running_release = Command::new("uname")
        .arg("-r")
        .output()
        .await
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    let running_pkg = running_kernel_package(&running_release);

    let restart_pkgs: Vec<String> = updates
        .iter()
        .filter(|u| package_requires_restart(&u.package, Some(&running_pkg)))
        .map(|u| u.package.clone())
        .collect();

    let reboot_info = check_reboot_needed(&running_release).await;

    Ok(CheckResult {
        needs_restart: !restart_pkgs.is_empty(),
        restart_packages: restart_pkgs,
        updates,
        reboot_info: Some(reboot_info),
        aur_error,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_checkupdates_output() {
        let out = "linux 6.9.1-1 -> 6.9.2-1\nsystemd 255.6-1 -> 255.7-1\n";
        let updates = parse_update_output(out);
        assert_eq!(updates.len(), 2);
        assert_eq!(updates[0].package, "linux");
        assert_eq!(updates[0].old_version, "6.9.1-1");
        assert_eq!(updates[0].new_version, "6.9.2-1");
    }

    #[test]
    fn parses_lines_with_trailing_annotations() {
        let out = "oh-my-posh-bin 29.17.0-1 -> 29.20.0-1 [20h11m]\n\
                   visual-studio-code-bin 1.125.0-1 -> 1.127.0-1 [2d5h]\n";
        let updates = parse_update_output(out);
        assert_eq!(updates.len(), 2);
        assert_eq!(updates[0].package, "oh-my-posh-bin");
        assert_eq!(updates[0].old_version, "29.17.0-1");
        assert_eq!(updates[0].new_version, "29.20.0-1");
        assert_eq!(updates[1].new_version, "1.127.0-1");
    }

    #[test]
    fn parses_package_descriptions() {
        let out = "Name            : linux\n\
                   Version         : 7.0.13.arch1-1\n\
                   Description     : The Linux kernel and modules\n\
                   Architecture    : x86_64\n\n\
                   Name            : systemd\n\
                   Version         : 261-1\n\
                   Description     : System and service manager\n";
        let descriptions = parse_package_descriptions(out);
        assert_eq!(descriptions.len(), 2);
        assert_eq!(descriptions["linux"], "The Linux kernel and modules");
        assert_eq!(descriptions["systemd"], "System and service manager");
    }

    #[test]
    fn ignores_malformed_lines() {
        let out = "\nsome random noise\npkg 1.0-1 ->\n:: querying AUR...\n";
        assert!(parse_update_output(out).is_empty());
    }

    #[test]
    fn tolerates_multiple_trailing_annotations() {
        let out = "pkg 1.0-1 -> 1.1-1 [ignored] [20h11m]\n";
        let updates = parse_update_output(out);
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].old_version, "1.0-1");
        assert_eq!(updates[0].new_version, "1.1-1");
    }

    #[test]
    fn rejects_noise_lines_without_version_shapes() {
        // ≥5-token noise shaped "word word -> word junk" must not become an
        // update — fake package names would reach shell commands downstream.
        let out = "warning: database -> outdated (run pacman -Sy)\nfoo bar -> baz qux\n";
        assert!(parse_update_output(out).is_empty());
    }

    #[test]
    fn rejects_package_names_with_shell_metacharacters() {
        // A name carrying shell metacharacters must never become an update —
        // it would otherwise flow unquoted into `yay -S`/`pacman -S` shells.
        let out = "openssl;reboot 1.0-1 -> 1.1-1\n$(id) 1.0-1 -> 1.1-1\na`b` 1.0-1 -> 1.1-1\n";
        assert!(parse_update_output(out).is_empty());
    }

    #[test]
    fn restart_matches_running_kernel_exactly() {
        // The exact running-kernel package (even a non-standard one) needs a
        // restart; a different flavor that isn't running does not.
        assert!(package_requires_restart("linux-cachyos", Some("linux-cachyos")));
        assert!(!package_requires_restart("linux", Some("linux-cachyos")));
        assert!(!package_requires_restart("linux-lts", Some("linux")));
        assert!(package_requires_restart("linux", Some("linux")));
    }

    #[test]
    fn restart_flags_always_and_unknown_kernel() {
        // nvidia-open (current Arch default) and other always-restart packages.
        assert!(package_requires_restart("nvidia-open", Some("linux")));
        assert!(package_requires_restart("systemd", Some("linux")));
        // Running kernel unknown → any recognized kernel package is conservative.
        assert!(package_requires_restart("linux-zen", None));
        assert!(!package_requires_restart("firefox", None));
    }

    #[test]
    fn package_url_only_for_official_repos() {
        assert_eq!(
            package_url("extra", "x86_64", "firefox"),
            "https://archlinux.org/packages/extra/x86_64/firefox/"
        );
        // Staging/testing/unstable variants are hosted on archlinux.org too.
        assert!(!package_url("core-staging", "x86_64", "glibc").is_empty());
        assert!(!package_url("kde-unstable", "x86_64", "plasma-desktop").is_empty());
        // Custom repos (e.g. a personal pacman repo) have no archlinux.org
        // page — the URL must stay empty so the UI hides the link.
        assert_eq!(package_url("paw", "x86_64", "some-tool"), "");
    }

    #[test]
    fn official_repos_matches_frontend_list() {
        // OFFICIAL_REPOS is duplicated across the Rust/TS boundary (URL gating
        // here, UI group ordering in src/lib/repo.ts). Parse the TS source and
        // compare so the two copies can't silently drift.
        let ts_path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/lib/repo.ts");
        let ts = std::fs::read_to_string(&ts_path).expect("read src/lib/repo.ts");
        let block = ts
            .split("OFFICIAL_REPOS = [")
            .nth(1)
            .and_then(|rest| rest.split(']').next())
            .expect("OFFICIAL_REPOS array literal in repo.ts");
        // String literals are every odd-indexed piece when splitting on quotes.
        let ts_repos: Vec<&str> = block.split('"').skip(1).step_by(2).collect();
        assert_eq!(ts_repos, OFFICIAL_REPOS);
    }

    #[test]
    fn maybe_kernel_pkg_excludes_userspace() {
        assert!(maybe_kernel_pkg("linux"));
        assert!(maybe_kernel_pkg("linux-zen"));
        assert!(maybe_kernel_pkg("linux-cachyos"));
        assert!(!maybe_kernel_pkg("linux-firmware"));
        assert!(!maybe_kernel_pkg("linux-headers"));
        assert!(!maybe_kernel_pkg("firefox"));
    }
}

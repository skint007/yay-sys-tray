//! AUR update checks that don't depend on parsing yay's human-readable output.
//!
//! `yay -Qua` renders for people, and its format has already shifted twice
//! (colorized columns, then the `[6d18h]` staleness marker). Each shift
//! silently zeroed out the AUR section instead of failing loudly. So we ask the
//! AUR for the same data yay does: `pacman -Qm` lists foreign packages in a
//! two-column format frozen by libalpm, the AUR's RPC v5 `info` endpoint
//! returns each package's current version as JSON, and version comparison uses
//! libalpm's own algorithm (ported below rather than shelled out to `vercmp`).
//!
//! yay is still what *installs* updates — that runs in a terminal and needs no
//! parsing, so it can't break this way.
//!
//! Two deliberate differences from `yay -Qua`. VCS packages (`-git`, `-svn`)
//! are compared on their AUR pkgver, not on upstream commits, so yay's
//! opt-in `--devel` scan has no equivalent here. And `IgnorePkg` entries are
//! listed rather than filtered, which matches how `checkupdates` already
//! reports ignored repo packages, so both halves behave the same way.

use crate::checker::{looks_like_package_name, UpdateInfo};
use serde::Deserialize;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::time::Duration;
use tokio::process::Command;

const RPC_URL: &str = "https://aur.archlinux.org/rpc/v5/info";

/// The AUR caps how many packages a single RPC request may name. Chunking well
/// under that cap also keeps any one request small enough to retry cheaply.
const BATCH_SIZE: usize = 200;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);

/// Subset of the RPC v5 envelope we care about. Unknown fields are ignored, so
/// the AUR adding response fields can't break the check.
#[derive(Deserialize)]
struct RpcResponse {
    #[serde(rename = "type", default)]
    kind: String,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    results: Vec<RpcPackage>,
}

#[derive(Deserialize)]
struct RpcPackage {
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "Version")]
    version: String,
}

/// Split `pacman -Qm` output into (name, installed version) pairs. The format
/// is exactly two whitespace-separated columns per line; anything else is not a
/// package record and is dropped rather than guessed at.
pub fn parse_foreign_packages(stdout: &str) -> Vec<(String, String)> {
    stdout
        .lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let name = parts.next()?;
            let version = parts.next()?;
            if parts.next().is_some() {
                return None;
            }
            if !looks_like_package_name(name) {
                return None;
            }
            Some((name.to_string(), version.to_string()))
        })
        .collect()
}

/// Packages installed from outside the sync databases — the AUR ones, plus any
/// locally built packages (which the AUR simply won't know about).
async fn foreign_packages() -> Result<Vec<(String, String)>, String> {
    let output = Command::new("pacman")
        .arg("-Qm")
        .output()
        .await
        .map_err(|e| format!("Failed to run pacman -Qm: {e}"))?;

    interpret_qm_output(
        output.status.success(),
        &String::from_utf8_lossy(&output.stdout),
        &String::from_utf8_lossy(&output.stderr),
    )
}

/// Turn a `pacman -Qm` run into a package list or an error. Split out from the
/// process call so both rules below are covered by tests.
fn interpret_qm_output(
    success: bool,
    stdout: &str,
    stderr: &str,
) -> Result<Vec<(String, String)>, String> {
    // `pacman -Qm` exits 1 simply because nothing matched, which is the normal
    // state on a machine with no AUR packages at all. A genuine failure (an
    // unreadable database, say) also writes to stderr, and that is what tells
    // the two apart — exit status alone would report healthy systems as broken.
    if !success && !stderr.trim().is_empty() {
        return Err(format!("pacman -Qm failed: {}", stderr.trim()));
    }

    let packages = parse_foreign_packages(stdout);

    // Output that yielded no records at all means the format moved out from
    // under us. Say so, rather than returning an empty list that reads as "no
    // AUR updates" — that silent zero is the exact failure this module exists
    // to prevent, and it is how the yay-based check broke twice.
    if packages.is_empty() && !stdout.trim().is_empty() {
        return Err("pacman -Qm output was not in the expected format".to_string());
    }

    Ok(packages)
}

/// Ask the AUR for the current version of each named package. Names the AUR
/// doesn't know are simply absent from the returned map.
async fn fetch_aur_versions(names: &[String]) -> Result<HashMap<String, String>, String> {
    let client = reqwest::Client::builder()
        .user_agent(concat!("yay-sys-tray/", env!("CARGO_PKG_VERSION")))
        .timeout(REQUEST_TIMEOUT)
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {e}"))?;

    let mut versions = HashMap::new();
    for chunk in names.chunks(BATCH_SIZE) {
        // POST rather than GET: the query form has no practical length limit,
        // so package counts never need a second encoding strategy.
        let form: Vec<(&str, &str)> = chunk.iter().map(|n| ("arg[]", n.as_str())).collect();
        let response = client
            .post(RPC_URL)
            .form(&form)
            .send()
            .await
            .map_err(|e| format!("AUR request failed: {e}"))?;

        let status = response.status();
        if !status.is_success() {
            return Err(format!("AUR returned HTTP {status}"));
        }

        let body: RpcResponse = response
            .json()
            .await
            .map_err(|e| format!("AUR returned unreadable JSON: {e}"))?;

        if body.kind == "error" {
            let msg = body.error.unwrap_or_else(|| "unknown error".to_string());
            return Err(format!("AUR error: {msg}"));
        }

        for pkg in body.results {
            versions.insert(pkg.name, pkg.version);
        }
    }
    Ok(versions)
}

/// Find AUR packages with a newer version available.
///
/// Errors are returned rather than swallowed: an unreachable AUR must not look
/// the same as "no AUR updates", which is exactly how the old yay-based check
/// failed.
pub async fn check_aur_updates() -> Result<Vec<UpdateInfo>, String> {
    let installed = foreign_packages().await?;
    if installed.is_empty() {
        return Ok(Vec::new());
    }

    let names: Vec<String> = installed.iter().map(|(name, _)| name.clone()).collect();
    let latest = fetch_aur_versions(&names).await?;

    let mut updates = Vec::new();
    for (name, current) in installed {
        // Foreign packages missing from the response aren't in the AUR at all
        // (locally built packages, debug packages split off during a build).
        // There is nothing to compare against, so they're not out of date.
        let Some(available) = latest.get(&name) else {
            continue;
        };
        if vercmp(available, &current) != Ordering::Greater {
            continue;
        }
        updates.push(UpdateInfo {
            url: format!("https://aur.archlinux.org/packages/{name}"),
            package: name,
            old_version: current,
            new_version: available.clone(),
            description: String::new(),
            repository: "aur".to_string(),
        });
    }

    updates.sort_by(|a, b| a.package.cmp(&b.package));
    Ok(updates)
}

/// Compare two pacman version strings, following libalpm's `alpm_pkg_vercmp`.
///
/// Ported instead of shelling out to `vercmp` so a check doesn't spawn a
/// process per installed package, and so the rules are pinned by the tests
/// below instead of by another program's output.
pub fn vercmp(a: &str, b: &str) -> Ordering {
    if a == b {
        return Ordering::Equal;
    }

    let (epoch_a, ver_a, rel_a) = parse_evr(a);
    let (epoch_b, ver_b, rel_b) = parse_evr(b);

    match rpmvercmp(epoch_a, epoch_b) {
        Ordering::Equal => {}
        ord => return ord,
    }
    match rpmvercmp(ver_a, ver_b) {
        Ordering::Equal => {}
        ord => return ord,
    }
    // A missing pkgrel compares equal to any pkgrel: libalpm treats "1.0" and
    // "1.0-2" as the same version rather than guessing a default.
    match (rel_a, rel_b) {
        (Some(x), Some(y)) => rpmvercmp(x, y),
        _ => Ordering::Equal,
    }
}

/// Split `[epoch:]version[-pkgrel]`. Unlike RPM, alpm always assumes an epoch,
/// defaulting to "0"; only a leading run of digits followed by `:` counts.
fn parse_evr(evr: &str) -> (&str, &str, Option<&str>) {
    let digits = evr
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(evr.len());

    let (epoch, rest) = if evr[digits..].starts_with(':') {
        let epoch = &evr[..digits];
        (if epoch.is_empty() { "0" } else { epoch }, &evr[digits + 1..])
    } else {
        ("0", evr)
    };

    // The pkgrel is whatever follows the *last* hyphen, so hyphens inside a
    // version string stay part of the version.
    match rest.rfind('-') {
        Some(i) => (epoch, &rest[..i], Some(&rest[i + 1..])),
        None => (epoch, rest, None),
    }
}

/// Segment-wise version comparison, ported from libalpm's `rpmvercmp`.
///
/// Both strings are walked in lockstep, one all-digit or all-alphabetic segment
/// at a time, with the separators between segments compared by length.
fn rpmvercmp(a: &str, b: &str) -> Ordering {
    if a == b {
        return Ordering::Equal;
    }

    let (x, y) = (a.as_bytes(), b.as_bytes());
    let (mut i, mut j) = (0usize, 0usize);

    while i < x.len() && j < y.len() {
        let (sep_x, sep_y) = (i, j);
        while i < x.len() && !x[i].is_ascii_alphanumeric() {
            i += 1;
        }
        while j < y.len() && !y[j].is_ascii_alphanumeric() {
            j += 1;
        }

        // One side ran out of segments; the tail check after the loop decides.
        if i >= x.len() || j >= y.len() {
            break;
        }

        // Separator runs of different lengths end the comparison: the shorter
        // run sorts first, so "1.a" < "1..a".
        match (i - sep_x).cmp(&(j - sep_y)) {
            Ordering::Equal => {}
            ord => return ord,
        }

        let (start_x, start_y) = (i, j);
        let numeric = x[i].is_ascii_digit();
        if numeric {
            while i < x.len() && x[i].is_ascii_digit() {
                i += 1;
            }
            while j < y.len() && y[j].is_ascii_digit() {
                j += 1;
            }
        } else {
            while i < x.len() && x[i].is_ascii_alphabetic() {
                i += 1;
            }
            while j < y.len() && y[j].is_ascii_alphabetic() {
                j += 1;
            }
        }

        // The two segments are of different types, leaving one of them empty.
        // A numeric segment always outranks an alphabetic one, so "1.10" beats
        // "1.a" but "1.a" loses to "1.10".
        if j == start_y {
            return if numeric {
                Ordering::Greater
            } else {
                Ordering::Less
            };
        }

        let (mut seg_x, mut seg_y) = (&x[start_x..i], &y[start_y..j]);
        if numeric {
            // Leading zeros carry no value, and once they're gone the longer
            // digit run is the larger number.
            while seg_x.first() == Some(&b'0') {
                seg_x = &seg_x[1..];
            }
            while seg_y.first() == Some(&b'0') {
                seg_y = &seg_y[1..];
            }
            match seg_x.len().cmp(&seg_y.len()) {
                Ordering::Equal => {}
                ord => return ord,
            }
        }
        // Equal-length digit runs and alphabetic runs both compare bytewise.
        match seg_x.cmp(seg_y) {
            Ordering::Equal => {}
            ord => return ord,
        }
    }

    // Every segment matched so far, so whatever is left over decides. Both
    // empty means the two differed only in their separators.
    let (rest_x, rest_y) = (x.get(i).copied(), y.get(j).copied());
    if rest_x.is_none() && rest_y.is_none() {
        return Ordering::Equal;
    }

    // A leftover alphabetic tail never beats an empty one, because it reads as
    // a pre-release marker: "1.0" is newer than "1.0a", but "1.0.1" is newer
    // than "1.0".
    let is_alpha = |c: Option<u8>| c.is_some_and(|c| c.is_ascii_alphabetic());
    if (rest_x.is_none() && !is_alpha(rest_y)) || is_alpha(rest_x) {
        Ordering::Less
    } else {
        Ordering::Greater
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cmp(a: &str, b: &str) -> i32 {
        match vercmp(a, b) {
            Ordering::Less => -1,
            Ordering::Equal => 0,
            Ordering::Greater => 1,
        }
    }

    #[test]
    fn compares_simple_versions() {
        assert_eq!(cmp("1.0-1", "1.0-1"), 0);
        assert_eq!(cmp("1.0-1", "1.0-2"), -1);
        assert_eq!(cmp("1.1-1", "1.0-1"), 1);
        assert_eq!(cmp("1.0.1-1", "1.0-1"), 1);
    }

    #[test]
    fn compares_numeric_segments_as_numbers() {
        assert_eq!(cmp("1.10-1", "1.9-1"), 1);
        assert_eq!(cmp("1.09-1", "1.9-1"), 0);
        assert_eq!(cmp("2.1.220-1", "2.1.216-1"), 1);
        assert_eq!(cmp("30.0.0-1", "29.33.0-1"), 1);
    }

    #[test]
    fn numeric_segments_outrank_alpha_segments() {
        assert_eq!(cmp("1.1-1", "1.a-1"), 1);
        assert_eq!(cmp("1.a-1", "1.1-1"), -1);
    }

    #[test]
    fn trailing_alpha_segment_loses_to_no_segment() {
        // A trailing alpha reads as a pre-release marker, so it is older than
        // the bare version — but a trailing numeric segment is newer.
        assert_eq!(cmp("1.0a-1", "1.0-1"), -1);
        assert_eq!(cmp("1.0-1", "1.0a-1"), 1);
        assert_eq!(cmp("1.0.1-1", "1.0-1"), 1);
    }

    #[test]
    fn epoch_beats_version() {
        assert_eq!(cmp("1:1.0-1", "2.0-1"), 1);
        assert_eq!(cmp("1:29.7.1-1", "29.7.2-1"), 1);
        assert_eq!(cmp("1:1.0-1", "2:1.0-1"), -1);
        assert_eq!(cmp("1:1.0-1", "1.0-1"), 1);
    }

    #[test]
    fn missing_pkgrel_compares_equal() {
        assert_eq!(cmp("1.0", "1.0-1"), 0);
        assert_eq!(cmp("1.0-1", "1.0"), 0);
        assert_eq!(cmp("1.0", "1.1-1"), -1);
    }

    #[test]
    fn handles_vcs_and_separator_forms() {
        assert_eq!(cmp("r120.abc1234-1", "r99.abc1234-1"), 1);
        assert_eq!(cmp("1.0.r5.g1234567-1", "1.0.r4.g1234567-1"), 1);
        assert_eq!(cmp("1.0_beta-1", "1.0-1"), 1);
        assert_eq!(cmp("1..a", "1.a"), 1);
    }

    #[test]
    fn parses_foreign_package_list() {
        let out = "ai-memory-bin 1.17.1-1\nclaude-code 2.1.216-1\nyay-bin 13.0.1-1\n";
        let pkgs = parse_foreign_packages(out);
        assert_eq!(pkgs.len(), 3);
        assert_eq!(pkgs[0], ("ai-memory-bin".to_string(), "1.17.1-1".to_string()));
        assert_eq!(pkgs[2], ("yay-bin".to_string(), "13.0.1-1".to_string()));
    }

    #[test]
    fn no_foreign_packages_is_not_an_error() {
        // pacman exits 1 when nothing matched. On a machine with no AUR
        // packages that is the healthy state, not a failure to report.
        assert_eq!(interpret_qm_output(false, "", ""), Ok(Vec::new()));
    }

    #[test]
    fn reports_pacman_failures() {
        let err = interpret_qm_output(false, "", "error: could not open database\n");
        assert_eq!(
            err,
            Err("pacman -Qm failed: error: could not open database".to_string())
        );
    }

    #[test]
    fn unparseable_output_errors_rather_than_reading_as_empty() {
        // If pacman -Qm ever grows a column, the check must say so instead of
        // quietly reporting zero AUR updates.
        let out = "ai-memory-bin 1.17.1-1 [installed as dependency]\n";
        assert!(interpret_qm_output(true, out, "").is_err());
    }

    #[test]
    fn rejects_non_package_lines() {
        // Warnings, blank lines and anything that isn't exactly two columns of
        // package-shaped text must not become a fake package name.
        let out = "\nwarning: database file for 'x' does not exist\npkg 1.0-1 extra\n$(rm -rf) 1.0-1\n";
        assert!(parse_foreign_packages(out).is_empty());
    }
}

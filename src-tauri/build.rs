use std::process::Command;

fn main() {
    // Embed git version at compile time (matching PKGBUILD pkgver formula)
    if let Some(version) = git_version() {
        println!("cargo:rustc-env=YST_VERSION={version}");
    }

    tauri_build::build();
}

fn git_version() -> Option<String> {
    let output = Command::new("git")
        .args(["describe", "--tags", "--long", "--abbrev=7"])
        .output()
        .ok()?;

    if !output.status.success() {
        // No tags — fall back to commit counting
        let count = Command::new("git")
            .args(["rev-list", "--count", "HEAD"])
            .output()
            .ok()?;
        let short = Command::new("git")
            .args(["rev-parse", "--short=7", "HEAD"])
            .output()
            .ok()?;

        let count = String::from_utf8_lossy(&count.stdout).trim().to_string();
        let short = String::from_utf8_lossy(&short.stdout).trim().to_string();

        if !count.is_empty() && !short.is_empty() {
            return Some(format!("r{count}.{short}"));
        }
        return None;
    }

    let desc = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let v = desc.strip_prefix('v').unwrap_or(&desc);

    // Parse: base-commits-ghash
    if let Some(last_dash_g) = v.rfind("-g") {
        let hash = &v[last_dash_g + 2..];
        let before_hash = &v[..last_dash_g];
        if let Some(second_last_dash) = before_hash.rfind('-') {
            let commits = &before_hash[second_last_dash + 1..];
            let base = &before_hash[..second_last_dash];
            if commits == "0" {
                return Some(base.to_string());
            }
            return Some(format!("{base}.{commits}.{hash}"));
        }
    }

    Some(v.to_string())
}

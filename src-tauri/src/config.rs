use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

fn config_path() -> PathBuf {
    let config_dir = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("yay-sys-tray");
    config_dir.join("config.json")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default = "default_interval")]
    pub check_interval_minutes: u32,
    #[serde(default = "default_notify")]
    pub notify: String,
    #[serde(default)]
    pub terminal: String,
    #[serde(default)]
    pub noconfirm: bool,
    #[serde(default)]
    pub autostart: bool,
    #[serde(default = "default_true")]
    pub animations: bool,
    #[serde(default = "default_recheck")]
    pub recheck_interval_minutes: u32,
    #[serde(default)]
    pub passwordless_updates: bool,
    #[serde(default)]
    pub tailscale_enabled: bool,
    #[serde(default = "default_tags")]
    pub tailscale_tags: String,
    #[serde(default = "default_timeout")]
    pub tailscale_timeout: u32,
}

fn default_interval() -> u32 { 60 }
fn default_notify() -> String { "new_only".into() }
fn default_true() -> bool { true }
fn default_recheck() -> u32 { 5 }
fn default_tags() -> String { "server,arch".into() }
fn default_timeout() -> u32 { 10 }

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            check_interval_minutes: 60,
            notify: "new_only".into(),
            terminal: detect_terminal(),
            noconfirm: false,
            autostart: false,
            animations: true,
            recheck_interval_minutes: 5,
            passwordless_updates: false,
            tailscale_enabled: false,
            tailscale_tags: "server,arch".into(),
            tailscale_timeout: 10,
        }
    }
}

impl AppConfig {
    pub fn load() -> Self {
        let path = config_path();
        match fs::read_to_string(&path) {
            Ok(contents) => {
                let mut config: AppConfig =
                    serde_json::from_str(&contents).unwrap_or_default();
                config.post_load();
                config
            }
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) -> Result<(), String> {
        let path = config_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        fs::write(&path, json).map_err(|e| e.to_string())
    }

    fn post_load(&mut self) {
        // Migrate old "tag:server,tag:arch" format to "server,arch"
        if self.tailscale_tags.contains("tag:") {
            self.tailscale_tags = self
                .tailscale_tags
                .split(',')
                .map(|t| t.trim().strip_prefix("tag:").unwrap_or(t.trim()))
                .collect::<Vec<_>>()
                .join(",");
        }
        // Auto-detect terminal if empty
        if self.terminal.is_empty() {
            self.terminal = detect_terminal();
        }
    }
}

fn detect_terminal() -> String {
    for name in &["kitty", "alacritty", "konsole", "foot", "xterm"] {
        if which::which(name).is_ok() {
            return name.to_string();
        }
    }
    String::new()
}

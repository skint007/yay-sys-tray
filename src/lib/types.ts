export interface UpdateInfo {
  package: string;
  old_version: string;
  new_version: string;
  description: string;
  repository: string;
  url: string;
}

export interface RebootInfo {
  needed: boolean;
  running_kernel: string;
  installed_kernel: string;
}

export interface CheckResult {
  updates: UpdateInfo[];
  needs_restart: boolean;
  restart_packages: string[];
  reboot_info: RebootInfo | null;
  /** Set when the AUR could not be reached, so an outage never reads as "no
   * AUR updates". Repo updates are still present alongside it. */
  aur_error: string | null;
}

export interface HostResult {
  hostname: string;
  updates: UpdateInfo[];
  needs_restart: boolean;
  restart_packages: string[];
  error: string | null;
  /** Set when this host's AUR check failed. Its repo updates are still listed
   * alongside; mirrors CheckResult.aur_error for the local check. */
  aur_error: string | null;
  /** The host has AUR updates but no yay to apply them with, so they are listed
   * as not applicable rather than as pending work. */
  aur_helper_missing: boolean;
}

export interface FullCheckResult {
  local: CheckResult | null;
  remote: HostResult[];
}

export interface AppConfig {
  check_interval_enabled: boolean;
  check_interval_minutes: number;
  notify: "always" | "new_only" | "never";
  terminal: string;
  noconfirm: boolean;
  hold_terminal: boolean;
  autostart: boolean;
  close_after_update: boolean;
  animations: boolean;
  theme: string;
  passwordless_updates: boolean;
  restart_delay_seconds: number;
  tailscale_enabled: boolean;
  tailscale_tags: string;
  tailscale_timeout: number;
  tailscale_ssh_user: string;
  scheduled_check_enabled: boolean;
  scheduled_check_day: number;
  scheduled_check_time: string;
}

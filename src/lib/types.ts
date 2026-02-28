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
}

export interface HostResult {
  hostname: string;
  updates: UpdateInfo[];
  needs_restart: boolean;
  restart_packages: string[];
  error: string | null;
}

export interface FullCheckResult {
  local: CheckResult | null;
  remote: HostResult[];
}

export interface AppConfig {
  check_interval_minutes: number;
  notify: "always" | "new_only" | "never";
  terminal: string;
  noconfirm: boolean;
  autostart: boolean;
  animations: boolean;
  theme: string;
  recheck_interval_minutes: number;
  passwordless_updates: boolean;
  tailscale_enabled: boolean;
  tailscale_tags: string;
  tailscale_timeout: number;
}

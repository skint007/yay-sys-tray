import { invoke } from "@tauri-apps/api/core";
import type { AppConfig, FullCheckResult } from "./types";

export async function getConfig(): Promise<AppConfig> {
  return invoke("get_config");
}

export async function saveConfig(config: AppConfig): Promise<void> {
  return invoke("save_config", { config });
}

export async function startCheck(): Promise<void> {
  return invoke("start_check");
}

export async function getCheckResult(): Promise<FullCheckResult | null> {
  return invoke("get_check_result");
}

export async function runLocalUpdate(restart: boolean): Promise<void> {
  return invoke("run_local_update", { restart });
}

export async function runRemoteUpdate(
  hostname: string,
  restart: boolean,
): Promise<void> {
  return invoke("run_remote_update", { hostname, restart });
}

export async function runRemove(
  packageName: string,
  flags: string,
): Promise<void> {
  return invoke("run_remove", { package: packageName, flags });
}

export async function isArchLinux(): Promise<boolean> {
  return invoke("is_arch_linux");
}

export async function getPactree(
  packageName: string,
  reverse: boolean,
): Promise<string> {
  return invoke("get_pactree", { package: packageName, reverse });
}

export async function discoverTailscaleTags(): Promise<string[]> {
  return invoke("discover_tailscale_tags");
}

export async function manageAutostart(enable: boolean): Promise<void> {
  return invoke("manage_autostart", { enable });
}

export async function managePasswordlessUpdates(
  enable: boolean,
): Promise<boolean> {
  return invoke("manage_passwordless_updates", { enable });
}

export async function getVersion(): Promise<string> {
  return invoke("get_version");
}

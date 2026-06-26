// Dev-only preview harness: renders a dialog with mocked Tauri IPC so the UI
// can be screenshotted in a plain browser (no Tauri runtime). Not shipped.
import { mockIPC } from "@tauri-apps/api/mocks";
import { mount } from "svelte";
import "./app.css";
import UpdatesDialog from "./lib/components/UpdatesDialog.svelte";
import SettingsDialog from "./lib/components/SettingsDialog.svelte";
import AboutDialog from "./lib/components/AboutDialog.svelte";
import DependencyTree from "./lib/components/DependencyTree.svelte";

const mkUpdates = () => [
  { package: "linux", old_version: "7.0.12.arch1-1", new_version: "7.0.13.arch1-1", description: "The Linux kernel and modules", repository: "core", url: "https://archlinux.org" },
  { package: "systemd", old_version: "260.2-2", new_version: "261-1", description: "system and service manager", repository: "core", url: "" },
  { package: "android-tools", old_version: "35.0.2-27", new_version: "35.0.2-28", description: "Android platform tools", repository: "extra", url: "" },
  { package: "grpc", old_version: "1.81.0-1", new_version: "1.81.1-1", description: "", repository: "extra", url: "" },
  { package: "lib32-systemd", old_version: "260.2-1", new_version: "261-1", description: "", repository: "multilib", url: "" },
  { package: "kitinerary", old_version: "26.04.2-3", new_version: "26.04.2-4", description: "itinerary extractor", repository: "aur", url: "https://aur.archlinux.org" },
];

const config = {
  check_interval_enabled: true, check_interval_minutes: 60, notify: "never", terminal: "kitty",
  noconfirm: true, hold_terminal: true, autostart: true, animations: true, theme: "dark",
  recheck_interval_minutes: 5, passwordless_updates: true, restart_delay_seconds: 30,
  tailscale_enabled: true, tailscale_tags: "arch,linux,server", tailscale_timeout: 10,
  tailscale_ssh_user: "skint007", vertical_update_tabs: true,
  scheduled_check_enabled: true, scheduled_check_day: 4, scheduled_check_time: "18:00",
};

const checkResult = {
  local: { updates: mkUpdates(), needs_restart: true, restart_packages: ["linux", "systemd"], reboot_info: null },
  remote: [
    { hostname: "arch-serv", updates: mkUpdates().slice(0, 4), needs_restart: true, restart_packages: ["linux"], error: null },
    { hostname: "arch-serv-gpu", updates: mkUpdates().slice(2, 5), needs_restart: false, restart_packages: [], error: null },
    { hostname: "uswest-arch-dns3", updates: mkUpdates().slice(3), needs_restart: true, restart_packages: ["systemd"], error: null },
  ],
};

const pactree = "systemd\n├─libcap\n├─libgcrypt\n│ └─libgpg-error\n├─libidn2\n│ ├─libunistring\n│ └─libpsl\n├─lz4\n├─xz\n└─pam";

// Surface #6 — checking state seed (3 of 10 hosts scanned, 56 updates so far).
const checkingSeed = {
  hosts: [
    { key: "local", name: "Local" },
    { key: "arch-serv", name: "arch-serv" },
    { key: "arch-serv-gpu", name: "arch-serv-gpu" },
    { key: "arch-serv-nor", name: "arch-serv-nor" },
    { key: "arch-serv-services", name: "arch-serv-services" },
    { key: "eucentral-arch-general", name: "eucentral-arch-general" },
    { key: "eucentral-arch-media", name: "eucentral-arch-media" },
    { key: "uswest-arch-dns", name: "uswest-arch-dns" },
    { key: "uswest-arch-dns3", name: "uswest-arch-dns3" },
    { key: "uswest-arch-dns4", name: "uswest-arch-dns4" },
  ],
  status: {
    local: "done", "arch-serv": "done", "arch-serv-gpu": "done",
    "arch-serv-nor": "checking", "arch-serv-services": "checking",
    "eucentral-arch-general": "queued", "eucentral-arch-media": "queued",
    "uswest-arch-dns": "queued", "uswest-arch-dns3": "queued", "uswest-arch-dns4": "queued",
  } as Record<string, "queued" | "checking" | "done" | "error">,
  counts: { local: 52, "arch-serv": 2, "arch-serv-gpu": 2 } as Record<string, number>,
  startedSecAgo: 3,
};

mockIPC((cmd: string) => {
  switch (cmd) {
    case "get_config": return config;
    case "get_check_result": return checkResult;
    case "discover_tailscale_tags":
      return ["admin", "arch", "cloud", "desktop", "dns", "kvm", "laptop", "linux", "mobile", "router", "server", "windows"];
    case "get_version": return "0.10.3";
    case "is_arch_linux": return true;
    case "get_pactree": return pactree;
    default:
      return null; // save_config, plugin:event|listen/unlisten, run_* etc.
  }
});

const params = new URLSearchParams(location.search);
const view = params.get("view") ?? "updates";
document.documentElement.setAttribute("data-theme", `skint007-${params.get("theme") ?? "dark"}`);

const sizes: Record<string, [number, number]> = {
  updates: [920, 620], settings: [560, 690], about: [480, 460], deptree: [720, 560],
  checking: [920, 620],
};
const [w, h] = sizes[view] ?? [920, 620];
const target = document.getElementById("app")!;
target.style.width = `${w}px`;
target.style.height = `${h}px`;
if (view === "deptree") target.style.background = "var(--ys-ground)";

const onclose = () => {};
if (view === "settings") mount(SettingsDialog, { target, props: { onclose } });
else if (view === "about") mount(AboutDialog, { target, props: { onclose } });
else if (view === "deptree") mount(DependencyTree, { target, props: { packageName: "systemd", reverse: false, repository: "core", onclose } });
else if (view === "checking") mount(UpdatesDialog, { target, props: { onclose, previewChecking: checkingSeed } });
else mount(UpdatesDialog, { target, props: { onclose } });

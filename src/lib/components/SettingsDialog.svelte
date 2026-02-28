<script lang="ts">
  import { onMount } from "svelte";
  import type { AppConfig } from "../types";
  import {
    getConfig,
    saveConfig,
    isArchLinux,
    manageAutostart,
    managePasswordlessUpdates,
  } from "../ipc";
  import DurationPicker from "./DurationPicker.svelte";
  import TagPillSelector from "./TagPillSelector.svelte";

  const THEMES = [
    "default", "light", "dark", "cupcake", "bumblebee", "emerald",
    "corporate", "synthwave", "retro", "cyberpunk", "valentine",
    "halloween", "garden", "forest", "aqua", "lofi", "pastel",
    "fantasy", "wireframe", "black", "luxury", "dracula", "cmyk",
    "autumn", "business", "acid", "lemonade", "night", "coffee",
    "winter", "dim", "nord", "sunset", "caramellatte", "abyss", "silk",
  ];

  let { onclose }: { onclose: () => void } = $props();

  let config = $state<AppConfig | null>(null);
  let isArch = $state(false);
  let activeTab = $state<"general" | "tailscale">("general");
  const initial = { autostart: false, passwordless: false };

  onMount(async () => {
    [config, isArch] = await Promise.all([getConfig(), isArchLinux()]);
    if (config) {
      initial.autostart = config.autostart;
      initial.passwordless = config.passwordless_updates;
    }
  });

  function applyTheme(theme: string) {
    document.documentElement.setAttribute(
      "data-theme",
      theme === "default" ? "" : theme,
    );
  }

  async function handleThemeChange(e: Event) {
    if (!config) return;
    const theme = (e.target as HTMLSelectElement).value;
    config.theme = theme;
    applyTheme(theme);
    await saveConfig(config);
  }

  async function handleSave() {
    if (!config) return;

    // Enforce minimum check interval
    if (config.check_interval_minutes < 5) {
      config.check_interval_minutes = 5;
    }

    // Handle autostart toggle (only if changed)
    if (config.autostart !== initial.autostart) {
      try {
        await manageAutostart(config.autostart);
        initial.autostart = config.autostart;
      } catch (e) {
        console.error("Failed to manage autostart:", e);
      }
    }

    // Handle passwordless updates toggle (only if changed)
    if (config.passwordless_updates !== initial.passwordless) {
      try {
        await managePasswordlessUpdates(config.passwordless_updates);
        initial.passwordless = config.passwordless_updates;
      } catch (e) {
        console.error("Failed to manage passwordless updates:", e);
      }
    }

    await saveConfig(config);
    onclose();
  }
</script>

{#if config}
  <div class="flex flex-col h-full p-4 gap-4">
    <!-- Tabs (scrollable radio tabs-lift + tab-content) -->
    <div class="overflow-x-auto flex-1 min-h-0">
      <div class="tabs tabs-lift min-w-max">
      <input
        type="radio"
        name="settings_tabs"
        class="tab z-1"
        aria-label="General"
        checked={activeTab === "general"}
        onchange={() => (activeTab = "general")}
      />
      <div class="sticky start-0 tab-content bg-base-100 border-base-300 p-4 overflow-y-auto">
        <div class="grid grid-cols-[auto_1fr] gap-x-4 gap-y-3 items-center">
          <span class="text-sm font-medium">Check interval:</span>
          <DurationPicker bind:value={config.check_interval_minutes} />

          <span class="text-sm font-medium">Notifications:</span>
          <select
            class="select select-sm select-bordered w-full"
            bind:value={config.notify}
          >
            <option value="always">Always</option>
            <option value="new_only">New only</option>
            <option value="never">Never</option>
          </select>

          <span class="text-sm font-medium">Terminal:</span>
          <input
            type="text"
            class="input input-sm input-bordered w-full"
            bind:value={config.terminal}
          />

          <span class="text-sm font-medium">--noconfirm:</span>
          <label class="label cursor-pointer justify-start gap-2">
            <input
              type="checkbox"
              class="checkbox checkbox-sm"
              bind:checked={config.noconfirm}
              disabled={!isArch}
            />
            <span class="label-text">Skip confirmation prompts</span>
          </label>

          <span class="text-sm font-medium">Autostart:</span>
          <label class="label cursor-pointer justify-start gap-2">
            <input
              type="checkbox"
              class="checkbox checkbox-sm"
              bind:checked={config.autostart}
              disabled={!isArch}
            />
            <span class="label-text">Start on login</span>
          </label>

          <span class="text-sm font-medium">Animations:</span>
          <label class="label cursor-pointer justify-start gap-2">
            <input
              type="checkbox"
              class="checkbox checkbox-sm"
              bind:checked={config.animations}
            />
            <span class="label-text">Animate tray icon</span>
          </label>

          <span class="text-sm font-medium">Theme:</span>
          <select
            class="select select-sm select-bordered w-full"
            bind:value={config.theme}
            onchange={handleThemeChange}
          >
            {#each THEMES as theme}
              <option value={theme}>{theme}</option>
            {/each}
          </select>

          <span class="text-sm font-medium">Re-check cooldown:</span>
          <label class="input input-sm input-bordered w-32 flex items-center gap-1">
            <input
              type="number"
              class="w-12 text-right"
              min="1"
              max="60"
              bind:value={config.recheck_interval_minutes}
            />
            <span class="text-xs opacity-60">minutes</span>
          </label>

          <span class="text-sm font-medium">Passwordless:</span>
          <label class="label cursor-pointer justify-start gap-2">
            <input
              type="checkbox"
              class="checkbox checkbox-sm"
              bind:checked={config.passwordless_updates}
              disabled={!isArch}
            />
            <span class="label-text">No sudo password for pacman</span>
          </label>
        </div>
      </div>

      <input
        type="radio"
        name="settings_tabs"
        class="tab z-1"
        aria-label="Tailscale"
        checked={activeTab === "tailscale"}
        onchange={() => (activeTab = "tailscale")}
      />
      <div class="sticky start-0 tab-content bg-base-100 border-base-300 p-4 overflow-y-auto">
        <div class="grid grid-cols-[auto_1fr] gap-x-4 gap-y-3 items-start">
          <span class="text-sm font-medium pt-1">Enable:</span>
          <label class="label cursor-pointer justify-start gap-2">
            <input
              type="checkbox"
              class="checkbox checkbox-sm"
              bind:checked={config.tailscale_enabled}
            />
            <span class="label-text">Check remote servers via Tailscale</span>
          </label>

          <span class="text-sm font-medium pt-1">Device tags:</span>
          <TagPillSelector
            bind:selected={config.tailscale_tags}
            disabled={!config.tailscale_enabled}
          />

          <span class="text-sm font-medium pt-1">SSH timeout:</span>
          <label class="input input-sm input-bordered w-32 flex items-center gap-1">
            <input
              type="number"
              class="w-12 text-right"
              min="5"
              max="60"
              bind:value={config.tailscale_timeout}
              disabled={!config.tailscale_enabled}
            />
            <span class="text-xs opacity-60">seconds</span>
          </label>
        </div>
      </div>
    </div>
    </div>

    <!-- Buttons -->
    <div class="flex justify-end gap-2">
      <button class="btn btn-sm" onclick={onclose}>Cancel</button>
      <button class="btn btn-sm btn-primary" onclick={handleSave}>Save</button>
    </div>
  </div>
{:else}
  <div class="flex items-center justify-center h-full">
    <span class="loading loading-spinner loading-md"></span>
  </div>
{/if}

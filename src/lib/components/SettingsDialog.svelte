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

  let { onclose }: { onclose: () => void } = $props();

  let config = $state<AppConfig | null>(null);
  let isArch = $state(false);
  let activeTab = $state<"general" | "tailscale">("general");

  onMount(async () => {
    [config, isArch] = await Promise.all([getConfig(), isArchLinux()]);
  });

  async function handleSave() {
    if (!config) return;

    // Enforce minimum check interval
    if (config.check_interval_minutes < 5) {
      config.check_interval_minutes = 5;
    }

    // Handle autostart toggle
    try {
      await manageAutostart(config.autostart);
    } catch (e) {
      console.error("Failed to manage autostart:", e);
    }

    // Handle passwordless updates toggle
    try {
      await managePasswordlessUpdates(config.passwordless_updates);
    } catch (e) {
      console.error("Failed to manage passwordless updates:", e);
    }

    await saveConfig(config);
    onclose();
  }
</script>

{#if config}
  <div class="flex flex-col h-full p-4 gap-4">
    <!-- Tabs -->
    <div role="tablist" class="tabs tabs-bordered">
      <button
        role="tab"
        class="tab"
        class:tab-active={activeTab === "general"}
        onclick={() => (activeTab = "general")}
      >
        General
      </button>
      <button
        role="tab"
        class="tab"
        class:tab-active={activeTab === "tailscale"}
        onclick={() => (activeTab = "tailscale")}
      >
        Tailscale
      </button>
    </div>

    <!-- Tab content -->
    <div class="flex-1 overflow-y-auto">
      {#if activeTab === "general"}
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
      {:else}
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
      {/if}
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

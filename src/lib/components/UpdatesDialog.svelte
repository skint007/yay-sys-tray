<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import type {
    CheckResult,
    HostResult,
  } from "../types";
  import { getCheckResult, runRemove, startCheck } from "../ipc";
  import UpdateCard from "./UpdateCard.svelte";
  import SplitButton from "./SplitButton.svelte";

  const RESTART_PACKAGES = new Set([
    "linux",
    "linux-lts",
    "linux-zen",
    "linux-hardened",
    "systemd",
    "glibc",
    "nvidia",
    "nvidia-lts",
  ]);

  let { onclose }: { onclose: () => void } = $props();

  let localResult = $state<CheckResult | null>(null);
  let remoteHosts = $state<HostResult[]>([]);
  let loading = $state(true);
  let activeTab = $state(0);

  let localUpdates = $derived(localResult?.updates ?? []);
  let localNeedsRestart = $derived(
    localUpdates.some((u) => RESTART_PACKAGES.has(u.package)),
  );
  let remoteWithUpdates = $derived(
    remoteHosts.filter((h) => h.updates.length > 0),
  );
  let useTabs = $derived(remoteWithUpdates.length > 0);
  let totalCount = $derived(
    localUpdates.length +
      remoteHosts.reduce((sum, h) => sum + h.updates.length, 0),
  );

  onMount(async () => {
    await loadResults();

    const unlisten = await listen<CheckResult>("check-complete", async () => {
      await loadResults();
    });

    return unlisten;
  });

  async function loadResults() {
    loading = true;
    try {
      const result = await getCheckResult();
      if (result) {
        localResult = result.local;
        remoteHosts = result.remote;
      }
    } catch (e) {
      console.error("Failed to load check results:", e);
    }
    loading = false;
  }

  function handleRemove(pkg: string, flags: string) {
    runRemove(pkg, flags).catch((e) =>
      console.error("Failed to remove package:", e),
    );
  }

  function handleLocalUpdate(restart: boolean) {
    // TODO: Phase 6 terminal.rs — run_local_update
    console.log("Local update, restart:", restart);
  }

  function handleRemoteUpdate(hostname: string, restart: boolean) {
    // TODO: Phase 6 terminal.rs — run_remote_update
    console.log("Remote update", hostname, "restart:", restart);
  }
</script>

<div class="flex flex-col h-full p-4 gap-3">
  <div class="flex items-center justify-between">
    <h2 class="text-lg font-bold">
      {#if totalCount > 0}
        {totalCount} Update{totalCount === 1 ? "" : "s"} Available
      {:else}
        No Updates
      {/if}
    </h2>
    <button class="btn btn-ghost btn-xs" onclick={() => startCheck()}>
      Refresh
    </button>
  </div>

  {#if loading}
    <div class="flex-1 flex items-center justify-center">
      <span class="loading loading-spinner loading-md"></span>
    </div>
  {:else if totalCount === 0}
    <div class="flex-1 flex items-center justify-center">
      <p class="text-base-content/50">System is up to date</p>
    </div>
  {:else if useTabs}
    <!-- Tabbed view: local + remote hosts -->
    <div role="tablist" class="tabs tabs-bordered">
      {#if localUpdates.length > 0}
        <button
          role="tab"
          class="tab"
          class:tab-active={activeTab === 0}
          class:text-error={localNeedsRestart}
          onclick={() => (activeTab = 0)}
        >
          Local ({localUpdates.length})
        </button>
      {/if}
      {#each remoteWithUpdates as host, i}
        <button
          role="tab"
          class="tab"
          class:tab-active={activeTab === i + 1}
          class:text-error={host.needs_restart}
          onclick={() => (activeTab = i + 1)}
        >
          {host.hostname} ({host.updates.length})
        </button>
      {/each}
    </div>

    <div class="flex-1 overflow-y-auto">
      {#if activeTab === 0 && localUpdates.length > 0}
        <div class="flex flex-col gap-1.5">
          {#each localUpdates as update (update.package)}
            <UpdateCard {update} onremove={handleRemove} />
          {/each}
        </div>
      {:else}
        {#each remoteWithUpdates as host, i}
          {#if activeTab === i + 1}
            <div class="flex flex-col gap-1.5">
              {#each host.updates as update (update.package)}
                <UpdateCard {update} />
              {/each}
            </div>
          {/if}
        {/each}
      {/if}
    </div>

    <!-- Action buttons for active tab -->
    <div class="flex justify-end gap-2">
      {#if activeTab === 0 && localUpdates.length > 0}
        {#if localNeedsRestart}
          <SplitButton onupdate={handleLocalUpdate} />
        {:else}
          <button
            class="btn btn-sm btn-primary"
            onclick={() => handleLocalUpdate(false)}
          >
            Update Now
          </button>
        {/if}
      {:else}
        {#each remoteWithUpdates as host, i}
          {#if activeTab === i + 1}
            {#if host.needs_restart}
              <SplitButton
                onupdate={(restart) =>
                  handleRemoteUpdate(host.hostname, restart)}
              />
            {:else}
              <button
                class="btn btn-sm btn-primary"
                onclick={() => handleRemoteUpdate(host.hostname, false)}
              >
                Update Now
              </button>
            {/if}
          {/if}
        {/each}
      {/if}
      <button class="btn btn-sm" onclick={onclose}>Close</button>
    </div>
  {:else}
    <!-- Single host view (local only) -->
    <div class="flex-1 overflow-y-auto">
      <div class="flex flex-col gap-1.5">
        {#each localUpdates as update (update.package)}
          <UpdateCard {update} onremove={handleRemove} />
        {/each}
      </div>
    </div>

    <div class="flex justify-end gap-2">
      {#if localNeedsRestart}
        <SplitButton onupdate={handleLocalUpdate} />
      {:else}
        <button
          class="btn btn-sm btn-primary"
          onclick={() => handleLocalUpdate(false)}
        >
          Update Now
        </button>
      {/if}
      <button class="btn btn-sm" onclick={onclose}>Close</button>
    </div>
  {/if}
</div>

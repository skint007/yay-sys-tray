<script lang="ts">
  import type { UpdateInfo } from "../types";
  import VersionDiff from "./VersionDiff.svelte";
  import RemoveMenu from "./RemoveMenu.svelte";
  import DependencyTree from "./DependencyTree.svelte";

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

  const REPO_COLORS: Record<string, string> = {
    core: "badge-info",
    extra: "badge-success",
    multilib: "badge-warning",
    aur: "badge-secondary",
  };

  let {
    update,
    onremove,
  }: {
    update: UpdateInfo;
    onremove?: (pkg: string, flags: string) => void;
  } = $props();

  let needsRestart = $derived(RESTART_PACKAGES.has(update.package));
  let repoClass = $derived(REPO_COLORS[update.repository] ?? "badge-ghost");

  let showDeps = $state<null | boolean>(null); // null=closed, true=forward, false=reverse
</script>

<div
  class="card card-compact bg-base-200 shadow-sm"
  title={update.description || undefined}
>
  <div class="card-body flex-row items-center gap-2 py-2 px-3">
    <!-- Package info -->
    <div class="flex-1 min-w-0">
      <div class="flex items-center gap-1.5 flex-wrap">
        <span class="font-bold text-sm truncate">{update.package}</span>
        {#if update.repository}
          <span class="badge badge-xs {repoClass}">{update.repository}</span>
        {/if}
        {#if needsRestart}
          <span class="badge badge-xs badge-error">restart</span>
        {/if}
      </div>
      <VersionDiff
        oldVersion={update.old_version}
        newVersion={update.new_version}
      />
    </div>

    <!-- Action icons -->
    <div class="flex items-center gap-0.5 shrink-0">
      {#if onremove}
        <RemoveMenu packageName={update.package} {onremove} />
      {/if}

      <button
        class="btn btn-ghost btn-xs btn-circle"
        title="Show dependencies"
        onclick={() => (showDeps = false)}
      >
        <span class="text-xs">&darr;</span>
      </button>

      <button
        class="btn btn-ghost btn-xs btn-circle"
        title="Show what depends on this"
        onclick={() => (showDeps = true)}
      >
        <span class="text-xs">&uarr;</span>
      </button>

      {#if update.url}
        <a
          href={update.url}
          target="_blank"
          rel="noopener"
          class="btn btn-ghost btn-xs btn-circle"
          title="Open package page"
        >
          <span class="text-xs">&nearr;</span>
        </a>
      {/if}
    </div>
  </div>
</div>

{#if showDeps !== null}
  <DependencyTree
    packageName={update.package}
    reverse={showDeps}
    onclose={() => (showDeps = null)}
  />
{/if}

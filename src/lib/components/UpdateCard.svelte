<script lang="ts">
  import type { UpdateInfo } from "../types";
  import VersionDiff from "./VersionDiff.svelte";

  let {
    update,
    selected = false,
    compact = false,
    restart = false,
    onToggle,
    onremove,
    onShowDeps,
  }: {
    update: UpdateInfo;
    selected?: boolean;
    compact?: boolean;
    restart?: boolean;
    onToggle?: () => void;
    onremove?: (pkg: string, flags: string) => void;
    onShowDeps?: (reverse: boolean) => void;
  } = $props();

  let removeOpen = $state(false);

  function doRemove(flags: string) {
    removeOpen = false;
    onremove?.(update.package, flags);
  }
</script>

<div class="pkg-card" class:selected class:compact>
  <input
    type="checkbox"
    class="ys-check"
    checked={selected}
    onchange={() => onToggle?.()}
    aria-label={`Select ${update.package}`}
  />

  <div class="info">
    <div class="name-row">
      <span class="pkg">{update.package}</span>
      {#if update.repository}
        <span class="pbadge repo" style={`--c: var(--ys-repo-${update.repository}, var(--ys-text-muted))`}>
          {update.repository}
        </span>
      {/if}
      {#if restart}
        <span class="pbadge restart">restart</span>
      {/if}
    </div>
    <VersionDiff oldVersion={update.old_version} newVersion={update.new_version} {compact} />
  </div>

  <div class="cluster">
    <button class="icon deps" title="Show dependencies" onclick={() => onShowDeps?.(false)} aria-label="Dependencies">
      <svg viewBox="0 0 24 24"><path d="M12 4v13M6 12l6 6 6-6" /></svg>
    </button>
    <button class="icon deps" title="Show what depends on this" onclick={() => onShowDeps?.(true)} aria-label="Required by">
      <svg viewBox="0 0 24 24"><path d="M12 20V7M6 12l6-6 6 6" /></svg>
    </button>
    {#if update.url}
      <a class="icon web" href={update.url} target="_blank" rel="noopener" title="Open package page" aria-label="Open package page">
        <svg viewBox="0 0 24 24"><path d="M8 16L16 8M9 8h7v7" /></svg>
      </a>
    {/if}
    {#if update.description}
      <span class="icon info-i" title={update.description} aria-label="Info">
        <svg viewBox="0 0 24 24"><circle cx="12" cy="12" r="8.5" /><path d="M12 11v5" /><circle cx="12" cy="7.8" r="0.6" fill="currentColor" stroke="none" /></svg>
      </span>
    {/if}
    {#if onremove}
      <div class="remove-wrap">
        <button class="icon remove" title="Remove package" onclick={() => (removeOpen = !removeOpen)} aria-label="Remove">
          <svg viewBox="0 0 24 24"><path d="M7 7l10 10M17 7L7 17" /></svg>
        </button>
        {#if removeOpen}
          <div class="rmenu">
            <button onclick={() => doRemove("R")}>Remove</button>
            <button onclick={() => doRemove("Rs")}>Remove + unneeded deps</button>
            <button onclick={() => doRemove("Rns")}>Remove + deps + config</button>
            <button class="danger" onclick={() => doRemove("Rdd")}>Force remove (skip dep checks)</button>
          </div>
        {/if}
      </div>
    {/if}
  </div>
</div>

<style>
  .pkg-card {
    display: flex;
    flex-direction: row;
    align-items: center;
    gap: 12px;
    background: var(--ys-surface-row);
    border: 1px solid var(--ys-line-soft);
    border-radius: 12px;
    padding: 13px 16px;
    transition: border-color 0.13s ease, background 0.13s ease;
  }
  .pkg-card.compact { padding: 6px 13px; gap: 10px; border-radius: 11px; }
  .pkg-card:hover { border-color: var(--ys-violet-500); background: var(--ys-surface); }
  .pkg-card.selected { border-color: var(--ys-violet-600); }

  .info { flex: 1; min-width: 0; }
  .name-row { display: flex; align-items: center; gap: 8px; margin-bottom: 3px; }
  .compact .name-row { margin-bottom: 1px; }
  .pkg {
    font-family: var(--font-display);
    font-weight: 600;
    font-size: 15px;
    color: var(--ys-text);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .compact .pkg { font-size: 13px; }

  .pbadge {
    font-family: var(--font-mono);
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.5px;
    padding: 2px 7px;
    border-radius: 5px;
    flex: none;
    line-height: 1.4;
  }
  .repo { color: var(--c); background: color-mix(in srgb, var(--c) 14%, transparent); }
  .restart {
    color: var(--ys-danger);
    background: color-mix(in srgb, var(--ys-danger) 14%, transparent);
  }

  .cluster {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 3px;
    border-radius: 9px;
    background: var(--ys-surface-input);
    border: 1px solid var(--ys-line-softer);
    flex: none;
  }
  .icon {
    width: 26px;
    height: 26px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 7px;
    color: var(--ys-text-dim);
    cursor: pointer;
    transition: color 0.12s ease, background 0.12s ease;
  }
  .icon svg { width: 15px; height: 15px; fill: none; stroke: currentColor; stroke-width: 1.7; stroke-linecap: round; stroke-linejoin: round; }
  .icon:hover { background: color-mix(in srgb, currentColor 12%, transparent); }
  .icon.deps:hover { color: var(--ys-violet-500); }
  .icon.web:hover, .icon.info-i:hover { color: var(--ys-sky); }
  .icon.remove:hover { color: var(--ys-danger); }

  .remove-wrap { position: relative; }
  .rmenu {
    position: absolute;
    right: 0;
    top: calc(100% + 4px);
    z-index: 20;
    display: flex;
    flex-direction: column;
    min-width: 220px;
    background: var(--ys-surface);
    border: 1px solid var(--ys-line);
    border-radius: 10px;
    padding: 5px;
  }
  .rmenu button {
    text-align: left;
    font-family: var(--font-body);
    font-size: 13px;
    color: var(--ys-text);
    padding: 7px 10px;
    border-radius: 6px;
    cursor: pointer;
  }
  .rmenu button:hover { background: color-mix(in srgb, var(--ys-violet-600) 20%, transparent); }
  .rmenu button.danger { color: var(--ys-danger); border-top: 1px solid var(--ys-line-soft); margin-top: 3px; padding-top: 9px; }
</style>

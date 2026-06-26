<script lang="ts">
  import { onMount } from "svelte";
  import { discoverTailscaleTags } from "../ipc";

  let {
    selected = $bindable(""),
    disabled = false,
  }: { selected: string; disabled?: boolean } = $props();

  let allTags = $state<string[]>([]);
  let selectedSet = $state<Set<string>>(new Set());

  onMount(async () => {
    const selectedTags = selected.split(",").map((t) => t.trim()).filter(Boolean);
    selectedSet = new Set(selectedTags);
    try {
      const available = await discoverTailscaleTags();
      allTags = [...new Set([...available, ...selectedTags])].sort();
    } catch {
      allTags = selectedTags.sort();
    }
  });

  function toggle(tag: string) {
    if (disabled) return;
    const next = new Set(selectedSet);
    next.has(tag) ? next.delete(tag) : next.add(tag);
    selectedSet = next;
    selected = [...next].sort().join(",");
  }
</script>

<div class="tags" class:disabled>
  {#if allTags.length === 0}
    <span class="empty">No tags found</span>
  {:else}
    {#each allTags as tag (tag)}
      <button
        type="button"
        class="tag"
        class:on={selectedSet.has(tag)}
        {disabled}
        onclick={() => toggle(tag)}
      >
        {tag}
      </button>
    {/each}
  {/if}
</div>

<style>
  .tags { display: flex; flex-wrap: wrap; gap: 6px; }
  .tags.disabled { opacity: 0.5; pointer-events: none; }
  .tag {
    font-family: var(--font-mono);
    font-weight: 600;
    font-size: 11px;
    color: var(--ys-text-muted);
    background: transparent;
    border: 1px solid var(--ys-line);
    border-radius: 13px;
    padding: 4px 12px;
    cursor: pointer;
    transition: color 0.12s ease, background 0.12s ease, border-color 0.12s ease;
  }
  .tag:hover { border-color: var(--ys-text-dim); color: var(--ys-text); }
  .tag.on {
    background: var(--ys-cyan);
    color: #0b0a12;
    border-color: var(--ys-cyan);
  }
  .empty { font-family: var(--font-body); font-size: 13px; font-style: italic; color: var(--ys-text-dim); }
</style>

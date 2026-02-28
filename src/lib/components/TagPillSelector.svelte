<script lang="ts">
  import { onMount } from "svelte";
  import { discoverTailscaleTags } from "../ipc";

  let {
    selected = $bindable(""),
    disabled = false,
  }: {
    selected: string;
    disabled: boolean;
  } = $props();

  let allTags = $state<string[]>([]);
  let selectedSet = $state<Set<string>>(new Set());

  onMount(async () => {
    // Parse the comma-separated selected tags
    const selectedTags = selected
      .split(",")
      .map((t) => t.trim())
      .filter(Boolean);
    selectedSet = new Set(selectedTags);

    // Discover available tags from Tailscale
    try {
      const available = await discoverTailscaleTags();
      const merged = new Set([...available, ...selectedTags]);
      allTags = [...merged].sort();
    } catch {
      allTags = selectedTags.sort();
    }
  });

  function toggle(tag: string) {
    if (disabled) return;
    const next = new Set(selectedSet);
    if (next.has(tag)) {
      next.delete(tag);
    } else {
      next.add(tag);
    }
    selectedSet = next;
    selected = [...next].sort().join(",");
  }
</script>

<div class="flex flex-wrap gap-1.5">
  {#if allTags.length === 0}
    <span class="text-sm italic opacity-50">No tags found</span>
  {:else}
    {#each allTags as tag}
      <button
        type="button"
        class="badge badge-lg cursor-pointer select-none transition-colors"
        class:badge-primary={selectedSet.has(tag)}
        class:badge-outline={!selectedSet.has(tag)}
        class:opacity-40={disabled}
        {disabled}
        onclick={() => toggle(tag)}
      >
        {tag}
      </button>
    {/each}
  {/if}
</div>

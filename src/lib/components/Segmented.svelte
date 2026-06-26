<script lang="ts">
  type Opt = { label: string; value: string };
  let {
    options,
    value = $bindable(""),
  }: { options: Opt[]; value: string } = $props();
</script>

<div class="seg">
  {#each options as o (o.value)}
    <button type="button" class:active={value === o.value} onclick={() => (value = o.value)}>
      {o.label}
    </button>
  {/each}
</div>

<style>
  .seg {
    display: inline-flex;
    background: var(--ys-surface);
    border: 1px solid var(--ys-line);
    border-radius: 999px;
    padding: 3px;
    gap: 2px;
  }
  .seg button {
    font-family: var(--font-body);
    font-weight: 600;
    font-size: 12px;
    color: var(--ys-text-dim);
    padding: 6px 16px;
    border-radius: 999px;
    cursor: pointer;
    transition: color 0.12s ease, background 0.12s ease;
  }
  .seg button.active { background: var(--ys-violet-600); color: #fff; }
</style>

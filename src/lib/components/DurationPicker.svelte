<script lang="ts">
  let { value = $bindable(0), disabled = false }: { value: number; disabled?: boolean } = $props();

  let days = $state(Math.floor(value / (24 * 60)));
  let hours = $state(Math.floor((value % (24 * 60)) / 60));
  let minutes = $state(value % 60);

  $effect(() => {
    value = (days * 24 + hours) * 60 + minutes;
  });
</script>

<div class="dur" class:disabled>
  <label><input type="number" min="0" max="99" bind:value={days} {disabled} /><span>d</span></label>
  <label><input type="number" min="0" max="23" bind:value={hours} {disabled} /><span>h</span></label>
  <label><input type="number" min="0" max="59" bind:value={minutes} {disabled} /><span>m</span></label>
</div>

<style>
  .dur { display: flex; gap: 6px; }
  .dur label {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    background: var(--ys-surface-input);
    border: 1px solid var(--ys-line);
    border-radius: 9px;
    padding: 6px 9px;
  }
  .dur input {
    width: 28px;
    text-align: right;
    background: transparent;
    color: var(--ys-text);
    font-family: var(--font-mono);
    font-size: 13px;
    outline: none;
    border: none;
    -moz-appearance: textfield;
  }
  .dur input::-webkit-outer-spin-button,
  .dur input::-webkit-inner-spin-button { -webkit-appearance: none; margin: 0; }
  .dur span { color: var(--ys-text-dim); font-family: var(--font-mono); font-size: 12px; }
  .dur.disabled { opacity: 0.5; }
</style>

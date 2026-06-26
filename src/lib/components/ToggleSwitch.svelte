<script lang="ts">
  let {
    checked = $bindable(false),
    accent = "violet",
    disabled = false,
  }: { checked?: boolean; accent?: "violet" | "cyan"; disabled?: boolean } = $props();
</script>

<button
  type="button"
  class="ys-toggle {accent}"
  class:on={checked}
  {disabled}
  role="switch"
  aria-checked={checked}
  onclick={() => !disabled && (checked = !checked)}
>
  <span class="knob"></span>
</button>

<style>
  .ys-toggle {
    width: 44px;
    height: 24px;
    flex: none;
    padding: 0;
    border-radius: 999px;
    background: var(--ys-surface-input);
    border: 1px solid var(--ys-checkbox-border);
    position: relative;
    cursor: pointer;
    transition: background 0.15s ease, border-color 0.15s ease, box-shadow 0.15s ease;
  }
  .knob {
    position: absolute;
    top: 2px;
    left: 2px;
    width: 18px;
    height: 18px;
    border-radius: 50%;
    background: var(--ys-text-dim);
    transition: left 0.15s cubic-bezier(0.34, 1.2, 0.64, 1), background 0.15s ease;
  }
  .ys-toggle.on { border-color: transparent; }
  .ys-toggle.violet.on {
    background: var(--ys-violet-600);
    box-shadow: 0 0 14px -2px color-mix(in srgb, var(--ys-violet-600) 70%, transparent);
  }
  .ys-toggle.cyan.on {
    background: var(--ys-cyan);
    box-shadow: 0 0 14px -2px color-mix(in srgb, var(--ys-cyan) 70%, transparent);
  }
  .ys-toggle.on .knob { left: 24px; background: #fff; }
  .ys-toggle:disabled { opacity: 0.5; cursor: not-allowed; }
</style>

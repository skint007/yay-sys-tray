<script lang="ts">
  let {
    onnavigate,
    onquit,
  }: { onnavigate?: (view: "settings" | "about") => void; onquit?: () => void } = $props();

  let open = $state(false);

  function settings() { open = false; onnavigate?.("settings"); }
  function about() { open = false; onnavigate?.("about"); }
  function quit() { open = false; onquit?.(); }

  // Close on an outside click while the menu is open.
  $effect(() => {
    if (!open) return;
    const handler = (e: MouseEvent) => {
      if (!(e.target as Element)?.closest(".overflow")) open = false;
    };
    window.addEventListener("click", handler);
    return () => window.removeEventListener("click", handler);
  });
</script>

<div class="overflow">
  <button class="ovf-btn" class:open title="Menu" aria-label="Menu" onclick={() => (open = !open)}>
    <svg viewBox="0 0 24 24" fill="currentColor"><circle cx="5" cy="12" r="1.7" /><circle cx="12" cy="12" r="1.7" /><circle cx="19" cy="12" r="1.7" /></svg>
  </button>

  {#if open}
    <div class="menu">
      <button onclick={settings}>
        <svg class="violet" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="3" /><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 1 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 1 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 1 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 1 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z" /></svg>
        Settings
      </button>
      <button onclick={about}>
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="9" /><path d="M12 16v-4M12 8h.01" /></svg>
        About
      </button>
      <div class="divider"></div>
      <button class="quit" onclick={quit}>
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4" /><path d="M16 17l5-5-5-5M21 12H9" /></svg>
        Quit
      </button>
    </div>
  {/if}
</div>

<style>
  .overflow { position: relative; display: flex; align-items: center; }
  .ovf-btn {
    width: 28px;
    height: 28px;
    display: flex;
    align-items: center;
    justify-content: center;
    border: none;
    border-radius: 8px;
    background: transparent;
    color: var(--ys-text-muted);
    cursor: pointer;
    transition: color 0.12s ease, background 0.12s ease;
  }
  .ovf-btn svg { width: 18px; height: 18px; }
  .ovf-btn:hover, .ovf-btn.open {
    color: var(--ys-violet-text);
    background: color-mix(in srgb, var(--ys-violet-600) 18%, transparent);
  }

  .menu {
    position: absolute;
    top: calc(100% + 8px);
    right: 0;
    z-index: 60;
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 184px;
    padding: 6px;
    background: var(--ys-surface-row);
    border: 1px solid var(--ys-line);
    border-radius: 12px;
    box-shadow: 0 14px 34px -10px rgba(0, 0, 0, 0.55);
  }
  .menu button {
    display: flex;
    align-items: center;
    gap: 10px;
    font-family: var(--font-body);
    font-weight: 600;
    font-size: 13px;
    color: var(--ys-text);
    padding: 8px 10px;
    border-radius: 8px;
    cursor: pointer;
    text-align: left;
  }
  .menu button:hover { background: color-mix(in srgb, var(--ys-violet-600) 16%, transparent); }
  .menu button svg { width: 16px; height: 16px; flex: none; color: var(--ys-text-muted); }
  .menu button svg.violet { color: var(--ys-violet-text); }
  .menu button.quit { color: var(--ys-text-muted); }
  .divider { height: 1px; background: var(--ys-line-soft); margin: 4px 2px; }
</style>

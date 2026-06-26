<script lang="ts">
  // Custom window controls for the frameless window. Calls are lazy + guarded
  // so the browser preview (no Tauri runtime) doesn't error.
  function ctl(action: "min" | "max" | "close") {
    import("@tauri-apps/api/window")
      .then(({ getCurrentWindow }) => {
        const w = getCurrentWindow();
        if (action === "min") w.minimize();
        else if (action === "max") w.toggleMaximize();
        else w.hide(); // tray app: close hides the window, tray stays
      })
      .catch(() => {});
  }
</script>

<div class="wctl">
  <button class="dot min" title="Minimize" aria-label="Minimize" onclick={() => ctl("min")}></button>
  <button class="dot max" title="Maximize" aria-label="Maximize" onclick={() => ctl("max")}></button>
  <button class="dot close" title="Close" aria-label="Close" onclick={() => ctl("close")}></button>
</div>

<style>
  .wctl { display: flex; align-items: center; gap: 9px; }
  .dot {
    width: 13px;
    height: 13px;
    border-radius: 50%;
    border: none;
    padding: 0;
    cursor: pointer;
    transition: filter 0.12s ease;
  }
  .min, .max { background: var(--ys-checkbox-border); }
  .close { background: var(--ys-danger); }
  .dot:hover { filter: brightness(1.25); }
</style>

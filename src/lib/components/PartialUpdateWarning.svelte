<script lang="ts">
  // Shown before an update that covers only some of a host's available
  // packages. Arch builds every package against the current version of its
  // dependencies, so leaving part of the system behind is the standard way to
  // end up with broken binaries — worth one interruption.
  let {
    selected,
    total,
    host,
    onconfirm,
    oncancel,
  }: {
    selected: number;
    total: number;
    host: string;
    onconfirm: (dontWarnAgain: boolean) => void;
    oncancel: () => void;
  } = $props();

  let dontWarnAgain = $state(false);
  let modalEl: HTMLElement | null = null;

  // Keep Tab inside the dialog. Without this, tabbing past the last button
  // walks into the list behind the overlay, where Enter can remove a package
  // or close the window while the warning is still up.
  function trapTab(e: KeyboardEvent) {
    if (e.key !== "Tab" || !modalEl) return;
    const stops = [...modalEl.querySelectorAll<HTMLElement>("button, input")];
    if (stops.length === 0) return;
    const first = stops[0];
    const last = stops[stops.length - 1];
    const here = document.activeElement;
    if (e.shiftKey ? here === first || here === modalEl : here === last) {
      e.preventDefault();
      (e.shiftKey ? last : first).focus();
    }
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="overlay" onclick={oncancel}>
  <!-- Focused on open so the warning is what a screen reader reads next, and
       so Enter/Space don't land back on the button that opened it. -->
  <div
    class="ysmodal"
    role="alertdialog"
    aria-labelledby="pu-title"
    tabindex="-1"
    bind:this={modalEl}
    onclick={(e) => e.stopPropagation()}
    onkeydown={trapTab}
    {@attach (el: HTMLElement) => el.focus()}
  >
    <div class="head">
      <span class="warn-dot"></span>
      <span class="title" id="pu-title">Partial update</span>
    </div>

    <div class="bodytext">
      <p>
        You're about to update <strong>{selected} of {total}</strong> packages on
        <strong>{host}</strong>, leaving the rest behind.
      </p>
      <p>
        Arch builds each package against the current version of everything it depends on. Updating
        only part of the system is the usual way to end up with programs that no longer start, and
        in bad cases an unbootable machine.
      </p>
      <p class="muted">Updating everything at once is the safe option.</p>
    </div>

    <div class="foot">
      <label class="dontshow">
        <input type="checkbox" class="ys-check sm" bind:checked={dontWarnAgain} />
        <span>Don't show this again</span>
      </label>
      <div class="acts">
        <button class="ysbtn ghost" onclick={oncancel}>Cancel</button>
        <button class="ysbtn danger" onclick={() => onconfirm(dontWarnAgain)}>
          Update {selected} anyway
        </button>
      </div>
    </div>
  </div>
</div>

<style>
  .overlay { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.5); display: flex; align-items: center; justify-content: center; z-index: 60; }
  .ysmodal {
    width: 90%; max-width: 460px;
    display: flex; flex-direction: column;
    background: var(--ys-ground); border: 1px solid var(--ys-line);
    border-radius: 14px; overflow: hidden;
    box-shadow: 0 30px 80px -20px rgba(0, 0, 0, 0.6);
  }

  .head { display: flex; align-items: center; gap: 10px; padding: 18px 20px 0; }
  .warn-dot { width: 8px; height: 8px; border-radius: 50%; flex: none; background: var(--ys-danger); }
  .title { font-family: var(--font-display); font-weight: 700; font-size: 17px; color: var(--ys-text); }

  .bodytext { padding: 12px 20px 4px; }
  .bodytext p { font-family: var(--font-body); font-size: 13px; line-height: 1.55; color: var(--ys-text-muted); margin: 0 0 10px; }
  .bodytext strong { color: var(--ys-text); font-weight: 700; }
  .bodytext .muted { color: var(--ys-text-dim); }

  .foot {
    display: flex; align-items: center; justify-content: space-between; gap: 12px;
    padding: 14px 20px 16px;
  }
  .dontshow {
    display: flex; align-items: center; gap: 8px; cursor: pointer;
    font-family: var(--font-body); font-size: 12px; color: var(--ys-text-dim);
  }
  .dontshow:hover { color: var(--ys-text-muted); }
  .acts { display: flex; align-items: center; gap: 8px; }

  .ysbtn { font-family: var(--font-display); font-weight: 600; font-size: 13px; border-radius: 19px; padding: 9px 16px; cursor: pointer; white-space: nowrap; }
  .ysbtn.ghost { background: var(--ys-surface); color: var(--ys-text-muted); border: 1px solid var(--ys-line); }
  .ysbtn.ghost:hover { border-color: var(--ys-violet-500); color: var(--ys-text); }
  .ysbtn.danger { background: color-mix(in srgb, var(--ys-danger) 16%, transparent); color: var(--ys-danger); border: 1px solid color-mix(in srgb, var(--ys-danger) 50%, transparent); }
  .ysbtn.danger:hover { background: color-mix(in srgb, var(--ys-danger) 24%, transparent); }
</style>

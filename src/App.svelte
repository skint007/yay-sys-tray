<script lang="ts">
  import "./app.css";
  import { emit, listen } from "@tauri-apps/api/event";
  import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";
  import { onMount } from "svelte";
  import { quitApp } from "./lib/ipc";
  import SettingsDialog from "./lib/components/SettingsDialog.svelte";
  import UpdatesDialog from "./lib/components/UpdatesDialog.svelte";
  import AboutDialog from "./lib/components/AboutDialog.svelte";
  import ResizeHandles from "./lib/components/ResizeHandles.svelte";

  type View = "settings" | "updates" | "about" | null;
  let currentView = $state<View>(null);
  let previousView: View = null;
  // Whether an update was launched from the Updates window since it was opened.
  // The only thing that may trigger the "close window after updating" setting.
  let updateLaunched = false;

  // SKINT007 follows the OS color scheme (dark by default, lit by intent).
  function applyTheme() {
    const dark = window.matchMedia("(prefers-color-scheme: dark)").matches;
    document.documentElement.setAttribute("data-theme", dark ? "skint007-dark" : "skint007-light");
  }

  // Per-view window size, seeded with defaults but remembered across switches
  // (so resizing the Updates window and coming back keeps your size) and
  // persisted to localStorage so it survives an app restart too.
  const SIZE_KEY = "ys-view-sizes";
  const defaultSizes: Record<string, [number, number]> = {
    updates: [900, 620],
    settings: [560, 700],
    about: [480, 470],
  };

  function loadSizes(): Record<string, [number, number]> {
    const result = { ...defaultSizes };
    try {
      const saved = JSON.parse(localStorage.getItem(SIZE_KEY) ?? "{}");
      for (const view of Object.keys(defaultSizes)) {
        const v = saved[view];
        if (Array.isArray(v) && v.length === 2 && v.every((n) => typeof n === "number" && n > 0)) {
          result[view] = [v[0], v[1]];
        }
      }
    } catch {}
    return result;
  }

  const sizes: Record<string, [number, number]> = loadSizes();
  let scaleFactor = 1;
  let suppressSave = false;
  let appliedView: View = null;
  let firstOpen = true;

  async function applyViewSize(view: string) {
    const [w, h] = sizes[view] ?? [900, 620];
    const win = getCurrentWindow();
    suppressSave = true;
    await win.setSize(new LogicalSize(w, h));
    if (firstOpen) {
      await win.center(); // centre only the first time; afterwards keep position
      firstOpen = false;
    }
    setTimeout(() => (suppressSave = false), 200);
  }

  $effect(() => {
    const view = currentView;
    if (!view || view === appliedView) return;
    appliedView = view;
    applyViewSize(view).catch(() => {});
  });

  onMount(async () => {
    applyTheme();
    window.matchMedia("(prefers-color-scheme: dark)").addEventListener("change", applyTheme);

    const win = getCurrentWindow();
    // Remember manual resizes per view (ignore the resizes we trigger ourselves).
    // Read the scale factor fresh each time — it changes when the window moves
    // between monitors of different DPI, and a stale factor would persist a
    // wrong (e.g. doubled) logical size.
    win.onResized(async ({ payload }) => {
      if (suppressSave || !currentView) return;
      let sf = scaleFactor;
      try { sf = await win.scaleFactor(); } catch {}
      sizes[currentView] = [
        Math.round(payload.width / sf),
        Math.round(payload.height / sf),
      ];
      try {
        localStorage.setItem(SIZE_KEY, JSON.stringify(sizes));
      } catch {}
    });

    await listen<{ view: string; fresh?: boolean }>("open-window", (event) => {
      // A fresh open is a new session: an update launched before the window was
      // last hidden must not close the window someone just asked for. A refocus
      // of a window that's already up is the same session continuing, and the
      // update running in it still owns the eventual close.
      if (event.payload.fresh !== false) updateLaunched = false;
      previousView = currentView;
      currentView = event.payload.view as View;
    });

    // "Close window after updating". The backend fires this once an update run
    // has left nothing pending anywhere. Owned here rather than in the Updates
    // dialog because that component is unmounted while the user is in Settings
    // or About, which would forget the update mid-run. If they are in one of
    // those views when it lands, the close is dropped rather than queued —
    // they're doing something else, and dismissing the window later would come
    // out of nowhere.
    await listen("close-after-update", () => {
      if (!updateLaunched || currentView !== "updates") return;
      hideWindow();
    });

    await emit("frontend-ready");
  });

  // Navigate to another view in-window (overflow menu), remembering where we
  // came from so the destination's Cancel/Save can go back.
  function navigate(view: View) {
    previousView = currentView;
    currentView = view;
  }

  function quit() {
    quitApp().catch(() => {});
  }

  // Back to tray-only mode: drop the view state so the next open re-applies its
  // size, then hide (a tray app never quits on close).
  function hideWindow() {
    previousView = null;
    currentView = null;
    appliedView = null;
    updateLaunched = false;
    getCurrentWindow().hide().catch(() => {});
  }

  function closeDialog() {
    // Settings/About reached from the Updates window → go back there instead of
    // hiding; otherwise (opened directly from the tray) hide the window.
    if ((currentView === "settings" || currentView === "about") && previousView) {
      currentView = previousView;
      previousView = null;
      return;
    }
    hideWindow();
  }
</script>

{#if currentView === "settings"}
  <SettingsDialog onclose={closeDialog} />
{:else if currentView === "updates"}
  <UpdatesDialog
    onclose={closeDialog}
    onnavigate={navigate}
    onquit={quit}
    onupdatelaunched={() => (updateLaunched = true)}
  />
{:else if currentView === "about"}
  <AboutDialog onclose={closeDialog} />
{:else}
  <!-- Hidden state: tray-only mode -->
{/if}

{#if currentView}
  <ResizeHandles />
{/if}

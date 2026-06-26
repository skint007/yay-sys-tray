<script lang="ts">
  import "./app.css";
  import { listen } from "@tauri-apps/api/event";
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

  // SKINT007 follows the OS color scheme (dark by default, lit by intent).
  function applyTheme() {
    const dark = window.matchMedia("(prefers-color-scheme: dark)").matches;
    document.documentElement.setAttribute("data-theme", dark ? "skint007-dark" : "skint007-light");
  }

  // Per-view window size, seeded with defaults but remembered across switches
  // (so resizing the Updates window and coming back keeps your size).
  const sizes: Record<string, [number, number]> = {
    updates: [900, 620],
    settings: [560, 700],
    about: [480, 470],
  };
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
    try {
      scaleFactor = await win.scaleFactor();
    } catch {}
    // Remember manual resizes per view (ignore the resizes we trigger ourselves).
    win.onResized(({ payload }) => {
      if (suppressSave || !currentView) return;
      sizes[currentView] = [
        Math.round(payload.width / scaleFactor),
        Math.round(payload.height / scaleFactor),
      ];
    });

    listen<{ view: string }>("open-window", (event) => {
      previousView = currentView;
      currentView = event.payload.view as View;
    });
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

  function closeDialog() {
    // Settings/About reached from the Updates window → go back there instead of
    // hiding; otherwise (opened directly from the tray) hide the window.
    if ((currentView === "settings" || currentView === "about") && previousView) {
      currentView = previousView;
      previousView = null;
      return;
    }
    previousView = null;
    currentView = null;
    appliedView = null;
    getCurrentWindow().hide().catch(() => {});
  }
</script>

{#if currentView === "settings"}
  <SettingsDialog onclose={closeDialog} />
{:else if currentView === "updates"}
  <UpdatesDialog onclose={closeDialog} onnavigate={navigate} onquit={quit} />
{:else if currentView === "about"}
  <AboutDialog onclose={closeDialog} />
{:else}
  <!-- Hidden state: tray-only mode -->
{/if}

{#if currentView}
  <ResizeHandles />
{/if}

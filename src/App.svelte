<script lang="ts">
  import "./app.css";
  import { listen } from "@tauri-apps/api/event";
  import { onMount } from "svelte";
  import { getConfig } from "./lib/ipc";
  import SettingsDialog from "./lib/components/SettingsDialog.svelte";
  import UpdatesDialog from "./lib/components/UpdatesDialog.svelte";
  import AboutDialog from "./lib/components/AboutDialog.svelte";

  let currentView = $state<"settings" | "updates" | "about" | null>(null);

  function applyTheme(theme: string) {
    const val = theme === "default" ? "" : theme;
    document.documentElement.setAttribute("data-theme", val);
  }

  onMount(async () => {
    const config = await getConfig();
    applyTheme(config.theme);

    listen<{ view: string }>("open-window", (event) => {
      currentView = event.payload.view as typeof currentView;
    });
  });

  function closeDialog() {
    currentView = null;
    // Hide the window when dialog closes
    import("@tauri-apps/api/webviewWindow").then(({ getCurrentWebviewWindow }) => {
      getCurrentWebviewWindow().hide();
    });
  }
</script>

{#if currentView === "settings"}
  <SettingsDialog onclose={closeDialog} />
{:else if currentView === "updates"}
  <UpdatesDialog onclose={closeDialog} />
{:else if currentView === "about"}
  <AboutDialog onclose={closeDialog} />
{:else}
  <!-- Hidden state: tray-only mode -->
{/if}

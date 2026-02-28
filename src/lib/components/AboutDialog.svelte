<script lang="ts">
  import { onMount } from "svelte";
  import { getVersion } from "../ipc";

  interface Props {
    onclose: () => void;
  }
  let { onclose }: Props = $props();
  let version = $state("...");

  onMount(async () => {
    version = await getVersion();
  });
</script>

<div class="p-6 text-center min-w-[300px]">
  <h2 class="text-xl font-bold mb-2">Yay Update Checker</h2>
  <p class="text-base-content/60 mb-4">Version {version}</p>
  <p class="text-sm text-base-content/40 mb-6">
    System tray update checker for Arch Linux
  </p>
  <button class="btn btn-sm" onclick={onclose}>Close</button>
</div>

<script lang="ts">
  import { getPactree } from "../ipc";

  let {
    packageName,
    reverse = false,
    onclose,
  }: { packageName: string; reverse: boolean; onclose: () => void } = $props();

  let tree = $state("");
  let loading = $state(true);

  $effect(() => {
    loading = true;
    getPactree(packageName, reverse).then((result) => {
      tree = result;
      loading = false;
    }).catch(() => {
      tree = "Failed to load dependency tree";
      loading = false;
    });
  });
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<!-- svelte-ignore a11y_click_events_have_key_events -->
<div
  class="fixed inset-0 bg-black/50 flex items-center justify-center z-50"
  onclick={onclose}
>
  <div
    class="bg-base-100 rounded-lg shadow-xl w-[90%] max-w-lg max-h-[80%] flex flex-col"
    onclick={(e) => e.stopPropagation()}
  >
    <div class="flex items-center justify-between p-3 border-b border-base-300">
      <h3 class="font-bold text-sm">
        {reverse ? "Reverse dependencies" : "Dependencies"}: {packageName}
      </h3>
      <button class="btn btn-ghost btn-xs btn-circle" onclick={onclose}
        >&times;</button
      >
    </div>
    <div class="flex-1 overflow-auto p-3">
      {#if loading}
        <div class="flex justify-center py-8">
          <span class="loading loading-spinner loading-sm"></span>
        </div>
      {:else}
        <pre class="text-xs font-mono whitespace-pre-wrap">{tree}</pre>
      {/if}
    </div>
  </div>
</div>

<script lang="ts">
  let {
    packageName,
    onremove,
  }: { packageName: string; onremove: (pkg: string, flags: string) => void } =
    $props();

  let open = $state(false);

  function handleRemove(flags: string) {
    open = false;
    onremove(packageName, flags);
  }
</script>

<div class="dropdown dropdown-end dropdown-top">
  <button
    class="btn btn-ghost btn-xs btn-circle text-error"
    title="Remove package"
    onclick={() => (open = !open)}
  >
    &times;
  </button>
  {#if open}
    <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
    <ul
      tabindex="0"
      class="dropdown-content menu bg-base-200 rounded-box z-10 w-72 p-2 shadow"
    >
      <li>
        <button onclick={() => handleRemove("R")}>Remove</button>
      </li>
      <li>
        <button onclick={() => handleRemove("Rs")}
          >Remove + unneeded deps</button
        >
      </li>
      <li>
        <button onclick={() => handleRemove("Rns")}
          >Remove + deps + config</button
        >
      </li>
      <li class="border-t border-base-300 mt-1 pt-1">
        <button class="text-warning" onclick={() => handleRemove("Rdd")}
          >Force remove (skip dep checks)</button
        >
      </li>
    </ul>
  {/if}
</div>

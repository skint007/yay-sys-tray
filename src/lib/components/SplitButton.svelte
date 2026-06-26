<script lang="ts">
  let {
    onupdate,
  }: { onupdate: (restart: boolean) => void } = $props();

  let open = $state(false);
</script>

<div class="join">
  <button
    class="btn btn-sm btn-primary join-item"
    onclick={() => onupdate(true)}
  >
    Update &amp; Restart
  </button>
  <div class="dropdown dropdown-end dropdown-top">
    <button
      class="btn btn-sm btn-primary join-item"
      onclick={() => (open = !open)}
    >
      <svg
        xmlns="http://www.w3.org/2000/svg"
        class="h-3 w-3"
        viewBox="0 0 20 20"
        fill="currentColor"
      >
        <path
          fill-rule="evenodd"
          d="M5.293 7.293a1 1 0 011.414 0L10 10.586l3.293-3.293a1 1 0 111.414 1.414l-4 4a1 1 0 01-1.414 0l-4-4a1 1 0 010-1.414z"
          clip-rule="evenodd"
        />
      </svg>
    </button>
    {#if open}
      <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
      <ul
        tabindex="0"
        class="dropdown-content menu bg-base-200 rounded-box z-10 w-52 p-2 shadow"
      >
        <li>
          <button
            onclick={() => {
              open = false;
              onupdate(false);
            }}>Update Only (no restart)</button
          >
        </li>
      </ul>
    {/if}
  </div>
</div>

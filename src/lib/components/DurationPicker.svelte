<script lang="ts">
  let { value = $bindable(0) }: { value: number } = $props();

  // Decompose total minutes into days/hours/minutes
  let days = $state(Math.floor(value / (24 * 60)));
  let hours = $state(Math.floor((value % (24 * 60)) / 60));
  let minutes = $state(value % 60);

  // Sync back to the bound value whenever any field changes
  $effect(() => {
    value = (days * 24 + hours) * 60 + minutes;
  });
</script>

<div class="flex items-center gap-1">
  <label class="input input-sm input-bordered w-20 flex items-center gap-1">
    <input
      type="number"
      class="w-10 text-right"
      min="0"
      max="99"
      bind:value={days}
    />
    <span class="text-xs opacity-60">d</span>
  </label>
  <label class="input input-sm input-bordered w-20 flex items-center gap-1">
    <input
      type="number"
      class="w-10 text-right"
      min="0"
      max="23"
      bind:value={hours}
    />
    <span class="text-xs opacity-60">h</span>
  </label>
  <label class="input input-sm input-bordered w-20 flex items-center gap-1">
    <input
      type="number"
      class="w-10 text-right"
      min="0"
      max="59"
      bind:value={minutes}
    />
    <span class="text-xs opacity-60">m</span>
  </label>
</div>

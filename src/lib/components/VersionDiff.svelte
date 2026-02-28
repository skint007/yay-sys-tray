<script lang="ts">
  let { oldVersion, newVersion }: { oldVersion: string; newVersion: string } =
    $props();

  function diffIndex(a: string, b: string): number {
    const min = Math.min(a.length, b.length);
    for (let i = 0; i < min; i++) {
      if (a[i] !== b[i]) return i;
    }
    return min;
  }

  let idx = $derived(diffIndex(oldVersion, newVersion));
  let oldCommon = $derived(oldVersion.slice(0, idx));
  let oldDiff = $derived(oldVersion.slice(idx));
  let newCommon = $derived(newVersion.slice(0, idx));
  let newDiff = $derived(newVersion.slice(idx));
</script>

<span class="text-xs font-mono whitespace-nowrap">
  <span class="opacity-50">{oldCommon}</span><span class="text-error"
    >{oldDiff}</span
  >
  <span class="opacity-40 mx-0.5">&rarr;</span>
  <span class="opacity-50">{newCommon}</span><span class="text-success"
    >{newDiff}</span
  >
</span>

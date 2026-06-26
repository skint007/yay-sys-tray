<script lang="ts">
  let {
    oldVersion,
    newVersion,
    compact = false,
  }: { oldVersion: string; newVersion: string; compact?: boolean } = $props();

  // Common prefix + common suffix; the differing middle is colored.
  function parts(a: string, b: string) {
    const n = Math.min(a.length, b.length);
    let p = 0;
    while (p < n && a[p] === b[p]) p++;
    let s = 0;
    while (s < n - p && a[a.length - 1 - s] === b[b.length - 1 - s]) s++;
    return {
      pre: a.slice(0, p),
      oldMid: a.slice(p, a.length - s),
      newMid: b.slice(p, b.length - s),
      suf: s ? a.slice(a.length - s) : "",
    };
  }

  let d = $derived(parts(oldVersion, newVersion));
</script>

<span class="vdiff" class:compact>
  <span class="base">{d.pre}</span><span class="old">{d.oldMid}</span><span class="base">{d.suf}</span>
  <span class="arrow">&rarr;</span>
  <span class="base">{d.pre}</span><span class="new">{d.newMid}</span><span class="base">{d.suf}</span>
</span>

<style>
  .vdiff {
    font-family: var(--font-mono);
    font-size: 13px;
    font-weight: 500;
    white-space: nowrap;
    letter-spacing: -0.2px;
  }
  .vdiff.compact { font-size: 12px; }
  .base { color: var(--ys-diff-base); }
  .old { color: var(--ys-diff-old); }
  .new { color: var(--ys-diff-new); }
  .arrow { color: var(--ys-diff-arrow); margin: 0 6px; }
</style>

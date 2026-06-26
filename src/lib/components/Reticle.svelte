<script lang="ts">
  // The SKINT007 reticle logomark — a targeting aperture (nod to "007").
  let { size = 20 }: { size?: number } = $props();
</script>

<svg width={size} height={size} viewBox="0 0 40 40" fill="none" class="reticle">
  <circle cx="20" cy="20" r="15" stroke="var(--ys-violet-text)" stroke-width="2" />
  <circle cx="20" cy="20" r="9" stroke="var(--ys-cyan)" stroke-width="1.5" />
  <g stroke="var(--ys-violet-text)" stroke-width="2" stroke-linecap="round">
    <line x1="20" y1="2" x2="20" y2="8" />
    <line x1="20" y1="32" x2="20" y2="38" />
    <line x1="2" y1="20" x2="8" y2="20" />
    <line x1="32" y1="20" x2="38" y2="20" />
  </g>
  <circle cx="20" cy="20" r="3.5" fill="var(--ys-violet-text)" />
</svg>

<style>
  .reticle { display: block; flex: none; }
</style>

<script lang="ts">
  import { getPactree } from "../ipc";

  let {
    packageName,
    reverse = false,
    repository = "",
    hostname = null,
    onclose,
  }: {
    packageName: string;
    reverse?: boolean;
    repository?: string;
    hostname?: string | null;
    onclose: () => void;
  } = $props();

  let dir = $state(reverse);
  let tree = $state("");
  let loading = $state(true);

  const BOX = new Set("│├└─┬┴┤┐┌┘ ");

  let lines = $derived.by(() => {
    return tree.split("\n").filter((l) => l.length > 0).map((line) => {
      let i = 0;
      while (i < line.length && BOX.has(line[i])) i++;
      const prefix = line.slice(0, i);
      const rest = line.slice(i);
      const sp = rest.indexOf(" ");
      return {
        prefix,
        name: sp === -1 ? rest : rest.slice(0, sp),
        ann: sp === -1 ? "" : rest.slice(sp),
        depth: Math.floor(prefix.length / 2),
      };
    });
  });

  let count = $derived(Math.max(0, lines.length - 1));
  let maxDepth = $derived(lines.reduce((m, l) => Math.max(m, l.depth), 0));

  function nameColor(depth: number) {
    if (depth === 0) return "var(--ys-text)";
    if (depth === 1) return "var(--ys-violet-text)";
    return "var(--ys-text-muted)";
  }

  $effect(() => {
    loading = true;
    getPactree(packageName, dir, hostname ?? undefined)
      .then((r) => { tree = r; loading = false; })
      .catch(() => { tree = "Failed to load dependency tree"; loading = false; });
  });
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="overlay" onclick={onclose}>
  <div class="ysmodal" onclick={(e) => e.stopPropagation()}>
    <div class="head">
      <span class="pkg">{packageName}</span>
      {#if repository}
        <span class="repo" style={`--c: var(--ys-repo-${repository}, var(--ys-text-muted))`}>{repository}</span>
      {/if}
      <div class="spacer"></div>
      <div class="seg">
        <button class:active={!dir} onclick={() => (dir = false)}>Depends on</button>
        <button class:active={dir} onclick={() => (dir = true)}>Required by</button>
      </div>
    </div>

    <div class="treebox" data-selectable>
      {#if loading}
        <div class="centered"><span class="spinner"></span></div>
      {:else}
        {#each lines as l}
          <div class="line"><span class="prefix">{l.prefix}</span><span style={`color:${nameColor(l.depth)}`}>{l.name}</span><span class="ann">{l.ann}</span></div>
        {/each}
      {/if}
    </div>

    <div class="foot">
      <span class="summary">{count} {dir ? "dependents" : "dependencies"} · depth {maxDepth}</span>
      <button class="ysbtn ghost" onclick={onclose}>Close</button>
    </div>
  </div>
</div>

<style>
  .overlay { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.5); display: flex; align-items: center; justify-content: center; z-index: 50; }
  .ysmodal {
    width: 90%; max-width: 600px; max-height: 82%;
    display: flex; flex-direction: column;
    background: var(--ys-ground); border: 1px solid var(--ys-line);
    border-radius: 14px; overflow: hidden;
    box-shadow: 0 30px 80px -20px rgba(0, 0, 0, 0.6);
  }
  .head { display: flex; align-items: center; gap: 10px; padding: 16px 18px; }
  .pkg { font-family: var(--font-display); font-weight: 700; font-size: 18px; color: var(--ys-text); }
  .repo { font-family: var(--font-mono); font-weight: 600; font-size: 11px; color: var(--c); background: color-mix(in srgb, var(--c) 14%, transparent); border-radius: 5px; padding: 2px 8px; }
  .spacer { flex: 1; }
  .seg { display: inline-flex; background: var(--ys-surface); border: 1px solid var(--ys-line); border-radius: 999px; padding: 3px; gap: 2px; }
  .seg button { font-family: var(--font-body); font-weight: 600; font-size: 12px; color: var(--ys-text-dim); padding: 5px 12px; border-radius: 999px; cursor: pointer; }
  .seg button.active { background: var(--ys-violet-600); color: #fff; }

  .treebox {
    flex: 1; overflow: auto; margin: 0 18px;
    background: var(--ys-surface); border: 1px solid var(--ys-line);
    border-radius: 12px; padding: 14px 16px;
    font-family: var(--font-mono); font-size: 13px; line-height: 1.7;
  }
  .line { white-space: pre; }
  .prefix { color: var(--ys-text-dim); }
  .ann { color: var(--ys-text-dim); }

  .foot { display: flex; align-items: center; justify-content: space-between; padding: 14px 18px; }
  .summary { font-family: var(--font-mono); font-weight: 500; font-size: 12px; color: var(--ys-text-dim); }
  .ysbtn { font-family: var(--font-display); font-weight: 600; font-size: 13px; border-radius: 19px; padding: 8px 18px; cursor: pointer; }
  .ysbtn.ghost { background: var(--ys-surface); color: var(--ys-text-muted); border: 1px solid var(--ys-line); }
  .ysbtn.ghost:hover { border-color: var(--ys-violet-500); color: var(--ys-text); }

  .centered { display: flex; align-items: center; justify-content: center; padding: 32px; }
  .spinner { width: 24px; height: 24px; border-radius: 50%; border: 3px solid var(--ys-line); border-top-color: var(--ys-violet-500); animation: spin 0.8s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
</style>

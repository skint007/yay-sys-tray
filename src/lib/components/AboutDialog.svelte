<script lang="ts">
  import { onMount } from "svelte";
  import { getVersion } from "../ipc";
  import Reticle from "./Reticle.svelte";
  import WindowControls from "./WindowControls.svelte";

  let { onclose }: { onclose: () => void } = $props();
  let version = $state("…");

  onMount(async () => {
    try { version = await getVersion(); } catch { version = "?"; }
  });
</script>

<div class="dialog">
  <header class="header" data-tauri-drag-region>
    <Reticle size={20} />
    <span class="title">About Yay-Sys-Tray</span>
    <div class="drag-spacer"></div>
    <WindowControls />
  </header>

  <div class="body">
    <div class="hero">
      <div class="bloom"></div>
      <Reticle size={64} />
    </div>
    <h1 class="hero-title">Yay Update Checker</h1>
    <div class="chips">
      <span class="chip violet">v{version}</span>
      <span class="chip muted">TAURI&nbsp;·&nbsp;SVELTE</span>
    </div>
    <p class="desc">
      A lightweight system-tray update checker for Arch Linux — with optional
      remote monitoring over Tailscale SSH.
    </p>
  </div>

  <footer class="footer">
    <span class="stack">Rust&nbsp;·&nbsp;Tauri&nbsp;2</span>
    <button class="ysbtn primary" onclick={onclose}>OK</button>
  </footer>
</div>

<style>
  .dialog { display: flex; flex-direction: column; height: 100%; background: var(--ys-ground); color: var(--ys-text); overflow: hidden; }
  .header { display: flex; align-items: center; gap: 10px; height: 50px; padding: 0 18px; background: var(--ys-titlebar); border-bottom: 1px solid var(--ys-line-softer); }
  .title { font-family: var(--font-display); font-weight: 600; font-size: 15px; }
  .drag-spacer { flex: 1; align-self: stretch; }
  .header > * { pointer-events: none; }
  .header > :last-child { pointer-events: auto; }

  .body { flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 14px; padding: 20px 40px; }
  .hero { position: relative; display: flex; align-items: center; justify-content: center; width: 150px; height: 120px; }
  .bloom { position: absolute; inset: 0; background: radial-gradient(circle at center, var(--ys-bloom), transparent 68%); }

  .hero-title { font-family: var(--font-display); font-weight: 700; font-size: 25px; letter-spacing: -0.3px; color: var(--ys-text); margin: 0; }
  .chips { display: flex; gap: 8px; }
  .chip { font-family: var(--font-mono); font-weight: 600; font-size: 11px; border-radius: 9px; padding: 3px 10px; }
  .chip.violet { color: var(--ys-violet-text); background: color-mix(in srgb, var(--ys-violet-600) 16%, transparent); border: 1px solid color-mix(in srgb, var(--ys-violet-600) 35%, transparent); }
  .chip.muted { color: var(--ys-text-dim); border: 1px solid var(--ys-line); }
  .desc { font-family: var(--font-body); font-size: 14px; color: var(--ys-text-muted); text-align: center; max-width: 370px; line-height: 1.5; margin: 0; }

  .footer { display: flex; align-items: center; justify-content: space-between; padding: 12px 18px; background: var(--ys-titlebar); border-top: 1px solid var(--ys-line-softer); }
  .stack { font-family: var(--font-mono); font-weight: 500; font-size: 11px; color: var(--ys-text-dim); }
  .ysbtn { font-family: var(--font-display); font-weight: 600; font-size: 13px; border-radius: 19px; padding: 9px 22px; cursor: pointer; }
  .ysbtn.primary { background: linear-gradient(var(--ys-violet-500), var(--ys-violet-600)); color: #fff; border: none; box-shadow: var(--ys-glow); }
  .ysbtn.primary:hover { background: linear-gradient(var(--ys-violet-400), var(--ys-violet-500)); }
</style>

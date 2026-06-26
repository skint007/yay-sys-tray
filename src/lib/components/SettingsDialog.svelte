<script lang="ts">
  import { onMount } from "svelte";
  import type { AppConfig } from "../types";
  import {
    getConfig,
    saveConfig,
    isArchLinux,
    manageAutostart,
    managePasswordlessUpdates,
  } from "../ipc";
  import Reticle from "./Reticle.svelte";
  import WindowControls from "./WindowControls.svelte";
  import Segmented from "./Segmented.svelte";
  import ToggleSwitch from "./ToggleSwitch.svelte";
  import DurationPicker from "./DurationPicker.svelte";
  import TagPillSelector from "./TagPillSelector.svelte";

  let { onclose }: { onclose: () => void } = $props();

  let config = $state<AppConfig | null>(null);
  let isArch = $state(false);
  let activeTab = $state("general");
  const initial = { autostart: false, passwordless: false };

  let tagCount = $derived(
    config ? config.tailscale_tags.split(",").map((t) => t.trim()).filter(Boolean).length : 0,
  );

  onMount(async () => {
    [config, isArch] = await Promise.all([getConfig(), isArchLinux()]);
    if (config) {
      initial.autostart = config.autostart;
      initial.passwordless = config.passwordless_updates;
    }
  });

  async function handleSave() {
    if (!config) return;
    if (config.check_interval_minutes < 5) config.check_interval_minutes = 5;
    if (config.autostart !== initial.autostart) {
      try { await manageAutostart(config.autostart); initial.autostart = config.autostart; } catch (e) { console.error(e); }
    }
    if (config.passwordless_updates !== initial.passwordless) {
      try { await managePasswordlessUpdates(config.passwordless_updates); initial.passwordless = config.passwordless_updates; } catch (e) { console.error(e); }
    }
    await saveConfig(config);
    onclose();
  }
</script>

<div class="dialog">
  <header class="header" data-tauri-drag-region>
    <Reticle size={20} />
    <span class="title">Settings</span>
    <div class="drag-spacer"></div>
    <WindowControls />
  </header>

  <div class="tabs-row">
    <Segmented options={[{ label: "General", value: "general" }, { label: "Tailscale", value: "tailscale" }]} bind:value={activeTab} />
  </div>

  {#if config}
    <div class="scroll">
      {#if activeTab === "general"}
        <div class="scard row">
          <div class="block">
            <div class="scard-title">Scheduled check</div>
            <div class="scard-desc">Runs automatically once a week</div>
          </div>
          <select class="ys-select compact" bind:value={config.scheduled_check_day} disabled={!config.scheduled_check_enabled}>
            {#each ["Monday","Tuesday","Wednesday","Thursday","Friday","Saturday","Sunday"] as d, i}
              <option value={i}>{d}</option>
            {/each}
          </select>
          <input class="ys-input compact" type="time" bind:value={config.scheduled_check_time} disabled={!config.scheduled_check_enabled} />
          <ToggleSwitch bind:checked={config.scheduled_check_enabled} />
        </div>

        <div class="scard row">
          <div class="block">
            <div class="scard-title">Automatic re-check</div>
            <div class="scard-desc">Check for updates on a fixed interval</div>
          </div>
          <DurationPicker bind:value={config.check_interval_minutes} disabled={!config.check_interval_enabled} />
          <ToggleSwitch bind:checked={config.check_interval_enabled} />
        </div>

        <div class="grid3">
          <div class="scard small">
            <div class="scard-label">NOTIFICATIONS</div>
            <select class="ys-select" bind:value={config.notify}>
              <option value="always">always</option>
              <option value="new_only">new_only</option>
              <option value="never">never</option>
            </select>
          </div>
          <div class="scard small">
            <div class="scard-label">TERMINAL</div>
            <input class="ys-input" type="text" bind:value={config.terminal} />
          </div>
          <div class="scard small">
            <div class="scard-label">RE-CHECK</div>
            <label class="ys-num"><input type="number" min="1" max="60" bind:value={config.recheck_interval_minutes} /><span>min</span></label>
          </div>
        </div>

        <div class="scard row">
          <div class="block">
            <div class="scard-title">Restart delay</div>
            <div class="scard-desc">Wait before rebooting after an update (0 = immediate)</div>
          </div>
          <label class="ys-num"><input type="number" min="0" max="600" bind:value={config.restart_delay_seconds} /><span>s</span></label>
        </div>

        <div class="scard toggles">
          <div class="trow">
            <div class="block"><div class="row-title">Skip confirmation prompts</div><div class="row-desc">--noconfirm</div></div>
            <ToggleSwitch bind:checked={config.noconfirm} disabled={!isArch} />
          </div>
          <div class="div"></div>
          <div class="trow">
            <div class="block"><div class="row-title">Keep terminal open after finish</div></div>
            <ToggleSwitch bind:checked={config.hold_terminal} />
          </div>
          <div class="div"></div>
          <div class="trow">
            <div class="block"><div class="row-title">Start on login</div></div>
            <ToggleSwitch bind:checked={config.autostart} disabled={!isArch} />
          </div>
          <div class="div"></div>
          <div class="trow">
            <div class="block"><div class="row-title">Animate tray icon</div></div>
            <ToggleSwitch bind:checked={config.animations} />
          </div>
          <div class="div"></div>
          <div class="trow">
            <div class="block"><div class="row-title">No sudo password for pacman</div></div>
            <ToggleSwitch bind:checked={config.passwordless_updates} disabled={!isArch} />
          </div>
        </div>
      {:else}
        <div class="scard row">
          <div class="block">
            <div class="scard-title">Check remote servers</div>
            <div class="scard-desc">Monitor other machines over Tailscale SSH</div>
          </div>
          <ToggleSwitch bind:checked={config.tailscale_enabled} />
        </div>

        <div class="scard">
          <div class="scard-label">DEVICE TAGS · {tagCount} SELECTED</div>
          <div style="margin-top:10px">
            <TagPillSelector bind:selected={config.tailscale_tags} disabled={!config.tailscale_enabled} />
          </div>
        </div>

        <div class="grid2">
          <div class="scard small">
            <div class="scard-label">SSH USER</div>
            <input class="ys-input" type="text" placeholder="(default)" bind:value={config.tailscale_ssh_user} disabled={!config.tailscale_enabled} />
          </div>
          <div class="scard small">
            <div class="scard-label">TIMEOUT</div>
            <label class="ys-num"><input type="number" min="5" max="60" bind:value={config.tailscale_timeout} disabled={!config.tailscale_enabled} /><span>s</span></label>
          </div>
        </div>

        <div class="scard row">
          <div class="block">
            <div class="scard-title">Vertical tabs on the left</div>
            <div class="scard-desc">Better for small screens</div>
          </div>
          <ToggleSwitch bind:checked={config.vertical_update_tabs} accent="cyan" />
        </div>
      {/if}
    </div>

    <footer class="footer">
      <button class="ysbtn ghost" onclick={onclose}>Cancel</button>
      <button class="ysbtn primary" onclick={handleSave}>Save</button>
    </footer>
  {:else}
    <div class="centered"><span class="spinner"></span></div>
  {/if}
</div>

<style>
  .dialog { display: flex; flex-direction: column; height: 100%; background: var(--ys-ground); color: var(--ys-text); overflow: hidden; }
  .header { display: flex; align-items: center; gap: 10px; height: 50px; padding: 0 18px; background: var(--ys-titlebar); border-bottom: 1px solid var(--ys-line-softer); }
  .title { font-family: var(--font-display); font-weight: 600; font-size: 15px; }
  .drag-spacer { flex: 1; align-self: stretch; }
  .header > * { pointer-events: none; }
  .header > :last-child { pointer-events: auto; }
  .tabs-row { padding: 14px 20px 6px; }
  .scroll { flex: 1; overflow-y: auto; padding: 8px 20px 16px; display: flex; flex-direction: column; gap: 12px; }

  .scard { background: var(--ys-surface); border: 1px solid var(--ys-line); border-radius: 11px; padding: 14px 16px; }
  .scard.row { display: flex; flex-direction: row; align-items: center; gap: 12px; }
  .scard.row .block { flex: 1; }
  .scard.small { padding: 12px 14px; }
  .scard-title { font-family: var(--font-display); font-weight: 600; font-size: 13px; color: var(--ys-text); }
  .scard-desc { font-family: var(--font-body); font-size: 12px; color: var(--ys-text-muted); margin-top: 2px; }
  .scard-label { font-family: var(--font-mono); font-weight: 600; font-size: 11px; letter-spacing: 1.5px; color: var(--ys-text-dim); }

  .grid3 { display: grid; grid-template-columns: repeat(3, 1fr); gap: 12px; }
  .grid2 { display: grid; grid-template-columns: repeat(2, 1fr); gap: 12px; }
  .scard.small .scard-label { margin-bottom: 8px; }
  .ys-select.compact { width: auto; min-width: 120px; }
  .ys-input.compact { width: auto; }
  input[type="time"].ys-input { font-family: var(--font-mono); }

  .toggles { padding: 4px 16px; }
  .trow { display: flex; align-items: center; gap: 12px; padding: 12px 0; }
  .trow .block { flex: 1; }
  .row-title { font-family: var(--font-body); font-weight: 600; font-size: 13px; color: var(--ys-text); }
  .row-desc { font-family: var(--font-mono); font-weight: 500; font-size: 11px; color: var(--ys-text-dim); margin-top: 2px; }
  .div { height: 1px; background: var(--ys-line-soft); }

  .ys-input, .ys-select {
    width: 100%;
    background: var(--ys-surface-input);
    border: 1px solid var(--ys-line);
    border-radius: 9px;
    padding: 7px 10px;
    color: var(--ys-text);
    font-family: var(--font-mono);
    font-size: 13px;
    outline: none;
  }
  .ys-input:focus, .ys-select:focus { border-color: var(--ys-violet-600); }
  .ys-select {
    font-family: var(--font-body);
    appearance: none;
    -webkit-appearance: none;
    background-image: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24' fill='none' stroke='%23807c92' stroke-width='2' stroke-linecap='round' stroke-linejoin='round'%3E%3Cpath d='M6 9l6 6 6-6'/%3E%3C/svg%3E");
    background-repeat: no-repeat;
    background-position: right 10px center;
    background-size: 14px;
    padding-right: 30px;
  }
  .ys-num {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    background: var(--ys-surface-input);
    border: 1px solid var(--ys-line);
    border-radius: 9px;
    padding: 7px 10px;
  }
  .ys-num input { width: 36px; text-align: right; background: transparent; border: none; outline: none; color: var(--ys-text); font-family: var(--font-mono); font-size: 13px; -moz-appearance: textfield; }
  .ys-num input::-webkit-outer-spin-button, .ys-num input::-webkit-inner-spin-button { -webkit-appearance: none; margin: 0; }
  .ys-num span { color: var(--ys-text-dim); font-family: var(--font-mono); font-size: 12px; }

  .footer { display: flex; justify-content: flex-end; gap: 10px; padding: 12px 18px; background: var(--ys-titlebar); border-top: 1px solid var(--ys-line-softer); }
  .ysbtn { font-family: var(--font-display); font-weight: 600; font-size: 13px; border-radius: 19px; padding: 9px 20px; cursor: pointer; }
  .ysbtn.ghost { background: var(--ys-surface); color: var(--ys-text-muted); border: 1px solid var(--ys-line); }
  .ysbtn.ghost:hover { border-color: var(--ys-violet-500); color: var(--ys-text); }
  .ysbtn.primary { background: linear-gradient(var(--ys-violet-500), var(--ys-violet-600)); color: #fff; border: none; box-shadow: var(--ys-glow); }
  .ysbtn.primary:hover { background: linear-gradient(var(--ys-violet-400), var(--ys-violet-500)); }

  .centered { flex: 1; display: flex; align-items: center; justify-content: center; }
  .spinner { width: 28px; height: 28px; border-radius: 50%; border: 3px solid var(--ys-line); border-top-color: var(--ys-violet-500); animation: spin 0.8s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
</style>

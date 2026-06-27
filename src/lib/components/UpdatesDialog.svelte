<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import type { CheckResult, HostResult, UpdateInfo } from "../types";
  import {
    getCheckResult,
    runRemove,
    runRemoteRemove,
    runLocalUpdate,
    runRemoteUpdate,
    runLocalUpdatePackages,
    runRemoteUpdatePackages,
    runUpdateSelected,
    startCheck,
  } from "../ipc";
  import UpdateCard from "./UpdateCard.svelte";
  import Reticle from "./Reticle.svelte";
  import DependencyTree from "./DependencyTree.svelte";
  import WindowControls from "./WindowControls.svelte";
  import OverflowMenu from "./OverflowMenu.svelte";

  type ScanStatus = "queued" | "checking" | "done" | "error";

  let {
    onclose,
    onnavigate,
    onquit,
    previewChecking,
  }: {
    onclose: () => void;
    onnavigate?: (view: "settings" | "about") => void;
    onquit?: () => void;
    // Dev-only: seed the "checking" view for the preview harness (no IPC).
    previewChecking?: {
      hosts: { key: string; name: string }[];
      status: Record<string, ScanStatus>;
      counts: Record<string, number>;
      startedSecAgo?: number;
    };
  } = $props();

  type Host = {
    key: string;
    name: string;
    updates: UpdateInfo[];
    needsRestart: boolean;
    restartPkgs: string[];
    checkable: boolean;
  };

  let localResult = $state<CheckResult | null>(null);
  let remoteHosts = $state<HostResult[]>([]);
  let loading = $state(true);

  // Surface #6 — live "checking" state, driven by backend scan-progress events.
  let checking = $state(false);
  let scanHosts = $state<{ key: string; name: string }[]>([]);
  let scanStatus = $state<Record<string, ScanStatus>>({});
  let scanCounts = $state<Record<string, number>>({});
  let checkStartAt = $state(0);
  let elapsed = $state(0);

  let activeKey = $state("");
  let checkedHosts = $state<string[]>([]);
  let selectedByHost = $state<Record<string, string[]>>({});
  let density = $state<"roomy" | "compact">("roomy");
  let search = $state("");
  let primaryMenu = $state(false);
  let showDeps = $state<{ pkg: string; reverse: boolean; repo: string; host: string | null } | null>(null);

  let hosts = $derived.by<Host[]>(() => {
    const list: Host[] = [];
    const lu = localResult?.updates ?? [];
    if (lu.length > 0) {
      list.push({
        key: "local",
        name: "Local",
        updates: lu,
        needsRestart: localResult?.needs_restart ?? false,
        restartPkgs: localResult?.restart_packages ?? [],
        checkable: false,
      });
    }
    for (const h of remoteHosts) {
      if (h.updates.length > 0) {
        list.push({
          key: h.hostname,
          name: h.hostname,
          updates: h.updates,
          needsRestart: h.needs_restart,
          restartPkgs: h.restart_packages,
          checkable: true,
        });
      }
    }
    return list;
  });

  let totalCount = $derived(hosts.reduce((s, h) => s + h.updates.length, 0));
  let multiHost = $derived(hosts.length > 1);
  let activeHost = $derived(hosts.find((h) => h.key === activeKey) ?? hosts[0]);
  let activeSelected = $derived(selectedByHost[activeKey] ?? []);

  let grouped = $derived.by(() => {
    const h = activeHost;
    if (!h) return { restart: [] as UpdateInfo[], normal: [] as UpdateInfo[] };
    const q = search.toLowerCase();
    const ups = h.updates.filter((u) => !q || u.package.toLowerCase().includes(q));
    const rset = new Set(h.restartPkgs);
    const byName = (a: UpdateInfo, b: UpdateInfo) => a.package.localeCompare(b.package);
    return {
      restart: ups.filter((u) => rset.has(u.package)).sort(byName),
      normal: ups.filter((u) => !rset.has(u.package)).sort(byName),
    };
  });

  let visiblePkgs = $derived([...grouped.restart, ...grouped.normal].map((u) => u.package));
  let selCount = $derived(activeSelected.length);
  let allSelected = $derived(visiblePkgs.length > 0 && visiblePkgs.every((p) => activeSelected.includes(p)));
  let primaryLabel = $derived.by(() => {
    if (!activeHost) return "Update";
    const base = selCount > 0 ? "Update Selected" : "Update All";
    const suffix = activeHost.needsRestart ? " & Restart" : "";
    const count = selCount > 0 ? selCount : activeHost.updates.length;
    return `${base}${suffix} (${count})`;
  });

  // Checking-view aggregates.
  let scanDone = $derived(
    scanHosts.filter((h) => scanStatus[h.key] === "done" || scanStatus[h.key] === "error").length,
  );
  let scanTotal = $derived(scanHosts.length);
  let updatesSoFar = $derived(Object.values(scanCounts).reduce((a, b) => a + b, 0));
  let startedLabel = $derived.by(() => {
    const s = elapsed;
    if (s < 60) return `Started ${s}s ago`;
    return `Started ${Math.floor(s / 60)}m ${s % 60}s ago`;
  });

  function beginChecking(hosts: { key: string; name: string }[]) {
    scanHosts = hosts;
    const st: Record<string, ScanStatus> = {};
    for (const h of hosts) st[h.key] = "queued";
    scanStatus = st;
    scanCounts = {};
    checkStartAt = Date.now();
    elapsed = 0;
    checking = true;
  }

  // Tick the "Started Ns ago" readout while a scan is running.
  $effect(() => {
    if (!checking) return;
    const id = setInterval(() => {
      elapsed = Math.max(0, Math.round((Date.now() - checkStartAt) / 1000));
    }, 1000);
    return () => clearInterval(id);
  });

  $effect(() => {
    // Keep an active host selected once data arrives.
    if (hosts.length > 0 && !hosts.some((h) => h.key === activeKey)) {
      activeKey = hosts[0].key;
    }
  });

  // Prune stale selections when the host set changes — e.g. a re-check drops a
  // host that went offline or has no updates left after updating. Otherwise a
  // removed host keeps inflating the "Update All Remote (N)" count (and would
  // still be acted on by runRemoteBulk).
  $effect(() => {
    const validKeys = new Set(hosts.map((h) => h.key));

    const prunedChecked = checkedHosts.filter((k) => validKeys.has(k));
    if (prunedChecked.length !== checkedHosts.length) {
      checkedHosts = prunedChecked;
    }

    for (const key of Object.keys(selectedByHost)) {
      const host = hosts.find((h) => h.key === key);
      if (!host) {
        delete selectedByHost[key];
        continue;
      }
      // Also drop any selected packages the host no longer offers.
      const offered = new Set(host.updates.map((u) => u.package));
      const cur = selectedByHost[key];
      const pruned = cur.filter((p) => offered.has(p));
      if (pruned.length !== cur.length) selectedByHost[key] = pruned;
    }
  });

  onMount(async () => {
    // Preview harness: seed the checking view directly, no Tauri runtime.
    if (previewChecking) {
      scanHosts = previewChecking.hosts;
      scanStatus = { ...previewChecking.status };
      scanCounts = { ...previewChecking.counts };
      checkStartAt = Date.now() - (previewChecking.startedSecAgo ?? 0) * 1000;
      elapsed = previewChecking.startedSecAgo ?? 0;
      checking = true;
      return;
    }

    await loadResults();
    const offs = await Promise.all([
      listen<{ hosts: { key: string; name: string }[] }>("check-started", (e) =>
        beginChecking(e.payload.hosts),
      ),
      listen<string>("check-host-start", (e) => {
        scanStatus = { ...scanStatus, [e.payload]: "checking" };
      }),
      listen<{ key: string; count: number; needs_restart: boolean; error: boolean }>(
        "check-host-done",
        (e) => {
          const { key, count, error } = e.payload;
          scanStatus = { ...scanStatus, [key]: error ? "error" : "done" };
          scanCounts = { ...scanCounts, [key]: count };
        },
      ),
      listen("check-complete", () => {
        checking = false;
        loadResults();
      }),
      listen("check-error", () => {
        checking = false;
        loadResults();
      }),
    ]);
    return () => offs.forEach((un) => un());
  });

  async function loadResults() {
    loading = true;
    try {
      const result = await getCheckResult();
      if (result) {
        localResult = result.local;
        remoteHosts = result.remote;
      }
    } catch (e) {
      console.error("Failed to load check results:", e);
    }
    loading = false;
  }

  function toggleHostCheck(key: string) {
    checkedHosts = checkedHosts.includes(key)
      ? checkedHosts.filter((k) => k !== key)
      : [...checkedHosts, key];
  }

  function togglePackage(pkg: string) {
    const cur = selectedByHost[activeKey] ?? [];
    selectedByHost[activeKey] = cur.includes(pkg) ? cur.filter((p) => p !== pkg) : [...cur, pkg];
  }

  function toggleSelectAll() {
    selectedByHost[activeKey] = allSelected ? [] : [...visiblePkgs];
  }

  function handleRemove(pkg: string, flags: string) {
    const p =
      activeKey === "local"
        ? runRemove(pkg, flags)
        : runRemoteRemove(activeKey, pkg, flags);
    p.catch((e) => console.error("remove failed:", e));
  }

  function runPrimary(restart: boolean) {
    primaryMenu = false;
    const sel = activeSelected;
    if (sel.length > 0) {
      const p =
        activeKey === "local"
          ? runLocalUpdatePackages(sel, restart)
          : runRemoteUpdatePackages(activeKey, sel, restart);
      p.catch(() => {});
    } else if (activeKey === "local") {
      runLocalUpdate(restart).catch(() => {});
    } else {
      runRemoteUpdate(activeKey, restart).catch(() => {});
    }
  }

  function runRemoteBulk() {
    // Each host reboots only if it actually needs a restart (backend decides).
    runUpdateSelected(checkedHosts, true).catch(() => {});
  }
</script>

<div class="dialog">
  <header class="header" data-tauri-drag-region>
    {#if checking}
      <span class="title-spinner"></span>
    {:else}
      <Reticle size={20} />
    {/if}
    <span class="title">Available Updates</span>
    {#if checking}
      <span class="checking-pill">CHECKING…</span>
    {:else}
      <span class="count-pill">{totalCount}</span>
    {/if}
    <div class="drag-spacer"></div>
    <div class="chrome-actions">
      <OverflowMenu {onnavigate} {onquit} />
      <WindowControls />
    </div>
  </header>

  {#if checking}
    <div class="progressbar" data-tauri-drag-region><span class="progressbar-seg"></span></div>
  {/if}

  {#if !checking}
    <div class="search">
      <svg class="search-icon" viewBox="0 0 24 24"><circle cx="11" cy="11" r="7" /><path d="M21 21l-4.3-4.3" /></svg>
      <input type="text" placeholder="Filter packages…" bind:value={search} />
    </div>
  {/if}

  {#if checking}
    <div class="body checking-body">
      {#if scanTotal > 1}
        <aside class="sidebar">
          {#each scanHosts as h (h.key)}
            <div class="scan-host" class:reboot={scanStatus[h.key] === "error"}>
              {#if scanStatus[h.key] === "checking"}
                <span class="scan-spinner"></span>
              {:else}
                <span
                  class="dot"
                  class:done={scanStatus[h.key] === "done"}
                  class:err={scanStatus[h.key] === "error"}
                ></span>
              {/if}
              <span class="host-name">{h.name}</span>
              <span class="scan-count">
                {#if scanStatus[h.key] === "done"}{scanCounts[h.key] ?? 0}
                {:else if scanStatus[h.key] === "checking"}···
                {:else if scanStatus[h.key] === "error"}!
                {:else}—{/if}
              </span>
            </div>
          {/each}
        </aside>
      {/if}

      <main class="hero">
        <div class="hero-spin">
          <span class="hero-ring"></span>
          <span class="hero-reticle"><Reticle size={42} /></span>
          <span class="hero-core"></span>
        </div>
        <div class="hero-title">Checking for updates…</div>
        <div class="hero-readout">
          {scanDone} OF {scanTotal} HOSTS SCANNED · {updatesSoFar} UPDATES SO FAR
        </div>
      </main>
    </div>

    <footer class="footer checking-footer">
      <span class="started">{startedLabel}</span>
      <button class="btn ghost cancel" onclick={onclose}>Cancel</button>
    </footer>
  {:else if loading}
    <div class="centered"><span class="spinner"></span></div>
  {:else if totalCount === 0}
    <div class="centered muted">System is up to date</div>
  {:else}
    <div class="body">
      {#if multiHost}
        <aside class="sidebar">
          {#each hosts as h (h.key)}
            <button
              class="host"
              class:active={h.key === activeKey}
              class:reboot={h.needsRestart}
              onclick={() => (activeKey = h.key)}
            >
              {#if h.checkable}
                <input
                  type="checkbox"
                  class="ys-check sm"
                  checked={checkedHosts.includes(h.key)}
                  onclick={(e) => e.stopPropagation()}
                  onchange={() => toggleHostCheck(h.key)}
                  aria-label={`Select ${h.name}`}
                />
              {:else}
                <span class="host-spacer"></span>
              {/if}
              <span class="dot"></span>
              <span class="host-name">{h.name}</span>
              <span class="host-count">{h.updates.length}</span>
            </button>
          {/each}
        </aside>
      {/if}

      <main class="list">
        {#if grouped.restart.length > 0}
          <div class="section restart"><span class="sdot"></span>RESTART REQUIRED</div>
          {#each grouped.restart as u (u.package)}
            <UpdateCard
              update={u}
              restart
              compact={density === "compact"}
              selected={activeSelected.includes(u.package)}
              onToggle={() => togglePackage(u.package)}
              onremove={handleRemove}
              onShowDeps={(reverse) => (showDeps = { pkg: u.package, reverse, repo: u.repository, host: activeKey === "local" ? null : activeKey })}
            />
          {/each}
        {/if}
        {#if grouped.normal.length > 0}
          <div class="section all"><span class="sdot"></span>ALL UPDATES</div>
          {#each grouped.normal as u (u.package)}
            <UpdateCard
              update={u}
              compact={density === "compact"}
              selected={activeSelected.includes(u.package)}
              onToggle={() => togglePackage(u.package)}
              onremove={handleRemove}
              onShowDeps={(reverse) => (showDeps = { pkg: u.package, reverse, repo: u.repository, host: activeKey === "local" ? null : activeKey })}
            />
          {/each}
        {/if}
      </main>
    </div>

    <footer class="footer">
      <div class="foot-left">
        <input type="checkbox" class="ys-check" checked={allSelected} onchange={toggleSelectAll} aria-label="Select all" />
        <span class="sel-label">{selCount} selected</span>
        <div class="seg">
          <button class:active={density === "roomy"} onclick={() => (density = "roomy")}>Roomy</button>
          <button class:active={density === "compact"} onclick={() => (density = "compact")}>Compact</button>
        </div>
      </div>
      <div class="foot-right">
        {#if checkedHosts.length > 0}
          <button class="btn cyan" onclick={runRemoteBulk}>Update All Remote ({checkedHosts.length})</button>
        {/if}
        {#if activeHost?.needsRestart}
          <button class="btn ghost" onclick={() => runPrimary(false)}>Update</button>
        {/if}
        <div class="split" class:split-caret={activeHost?.needsRestart}>
          <button class="btn primary main" onclick={() => runPrimary(activeHost?.needsRestart ?? false)}>{primaryLabel}</button>
          {#if activeHost?.needsRestart}
            <button class="btn primary caret" onclick={() => (primaryMenu = !primaryMenu)} aria-label="More update options">
              <svg viewBox="0 0 24 24"><path d="M6 9l6 6 6-6" /></svg>
            </button>
            {#if primaryMenu}
              <div class="primary-menu">
                <button onclick={() => runPrimary(false)}>Update only (no restart)</button>
              </div>
            {/if}
          {/if}
        </div>
      </div>
    </footer>
  {/if}
</div>

{#if showDeps}
  <DependencyTree packageName={showDeps.pkg} reverse={showDeps.reverse} repository={showDeps.repo} hostname={showDeps.host} onclose={() => (showDeps = null)} />
{/if}

<svelte:window onkeydown={(e) => e.key === "Escape" && onclose()} />

<style>
  .dialog {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: var(--ys-ground);
    color: var(--ys-text);
    overflow: hidden;
  }

  .header {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 50px;
    padding: 0 18px;
    background: var(--ys-titlebar);
    border-bottom: 1px solid var(--ys-line-softer);
  }
  .title { font-family: var(--font-display); font-weight: 600; font-size: 15px; }
  .drag-spacer { flex: 1; align-self: stretch; }
  /* Let clicks on the reticle/title/pill fall through to the header so the
     whole bar drags; keep the right-side chrome actions clickable. */
  .header > * { pointer-events: none; }
  .header > .chrome-actions { pointer-events: auto; }
  .chrome-actions { display: flex; align-items: center; gap: 12px; }
  .count-pill {
    font-family: var(--font-mono);
    font-weight: 600;
    font-size: 11px;
    color: var(--ys-violet-text);
    background: color-mix(in srgb, var(--ys-violet-600) 16%, transparent);
    border: 1px solid color-mix(in srgb, var(--ys-violet-600) 35%, transparent);
    border-radius: 9px;
    padding: 1px 9px;
  }

  .search { position: relative; padding: 12px 18px 8px; }
  .search-icon {
    position: absolute; left: 30px; top: 50%; transform: translateY(-30%);
    width: 16px; height: 16px; fill: none; stroke: var(--ys-text-dim);
    stroke-width: 1.7; stroke-linecap: round; pointer-events: none;
  }
  .search input {
    width: 100%;
    background: var(--ys-surface-input);
    border: 1px solid var(--ys-line);
    border-radius: 11px;
    padding: 9px 12px 9px 34px;
    color: var(--ys-text);
    font-family: var(--font-body);
    font-size: 13px;
    outline: none;
  }
  .search input::placeholder { color: var(--ys-text-dim); }
  .search input:focus { border-color: var(--ys-violet-600); }

  .body { display: flex; gap: 14px; padding: 0 18px; flex: 1; min-height: 0; }

  .sidebar { width: 240px; flex: none; overflow-y: auto; display: flex; flex-direction: column; gap: 8px; padding: 2px; }
  .host {
    display: flex; align-items: center; gap: 9px;
    padding: 10px 11px; border-radius: 10px;
    background: var(--ys-surface-row);
    border: 1px solid var(--ys-line-soft);
    cursor: pointer; text-align: left; width: 100%;
    transition: border-color 0.13s ease, background 0.13s ease;
  }
  .host:hover { border-color: var(--ys-line); }
  .host.active { background: color-mix(in srgb, var(--ys-violet-600) 14%, transparent); border-color: color-mix(in srgb, var(--ys-violet-600) 55%, transparent); }
  .host.reboot { border-color: color-mix(in srgb, var(--ys-danger) 45%, transparent); }
  .host.active.reboot { border-color: color-mix(in srgb, var(--ys-violet-600) 60%, transparent); background: color-mix(in srgb, var(--ys-violet-600) 16%, transparent); }
  .host-spacer { width: 16px; height: 16px; flex: none; }
  .dot { width: 8px; height: 8px; border-radius: 50%; flex: none; background: var(--ys-pending); }
  .host.reboot .dot { background: var(--ys-danger); }
  .host-name { font-family: var(--font-body); font-weight: 600; font-size: 13px; flex: 1; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .host.reboot .host-name { color: var(--ys-danger); }
  .host-count {
    font-family: var(--font-mono); font-weight: 600; font-size: 11px;
    color: var(--ys-text-dim); background: var(--ys-surface);
    border-radius: 8px; padding: 0 7px; min-width: 22px; text-align: center;
  }
  .host.active .host-count { color: var(--ys-violet-text); background: color-mix(in srgb, var(--ys-violet-600) 22%, transparent); }
  .host.reboot .host-count { color: var(--ys-danger); background: color-mix(in srgb, var(--ys-danger) 18%, transparent); }

  .list { flex: 1; min-width: 0; overflow-y: auto; display: flex; flex-direction: column; gap: 9px; padding: 2px 2px 8px; }
  .section {
    display: flex; align-items: center; gap: 8px;
    font-family: var(--font-mono); font-weight: 600; font-size: 11px;
    letter-spacing: 2px; color: var(--ys-text-dim);
    margin-top: 6px; padding-left: 2px;
  }
  .section:first-child { margin-top: 0; }
  .sdot { width: 6px; height: 6px; border-radius: 50%; }
  .section.restart { color: var(--ys-danger); }
  .section.restart .sdot { background: var(--ys-danger); }
  .section.all .sdot { background: var(--ys-pending); }

  .footer {
    display: flex; align-items: center; justify-content: space-between;
    gap: 12px; padding: 10px 18px;
    background: var(--ys-titlebar); border-top: 1px solid var(--ys-line-softer);
  }
  .foot-left { display: flex; align-items: center; gap: 12px; }
  .sel-label { font-family: var(--font-mono); font-weight: 600; font-size: 12px; color: var(--ys-text-muted); }
  .foot-right { display: flex; align-items: center; gap: 10px; }

  .seg { display: flex; background: var(--ys-surface); border: 1px solid var(--ys-line); border-radius: 999px; padding: 3px; gap: 2px; }
  .seg button { font-family: var(--font-body); font-weight: 600; font-size: 12px; color: var(--ys-text-dim); padding: 5px 14px; border-radius: 999px; cursor: pointer; }
  .seg button.active { background: var(--ys-violet-600); color: #fff; }

  .btn { font-family: var(--font-display); font-weight: 600; font-size: 13px; border-radius: 19px; padding: 9px 18px; cursor: pointer; white-space: nowrap; }
  .btn.ghost { background: var(--ys-surface); color: var(--ys-text-muted); border: 1px solid var(--ys-line); }
  .btn.ghost:hover { border-color: var(--ys-violet-500); color: var(--ys-text); }
  .btn.cyan { background: color-mix(in srgb, var(--ys-cyan) 14%, transparent); color: var(--ys-cyan-text); border: 1px solid color-mix(in srgb, var(--ys-cyan) 45%, transparent); }
  .btn.cyan:hover { background: color-mix(in srgb, var(--ys-cyan) 22%, transparent); }
  .btn.primary { background: linear-gradient(var(--ys-violet-500), var(--ys-violet-600)); color: #fff; border: none; }
  .btn.primary:hover { background: linear-gradient(var(--ys-violet-400), var(--ys-violet-500)); }

  .split { position: relative; display: flex; box-shadow: var(--ys-glow); border-radius: 19px; }
  /* Only flatten the main button's right corners when the caret segment is shown. */
  .split-caret .main { border-top-right-radius: 0; border-bottom-right-radius: 0; }
  .split .caret { border-top-left-radius: 0; border-bottom-left-radius: 0; padding: 9px 8px; background: var(--ys-violet-700); display: flex; align-items: center; }
  .split .caret svg { width: 14px; height: 14px; fill: none; stroke: #fff; stroke-width: 2.2; stroke-linecap: round; stroke-linejoin: round; }
  .primary-menu {
    position: absolute; right: 0; bottom: calc(100% + 6px); z-index: 20;
    background: var(--ys-surface); border: 1px solid var(--ys-line); border-radius: 10px; padding: 5px; min-width: 200px;
  }
  .primary-menu button { width: 100%; text-align: left; font-family: var(--font-body); font-size: 13px; color: var(--ys-text); padding: 7px 10px; border-radius: 6px; cursor: pointer; }
  .primary-menu button:hover { background: color-mix(in srgb, var(--ys-violet-600) 20%, transparent); }

  .centered { flex: 1; display: flex; align-items: center; justify-content: center; }
  .muted { color: var(--ys-text-dim); font-family: var(--font-body); }
  .spinner { width: 28px; height: 28px; border-radius: 50%; border: 3px solid var(--ys-line); border-top-color: var(--ys-violet-500); animation: spin 0.8s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }

  /* ── Checking state (surface #6) ───────────────────────────── */
  .title-spinner {
    width: 18px; height: 18px; flex: none; border-radius: 50%;
    border: 2px solid color-mix(in srgb, var(--ys-cyan) 28%, transparent);
    border-top-color: var(--ys-cyan); animation: spin 0.85s linear infinite;
  }
  .checking-pill {
    font-family: var(--font-mono); font-weight: 600; font-size: 11px; letter-spacing: 1px;
    color: var(--ys-cyan-text);
    background: color-mix(in srgb, var(--ys-cyan) 14%, transparent);
    border: 1px solid color-mix(in srgb, var(--ys-cyan) 40%, transparent);
    border-radius: 9px; padding: 1px 9px;
  }
  .progressbar {
    height: 2px; position: relative; overflow: hidden;
    background: var(--ys-titlebar); border-bottom: 1px solid var(--ys-line-softer);
  }
  .progressbar-seg {
    position: absolute; top: 0; height: 100%; width: 34%; border-radius: 999px;
    background: linear-gradient(90deg, transparent, var(--ys-cyan), transparent);
    animation: indeterminate 1.3s ease-in-out infinite;
  }
  @keyframes indeterminate { 0% { left: -38%; } 100% { left: 104%; } }

  .checking-body { padding-top: 16px; }
  .scan-host {
    display: flex; align-items: center; gap: 9px; padding: 10px 11px; border-radius: 10px;
    background: var(--ys-surface-row); border: 1px solid var(--ys-line-soft);
  }
  .scan-host.reboot { border-color: color-mix(in srgb, var(--ys-danger) 40%, transparent); }
  .scan-host .dot { background: var(--ys-text-dim); opacity: 0.45; }
  .scan-host .dot.done { background: var(--ys-good); opacity: 1; }
  .scan-host .dot.err { background: var(--ys-danger); opacity: 1; }
  .scan-host .host-name { color: var(--ys-text-muted); }
  .scan-spinner {
    width: 12px; height: 12px; flex: none; border-radius: 50%;
    border: 2px solid color-mix(in srgb, var(--ys-cyan) 25%, transparent);
    border-top-color: var(--ys-cyan); animation: spin 0.8s linear infinite;
  }
  .scan-count {
    font-family: var(--font-mono); font-weight: 600; font-size: 11px;
    color: var(--ys-text-dim); min-width: 22px; text-align: right;
  }

  .hero {
    flex: 1; min-width: 0; display: flex; flex-direction: column;
    align-items: center; justify-content: center; gap: 16px; text-align: center; padding-bottom: 16px;
  }
  .hero-spin { position: relative; width: 84px; height: 84px; display: flex; align-items: center; justify-content: center; }
  .hero-spin::before {
    content: ""; position: absolute; inset: -36px; border-radius: 50%;
    background: radial-gradient(circle, var(--ys-bloom), transparent 68%);
  }
  .hero-ring {
    position: absolute; inset: 0; border-radius: 50%;
    border: 3px solid color-mix(in srgb, var(--ys-violet-500) 20%, transparent);
    border-top-color: var(--ys-violet-500); animation: spin 0.9s linear infinite;
  }
  .hero-reticle { position: relative; opacity: 0.32; }
  .hero-core {
    position: absolute; width: 9px; height: 9px; border-radius: 50%;
    background: var(--ys-violet-500); box-shadow: 0 0 12px -1px var(--ys-violet-500);
    animation: corepulse 1.5s ease-in-out infinite;
  }
  @keyframes corepulse {
    0%, 100% { transform: scale(0.7); opacity: 0.55; }
    50% { transform: scale(1.15); opacity: 1; }
  }
  .hero-title { font-family: var(--font-display); font-weight: 600; font-size: 18px; color: var(--ys-text); }
  .hero-readout {
    font-family: var(--font-mono); font-weight: 600; font-size: 11px;
    letter-spacing: 1px; color: var(--ys-text-dim);
  }

  .checking-footer { justify-content: space-between; }
  .started { font-family: var(--font-mono); font-weight: 600; font-size: 12px; color: var(--ys-text-dim); }
  .btn.ghost.cancel:hover { border-color: var(--ys-danger); color: var(--ys-danger); }

  :global(.ys-check) {
    appearance: none; -webkit-appearance: none;
    width: 18px; height: 18px; flex: none;
    border-radius: 5px; border: 1.5px solid var(--ys-checkbox-border);
    background: var(--ys-surface-input); cursor: pointer; position: relative;
    transition: background 0.12s ease, border-color 0.12s ease;
  }
  :global(.ys-check.sm) { width: 16px; height: 16px; }
  :global(.ys-check:hover) { border-color: var(--ys-violet-500); }
  :global(.ys-check:checked) { background: var(--ys-violet-600); border-color: var(--ys-violet-600); }
  :global(.ys-check:checked::after) {
    content: ""; position: absolute; left: 5px; top: 1.5px;
    width: 5px; height: 9px; border: solid #fff; border-width: 0 2px 2px 0; transform: rotate(45deg);
  }
  :global(.ys-check.sm:checked::after) { left: 4.5px; top: 1px; width: 4.5px; height: 8px; }
</style>

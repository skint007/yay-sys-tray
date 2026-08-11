<script lang="ts">
  import { onMount, onDestroy } from "svelte";
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
  } from "../ipc";
  import { repoRank, repoColorVar, UNKNOWN_REPO } from "../repo";
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

  // Every host whose AUR half failed, local or remote. Collected fleet-wide
  // rather than per selected host: a host with an AUR error and no repo updates
  // never enters the sidebar, so a per-host banner would never show it.
  let aurFailures = $derived.by(() => {
    const out: { name: string; error: string }[] = [];
    if (localResult?.aur_error) out.push({ name: "Local", error: localResult.aur_error });
    for (const h of remoteHosts) {
      // A host we couldn't reach at all already reports that as its error; its
      // AUR result is meaningless and would only crowd the banner.
      if (h.aur_error && !h.error) out.push({ name: h.hostname, error: h.aur_error });
    }
    return out;
  });

  // The AUR query for every host runs from this machine, so one outage sets the
  // error on all of them at once — the multi-host case is the common one, and
  // dropping the reason there would leave it visible nowhere in the UI.
  let aurSummary = $derived.by(() => {
    if (aurFailures.length === 0) return null;
    const names = aurFailures.map((f) => f.name).join(", ");
    const shared = aurFailures.every((f) => f.error === aurFailures[0].error);
    return { count: aurFailures.length, names, error: aurFailures[0].error, shared };
  });

  let totalCount = $derived(hosts.reduce((s, h) => s + h.updates.length, 0));
  let multiHost = $derived(hosts.length > 1);
  // Any remote host was scanned this run — the Tailscale feature is in play, so
  // the active device stays worth naming even after the sidebar collapses.
  let remoteInPlay = $derived(remoteHosts.length > 0);
  // Remote hosts carry the bulk-update checkboxes; local updates via the
  // primary button, so it has no checkbox and is excluded from "select all".
  let checkableHosts = $derived(hosts.filter((h) => h.checkable));
  let allHostsChecked = $derived(
    checkableHosts.length > 0 && checkableHosts.every((h) => checkedHosts.includes(h.key)),
  );
  let someHostsChecked = $derived(checkableHosts.some((h) => checkedHosts.includes(h.key)));
  let hostsIndeterminate = $derived(someHostsChecked && !allHostsChecked);
  let activeHost = $derived(hosts.find((h) => h.key === activeKey) ?? hosts[0]);
  let activeSelected = $derived(selectedByHost[activeKey] ?? []);

  type RepoGroup = { name: string; updates: UpdateInfo[] };

  let grouped = $derived.by(() => {
    const h = activeHost;
    if (!h) return { restart: [] as UpdateInfo[], repos: [] as RepoGroup[], visible: [] as UpdateInfo[] };
    const q = search.toLowerCase();
    const ups = h.updates.filter((u) => !q || u.package.toLowerCase().includes(q));
    const rset = new Set(h.restartPkgs);
    const byName = (a: UpdateInfo, b: UpdateInfo) => a.package.localeCompare(b.package);
    const restart: UpdateInfo[] = [];
    const byRepo = new Map<string, UpdateInfo[]>();
    for (const u of ups) {
      if (rset.has(u.package)) {
        restart.push(u);
        continue;
      }
      const repo = u.repository || UNKNOWN_REPO;
      const group = byRepo.get(repo);
      if (group) group.push(u);
      else byRepo.set(repo, [u]);
    }
    restart.sort(byName);
    const repos = [...byRepo.entries()]
      .map(([name, updates]) => ({ name, updates: updates.sort(byName) }))
      .sort((a, b) => repoRank(a.name) - repoRank(b.name) || a.name.localeCompare(b.name));
    return { restart, repos, visible: [...restart, ...repos.flatMap((r) => r.updates)] };
  });

  let visiblePkgs = $derived(grouped.visible.map((u) => u.package));
  let selectedSet = $derived(new Set(activeSelected));
  let selCount = $derived(activeSelected.length);
  let allSelected = $derived(visiblePkgs.length > 0 && visiblePkgs.every((p) => selectedSet.has(p)));
  let pkgsIndeterminate = $derived(!allSelected && visiblePkgs.some((p) => selectedSet.has(p)));
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
    // Register the scan-progress listeners. onMount's async return value is a
    // Promise, which Svelte ignores for cleanup, so the unlisteners are torn
    // down from onDestroy below instead — otherwise every reopen leaks 5
    // handlers that keep firing into destroyed instances.
    unlisteners = await Promise.all([
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
  });

  let unlisteners: Array<() => void> = [];
  onDestroy(() => unlisteners.forEach((un) => un()));

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

  function toggleAllHosts() {
    checkedHosts = allHostsChecked ? [] : checkableHosts.map((h) => h.key);
  }

  function togglePackage(pkg: string) {
    const cur = selectedByHost[activeKey] ?? [];
    selectedByHost[activeKey] = cur.includes(pkg) ? cur.filter((p) => p !== pkg) : [...cur, pkg];
  }

  // Only add/remove the packages currently visible under the search filter —
  // selections of filtered-out packages must survive so "Update Selected" acts
  // on the full set the user built, not just what's on screen.
  function setSelected(pkgs: string[], on: boolean) {
    const cur = selectedByHost[activeKey] ?? [];
    if (on) {
      const set = new Set(cur);
      for (const p of pkgs) set.add(p);
      selectedByHost[activeKey] = [...set];
    } else {
      const drop = new Set(pkgs);
      selectedByHost[activeKey] = cur.filter((p) => !drop.has(p));
    }
  }

  function toggleSelectAll() {
    setSelected(visiblePkgs, !allSelected);
  }

  // A group header's checkbox covers only that group's visible rows, so the
  // top-level select-all and the group boxes stay consistent with each other:
  // every group checked === all checked.
  function groupState(updates: UpdateInfo[]) {
    const n = updates.reduce((acc, u) => acc + (selectedSet.has(u.package) ? 1 : 0), 0);
    return { all: updates.length > 0 && n === updates.length, some: n > 0 && n < updates.length };
  }

  function toggleGroup(updates: UpdateInfo[]) {
    setSelected(
      updates.map((u) => u.package),
      !groupState(updates).all,
    );
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
      // The remote command rejects when the selection no longer matches that
      // host's last check. Swallowing it entirely would mean the click opened
      // no terminal and said nothing anywhere.
      p.catch((e) => console.error("update failed:", e));
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
      {#if !multiHost && remoteInPlay && activeHost}
        <span class="active-host" class:reboot={activeHost.needsRestart}>
          <span class="dot"></span>{activeHost.name}
        </span>
      {/if}
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

  <!-- Rendered above the results so it shows whether or not any repo updates
       were found — an AUR outage with an otherwise up-to-date system would
       otherwise land on the "System is up to date" screen. -->
  {#if !checking && !loading && aurSummary}
    <div class="aur-error" role="status">
      <span class="aur-error-dot"></span>
      <span class="aur-error-text">
        {#if aurSummary.count === 1}
          AUR check failed on {aurSummary.names} — AUR updates not shown. {aurSummary.error}
        {:else if aurSummary.shared}
          AUR check failed on {aurSummary.count} hosts ({aurSummary.names}) — AUR updates not
          shown. {aurSummary.error}
        {:else}
          AUR check failed on {aurSummary.count} hosts ({aurSummary.names}) — AUR updates not
          shown. First: {aurSummary.error}
        {/if}
      </span>
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
          {#if checkableHosts.length > 1}
            <div class="sidebar-head">
              <input
                type="checkbox"
                class="ys-check sm"
                checked={allHostsChecked}
                indeterminate={hostsIndeterminate}
                onchange={toggleAllHosts}
                aria-label="Select all hosts"
              />
              <span class="sidebar-head-label">All hosts</span>
            </div>
          {/if}
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
        <div class="list-head">
          <label class="select-all">
            <input
              type="checkbox"
              class="ys-check"
              checked={allSelected}
              indeterminate={pkgsIndeterminate}
              onchange={toggleSelectAll}
              aria-label="Select all packages"
            />
            <span>SELECT ALL</span>
          </label>
          <span class="sel-label">{selCount} selected</span>
        </div>
        {#if grouped.restart.length > 0}
          <label class="section restart">
            <input
              type="checkbox"
              class="ys-check sm"
              checked={groupState(grouped.restart).all}
              indeterminate={groupState(grouped.restart).some}
              onchange={() => toggleGroup(grouped.restart)}
              aria-label="Select all restart-required packages"
            />
            <span class="sdot"></span>RESTART REQUIRED
          </label>
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
        {#each grouped.repos as r (r.name)}
          <label class="section repo" style={`--c: ${repoColorVar(r.name, "--ys-pending")}`}>
            <input
              type="checkbox"
              class="ys-check sm"
              checked={groupState(r.updates).all}
              indeterminate={groupState(r.updates).some}
              onchange={() => toggleGroup(r.updates)}
              aria-label={`Select all ${r.name} packages`}
            />
            <span class="sdot"></span>{r.name.toUpperCase()}
            <span class="scount">{r.updates.length}</span>
          </label>
          {#each r.updates as u (u.package)}
            <UpdateCard
              update={u}
              compact={density === "compact"}
              selected={activeSelected.includes(u.package)}
              onToggle={() => togglePackage(u.package)}
              onremove={handleRemove}
              onShowDeps={(reverse) => (showDeps = { pkg: u.package, reverse, repo: u.repository, host: activeKey === "local" ? null : activeKey })}
            />
          {/each}
        {/each}
      </main>
    </div>

    <footer class="footer">
      <div class="foot-left">
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

<svelte:window
  onkeydown={(e) => {
    if (e.key !== "Escape") return;
    // Dismiss the dependency-tree overlay first if it's open, rather than
    // hiding the whole window out from under it.
    if (showDeps) showDeps = null;
    else onclose();
  }}
/>

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
  /* Names the active device in the header once the sidebar collapses to a
     single host — otherwise the last remote device is left unlabeled (#12). */
  .active-host {
    display: inline-flex; align-items: center; gap: 6px;
    font-family: var(--font-body); font-weight: 600; font-size: 12px;
    color: var(--ys-text-muted);
    max-width: 200px; overflow: hidden; white-space: nowrap; text-overflow: ellipsis;
  }
  .active-host .dot { width: 7px; height: 7px; border-radius: 50%; flex: none; background: var(--ys-pending); }
  .active-host.reboot { color: var(--ys-danger); }
  .active-host.reboot .dot { background: var(--ys-danger); }

  .aur-error {
    display: flex; align-items: center; gap: 8px;
    margin: 0 18px 8px; padding: 8px 12px;
    border: 1px solid color-mix(in srgb, var(--ys-pending) 45%, transparent);
    background: color-mix(in srgb, var(--ys-pending) 12%, transparent);
    border-radius: 11px;
  }
  .aur-error-dot {
    flex: none; width: 6px; height: 6px; border-radius: 50%;
    background: var(--ys-pending);
  }
  /* The dot and border carry the warning color; the text itself stays on the
     normal text color, which is the only way it clears AA contrast against the
     tinted background at this size in the light theme. */
  .aur-error-text {
    font-family: var(--font-mono); font-size: 11px; line-height: 1.45;
    color: var(--ys-text);
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
  .sidebar-head { display: flex; align-items: center; gap: 9px; padding: 2px 12px 4px; }
  .sidebar-head-label {
    font-family: var(--font-mono); font-weight: 600; font-size: 11px;
    letter-spacing: 1.5px; text-transform: uppercase; color: var(--ys-text-dim);
  }
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

  .list { flex: 1; min-width: 0; overflow-y: auto; display: flex; flex-direction: column; gap: 9px; padding: 0 2px 8px; }
  /* Pinned to the top of the scroller so select-all stays reachable no matter
     how far down the list you are. The negative margins let its background
     cover the list's side padding, which rows would otherwise scroll through. */
  .list-head {
    position: sticky; top: 0; z-index: 2;
    display: flex; align-items: center; justify-content: space-between; gap: 12px;
    margin: 0 -2px; padding: 4px 2px 6px;
    background: var(--ys-ground);
  }
  .select-all {
    display: flex; align-items: center; gap: 9px; cursor: pointer;
    font-family: var(--font-mono); font-weight: 600; font-size: 11px;
    letter-spacing: 1.5px; color: var(--ys-text-dim);
  }
  .select-all:hover { color: var(--ys-text-muted); }
  .sel-label { font-family: var(--font-mono); font-weight: 600; font-size: 12px; color: var(--ys-text-muted); }
  .section {
    display: flex; align-items: center; gap: 8px;
    font-family: var(--font-mono); font-weight: 600; font-size: 11px;
    letter-spacing: 2px; color: var(--ys-text-dim);
    margin-top: 6px; padding-left: 2px;
    cursor: pointer;
  }
  .section:first-of-type { margin-top: 0; }
  .sdot { width: 6px; height: 6px; border-radius: 50%; }
  .section.restart { color: var(--ys-danger); }
  .section.restart .sdot { background: var(--ys-danger); }
  .section.repo .sdot { background: var(--c, var(--ys-pending)); }
  .scount {
    font-family: var(--font-mono); font-weight: 600; font-size: 10px;
    color: var(--ys-text-dim); background: var(--ys-surface);
    border-radius: 7px; padding: 0 6px;
  }

  .footer {
    display: flex; align-items: center; justify-content: space-between;
    gap: 12px; padding: 10px 18px;
    background: var(--ys-titlebar); border-top: 1px solid var(--ys-line-softer);
  }
  .foot-left { display: flex; align-items: center; gap: 12px; }
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
  :global(.ys-check:checked),
  :global(.ys-check:indeterminate) { background: var(--ys-violet-600); border-color: var(--ys-violet-600); }
  :global(.ys-check:checked::after) {
    content: ""; position: absolute; left: 5px; top: 1.5px;
    width: 5px; height: 9px; border: solid #fff; border-width: 0 2px 2px 0; transform: rotate(45deg);
  }
  :global(.ys-check.sm:checked::after) { left: 4.5px; top: 1px; width: 4.5px; height: 8px; }
  /* Partial selection — a horizontal bar instead of the tick. */
  :global(.ys-check:indeterminate::after) {
    content: ""; position: absolute; left: 4px; top: 7px;
    width: 8px; height: 0; border-top: 2px solid #fff; border-radius: 1px;
  }
  :global(.ys-check.sm:indeterminate::after) { left: 3.5px; top: 6px; width: 7px; }
</style>

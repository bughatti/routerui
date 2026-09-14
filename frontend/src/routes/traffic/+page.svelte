<script>
  import { onMount, onDestroy } from "svelte";

  let loading = $state(true);
  let enabled = $state(true);
  let clients = $state([]);
  let settings = $state({ insights_enabled: true, retention_hours: 48 });
  let selected = $state(null);        // client ip currently expanded
  let flows = $state([]);
  let domains = $state({ available: false, domains: [] });
  let dpi = $state({ installed: false, running: false });
  let routerHost = $state("");
  let timer;

  function fmtBytes(n) {
    if (!n) return "0 B";
    const u = ["B", "KB", "MB", "GB", "TB"];
    let i = 0; let v = n;
    while (v >= 1024 && i < u.length - 1) { v /= 1024; i++; }
    return `${v.toFixed(v < 10 && i > 0 ? 1 : 0)} ${u[i]}`;
  }
  function fmtRate(bps) {
    if (!bps) return "—";
    if (bps >= 1e6) return `${(bps / 1e6).toFixed(1)} Mbps`;
    if (bps >= 1e3) return `${(bps / 1e3).toFixed(0)} kbps`;
    return `${bps} bps`;
  }

  async function loadClients() {
    try {
      const res = await fetch("/api/traffic/clients");
      if (res.ok) {
        const d = await res.json();
        enabled = d.enabled;
        clients = d.clients || [];
      }
    } catch (e) { console.error(e); }
  }

  async function loadMeta() {
    try {
      const [s, a] = await Promise.all([fetch("/api/traffic/settings"), fetch("/api/addons/status")]);
      if (s.ok) settings = await s.json();
      if (a.ok) { const ad = await a.json(); dpi = ad.dpi || { installed: false, running: false }; }
    } catch (e) { console.error(e); }
    routerHost = window.location.hostname;
  }

  async function selectClient(ip) {
    if (selected === ip) { selected = null; return; }
    selected = ip;
    flows = []; domains = { available: false, domains: [] };
    try {
      const [f, d] = await Promise.all([
        fetch(`/api/traffic/connections?ip=${encodeURIComponent(ip)}`),
        fetch(`/api/traffic/domains?ip=${encodeURIComponent(ip)}`)
      ]);
      if (f.ok) flows = await f.json();
      if (d.ok) domains = await d.json();
    } catch (e) { console.error(e); }
  }

  async function saveSettings() {
    await fetch("/api/traffic/settings", {
      method: "POST", headers: { "Content-Type": "application/json" },
      body: JSON.stringify(settings)
    });
    await loadClients();
  }

  async function resetCounters() {
    if (!confirm("Reset all per-client usage counters to zero?")) return;
    await fetch("/api/traffic/reset", { method: "POST" });
    await loadClients();
  }

  onMount(async () => {
    await Promise.all([loadMeta(), loadClients()]);
    loading = false;
    timer = setInterval(async () => {
      await loadClients();
      if (selected) await selectClient(selected === selected ? selected : selected);
    }, 3000);
  });
  onDestroy(() => clearInterval(timer));
</script>

<div class="max-w-5xl">
  <div class="mb-6 flex items-start justify-between">
    <div>
      <h2 class="text-2xl font-bold">Traffic Insight</h2>
      <p class="text-gray-400 text-sm">Per-device usage, live connections, real-time bandwidth, and top domains. Click a device to drill in.</p>
    </div>
    <button class="btn-danger text-sm" onclick={resetCounters}>Reset counters</button>
  </div>

  {#if loading}
    <p class="text-gray-400">Loading…</p>
  {:else}
    <!-- Privacy / settings -->
    <div class="card mb-4">
      <div class="flex flex-wrap items-center gap-6">
        <label class="text-sm flex items-center gap-2">
          <input type="checkbox" bind:checked={settings.insights_enabled} onchange={saveSettings} />
          Traffic insight enabled
        </label>
        <label class="text-sm flex items-center gap-2">
          Domain-history retention (hours)
          <input type="number" min="1" max="720" bind:value={settings.retention_hours} class="input w-24" />
          <button class="btn-primary text-sm" onclick={saveSettings}>Save</button>
        </label>
        <span class="text-xs text-gray-500">You (admin) can see every domain each device resolves. Retention bounds how long that history is kept.</span>
      </div>
    </div>

    {#if !enabled}
      <div class="card text-gray-400">Traffic insight is turned off. Enable it above to see per-device traffic.</div>
    {:else}
      <!-- Client list -->
      <div class="card mb-4">
        <table class="w-full text-sm">
          <thead class="text-gray-400 text-left">
            <tr><th class="py-1">Device</th><th>IP</th><th class="text-right">↓ total</th><th class="text-right">↑ total</th><th class="text-right">↓ now</th><th class="text-right">↑ now</th></tr>
          </thead>
          <tbody>
            {#if clients.length === 0}
              <tr><td colspan="6" class="text-gray-500 py-3">No active devices with leases yet.</td></tr>
            {/if}
            {#each clients as c}
              <tr class="border-t border-gray-700 hover:bg-gray-700/40 cursor-pointer" onclick={() => selectClient(c.ip)}>
                <td class="py-2">{c.hostname || "(unknown)"}<span class="text-gray-600 text-xs ml-2">{c.mac}</span></td>
                <td class="font-mono text-gray-400">{c.ip}</td>
                <td class="text-right">{fmtBytes(c.rx_bytes)}</td>
                <td class="text-right">{fmtBytes(c.tx_bytes)}</td>
                <td class="text-right {c.rx_rate_bps ? 'status-active' : 'text-gray-600'}">{fmtRate(c.rx_rate_bps)}</td>
                <td class="text-right {c.tx_rate_bps ? 'status-active' : 'text-gray-600'}">{fmtRate(c.tx_rate_bps)}</td>
              </tr>
              {#if selected === c.ip}
                <tr class="bg-gray-900/60"><td colspan="6" class="p-3">
                  <div class="grid grid-cols-2 gap-4">
                    <div>
                      <p class="font-semibold mb-2 text-sm">Live connections</p>
                      {#if flows.length === 0}<p class="text-gray-500 text-xs">No active flows.</p>{/if}
                      <div class="max-h-64 overflow-auto">
                        {#each flows.slice(0, 40) as f}
                          <div class="text-xs flex justify-between border-b border-gray-800 py-1">
                            <span class="truncate mr-2">{f.dst_host || f.dst}<span class="text-gray-600">:{f.dst_port} {f.proto}</span></span>
                            <span class="text-gray-400 whitespace-nowrap">{fmtBytes(f.bytes)}</span>
                          </div>
                        {/each}
                      </div>
                    </div>
                    <div>
                      <p class="font-semibold mb-2 text-sm">Top domains</p>
                      {#if !domains.available}
                        <p class="text-gray-500 text-xs">Install the AdGuard add-on to see per-device domains.</p>
                      {:else if domains.domains.length === 0}
                        <p class="text-gray-500 text-xs">No recent DNS activity.</p>
                      {:else}
                        <div class="max-h-64 overflow-auto">
                          {#each domains.domains as d}
                            <div class="text-xs flex justify-between border-b border-gray-800 py-1">
                              <span class="truncate mr-2">{d.domain}</span>
                              <span class="text-gray-400">{d.count}</span>
                            </div>
                          {/each}
                        </div>
                      {/if}
                    </div>
                  </div>
                </td></tr>
              {/if}
            {/each}
          </tbody>
        </table>
      </div>

      <!-- L5 DPI -->
      <div class="card">
        <h3 class="font-semibold mb-2">Deep traffic analysis (DPI)</h3>
        {#if dpi.installed && dpi.running}
          <p class="text-gray-400 text-sm mb-2">Per-application classification (Netflix / gaming / BitTorrent…), top talkers and historical analytics are provided by ntopng.</p>
          <a class="btn-primary inline-block" href={`http://${routerHost}:3001`} target="_blank" rel="noopener">Open ntopng dashboard →</a>
        {:else}
          <p class="text-gray-400 text-sm">Application-level DPI is an optional add-on (ntopng). It's heavier than the per-device view above and monitors the LAN interface. Install it from the <a href="/addons" class="text-blue-400">Add-ons</a> page.</p>
        {/if}
      </div>
    {/if}
  {/if}
</div>

<style>
  .input { @apply bg-gray-900 border border-gray-700 rounded px-3 py-2 text-gray-100; }
</style>

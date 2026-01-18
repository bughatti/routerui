<script>
  import { onMount } from "svelte";

  // State
  let loading = $state(true);
  let activeTab = $state("blocklists"); // blocklists, countries, log, whitelist
  let status = $state(null);
  let blocklists = $state({ sources: [], total_ips: 0 });
  let blockedLog = $state({ entries: [], total_blocked_24h: 0 });
  let whitelist = $state([]);
  let countries = $state([]);
  let updating = $state(false);

  // Fetch all data
  async function fetchData() {
    try {
      const [statusRes, blocklistsRes, logRes, whitelistRes, countriesRes] = await Promise.all([
        fetch("/api/protection/status"),
        fetch("/api/protection/blocklists"),
        fetch("/api/protection/blocked-log"),
        fetch("/api/protection/whitelist"),
        fetch("/api/protection/countries")
      ]);

      if (statusRes.ok) status = await statusRes.json();
      if (blocklistsRes.ok) blocklists = await blocklistsRes.json();
      if (logRes.ok) blockedLog = await logRes.json();
      if (whitelistRes.ok) whitelist = await whitelistRes.json();
      if (countriesRes.ok) countries = await countriesRes.json();
    } catch (e) {
      console.error(e);
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    fetchData();
    const interval = setInterval(fetchData, 10000); // Refresh every 10s
    return () => clearInterval(interval);
  });

  // Toggle blocklist
  async function toggleBlocklist(id, enabled) {
    updating = true;
    try {
      const res = await fetch("/api/protection/blocklists/toggle", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ id, enabled })
      });
      if (res.ok) {
        await fetchData();
      }
    } finally {
      updating = false;
    }
  }

  // Update all blocklists
  async function updateAllBlocklists() {
    updating = true;
    try {
      await fetch("/api/protection/blocklists/update", { method: "POST" });
      await fetchData();
    } finally {
      updating = false;
    }
  }

  // Toggle country
  async function toggleCountry(code, blocked) {
    updating = true;
    try {
      const res = await fetch("/api/protection/countries/toggle", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ code, blocked })
      });
      if (res.ok) {
        await fetchData();
      }
    } finally {
      updating = false;
    }
  }

  // Quick allow from blocked log
  async function quickAllow(ip, description) {
    const res = await fetch("/api/protection/whitelist/add", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ ip, description })
    });
    if (res.ok) {
      await fetchData();
    }
  }

  // Add to whitelist
  let newWhitelistIP = $state("");
  let newWhitelistDesc = $state("");

  async function addToWhitelist() {
    if (!newWhitelistIP) return;
    const res = await fetch("/api/protection/whitelist/add", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ ip: newWhitelistIP, description: newWhitelistDesc || undefined })
    });
    if (res.ok) {
      newWhitelistIP = "";
      newWhitelistDesc = "";
      await fetchData();
    }
  }

  // Remove from whitelist
  async function removeFromWhitelist(ip) {
    const res = await fetch("/api/protection/whitelist/remove", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ ip })
    });
    if (res.ok) {
      await fetchData();
    }
  }

  // Enable logging
  async function enableLogging() {
    await fetch("/api/protection/enable-logging", { method: "POST" });
    await fetchData();
  }

  // Format number with commas
  function formatNumber(n) {
    return n?.toLocaleString() || "0";
  }

  // Country code to flag emoji
  function getFlagEmoji(countryCode) {
    if (!countryCode) return "";
    const codePoints = countryCode
      .toUpperCase()
      .split("")
      .map((char) => 127397 + char.charCodeAt(0));
    return String.fromCodePoint(...codePoints);
  }

  // Get time ago
  function timeAgo(timestamp) {
    if (!timestamp) return "";
    const date = new Date(timestamp);
    const now = new Date();
    const diffMs = now - date;
    const diffMins = Math.floor(diffMs / 60000);
    if (diffMins < 1) return "just now";
    if (diffMins < 60) return `${diffMins}m ago`;
    const diffHours = Math.floor(diffMins / 60);
    if (diffHours < 24) return `${diffHours}h ago`;
    return `${Math.floor(diffHours / 24)}d ago`;
  }
</script>

<svelte:head>
  <title>Protection - RouterUI</title>
</svelte:head>

<div class="space-y-6">
  <!-- Header -->
  <div class="flex items-center justify-between">
    <div>
      <h2 class="text-2xl font-bold">Protection</h2>
      <p class="text-sm text-gray-500">Block known threats and see what's being stopped.</p>
    </div>
  </div>

  {#if loading}
    <div class="text-gray-400">Loading...</div>
  {:else}
    <!-- Status Overview -->
    <div class="card">
      <div class="grid grid-cols-2 md:grid-cols-4 gap-4">
        <div class="bg-gray-700/50 rounded p-3 text-center">
          <p class="text-2xl font-bold text-green-400">{status?.blocklists_active || 0}</p>
          <p class="text-xs text-gray-400">Active Blocklists</p>
        </div>
        <div class="bg-gray-700/50 rounded p-3 text-center">
          <p class="text-2xl font-bold text-blue-400">{formatNumber(blocklists.total_ips)}</p>
          <p class="text-xs text-gray-400">Blocked IPs</p>
        </div>
        <div class="bg-gray-700/50 rounded p-3 text-center">
          <p class="text-2xl font-bold text-red-400">{formatNumber(blockedLog.total_blocked_24h)}</p>
          <p class="text-xs text-gray-400">Blocked (24h)</p>
        </div>
        <div class="bg-gray-700/50 rounded p-3 text-center">
          <p class="text-2xl font-bold text-purple-400">{status?.whitelist_count || 0}</p>
          <p class="text-xs text-gray-400">Whitelisted</p>
        </div>
      </div>
    </div>

    <!-- Tabs -->
    <div class="border-b border-gray-700">
      <nav class="flex gap-4">
        <button
          onclick={() => activeTab = "blocklists"}
          class="tab-btn {activeTab === 'blocklists' ? 'tab-active' : ''}"
        >
          Threat Lists
        </button>
        <button
          onclick={() => activeTab = "countries"}
          class="tab-btn {activeTab === 'countries' ? 'tab-active' : ''}"
        >
          Countries
        </button>
        <button
          onclick={() => activeTab = "log"}
          class="tab-btn {activeTab === 'log' ? 'tab-active' : ''}"
        >
          Blocked Traffic
        </button>
        <button
          onclick={() => activeTab = "whitelist"}
          class="tab-btn {activeTab === 'whitelist' ? 'tab-active' : ''}"
        >
          Allowed IPs
        </button>
      </nav>
    </div>

    <!-- Tab Content -->
    {#if activeTab === "blocklists"}
      <div class="card">
        <div class="flex items-center justify-between mb-4">
          <h3 class="text-lg font-semibold">Threat Blocklists</h3>
          <button
            onclick={updateAllBlocklists}
            disabled={updating}
            class="btn-secondary text-sm"
          >
            {updating ? "Updating..." : "Update All"}
          </button>
        </div>

        <p class="text-sm text-gray-400 mb-4">
          Enable blocklists to automatically block known malicious IP addresses. These are updated from trusted security sources.
        </p>

        <div class="space-y-3">
          {#each blocklists.sources as source}
            <div class="flex items-center justify-between p-3 bg-gray-700/50 rounded">
              <div class="flex-1">
                <div class="flex items-center gap-2">
                  <span class="font-medium">{source.name}</span>
                  {#if source.enabled}
                    <span class="text-xs px-2 py-0.5 bg-green-500/20 text-green-400 rounded">
                      {formatNumber(source.ip_count)} IPs
                    </span>
                  {/if}
                </div>
                <p class="text-sm text-gray-400">{source.description}</p>
                {#if source.last_updated}
                  <p class="text-xs text-gray-500">Updated: {source.last_updated}</p>
                {/if}
              </div>
              <label class="toggle">
                <input
                  type="checkbox"
                  checked={source.enabled}
                  onchange={() => toggleBlocklist(source.id, !source.enabled)}
                  disabled={updating}
                />
                <span class="toggle-slider"></span>
              </label>
            </div>
          {/each}
        </div>
      </div>

    {:else if activeTab === "countries"}
      <div class="card">
        <h3 class="text-lg font-semibold mb-4">Country Blocking</h3>

        <div class="bg-yellow-900/20 border border-yellow-700/50 rounded p-3 mb-4">
          <p class="text-sm text-yellow-400">
            <strong>Note:</strong> If you use a VPN (like NordVPN to Netherlands), make sure to add the VPN server IPs to your Allowed IPs list before blocking that country.
          </p>
        </div>

        <p class="text-sm text-gray-400 mb-4">
          Block all traffic from specific countries. Useful for blocking regions known for high attack volumes.
        </p>

        <div class="grid grid-cols-2 md:grid-cols-3 gap-3">
          {#each countries as country}
            <div class="flex items-center justify-between p-3 bg-gray-700/50 rounded">
              <div class="flex items-center gap-2">
                <span class="text-xl">{getFlagEmoji(country.code)}</span>
                <span>{country.name}</span>
              </div>
              <label class="toggle">
                <input
                  type="checkbox"
                  checked={country.blocked}
                  onchange={() => toggleCountry(country.code, !country.blocked)}
                  disabled={updating}
                />
                <span class="toggle-slider"></span>
              </label>
            </div>
          {/each}
        </div>
      </div>

    {:else if activeTab === "log"}
      <div class="card">
        <div class="flex items-center justify-between mb-4">
          <h3 class="text-lg font-semibold">Blocked Traffic Log</h3>
          {#if !status?.log_enabled}
            <button onclick={enableLogging} class="btn-secondary text-sm">
              Enable Logging
            </button>
          {:else}
            <span class="text-xs px-2 py-1 bg-green-500/20 text-green-400 rounded">
              Logging Active
            </span>
          {/if}
        </div>

        <p class="text-sm text-gray-400 mb-4">
          See traffic that was blocked in the last 24 hours. Click "Allow" to add an IP to your whitelist if it was blocked by mistake.
        </p>

        {#if blockedLog.entries.length === 0}
          <div class="text-center py-8 text-gray-500">
            <p>No blocked traffic logged yet.</p>
            <p class="text-sm">Enable logging above to start tracking blocked connections.</p>
          </div>
        {:else}
          <div class="overflow-x-auto">
            <table class="w-full text-sm">
              <thead>
                <tr class="text-left text-gray-400 border-b border-gray-700">
                  <th class="pb-2">Time</th>
                  <th class="pb-2">Direction</th>
                  <th class="pb-2">Source IP</th>
                  <th class="pb-2">Dest</th>
                  <th class="pb-2">Reason</th>
                  <th class="pb-2">Action</th>
                </tr>
              </thead>
              <tbody>
                {#each blockedLog.entries as entry}
                  <tr class="border-b border-gray-700/50 hover:bg-gray-700/30">
                    <td class="py-2 text-gray-400">{timeAgo(entry.timestamp)}</td>
                    <td class="py-2">
                      <span class={entry.direction === "inbound" ? "text-red-400" : "text-yellow-400"}>
                        {entry.direction === "inbound" ? "IN" : "OUT"}
                      </span>
                    </td>
                    <td class="py-2 font-mono text-sm">{entry.src_ip}</td>
                    <td class="py-2 font-mono text-sm">
                      {entry.dst_ip}:{entry.dst_port}
                    </td>
                    <td class="py-2">
                      <span class="text-xs px-2 py-0.5 bg-red-500/20 text-red-400 rounded">
                        {entry.reason || "firewall"}
                      </span>
                    </td>
                    <td class="py-2">
                      <button
                        onclick={() => quickAllow(entry.src_ip, `Allowed from blocked log - ${entry.reason}`)}
                        class="text-xs px-2 py-1 bg-green-500/20 text-green-400 rounded hover:bg-green-500/30"
                      >
                        Allow
                      </button>
                    </td>
                  </tr>
                {/each}
              </tbody>
            </table>
          </div>
        {/if}
      </div>

    {:else if activeTab === "whitelist"}
      <div class="card">
        <h3 class="text-lg font-semibold mb-4">Allowed IPs (Whitelist)</h3>

        <p class="text-sm text-gray-400 mb-4">
          IPs on this list will never be blocked, even if they appear in a blocklist or blocked country.
        </p>

        <!-- Add new -->
        <div class="flex gap-2 mb-4">
          <input
            type="text"
            placeholder="IP address (e.g., 1.2.3.4)"
            bind:value={newWhitelistIP}
            class="input flex-1"
          />
          <input
            type="text"
            placeholder="Description (optional)"
            bind:value={newWhitelistDesc}
            class="input flex-1"
          />
          <button onclick={addToWhitelist} class="btn-primary">
            Add
          </button>
        </div>

        {#if whitelist.length === 0}
          <div class="text-center py-8 text-gray-500">
            <p>No IPs whitelisted yet.</p>
          </div>
        {:else}
          <div class="space-y-2">
            {#each whitelist as entry}
              <div class="flex items-center justify-between p-3 bg-gray-700/50 rounded">
                <div>
                  <span class="font-mono">{entry.ip}</span>
                  {#if entry.description}
                    <span class="text-gray-400 ml-2">- {entry.description}</span>
                  {/if}
                  <span class="text-xs text-gray-500 ml-2">Added {entry.added_at}</span>
                </div>
                <button
                  onclick={() => removeFromWhitelist(entry.ip)}
                  class="text-red-400 hover:text-red-300 text-sm"
                >
                  Remove
                </button>
              </div>
            {/each}
          </div>
        {/if}
      </div>
    {/if}
  {/if}
</div>

<style>
  .tab-btn {
    padding: 0.75rem 1rem;
    border-bottom: 2px solid transparent;
    color: #9ca3af;
    transition: all 0.15s;
  }

  .tab-btn:hover {
    color: #f3f4f6;
  }

  .tab-active {
    color: #60a5fa;
    border-bottom-color: #60a5fa;
  }

  .toggle {
    position: relative;
    display: inline-block;
    width: 44px;
    height: 24px;
  }

  .toggle input {
    opacity: 0;
    width: 0;
    height: 0;
  }

  .toggle-slider {
    position: absolute;
    cursor: pointer;
    top: 0;
    left: 0;
    right: 0;
    bottom: 0;
    background-color: #374151;
    transition: 0.3s;
    border-radius: 24px;
  }

  .toggle-slider:before {
    position: absolute;
    content: "";
    height: 18px;
    width: 18px;
    left: 3px;
    bottom: 3px;
    background-color: white;
    transition: 0.3s;
    border-radius: 50%;
  }

  .toggle input:checked + .toggle-slider {
    background-color: #22c55e;
  }

  .toggle input:checked + .toggle-slider:before {
    transform: translateX(20px);
  }

  .toggle input:disabled + .toggle-slider {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .input {
    background: #374151;
    border: 1px solid #4b5563;
    border-radius: 0.375rem;
    padding: 0.5rem 0.75rem;
    color: #f3f4f6;
  }

  .input:focus {
    outline: none;
    border-color: #60a5fa;
  }

  .btn-primary {
    background: #3b82f6;
    color: white;
    padding: 0.5rem 1rem;
    border-radius: 0.375rem;
    font-weight: 500;
  }

  .btn-primary:hover {
    background: #2563eb;
  }

  .btn-secondary {
    background: #374151;
    color: #f3f4f6;
    padding: 0.5rem 1rem;
    border-radius: 0.375rem;
    font-weight: 500;
    border: 1px solid #4b5563;
  }

  .btn-secondary:hover {
    background: #4b5563;
  }

  .btn-secondary:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
</style>


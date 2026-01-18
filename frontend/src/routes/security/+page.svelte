<script>
  import { onMount } from "svelte";

  let loading = $state(true);
  let overview = $state(null);
  let connections = $state([]);
  let activeTab = $state("overview");
  let autoRefresh = $state(true);

  async function fetchOverview() {
    try {
      const res = await fetch("/api/security/overview");
      if (res.ok) {
        overview = await res.json();
      }
    } catch (e) {
      console.error(e);
    } finally {
      loading = false;
    }
  }

  async function fetchConnections() {
    try {
      const res = await fetch("/api/security/connections");
      if (res.ok) {
        connections = await res.json();
      }
    } catch (e) {
      console.error(e);
    }
  }

  onMount(() => {
    fetchOverview();
    fetchConnections();

    const interval = setInterval(() => {
      if (autoRefresh) {
        fetchOverview();
        if (activeTab === "connections") {
          fetchConnections();
        }
      }
    }, 30000);

    return () => clearInterval(interval);
  });

  function getSeverityClass(severity) {
    switch (severity) {
      case "high": return "bg-red-500/20 text-red-400 border-red-500/50";
      case "medium": return "bg-yellow-500/20 text-yellow-400 border-yellow-500/50";
      case "info": return "bg-blue-500/20 text-blue-400 border-blue-500/50";
      default: return "bg-gray-500/20 text-gray-400 border-gray-500/50";
    }
  }

  function getEventIcon(eventType) {
    switch (eventType) {
      case "Failed Login": return "🚫";
      case "Successful Login": return "✅";
      case "Sudo Command": return "⚡";
      case "SSH Session Opened": return "🔓";
      case "SSH Session Closed": return "🔒";
      case "Session Opened": return "🔓";
      case "Session Closed": return "🔒";
      default: return "📋";
    }
  }

  function formatTimestamp(ts) {
    if (!ts || ts === "Unknown") return ts;
    // Just return the time portion for cleaner display
    return ts.split('T')[1]?.split('.')[0] || ts;
  }
</script>

<svelte:head>
  <title>Security - RouterUI</title>
</svelte:head>

<div class="space-y-6">
  <div class="flex items-center justify-between">
    <div>
      <h2 class="text-2xl font-bold">Security Monitor</h2>
      <p class="text-sm text-gray-500">Real-time security events and threat monitoring</p>
    </div>
    <label class="flex items-center gap-2 text-sm">
      <input type="checkbox" bind:checked={autoRefresh} class="rounded" />
      <span class="text-gray-400">Auto-refresh (30s)</span>
    </label>
  </div>

  {#if loading}
    <div class="text-gray-400">Loading security data...</div>
  {:else if overview}
    <!-- Summary Cards -->
    <div class="grid grid-cols-2 md:grid-cols-4 gap-4">
      <div class="card text-center">
        <p class="text-3xl font-bold text-red-400">{overview.firewall_drops_24h}</p>
        <p class="text-sm text-gray-400">Firewall Blocks</p>
      </div>
      <div class="card text-center">
        <p class="text-3xl font-bold text-yellow-400">{overview.blocklist_hits.spamhaus_drop + overview.blocklist_hits.emerging_threats}</p>
        <p class="text-sm text-gray-400">Blocklist Hits</p>
      </div>
      <div class="card text-center">
        <p class="text-3xl font-bold text-orange-400">{overview.failed_ssh_attempts_24h}</p>
        <p class="text-sm text-gray-400">Failed SSH</p>
      </div>
      <div class="card text-center">
        <p class="text-3xl font-bold text-green-400">{overview.active_connections}</p>
        <p class="text-sm text-gray-400">Active Connections</p>
      </div>
    </div>

    <!-- Blocklist Breakdown -->
    <div class="card">
      <h3 class="text-lg font-semibold mb-4">Blocklist Status</h3>
      <div class="grid grid-cols-2 gap-4">
        <div class="p-4 bg-gray-700/50 rounded flex items-center justify-between">
          <div>
            <p class="font-medium">Spamhaus DROP</p>
            <p class="text-xs text-gray-400">Known spam/malware sources</p>
          </div>
          <span class="text-2xl font-bold {overview.blocklist_hits.spamhaus_drop > 0 ? 'text-red-400' : 'text-green-400'}">
            {overview.blocklist_hits.spamhaus_drop}
          </span>
        </div>
        <div class="p-4 bg-gray-700/50 rounded flex items-center justify-between">
          <div>
            <p class="font-medium">Emerging Threats</p>
            <p class="text-xs text-gray-400">Malicious IP addresses</p>
          </div>
          <span class="text-2xl font-bold {overview.blocklist_hits.emerging_threats > 0 ? 'text-red-400' : 'text-green-400'}">
            {overview.blocklist_hits.emerging_threats}
          </span>
        </div>
      </div>
    </div>

    <!-- Tabs -->
    <div class="border-b border-gray-700">
      <nav class="flex gap-4">
        {#each [
          { id: "overview", label: "Event Feed" },
          { id: "connections", label: "Active Connections" },
          { id: "sessions", label: "SSH Sessions" }
        ] as tab}
          <button
            onclick={() => { activeTab = tab.id; if (tab.id === "connections") fetchConnections(); }}
            class="tab-btn {activeTab === tab.id ? 'tab-active' : ''}"
          >
            {tab.label}
          </button>
        {/each}
      </nav>
    </div>

    <!-- Event Feed Tab -->
    {#if activeTab === "overview"}
      <div class="card">
        <div class="flex items-center justify-between mb-4">
          <h3 class="text-lg font-semibold">Recent Security Events</h3>
          <button onclick={fetchOverview} class="text-sm text-blue-400 hover:text-blue-300">
            Refresh
          </button>
        </div>

        {#if overview.recent_events.length === 0}
          <p class="text-gray-500 text-center py-8">No recent security events</p>
        {:else}
          <div class="space-y-2 max-h-96 overflow-y-auto">
            {#each overview.recent_events as event}
              <div class="p-3 rounded border {getSeverityClass(event.severity)}">
                <div class="flex items-center justify-between mb-1">
                  <div class="flex items-center gap-2">
                    <span>{getEventIcon(event.event_type)}</span>
                    <span class="font-medium">{event.event_type}</span>
                    {#if event.is_external}
                      <span class="text-xs px-1.5 py-0.5 bg-red-500/30 text-red-300 rounded">WAN</span>
                    {:else}
                      <span class="text-xs px-1.5 py-0.5 bg-gray-500/30 text-gray-400 rounded">LAN</span>
                    {/if}
                  </div>
                  <span class="text-xs font-mono">{formatTimestamp(event.timestamp)}</span>
                </div>
                <div class="flex items-center gap-4 text-sm">
                  {#if event.source_ip !== "N/A"}
                    <span class="font-mono text-xs bg-gray-700 px-2 py-0.5 rounded">{event.source_ip}</span>
                  {/if}
                  <span class="text-gray-400 truncate">{event.details}</span>
                </div>
              </div>
            {/each}
          </div>
        {/if}
      </div>

    <!-- Connections Tab -->
    {:else if activeTab === "connections"}
      <div class="card">
        <div class="flex items-center justify-between mb-4">
          <h3 class="text-lg font-semibold">Active Network Connections ({connections.length})</h3>
          <button onclick={fetchConnections} class="text-sm text-blue-400 hover:text-blue-300">
            Refresh
          </button>
        </div>

        {#if connections.length === 0}
          <p class="text-gray-500 text-center py-8">No active connections</p>
        {:else}
          <div class="overflow-x-auto">
            <table class="w-full text-sm">
              <thead>
                <tr class="text-left text-gray-400 border-b border-gray-700">
                  <th class="pb-2">Local Address</th>
                  <th class="pb-2">Remote Address</th>
                  <th class="pb-2">State</th>
                  <th class="pb-2">Process</th>
                </tr>
              </thead>
              <tbody>
                {#each connections as conn}
                  <tr class="border-b border-gray-700/50">
                    <td class="py-2 font-mono text-xs">{conn.local_addr}</td>
                    <td class="py-2 font-mono text-xs">{conn.remote_addr}</td>
                    <td class="py-2">
                      <span class="text-xs px-2 py-0.5 bg-green-500/20 text-green-400 rounded">
                        {conn.state}
                      </span>
                    </td>
                    <td class="py-2 text-gray-400 text-xs">{conn.process || "-"}</td>
                  </tr>
                {/each}
              </tbody>
            </table>
          </div>
        {/if}
      </div>

    <!-- SSH Sessions Tab -->
    {:else if activeTab === "sessions"}
      <div class="card">
        <h3 class="text-lg font-semibold mb-4">Active SSH Sessions</h3>

        {#if overview.ssh_sessions.length === 0}
          <p class="text-gray-500 text-center py-8">No active SSH sessions</p>
        {:else}
          <div class="space-y-3">
            {#each overview.ssh_sessions as session}
              <div class="p-4 bg-gray-700/50 rounded flex items-center justify-between">
                <div class="flex items-center gap-4">
                  <div class="w-10 h-10 bg-blue-500/20 rounded-full flex items-center justify-center">
                    <span class="text-blue-400">👤</span>
                  </div>
                  <div>
                    <p class="font-medium">{session.user}</p>
                    <p class="text-sm text-gray-400">from {session.source_ip}</p>
                  </div>
                </div>
                <div class="text-right">
                  <span class="text-xs px-2 py-1 bg-green-500/20 text-green-400 rounded">
                    {session.status}
                  </span>
                  <p class="text-xs text-gray-400 mt-1">{session.timestamp}</p>
                </div>
              </div>
            {/each}
          </div>
        {/if}
      </div>
    {/if}

    <!-- Top Blocked IPs -->
    {#if overview.top_blocked_ips.length > 0}
      <div class="card">
        <h3 class="text-lg font-semibold mb-4">Top Blocked IPs</h3>
        <div class="overflow-x-auto">
          <table class="w-full text-sm">
            <thead>
              <tr class="text-left text-gray-400 border-b border-gray-700">
                <th class="pb-2">IP Address</th>
                <th class="pb-2">Hits</th>
                <th class="pb-2">Last Seen</th>
                <th class="pb-2">Reason</th>
              </tr>
            </thead>
            <tbody>
              {#each overview.top_blocked_ips as ip}
                <tr class="border-b border-gray-700/50">
                  <td class="py-2 font-mono">{ip.ip}</td>
                  <td class="py-2 text-red-400">{ip.hits}</td>
                  <td class="py-2 text-gray-400">{ip.last_seen}</td>
                  <td class="py-2">{ip.reason}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      </div>
    {/if}
  {/if}
</div>

<style>
  .card {
    background: #1f2937;
    border: 1px solid #374151;
    border-radius: 0.5rem;
    padding: 1.5rem;
  }

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
</style>

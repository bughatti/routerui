<script>
  import { onMount } from "svelte";

  let overview = $state(null);
  let queryLog = $state([]);
  let filters = $state({ filters: [], user_rules: [] });
  let loading = $state(true);
  let newRule = $state("");
  let activeTab = $state("overview");

  async function fetchData() {
    try {
      const [ovRes, logRes, filRes] = await Promise.all([
        fetch("/api/adguard/overview"),
        fetch("/api/adguard/querylog"),
        fetch("/api/adguard/filters")
      ]);

      if (ovRes.ok) overview = await ovRes.json();
      if (logRes.ok) queryLog = await logRes.json();
      if (filRes.ok) filters = await filRes.json();
    } catch (e) {
      console.error(e);
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    fetchData();
    const interval = setInterval(fetchData, 10000);
    return () => clearInterval(interval);
  });

  async function toggleProtection() {
    const res = await fetch("/api/adguard/protection", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ enabled: !overview.protection_enabled })
    });
    if (res.ok) fetchData();
  }

  async function addRule() {
    if (!newRule.trim()) return;
    const res = await fetch("/api/adguard/rules/add", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ rule: newRule.trim() })
    });
    if (res.ok) {
      newRule = "";
      fetchData();
    }
  }

  async function removeRule(rule) {
    const res = await fetch("/api/adguard/rules/remove", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ rule })
    });
    if (res.ok) fetchData();
  }

  async function whitelistDomain(domain) {
    if (!domain) return;
    const rule = `@@||${domain}^`;
    try {
      const res = await fetch("/api/adguard/rules/add", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        credentials: "include",
        body: JSON.stringify({ rule })
      });
      if (res.ok) {
        alert(`Whitelisted: ${domain}`);
        fetchData();
      } else {
        alert(`Failed to whitelist: ${res.status}`);
      }
    } catch (e) {
      alert(`Error: ${e.message}`);
    }
  }

  async function blacklistDomain(domain) {
    if (!domain) return;
    const rule = `||${domain}^`;
    try {
      const res = await fetch("/api/adguard/rules/add", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        credentials: "include",
        body: JSON.stringify({ rule })
      });
      if (res.ok) {
        alert(`Blocked: ${domain}`);
        fetchData();
      } else {
        alert(`Failed to block: ${res.status}`);
      }
    } catch (e) {
      alert(`Error: ${e.message}`);
    }
  }

  function formatTime(timeStr) {
    try {
      const d = new Date(timeStr);
      return d.toLocaleTimeString();
    } catch { return timeStr; }
  }

  function getReasonClass(reason) {
    if (reason === "NotFilteredNotFound" || reason === "NotFilteredWhiteList") return "text-green-400";
    if (reason.includes("Filtered")) return "text-red-400";
    return "text-gray-400";
  }

  function getReasonLabel(reason) {
    const labels = {
      "NotFilteredNotFound": "Allowed",
      "NotFilteredWhiteList": "Whitelisted",
      "FilteredBlackList": "Blocked (blacklist)",
      "FilteredSafeBrowsing": "Blocked (safe browsing)",
      "FilteredParental": "Blocked (parental)",
      "FilteredBlockedService": "Blocked (service)",
      "Rewrite": "Rewritten"
    };
    return labels[reason] || reason;
  }
</script>

<svelte:head>
  <title>AdGuard - RouterUI</title>
</svelte:head>

<div class="space-y-6">
  <div class="flex items-center justify-between">
    <h2 class="text-2xl font-bold">AdGuard Home</h2>
    {#if overview}
      <button
        onclick={toggleProtection}
        class="btn {overview.protection_enabled ? 'btn-danger' : 'btn-primary'}"
      >
        {overview.protection_enabled ? "Disable Protection" : "Enable Protection"}
      </button>
    {/if}
  </div>

  {#if loading}
    <div class="text-gray-400">Loading...</div>
  {:else}
    <!-- Stats Cards -->
    {#if overview}
    <div class="grid grid-cols-2 md:grid-cols-5 gap-4">
      <div class="card text-center">
        <p class="text-3xl font-bold {overview.protection_enabled ? 'text-green-400' : 'text-red-400'}">
          {overview.protection_enabled ? "ON" : "OFF"}
        </p>
        <p class="text-xs text-gray-400">Protection</p>
      </div>
      <div class="card text-center">
        <p class="text-2xl font-bold">{overview.dns_queries.toLocaleString()}</p>
        <p class="text-xs text-gray-400">DNS Queries</p>
      </div>
      <div class="card text-center">
        <p class="text-2xl font-bold text-red-400">{overview.blocked_filtering.toLocaleString()}</p>
        <p class="text-xs text-gray-400">Blocked</p>
      </div>
      <div class="card text-center">
        <p class="text-2xl font-bold">{overview.blocked_percentage.toFixed(1)}%</p>
        <p class="text-xs text-gray-400">Block Rate</p>
      </div>
      <div class="card text-center">
        <p class="text-2xl font-bold">{overview.avg_processing_time.toFixed(1)}ms</p>
        <p class="text-xs text-gray-400">Avg Response</p>
      </div>
    </div>
    {/if}

    <!-- Tabs -->
    <div class="flex gap-2 border-b border-gray-700 pb-2">
      <button
        onclick={() => activeTab = "overview"}
        class="px-4 py-2 rounded-t {activeTab === 'overview' ? 'bg-gray-700 text-white' : 'text-gray-400 hover:text-white'}"
      >Query Log</button>
      <button
        onclick={() => activeTab = "filters"}
        class="px-4 py-2 rounded-t {activeTab === 'filters' ? 'bg-gray-700 text-white' : 'text-gray-400 hover:text-white'}"
      >Filter Lists</button>
      <button
        onclick={() => activeTab = "rules"}
        class="px-4 py-2 rounded-t {activeTab === 'rules' ? 'bg-gray-700 text-white' : 'text-gray-400 hover:text-white'}"
      >Custom Rules</button>
    </div>

    <!-- Query Log Tab -->
    {#if activeTab === "overview"}
    <div class="card">
      <h3 class="text-lg font-semibold mb-3">Recent DNS Queries</h3>
      <div class="overflow-x-auto max-h-96 overflow-y-auto">
        <table class="w-full text-sm">
          <thead class="sticky top-0 bg-gray-800">
            <tr class="text-left text-gray-400 border-b border-gray-700">
              <th class="pb-2 pr-4">Time</th>
              <th class="pb-2 pr-4">Client</th>
              <th class="pb-2 pr-4">Domain</th>
              <th class="pb-2 pr-4">Type</th>
              <th class="pb-2 pr-4">Status</th>
              <th class="pb-2">Actions</th>
            </tr>
          </thead>
          <tbody>
            {#each queryLog.slice(0, 50) as entry}
              <tr class="border-b border-gray-700/50 hover:bg-gray-700/30">
                <td class="py-1 pr-4 text-gray-400">{formatTime(entry.time)}</td>
                <td class="py-1 pr-4">{entry.client}</td>
                <td class="py-1 pr-4 font-mono text-xs">{entry.question?.name || "-"}</td>
                <td class="py-1 pr-4 text-gray-400">{entry.question?.qtype || "-"}</td>
                <td class="py-1 pr-4 {getReasonClass(entry.reason)}">{getReasonLabel(entry.reason)}</td>
                <td class="py-1">
                  <div class="flex gap-1">
                    <button
                      onclick={() => whitelistDomain(entry.question?.name)}
                      class="px-2 py-0.5 text-xs bg-green-600/20 text-green-400 hover:bg-green-600/40 rounded"
                      title="Whitelist this domain"
                    >Allow</button>
                    <button
                      onclick={() => blacklistDomain(entry.question?.name)}
                      class="px-2 py-0.5 text-xs bg-red-600/20 text-red-400 hover:bg-red-600/40 rounded"
                      title="Block this domain"
                    >Block</button>
                  </div>
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    </div>
    {/if}

    <!-- Filters Tab -->
    {#if activeTab === "filters"}
    <div class="card">
      <h3 class="text-lg font-semibold mb-3">Filter Lists</h3>
      <div class="space-y-2">
        {#each filters.filters as filter}
          <div class="flex items-center justify-between p-3 bg-gray-700/50 rounded">
            <div>
              <p class="font-medium">{filter.name}</p>
              <p class="text-xs text-gray-400">{filter.rules_count.toLocaleString()} rules</p>
            </div>
            <span class={filter.enabled ? "text-green-400" : "text-gray-500"}>
              {filter.enabled ? "Enabled" : "Disabled"}
            </span>
          </div>
        {/each}
      </div>
    </div>
    {/if}

    <!-- Custom Rules Tab -->
    {#if activeTab === "rules"}
    <div class="card">
      <h3 class="text-lg font-semibold mb-3">Custom Rules</h3>
      <p class="text-sm text-gray-400 mb-4">
        Add domains to block or whitelist. Use <code class="bg-gray-700 px-1 rounded">||domain.com^</code> to block,
        <code class="bg-gray-700 px-1 rounded">@@||domain.com^</code> to whitelist.
      </p>

      <div class="flex gap-2 mb-4">
        <input
          type="text"
          bind:value={newRule}
          placeholder="||ads.example.com^"
          class="flex-1 bg-gray-700 border border-gray-600 rounded px-3 py-2 text-sm focus:outline-none focus:border-blue-500"
          onkeydown={(e) => e.key === "Enter" && addRule()}
        />
        <button onclick={addRule} class="btn btn-primary">Add Rule</button>
      </div>

      <div class="space-y-1">
        {#each filters.user_rules as rule}
          <div class="flex items-center justify-between p-2 bg-gray-700/50 rounded font-mono text-sm">
            <span class={rule.startsWith("@@") ? "text-green-400" : "text-red-400"}>{rule}</span>
            <button onclick={() => removeRule(rule)} class="text-gray-400 hover:text-red-400 px-2">X</button>
          </div>
        {:else}
          <p class="text-gray-500 text-sm">No custom rules defined</p>
        {/each}
      </div>
    </div>
    {/if}
  {/if}
</div>

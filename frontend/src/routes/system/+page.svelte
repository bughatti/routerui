<script>
  import { onMount } from "svelte";

  let loading = $state(true);
  let activeTab = $state("logs");

  // Logs state
  let logUnits = $state([]);
  let selectedUnit = $state("");
  let selectedPriority = $state("");
  let logLines = $state(100);
  let logGrep = $state("");
  let logs = $state("");
  let logsLoading = $state(false);

  // Backup state
  let backups = $state([]);
  let backupCreating = $state(false);
  let restoreInProgress = $state(false);
  let selectedBackup = $state(null);

  // Updates state
  let updateOutput = $state("");
  let updatesLoading = $state(false);
  let updatesRunning = $state(false);

  async function fetchData() {
    try {
      const [unitsRes, backupsRes] = await Promise.all([
        fetch("/api/tools/logs/units"),
        fetch("/api/tools/backup/list")
      ]);

      if (unitsRes.ok) logUnits = await unitsRes.json();
      if (backupsRes.ok) backups = await backupsRes.json();
    } catch (e) {
      console.error(e);
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    fetchData();
    fetchLogs();
  });

  async function fetchLogs() {
    logsLoading = true;
    try {
      const res = await fetch("/api/tools/logs", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          unit: selectedUnit || null,
          priority: selectedPriority || null,
          lines: logLines,
          grep: logGrep || null
        })
      });
      if (res.ok) {
        const data = await res.json();
        logs = data.logs;
      }
    } finally {
      logsLoading = false;
    }
  }

  async function checkUpdates() {
    updatesLoading = true;
    updateOutput = "Checking for updates...\n";
    try {
      const res = await fetch("/api/system/updates/check", { method: "POST" });
      if (res.ok) {
        const data = await res.json();
        updateOutput = data.output;
      } else {
        updateOutput = "Failed to check for updates";
      }
    } catch (e) {
      updateOutput = "Error: " + e.message;
    } finally {
      updatesLoading = false;
    }
  }

  async function runUpdates() {
    if (!confirm("This will install system updates. The system may need to restart some services. Continue?")) return;
    updatesRunning = true;
    updateOutput = "Starting system update...\n\n";
    try {
      const res = await fetch("/api/system/updates/install", { method: "POST" });
      if (res.ok) {
        const data = await res.json();
        updateOutput = data.output;
      } else {
        updateOutput += "\nFailed to run updates";
      }
    } catch (e) {
      updateOutput += "\nError: " + e.message;
    } finally {
      updatesRunning = false;
    }
  }

  async function createBackup() {
    backupCreating = true;
    try {
      const res = await fetch("/api/tools/backup/create", { method: "POST" });
      if (res.ok) {
        await fetchData();
      }
    } finally {
      backupCreating = false;
    }
  }

  async function downloadBackup(filename) {
    const res = await fetch("/api/tools/backup/download", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ filename })
    });
    if (res.ok) {
      selectedBackup = await res.json();
    }
  }

  async function restoreBackup() {
    if (!selectedBackup?.configs) return;
    if (!confirm("This will restore the selected backup and overwrite current configurations. Continue?")) return;

    restoreInProgress = true;
    try {
      const res = await fetch("/api/tools/backup/restore", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(selectedBackup.configs)
      });
      if (res.ok) {
        const result = await res.json();
        if (result.success) {
          alert("Backup restored successfully! Some services may need to be restarted.");
        } else {
          alert("Restore completed with errors: " + result.errors.join(", "));
        }
        selectedBackup = null;
      }
    } finally {
      restoreInProgress = false;
    }
  }

  async function deleteBackup(filename) {
    if (!confirm("Delete this backup?")) return;
    const res = await fetch("/api/tools/backup/delete", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ filename })
    });
    if (res.ok) {
      await fetchData();
    }
  }

  function formatBytes(bytes) {
    if (!bytes || bytes === 0) return "0 B";
    const k = 1024;
    const sizes = ["B", "KB", "MB"];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + " " + sizes[i];
  }

  function formatDate(dateStr) {
    try {
      const date = new Date(dateStr);
      return date.toLocaleString();
    } catch {
      return dateStr;
    }
  }
</script>

<svelte:head>
  <title>System - RouterUI</title>
</svelte:head>

<div class="space-y-6">
  <div>
    <h2 class="text-2xl font-bold">System</h2>
    <p class="text-sm text-gray-500">System logs, updates, and backup management.</p>
  </div>

  {#if loading}
    <div class="text-gray-400">Loading...</div>
  {:else}
    <!-- Tabs -->
    <div class="border-b border-gray-700">
      <nav class="flex gap-4">
        <button
          onclick={() => activeTab = "logs"}
          class="tab-btn {activeTab === 'logs' ? 'tab-active' : ''}"
        >
          System Logs
        </button>
        <button
          onclick={() => activeTab = "updates"}
          class="tab-btn {activeTab === 'updates' ? 'tab-active' : ''}"
        >
          Updates
        </button>
        <button
          onclick={() => activeTab = "backup"}
          class="tab-btn {activeTab === 'backup' ? 'tab-active' : ''}"
        >
          Backup & Restore
        </button>
      </nav>
    </div>

    <!-- Logs Tab -->
    {#if activeTab === "logs"}
      <div class="card">
        <h3 class="text-lg font-semibold mb-4">System Logs</h3>

        <!-- Filters -->
        <div class="flex flex-wrap gap-3 mb-4">
          <select bind:value={selectedUnit} onchange={fetchLogs} class="input">
            {#each logUnits as unit}
              <option value={unit.name}>{unit.description || unit.name || "All Logs"}</option>
            {/each}
          </select>

          <select bind:value={selectedPriority} onchange={fetchLogs} class="input">
            <option value="">All Priorities</option>
            <option value="0">Emergency</option>
            <option value="1">Alert</option>
            <option value="2">Critical</option>
            <option value="3">Error</option>
            <option value="4">Warning</option>
            <option value="5">Notice</option>
            <option value="6">Info</option>
            <option value="7">Debug</option>
          </select>

          <select bind:value={logLines} onchange={fetchLogs} class="input">
            <option value={50}>50 lines</option>
            <option value={100}>100 lines</option>
            <option value={250}>250 lines</option>
            <option value={500}>500 lines</option>
            <option value={1000}>1000 lines</option>
          </select>

          <input
            type="text"
            placeholder="Filter text..."
            bind:value={logGrep}
            onkeypress={(e) => e.key === 'Enter' && fetchLogs()}
            class="input flex-1 min-w-48"
          />

          <button onclick={fetchLogs} class="btn-primary">
            {logsLoading ? "Loading..." : "Refresh"}
          </button>
        </div>

        <!-- Log Output -->
        <div class="bg-gray-900 rounded p-4 font-mono text-xs overflow-auto max-h-[600px]">
          {#if logsLoading}
            <p class="text-gray-400">Loading logs...</p>
          {:else if logs}
            <pre class="whitespace-pre-wrap text-gray-300">{logs}</pre>
          {:else}
            <p class="text-gray-500">No logs to display</p>
          {/if}
        </div>
      </div>

    <!-- Updates Tab -->
    {:else if activeTab === "updates"}
      <div class="space-y-4">
        <div class="card">
          <div class="flex items-center justify-between mb-4">
            <div>
              <h3 class="text-lg font-semibold">System Updates</h3>
              <p class="text-sm text-gray-400">Check and install system package updates</p>
            </div>
            <div class="flex gap-2">
              <button onclick={checkUpdates} disabled={updatesLoading || updatesRunning} class="btn-secondary">
                {updatesLoading ? "Checking..." : "Check Updates"}
              </button>
              <button onclick={runUpdates} disabled={updatesLoading || updatesRunning} class="btn-primary">
                {updatesRunning ? "Updating..." : "Install Updates"}
              </button>
            </div>
          </div>

          <!-- Terminal Output -->
          <div class="terminal">
            {#if updateOutput}
              <pre class="whitespace-pre-wrap text-green-400">{updateOutput}</pre>
            {:else}
              <p class="text-gray-500">Click "Check Updates" to see available updates</p>
            {/if}
            {#if updatesRunning}
              <span class="animate-pulse text-green-400">█</span>
            {/if}
          </div>
        </div>
      </div>

    <!-- Backup Tab -->
    {:else if activeTab === "backup"}
      <div class="space-y-4">
        <!-- Create Backup -->
        <div class="card">
          <div class="flex items-center justify-between">
            <div>
              <h3 class="text-lg font-semibold">Create Backup</h3>
              <p class="text-sm text-gray-400">Backup current router configurations</p>
            </div>
            <button onclick={createBackup} disabled={backupCreating} class="btn-primary">
              {backupCreating ? "Creating..." : "Create Backup"}
            </button>
          </div>
          <p class="text-xs text-gray-500 mt-3">
            Backups include: DHCP config, WiFi config, static leases, firewall rules, WoL devices, and protection whitelist.
          </p>
        </div>

        <!-- Backup List -->
        <div class="card">
          <h3 class="text-lg font-semibold mb-4">Available Backups</h3>

          {#if backups.length > 0}
            <div class="space-y-2">
              {#each backups as backup}
                <div class="flex items-center justify-between p-3 bg-gray-700/50 rounded">
                  <div>
                    <p class="font-medium font-mono text-sm">{backup.filename}</p>
                    <p class="text-xs text-gray-400">
                      {formatDate(backup.created)} • {formatBytes(backup.size)}
                    </p>
                  </div>
                  <div class="flex gap-2">
                    <button
                      onclick={() => downloadBackup(backup.filename)}
                      class="btn-secondary text-sm"
                    >
                      View / Restore
                    </button>
                    <button
                      onclick={() => deleteBackup(backup.filename)}
                      class="btn-danger text-sm"
                    >
                      Delete
                    </button>
                  </div>
                </div>
              {/each}
            </div>
          {:else}
            <div class="text-center py-8 text-gray-500">
              <p>No backups found.</p>
              <p class="text-sm">Create a backup to get started.</p>
            </div>
          {/if}
        </div>

        <!-- Backup Details Modal -->
        {#if selectedBackup}
          <div class="fixed inset-0 bg-black/70 flex items-center justify-center z-50 p-4" onclick={() => selectedBackup = null}>
            <div class="bg-gray-800 rounded-lg w-full max-w-2xl max-h-[80vh] flex flex-col" onclick={(e) => e.stopPropagation()}>
              <div class="flex items-center justify-between p-4 border-b border-gray-700">
                <div>
                  <h3 class="text-lg font-semibold">Backup Details</h3>
                  <p class="text-sm text-gray-400">{formatDate(selectedBackup.created)} • {selectedBackup.hostname}</p>
                </div>
                <button onclick={() => selectedBackup = null} class="text-gray-400 hover:text-white text-2xl">&times;</button>
              </div>

              <div class="flex-1 overflow-auto p-4 space-y-3">
                {#if selectedBackup.configs.dnsmasq}
                  <div>
                    <p class="text-sm font-medium text-gray-400 mb-1">DHCP/DNS Config</p>
                    <pre class="text-xs bg-gray-900 p-2 rounded overflow-x-auto max-h-32">{selectedBackup.configs.dnsmasq}</pre>
                  </div>
                {/if}

                {#if selectedBackup.configs.hostapd}
                  <div>
                    <p class="text-sm font-medium text-gray-400 mb-1">WiFi Config</p>
                    <pre class="text-xs bg-gray-900 p-2 rounded overflow-x-auto max-h-32">{selectedBackup.configs.hostapd}</pre>
                  </div>
                {/if}

                {#if selectedBackup.configs.static_leases}
                  <div>
                    <p class="text-sm font-medium text-gray-400 mb-1">Static Leases</p>
                    <pre class="text-xs bg-gray-900 p-2 rounded overflow-x-auto max-h-32">{selectedBackup.configs.static_leases}</pre>
                  </div>
                {/if}

                {#if selectedBackup.configs.iptables}
                  <div>
                    <p class="text-sm font-medium text-gray-400 mb-1">Firewall Rules</p>
                    <pre class="text-xs bg-gray-900 p-2 rounded overflow-x-auto max-h-32">{selectedBackup.configs.iptables}</pre>
                  </div>
                {/if}
              </div>

              <div class="p-4 border-t border-gray-700 flex justify-end gap-2">
                <button onclick={() => selectedBackup = null} class="btn-secondary">Close</button>
                <button onclick={restoreBackup} disabled={restoreInProgress} class="btn-warning">
                  {restoreInProgress ? "Restoring..." : "Restore This Backup"}
                </button>
              </div>
            </div>
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

  .card {
    background: #1f2937;
    border-radius: 0.5rem;
    padding: 1.5rem;
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

  select.input {
    appearance: none;
    background-image: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' fill='none' viewBox='0 0 24 24' stroke='%239ca3af'%3E%3Cpath stroke-linecap='round' stroke-linejoin='round' stroke-width='2' d='M19 9l-7 7-7-7'%3E%3C/path%3E%3C/svg%3E");
    background-repeat: no-repeat;
    background-position: right 0.5rem center;
    background-size: 1.5em 1.5em;
    padding-right: 2.5rem;
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

  .btn-primary:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .btn-secondary {
    background: rgba(107, 114, 128, 0.2);
    color: #9ca3af;
    padding: 0.5rem 1rem;
    border-radius: 0.375rem;
    font-weight: 500;
  }

  .btn-secondary:hover {
    background: rgba(107, 114, 128, 0.3);
    color: white;
  }

  .btn-secondary:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .btn-danger {
    background: rgba(239, 68, 68, 0.2);
    color: #ef4444;
    padding: 0.5rem 1rem;
    border-radius: 0.375rem;
    font-weight: 500;
  }

  .btn-danger:hover {
    background: rgba(239, 68, 68, 0.3);
  }

  .btn-warning {
    background: #f59e0b;
    color: white;
    padding: 0.5rem 1rem;
    border-radius: 0.375rem;
    font-weight: 500;
  }

  .btn-warning:hover {
    background: #d97706;
  }

  .btn-warning:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .terminal {
    background: #0d1117;
    border: 1px solid #30363d;
    border-radius: 0.5rem;
    padding: 1rem;
    font-family: 'Monaco', 'Menlo', 'Ubuntu Mono', monospace;
    font-size: 0.75rem;
    overflow: auto;
    height: 500px;
  }
</style>

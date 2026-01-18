<script>
  import { onMount } from "svelte";

  // State
  let loading = $state(true);
  let status = $state(null);
  let scanHistory = $state([]);
  let quarantine = $state([]);
  let scanning = $state(false);
  let updating = $state(false);
  let activeTab = $state("status"); // status, scan, quarantine, history

  // Custom scan path
  let customPath = $state("/home");

  async function fetchData() {
    try {
      const [statusRes, historyRes, quarantineRes] = await Promise.all([
        fetch("/api/antivirus/status"),
        fetch("/api/antivirus/history"),
        fetch("/api/antivirus/quarantine")
      ]);

      if (statusRes.ok) status = await statusRes.json();
      if (historyRes.ok) scanHistory = await historyRes.json();
      if (quarantineRes.ok) quarantine = await quarantineRes.json();
    } catch (e) {
      console.error(e);
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    fetchData();
    const interval = setInterval(fetchData, 30000); // Refresh every 30s
    return () => clearInterval(interval);
  });

  // Update signatures
  async function updateSignatures() {
    updating = true;
    try {
      const res = await fetch("/api/antivirus/update", { method: "POST" });
      if (res.ok) {
        await fetchData();
      }
    } finally {
      updating = false;
    }
  }

  // Start scan
  async function startScan(path) {
    scanning = true;
    try {
      const res = await fetch("/api/antivirus/scan", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ path, quarantine: true })
      });
      if (res.ok) {
        const result = await res.json();
        scanHistory = [result, ...scanHistory];
        activeTab = "history";
      }
    } finally {
      scanning = false;
      await fetchData();
    }
  }

  // Quarantine action
  async function handleQuarantineAction(id, action) {
    const res = await fetch("/api/antivirus/quarantine/action", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ id, action })
    });
    if (res.ok) {
      await fetchData();
    }
  }

  // Toggle daemon
  async function toggleDaemon() {
    const res = await fetch("/api/antivirus/daemon", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ enabled: !status.daemon_running })
    });
    if (res.ok) {
      await fetchData();
    }
  }

  // Format file size
  function formatSize(bytes) {
    if (bytes === 0) return "0 B";
    const k = 1024;
    const sizes = ["B", "KB", "MB", "GB"];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + " " + sizes[i];
  }

  // Format number with commas
  function formatNumber(n) {
    return n?.toLocaleString() || "0";
  }
</script>

<svelte:head>
  <title>Antivirus - RouterUI</title>
</svelte:head>

<div class="space-y-6">
  <!-- Header -->
  <div class="flex items-center justify-between">
    <div>
      <h2 class="text-2xl font-bold">Antivirus</h2>
      <p class="text-sm text-gray-500">ClamAV virus scanner and protection.</p>
    </div>
  </div>

  {#if loading}
    <div class="text-gray-400">Loading...</div>
  {:else if !status?.installed}
    <div class="card bg-red-900/20 border-red-700">
      <p class="text-red-400">ClamAV is not installed. Please install it to use antivirus features.</p>
    </div>
  {:else}
    <!-- Status Overview -->
    <div class="card">
      <div class="grid grid-cols-2 md:grid-cols-4 gap-4">
        <div class="bg-gray-700/50 rounded p-3 text-center">
          <p class="text-2xl font-bold text-green-400">{formatNumber(status.signature_count)}</p>
          <p class="text-xs text-gray-400">Virus Signatures</p>
        </div>
        <div class="bg-gray-700/50 rounded p-3 text-center">
          <p class="text-2xl font-bold text-blue-400">{status.signature_version}</p>
          <p class="text-xs text-gray-400">Database Version</p>
        </div>
        <div class="bg-gray-700/50 rounded p-3 text-center">
          <p class="text-2xl font-bold {status.daemon_running ? 'text-green-400' : 'text-yellow-400'}">
            {status.daemon_running ? "Active" : "Stopped"}
          </p>
          <p class="text-xs text-gray-400">Daemon Status</p>
        </div>
        <div class="bg-gray-700/50 rounded p-3 text-center">
          <p class="text-2xl font-bold text-orange-400">{status.quarantine_count}</p>
          <p class="text-xs text-gray-400">Quarantined</p>
        </div>
      </div>
      <div class="mt-4 flex items-center justify-between text-sm text-gray-400">
        <span>Last Updated: {status.last_update}</span>
        <button
          onclick={updateSignatures}
          disabled={updating}
          class="btn-secondary text-sm"
        >
          {updating ? "Updating..." : "Update Signatures"}
        </button>
      </div>
    </div>

    <!-- Tabs -->
    <div class="border-b border-gray-700">
      <nav class="flex gap-4">
        <button
          onclick={() => activeTab = "scan"}
          class="tab-btn {activeTab === 'scan' ? 'tab-active' : ''}"
        >
          Scan Now
        </button>
        <button
          onclick={() => activeTab = "history"}
          class="tab-btn {activeTab === 'history' ? 'tab-active' : ''}"
        >
          Scan History
        </button>
        <button
          onclick={() => activeTab = "quarantine"}
          class="tab-btn {activeTab === 'quarantine' ? 'tab-active' : ''}"
        >
          Quarantine ({status.quarantine_count})
        </button>
        <button
          onclick={() => activeTab = "settings"}
          class="tab-btn {activeTab === 'settings' ? 'tab-active' : ''}"
        >
          Settings
        </button>
      </nav>
    </div>

    <!-- Tab Content -->
    {#if activeTab === "scan"}
      <div class="card">
        <h3 class="text-lg font-semibold mb-4">Scan for Threats</h3>

        {#if scanning}
          <div class="text-center py-8">
            <div class="inline-block animate-spin rounded-full h-12 w-12 border-b-2 border-blue-400 mb-4"></div>
            <p class="text-blue-400">Scanning in progress...</p>
            <p class="text-sm text-gray-500">This may take a while depending on the number of files.</p>
          </div>
        {:else}
          <div class="grid grid-cols-1 md:grid-cols-3 gap-4 mb-6">
            <button
              onclick={() => startScan("/home")}
              class="scan-card"
            >
              <div class="text-3xl mb-2">🏠</div>
              <div class="font-medium">Scan Home</div>
              <div class="text-sm text-gray-400">/home directory</div>
            </button>

            <button
              onclick={() => startScan("/tmp")}
              class="scan-card"
            >
              <div class="text-3xl mb-2">📁</div>
              <div class="font-medium">Scan Temp</div>
              <div class="text-sm text-gray-400">/tmp directory</div>
            </button>

            <button
              onclick={() => startScan("/var/www")}
              class="scan-card"
            >
              <div class="text-3xl mb-2">🌐</div>
              <div class="font-medium">Scan Web</div>
              <div class="text-sm text-gray-400">/var/www directory</div>
            </button>
          </div>

          <div class="border-t border-gray-700 pt-4">
            <h4 class="font-medium mb-2">Custom Scan</h4>
            <div class="flex gap-2">
              <input
                type="text"
                placeholder="Enter path to scan (e.g., /home/user/downloads)"
                bind:value={customPath}
                class="input flex-1"
              />
              <button
                onclick={() => startScan(customPath)}
                class="btn-primary"
              >
                Scan
              </button>
            </div>
          </div>
        {/if}
      </div>

    {:else if activeTab === "history"}
      <div class="card">
        <h3 class="text-lg font-semibold mb-4">Scan History</h3>

        {#if scanHistory.length === 0}
          <div class="text-center py-8 text-gray-500">
            <p>No scans performed yet.</p>
            <p class="text-sm">Run a scan to see results here.</p>
          </div>
        {:else}
          <div class="space-y-3">
            {#each scanHistory as scan}
              <div class="p-4 bg-gray-700/50 rounded">
                <div class="flex items-center justify-between mb-2">
                  <div class="flex items-center gap-2">
                    <span class="font-medium">{scan.path}</span>
                    {#if scan.threats_found > 0}
                      <span class="text-xs px-2 py-0.5 bg-red-500/20 text-red-400 rounded">
                        {scan.threats_found} threats
                      </span>
                    {:else}
                      <span class="text-xs px-2 py-0.5 bg-green-500/20 text-green-400 rounded">
                        Clean
                      </span>
                    {/if}
                  </div>
                  <span class="text-sm text-gray-400">{scan.completed_at || scan.started_at}</span>
                </div>
                <div class="text-sm text-gray-400">
                  Scanned {formatNumber(scan.files_scanned)} files
                </div>
                {#if scan.threats.length > 0}
                  <div class="mt-2 pt-2 border-t border-gray-600">
                    <div class="text-sm font-medium text-red-400 mb-1">Threats Found:</div>
                    {#each scan.threats as threat}
                      <div class="text-sm text-gray-300 flex justify-between">
                        <span class="font-mono truncate flex-1">{threat.file_path}</span>
                        <span class="text-red-400 ml-2">{threat.threat_name}</span>
                      </div>
                    {/each}
                  </div>
                {/if}
              </div>
            {/each}
          </div>
        {/if}
      </div>

    {:else if activeTab === "quarantine"}
      <div class="card">
        <h3 class="text-lg font-semibold mb-4">Quarantined Files</h3>

        <p class="text-sm text-gray-400 mb-4">
          Files detected as threats are moved here. You can delete them permanently or restore if it was a false positive.
        </p>

        {#if quarantine.length === 0}
          <div class="text-center py-8 text-gray-500">
            <p>No files in quarantine.</p>
          </div>
        {:else}
          <div class="space-y-2">
            {#each quarantine as entry}
              <div class="flex items-center justify-between p-3 bg-gray-700/50 rounded">
                <div class="flex-1">
                  <div class="font-mono text-sm">{entry.id}</div>
                  <div class="text-xs text-gray-400">
                    {formatSize(entry.size_bytes)} - Quarantined {entry.quarantined_at}
                  </div>
                </div>
                <div class="flex gap-2">
                  <button
                    onclick={() => handleQuarantineAction(entry.id, "restore")}
                    class="text-xs px-3 py-1 bg-yellow-500/20 text-yellow-400 rounded hover:bg-yellow-500/30"
                  >
                    Restore
                  </button>
                  <button
                    onclick={() => handleQuarantineAction(entry.id, "delete")}
                    class="text-xs px-3 py-1 bg-red-500/20 text-red-400 rounded hover:bg-red-500/30"
                  >
                    Delete
                  </button>
                </div>
              </div>
            {/each}
          </div>
        {/if}
      </div>

    {:else if activeTab === "settings"}
      <div class="card">
        <h3 class="text-lg font-semibold mb-4">Settings</h3>

        <div class="space-y-4">
          <!-- Daemon Toggle -->
          <div class="flex items-center justify-between p-3 bg-gray-700/50 rounded">
            <div>
              <div class="font-medium">ClamAV Daemon</div>
              <div class="text-sm text-gray-400">Keep daemon running for faster scans</div>
            </div>
            <label class="toggle">
              <input
                type="checkbox"
                checked={status.daemon_running}
                onchange={toggleDaemon}
              />
              <span class="toggle-slider"></span>
            </label>
          </div>

          <!-- Version Info -->
          <div class="p-3 bg-gray-700/50 rounded">
            <div class="font-medium mb-2">Version Information</div>
            <div class="text-sm text-gray-400 space-y-1">
              <div>ClamAV: {status.version}</div>
              <div>Signature Database: {status.signature_version}</div>
              <div>Signatures: {formatNumber(status.signature_count)}</div>
              <div>Last Update: {status.last_update}</div>
            </div>
          </div>

          <!-- Info Box -->
          <div class="p-3 bg-blue-900/20 border border-blue-700/50 rounded">
            <div class="text-sm text-blue-400">
              <strong>Note:</strong> Virus signatures are automatically updated by the freshclam service.
              You can manually update using the button above if needed.
            </div>
          </div>
        </div>
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

  .scan-card {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    padding: 1.5rem;
    background: rgba(55, 65, 81, 0.5);
    border-radius: 0.5rem;
    transition: all 0.2s;
    cursor: pointer;
  }

  .scan-card:hover {
    background: rgba(75, 85, 99, 0.5);
    transform: translateY(-2px);
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

  @keyframes spin {
    from { transform: rotate(0deg); }
    to { transform: rotate(360deg); }
  }

  .animate-spin {
    animation: spin 1s linear infinite;
  }
</style>

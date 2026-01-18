<script>
  import { onMount } from "svelte";

  let status = $state(null);
  let portForwards = $state([]);
  let blockedIPs = $state([]);
  let dmz = $state(null);
  let rawRules = $state(null);
  let loading = $state(true);
  let showRawRules = $state(false);

  // Pending state
  let pendingInfo = $state({ pending: false, seconds_remaining: null });

  // Form states
  let newPortForward = $state({
    protocol: "tcp",
    external_port: "",
    internal_ip: "",
    internal_port: ""
  });
  let newBlockedIP = $state("");
  let dmzIP = $state("");

  async function fetchData() {
    try {
      const [statusRes, portsRes, blockedRes, dmzRes, pendingRes] = await Promise.all([
        fetch("/api/firewall/status"),
        fetch("/api/firewall/port-forwards"),
        fetch("/api/firewall/blocked-ips"),
        fetch("/api/firewall/dmz"),
        fetch("/api/firewall/pending")
      ]);

      if (statusRes.ok) status = await statusRes.json();
      if (portsRes.ok) portForwards = await portsRes.json();
      if (blockedRes.ok) blockedIPs = await blockedRes.json();
      if (dmzRes.ok) {
        dmz = await dmzRes.json();
        if (dmz.target_ip) dmzIP = dmz.target_ip;
      }
      if (pendingRes.ok) pendingInfo = await pendingRes.json();
    } catch (e) {
      console.error(e);
    } finally {
      loading = false;
    }
  }

  async function fetchRawRules() {
    const res = await fetch("/api/firewall/rules");
    if (res.ok) rawRules = await res.json();
  }

  onMount(() => {
    fetchData();
    // Poll more frequently when changes are pending
    const interval = setInterval(() => {
      fetchData();
    }, pendingInfo.pending ? 1000 : 5000);
    return () => clearInterval(interval);
  });

  // Re-setup interval when pending state changes
  $effect(() => {
    if (pendingInfo.pending) {
      const quickPoll = setInterval(fetchData, 1000);
      return () => clearInterval(quickPoll);
    }
  });

  async function toggleFirewall() {
    const res = await fetch("/api/firewall/toggle", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ enabled: !status.enabled })
    });
    if (res.ok) {
      status = await res.json();
      fetchData();
    }
  }

  async function confirmChanges() {
    const res = await fetch("/api/firewall/confirm", { method: "POST" });
    if (res.ok) {
      pendingInfo = await res.json();
      fetchData();
    }
  }

  async function revertChanges() {
    const res = await fetch("/api/firewall/revert", { method: "POST" });
    if (res.ok) {
      pendingInfo = await res.json();
      fetchData();
    }
  }

  async function addPortForward() {
    if (!newPortForward.external_port || !newPortForward.internal_ip || !newPortForward.internal_port) return;

    const res = await fetch("/api/firewall/port-forwards/add", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        protocol: newPortForward.protocol,
        external_port: parseInt(newPortForward.external_port),
        internal_ip: newPortForward.internal_ip,
        internal_port: parseInt(newPortForward.internal_port)
      })
    });

    if (res.ok) {
      newPortForward = { protocol: "tcp", external_port: "", internal_ip: "", internal_port: "" };
      fetchData();
    }
  }

  async function removePortForward(pf) {
    const res = await fetch("/api/firewall/port-forwards/remove", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        protocol: pf.protocol,
        external_port: pf.external_port,
        internal_ip: pf.internal_ip,
        internal_port: pf.internal_port
      })
    });
    if (res.ok) fetchData();
  }

  async function addBlockedIP() {
    if (!newBlockedIP.trim()) return;

    const res = await fetch("/api/firewall/blocked-ips/add", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ ip: newBlockedIP.trim() })
    });

    if (res.ok) {
      newBlockedIP = "";
      fetchData();
    }
  }

  async function removeBlockedIP(ip) {
    const res = await fetch("/api/firewall/blocked-ips/remove", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ ip })
    });
    if (res.ok) fetchData();
  }

  async function setDMZ() {
    const res = await fetch("/api/firewall/dmz/set", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        enabled: !!dmzIP.trim(),
        target_ip: dmzIP.trim() || null
      })
    });
    if (res.ok) {
      dmz = await res.json();
      fetchData();
    }
  }

  async function disableDMZ() {
    const res = await fetch("/api/firewall/dmz/set", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ enabled: false, target_ip: null })
    });
    if (res.ok) {
      dmz = await res.json();
      dmzIP = "";
      fetchData();
    }
  }

  function formatTime(seconds) {
    const mins = Math.floor(seconds / 60);
    const secs = seconds % 60;
    return `${mins}:${secs.toString().padStart(2, '0')}`;
  }
</script>

<svelte:head>
  <title>Firewall - RouterUI</title>
</svelte:head>

<div class="space-y-6">
  <h2 class="text-2xl font-bold">Firewall</h2>

  <!-- Pending Changes Banner -->
  {#if pendingInfo.pending}
    <div class="pending-banner">
      <div class="flex items-center gap-4">
        <div class="pending-icon">!</div>
        <div>
          <p class="font-semibold">Changes Pending Confirmation</p>
          <p class="text-sm opacity-90">
            Auto-revert in <span class="font-mono font-bold">{formatTime(pendingInfo.seconds_remaining || 0)}</span>
          </p>
        </div>
      </div>
      <div class="flex gap-2">
        <button onclick={confirmChanges} class="confirm-btn">
          Confirm Changes
        </button>
        <button onclick={revertChanges} class="revert-btn">
          Revert Now
        </button>
      </div>
    </div>
  {/if}

  {#if loading}
    <div class="text-gray-400">Loading...</div>
  {:else}
    <!-- Firewall Status -->
    <div class="card">
      <div class="flex items-center justify-between">
        <div>
          <h3 class="text-lg font-semibold">Firewall Status</h3>
          <p class="text-sm text-gray-400">
            Default policies - Input: <span class="font-mono">{status?.input_policy}</span>,
            Forward: <span class="font-mono">{status?.forward_policy}</span>,
            Output: <span class="font-mono">{status?.output_policy}</span>
          </p>
        </div>
        <div class="flex items-center gap-4">
          <span class={status?.enabled ? "status-active" : "status-inactive"}>
            {status?.enabled ? "Enabled (DROP)" : "Disabled (ACCEPT)"}
          </span>
          <button
            onclick={toggleFirewall}
            class="btn {status?.enabled ? 'btn-danger' : 'btn-primary'}"
          >
            {status?.enabled ? "Disable" : "Enable"}
          </button>
        </div>
      </div>
      {#if !status?.enabled}
        <p class="text-sm text-yellow-400 mt-2">
          Note: When enabled, the firewall will DROP all incoming WAN traffic except established connections and LAN traffic.
        </p>
      {/if}
    </div>

    <!-- Port Forwarding -->
    <div class="card">
      <h3 class="text-lg font-semibold mb-4">Port Forwarding</h3>
      <p class="text-sm text-gray-400 mb-4">
        Forward external ports to internal devices on your network.
      </p>

      <!-- Add new port forward -->
      <div class="flex flex-wrap gap-2 mb-4 p-3 bg-gray-700/30 rounded">
        <select
          bind:value={newPortForward.protocol}
          class="bg-gray-700 border border-gray-600 rounded px-3 py-2 text-sm"
        >
          <option value="tcp">TCP</option>
          <option value="udp">UDP</option>
          <option value="both">Both</option>
        </select>
        <input
          type="number"
          bind:value={newPortForward.external_port}
          placeholder="External Port"
          class="w-32 bg-gray-700 border border-gray-600 rounded px-3 py-2 text-sm"
        />
        <span class="text-gray-400 self-center">to</span>
        <input
          type="text"
          bind:value={newPortForward.internal_ip}
          placeholder="Internal IP (e.g. 10.22.22.100)"
          class="w-48 bg-gray-700 border border-gray-600 rounded px-3 py-2 text-sm"
        />
        <span class="text-gray-400 self-center">:</span>
        <input
          type="number"
          bind:value={newPortForward.internal_port}
          placeholder="Internal Port"
          class="w-32 bg-gray-700 border border-gray-600 rounded px-3 py-2 text-sm"
        />
        <button onclick={addPortForward} class="btn btn-primary">Add Rule</button>
      </div>

      <!-- Port forwards list -->
      {#if portForwards.length > 0}
        <div class="overflow-x-auto">
          <table class="w-full text-sm">
            <thead>
              <tr class="text-left text-gray-400 border-b border-gray-700">
                <th class="pb-2">Protocol</th>
                <th class="pb-2">External Port</th>
                <th class="pb-2">Internal Destination</th>
                <th class="pb-2">Actions</th>
              </tr>
            </thead>
            <tbody>
              {#each portForwards as pf}
                <tr class="border-b border-gray-700/50">
                  <td class="py-2 uppercase text-blue-400">{pf.protocol}</td>
                  <td class="py-2">{pf.external_port}</td>
                  <td class="py-2 font-mono">{pf.internal_ip}:{pf.internal_port}</td>
                  <td class="py-2">
                    <button
                      onclick={() => removePortForward(pf)}
                      class="text-red-400 hover:text-red-300"
                    >
                      Delete
                    </button>
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {:else}
        <p class="text-gray-500 text-sm">No port forwards configured</p>
      {/if}
    </div>

    <!-- Blocked IPs -->
    <div class="card">
      <h3 class="text-lg font-semibold mb-4">Blocked IPs</h3>
      <p class="text-sm text-gray-400 mb-4">
        Block specific IP addresses from accessing your network.
      </p>

      <!-- Add blocked IP -->
      <div class="flex gap-2 mb-4">
        <input
          type="text"
          bind:value={newBlockedIP}
          placeholder="IP address (e.g. 192.168.1.100)"
          class="flex-1 max-w-xs bg-gray-700 border border-gray-600 rounded px-3 py-2 text-sm"
          onkeydown={(e) => e.key === "Enter" && addBlockedIP()}
        />
        <button onclick={addBlockedIP} class="btn btn-danger">Block IP</button>
      </div>

      <!-- Blocked IPs list -->
      {#if blockedIPs.length > 0}
        <div class="space-y-2">
          {#each blockedIPs as blocked}
            <div class="flex items-center justify-between p-2 bg-gray-700/50 rounded">
              <span class="font-mono text-red-400">{blocked.ip}</span>
              <button
                onclick={() => removeBlockedIP(blocked.ip)}
                class="text-gray-400 hover:text-green-400 text-sm"
              >
                Unblock
              </button>
            </div>
          {/each}
        </div>
      {:else}
        <p class="text-gray-500 text-sm">No IPs blocked</p>
      {/if}
    </div>

    <!-- DMZ -->
    <div class="card">
      <h3 class="text-lg font-semibold mb-4">DMZ (Demilitarized Zone)</h3>
      <p class="text-sm text-gray-400 mb-4">
        Forward all incoming traffic to a single host. <span class="text-yellow-400">Warning:</span> This exposes the target device to all external traffic.
      </p>

      <div class="flex items-center gap-4">
        <div class="flex gap-2">
          <input
            type="text"
            bind:value={dmzIP}
            placeholder="Target IP (e.g. 10.22.22.100)"
            class="w-48 bg-gray-700 border border-gray-600 rounded px-3 py-2 text-sm"
          />
          <button onclick={setDMZ} class="btn btn-primary">
            {dmz?.enabled ? "Update DMZ" : "Enable DMZ"}
          </button>
          {#if dmz?.enabled}
            <button onclick={disableDMZ} class="btn btn-danger">Disable DMZ</button>
          {/if}
        </div>
        {#if dmz?.enabled}
          <span class="text-yellow-400">
            DMZ active: All traffic forwarded to <span class="font-mono">{dmz.target_ip}</span>
          </span>
        {/if}
      </div>
    </div>

    <!-- Raw Rules -->
    <div class="card">
      <div class="flex items-center justify-between mb-4">
        <h3 class="text-lg font-semibold">Raw iptables Rules</h3>
        <button
          onclick={() => {
            showRawRules = !showRawRules;
            if (showRawRules && !rawRules) fetchRawRules();
          }}
          class="btn btn-secondary"
        >
          {showRawRules ? "Hide" : "Show"} Raw Rules
        </button>
      </div>

      {#if showRawRules}
        {#if rawRules}
          <div class="space-y-4">
            <div>
              <h4 class="text-sm font-semibold text-gray-400 mb-2">Filter Table</h4>
              <pre class="bg-gray-900 p-3 rounded text-xs overflow-x-auto max-h-64 overflow-y-auto">{rawRules.filter}</pre>
            </div>
            <div>
              <h4 class="text-sm font-semibold text-gray-400 mb-2">NAT Table</h4>
              <pre class="bg-gray-900 p-3 rounded text-xs overflow-x-auto max-h-64 overflow-y-auto">{rawRules.nat}</pre>
            </div>
          </div>
        {:else}
          <p class="text-gray-400">Loading...</p>
        {/if}
      {/if}
    </div>
  {/if}
</div>

<style>
  .btn-secondary {
    background-color: #374151;
    color: #f3f4f6;
    padding: 0.5rem 1rem;
    border-radius: 0.375rem;
    font-size: 0.875rem;
  }
  .btn-secondary:hover {
    background-color: #4b5563;
  }

  .pending-banner {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 1rem 1.5rem;
    background: linear-gradient(135deg, #b45309 0%, #d97706 100%);
    border-radius: 0.5rem;
    color: white;
    animation: pulse-border 2s infinite;
  }

  @keyframes pulse-border {
    0%, 100% { box-shadow: 0 0 0 0 rgba(251, 191, 36, 0.4); }
    50% { box-shadow: 0 0 0 8px rgba(251, 191, 36, 0); }
  }

  .pending-icon {
    width: 2.5rem;
    height: 2.5rem;
    background: rgba(255, 255, 255, 0.2);
    border-radius: 50%;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 1.5rem;
    font-weight: bold;
  }

  .confirm-btn {
    background: #22c55e;
    color: white;
    padding: 0.5rem 1.5rem;
    border-radius: 0.375rem;
    font-weight: 600;
    transition: background 0.2s;
  }
  .confirm-btn:hover {
    background: #16a34a;
  }

  .revert-btn {
    background: rgba(255, 255, 255, 0.2);
    color: white;
    padding: 0.5rem 1.5rem;
    border-radius: 0.375rem;
    font-weight: 600;
    transition: background 0.2s;
  }
  .revert-btn:hover {
    background: rgba(255, 255, 255, 0.3);
  }
</style>

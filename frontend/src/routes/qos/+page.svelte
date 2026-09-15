<script>
  import { onMount } from "svelte";

  let loading = $state(true);
  let saving = $state(false);
  let error = $state("");
  let status = $state({
    enabled: false,
    smart_queue: false,
    clients: [],
    applied: false,
    lan_iface: "",
    wan_iface: "",
  });
  let leases = $state([]);

  // Editable working copy
  let enabled = $state(false);
  let smartQueue = $state(false);
  let clients = $state([]); // [{ ip, name, down_mbps, up_mbps }]
  let newClient = $state({ ip: "", name: "", down_mbps: 0, up_mbps: 0 });

  async function fetchData() {
    try {
      const [qosRes, dhcpRes] = await Promise.all([
        fetch("/api/qos/status"),
        fetch("/api/network/dhcp"),
      ]);
      if (qosRes.ok) {
        status = await qosRes.json();
        enabled = !!status.enabled;
        smartQueue = !!status.smart_queue;
        clients = (status.clients || []).map((c) => ({
          ip: c.ip,
          name: c.name || "",
          down_mbps: c.down_mbps || 0,
          up_mbps: c.up_mbps || 0,
        }));
      }
      if (dhcpRes.ok) {
        const d = await dhcpRes.json();
        leases = d.leases || [];
      }
    } catch (e) {
      console.error(e);
      error = "Failed to load QoS status.";
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    fetchData();
  });

  function leaseName(ip) {
    const l = leases.find((x) => x.ip_address === ip);
    return l ? l.hostname || l.mac_address : "";
  }

  function addClient() {
    const ip = newClient.ip.trim();
    if (!ip) return;
    if (clients.some((c) => c.ip === ip)) {
      error = "That IP is already in the list.";
      return;
    }
    clients = [
      ...clients,
      {
        ip,
        name: newClient.name.trim() || leaseName(ip),
        down_mbps: Number(newClient.down_mbps) || 0,
        up_mbps: Number(newClient.up_mbps) || 0,
      },
    ];
    newClient = { ip: "", name: "", down_mbps: 0, up_mbps: 0 };
    error = "";
  }

  function removeClient(ip) {
    clients = clients.filter((c) => c.ip !== ip);
  }

  function addFromLease(ip) {
    if (clients.some((c) => c.ip === ip)) return;
    clients = [...clients, { ip, name: leaseName(ip), down_mbps: 0, up_mbps: 0 }];
  }

  async function save() {
    saving = true;
    error = "";
    try {
      const body = {
        enabled,
        smart_queue: smartQueue,
        clients: clients.map((c) => ({
          ip: c.ip,
          name: c.name || "",
          down_mbps: Math.max(0, Math.min(10000, Number(c.down_mbps) || 0)),
          up_mbps: Math.max(0, Math.min(10000, Number(c.up_mbps) || 0)),
        })),
      };
      const res = await fetch("/api/qos/config", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(body),
      });
      if (!res.ok) {
        error = (await res.text()) || "Save failed.";
      }
      await fetchData();
    } catch (e) {
      error = "Save failed.";
    } finally {
      saving = false;
    }
  }

  async function clearAll() {
    saving = true;
    error = "";
    try {
      const res = await fetch("/api/qos/clear", { method: "POST" });
      if (!res.ok) error = (await res.text()) || "Clear failed.";
      await fetchData();
    } catch (e) {
      error = "Clear failed.";
    } finally {
      saving = false;
    }
  }

  // Leases not already limited, for the quick-add list
  let unlimitedLeases = $derived(
    leases.filter((l) => !clients.some((c) => c.ip === l.ip_address))
  );
</script>

<svelte:head>
  <title>QoS &amp; Bandwidth — RouterUI</title>
</svelte:head>

<div class="space-y-6">
  <div class="flex items-center justify-between">
    <div>
      <h2 class="text-2xl font-bold">QoS &amp; Bandwidth</h2>
      <p class="text-sm text-gray-500">
        Limit per-client download/upload speeds and smooth latency under load.
      </p>
    </div>
    <div class="text-right">
      <span
        class="text-xs px-2 py-1 rounded {status.applied
          ? 'bg-green-500/20 text-green-400'
          : 'bg-gray-500/20 text-gray-400'}"
      >
        {status.applied ? "Shaping active" : "Not shaping"}
      </span>
      {#if status.lan_iface}
        <p class="text-xs text-gray-500 mt-1">
          LAN {status.lan_iface} · WAN {status.wan_iface}
        </p>
      {/if}
    </div>
  </div>

  {#if error}
    <div class="p-3 rounded bg-red-500/10 text-red-400 text-sm">{error}</div>
  {/if}

  {#if loading}
    <div class="text-gray-400">Loading...</div>
  {:else}
    <!-- Global controls -->
    <div class="card">
      <h3 class="text-lg font-semibold mb-4">Global</h3>
      <div class="space-y-4">
        <div class="flex items-center justify-between">
          <div>
            <div class="font-medium">Enable QoS</div>
            <div class="text-sm text-gray-500">
              Master switch. When off, all shaping is removed and traffic is unrestricted.
            </div>
          </div>
          <label class="toggle">
            <input type="checkbox" bind:checked={enabled} />
            <span class="toggle-slider"></span>
          </label>
        </div>
        <div class="flex items-center justify-between">
          <div>
            <div class="font-medium">Smart Queue (anti-bufferbloat)</div>
            <div class="text-sm text-gray-500">
              Applies fq_codel so a heavy download or upload can't spike latency for
              everything else (video calls, gaming).
            </div>
          </div>
          <label class="toggle">
            <input type="checkbox" bind:checked={smartQueue} />
            <span class="toggle-slider"></span>
          </label>
        </div>
      </div>
    </div>

    <!-- Per-client limits -->
    <div class="card">
      <h3 class="text-lg font-semibold mb-4">Per-Client Limits</h3>
      <p class="text-sm text-gray-500 mb-4">
        Set download/upload caps in Mbit/s for a client by LAN IP. Leave a field at 0
        for no limit in that direction.
      </p>

      {#if clients.length === 0}
        <p class="text-gray-500 mb-4">No per-client limits configured.</p>
      {:else}
        <div class="overflow-x-auto mb-4">
          <table class="w-full text-sm">
            <thead>
              <tr class="text-left text-gray-400 border-b border-gray-700">
                <th class="pb-2">Client</th>
                <th class="pb-2">IP Address</th>
                <th class="pb-2">Download (Mbps)</th>
                <th class="pb-2">Upload (Mbps)</th>
                <th class="pb-2"></th>
              </tr>
            </thead>
            <tbody>
              {#each clients as c (c.ip)}
                <tr class="border-b border-gray-700/50">
                  <td class="py-2">{c.name || leaseName(c.ip) || "-"}</td>
                  <td class="py-2 font-mono">{c.ip}</td>
                  <td class="py-2">
                    <input
                      type="number"
                      min="0"
                      max="10000"
                      bind:value={c.down_mbps}
                      class="input w-24"
                    />
                  </td>
                  <td class="py-2">
                    <input
                      type="number"
                      min="0"
                      max="10000"
                      bind:value={c.up_mbps}
                      class="input w-24"
                    />
                  </td>
                  <td class="py-2 text-right">
                    <button
                      onclick={() => removeClient(c.ip)}
                      class="text-red-400 hover:text-red-300 text-sm"
                    >
                      Remove
                    </button>
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {/if}

      <!-- Add by IP -->
      <div class="grid grid-cols-1 md:grid-cols-5 gap-2 items-end">
        <div>
          <label class="block text-sm text-gray-400 mb-1">IP Address</label>
          <input type="text" bind:value={newClient.ip} placeholder="192.168.1.50" class="input w-full" />
        </div>
        <div>
          <label class="block text-sm text-gray-400 mb-1">Name (optional)</label>
          <input type="text" bind:value={newClient.name} class="input w-full" />
        </div>
        <div>
          <label class="block text-sm text-gray-400 mb-1">Download</label>
          <input type="number" min="0" max="10000" bind:value={newClient.down_mbps} class="input w-full" />
        </div>
        <div>
          <label class="block text-sm text-gray-400 mb-1">Upload</label>
          <input type="number" min="0" max="10000" bind:value={newClient.up_mbps} class="input w-full" />
        </div>
        <button onclick={addClient} class="btn btn-secondary">Add</button>
      </div>
    </div>

    <!-- Quick add from active leases -->
    {#if unlimitedLeases.length > 0}
      <div class="card">
        <h3 class="text-lg font-semibold mb-4">Active Clients</h3>
        <div class="space-y-2">
          {#each unlimitedLeases as l}
            <div class="flex items-center justify-between p-2 bg-gray-700/50 rounded">
              <div class="text-sm">
                <span class="font-medium">{l.hostname || "-"}</span>
                <span class="ml-2 font-mono text-gray-400">{l.ip_address}</span>
              </div>
              <button onclick={() => addFromLease(l.ip_address)} class="text-blue-400 hover:text-blue-300 text-sm">
                Add limit
              </button>
            </div>
          {/each}
        </div>
      </div>
    {/if}

    <!-- Actions -->
    <div class="flex gap-3">
      <button onclick={save} disabled={saving} class="btn btn-primary">
        {saving ? "Applying..." : "Save & Apply"}
      </button>
      <button onclick={clearAll} disabled={saving} class="btn btn-secondary">
        Clear All Shaping
      </button>
    </div>
  {/if}
</div>

<style>

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
</style>

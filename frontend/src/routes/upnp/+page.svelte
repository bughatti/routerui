<script>
  import { onMount } from "svelte";

  let status = $state(null);
  let mappings = $state([]);
  let loading = $state(true);
  let busy = $state(false);

  async function fetchData() {
    try {
      const [statusRes, mapRes] = await Promise.all([
        fetch("/api/upnp/status"),
        fetch("/api/upnp/mappings")
      ]);
      if (statusRes.ok) status = await statusRes.json();
      if (mapRes.ok) mappings = await mapRes.json();
    } catch (e) {
      console.error(e);
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    fetchData();
    const interval = setInterval(fetchData, 5000);
    return () => clearInterval(interval);
  });

  async function enableUpnp() {
    busy = true;
    try {
      const res = await fetch("/api/upnp/enable", { method: "POST" });
      if (res.ok) await fetchData();
    } finally {
      busy = false;
    }
  }

  async function disableUpnp() {
    busy = true;
    try {
      const res = await fetch("/api/upnp/disable", { method: "POST" });
      if (res.ok) await fetchData();
    } finally {
      busy = false;
    }
  }
</script>

<svelte:head>
  <title>UPnP - RouterUI</title>
</svelte:head>

<div class="space-y-6">
  <h2 class="text-2xl font-bold">UPnP / NAT-PMP</h2>

  {#if loading}
    <div class="text-gray-400">Loading...</div>
  {:else}
    <!-- Status -->
    <div class="card">
      <div class="flex items-center justify-between">
        <div>
          <h3 class="text-lg font-semibold">Automatic Port Mapping</h3>
          <p class="text-sm text-gray-400">
            WAN interface: <span class="font-mono">{status?.wan_interface || "unknown"}</span>
            {#if status?.installed}
              &middot; miniupnpd installed
            {:else}
              &middot; <span class="text-yellow-400">will be installed on enable</span>
            {/if}
          </p>
        </div>
        <div class="flex items-center gap-4">
          <span class={status?.running ? "status-active" : "status-inactive"}>
            {status?.running ? "Enabled" : "Disabled"}
          </span>
          {#if status?.running}
            <button onclick={disableUpnp} class="btn btn-danger" disabled={busy}>
              {busy ? "Working..." : "Disable"}
            </button>
          {:else}
            <button onclick={enableUpnp} class="btn btn-primary" disabled={busy}>
              {busy ? "Working..." : "Enable"}
            </button>
          {/if}
        </div>
      </div>

      <div class="mt-3 p-3 bg-gray-700/30 rounded text-sm text-gray-300">
        <p class="text-yellow-400 font-semibold mb-1">Security note</p>
        <p>
          UPnP and NAT-PMP let devices on your network (game consoles, some apps)
          open ports through the firewall automatically, without you approving each one.
          This is convenient but widens your attack surface. RouterUI keeps it
          <span class="font-semibold">off by default</span> and, when enabled, restricts
          requests to your LAN subnet with secure mode on (a device may only map ports to
          itself). Leave it disabled unless something on your network needs it.
        </p>
      </div>
    </div>

    <!-- Active mappings -->
    <div class="card">
      <div class="flex items-center justify-between mb-4">
        <h3 class="text-lg font-semibold">Active Mappings</h3>
        <button onclick={fetchData} class="btn btn-secondary">Refresh</button>
      </div>

      {#if mappings.length > 0}
        <div class="overflow-x-auto">
          <table class="w-full text-sm">
            <thead>
              <tr class="text-left text-gray-400 border-b border-gray-700">
                <th class="pb-2">Protocol</th>
                <th class="pb-2">External Port</th>
                <th class="pb-2">Internal Destination</th>
                <th class="pb-2">Description</th>
              </tr>
            </thead>
            <tbody>
              {#each mappings as m}
                <tr class="border-b border-gray-700/50">
                  <td class="py-2 uppercase text-blue-400">{m.protocol}</td>
                  <td class="py-2">{m.external_port}</td>
                  <td class="py-2 font-mono">{m.internal_ip}:{m.internal_port}</td>
                  <td class="py-2 text-gray-400">{m.description || "-"}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {:else}
        <p class="text-gray-500 text-sm">
          {status?.running ? "No active UPnP mappings" : "UPnP is disabled"}
        </p>
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
</style>

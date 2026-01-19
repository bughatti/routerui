<script>
  import { onMount } from "svelte";
  import Gauge from "$lib/components/Gauge.svelte";

  let dashboard = $state(null);
  let adguard = $state(null);
  let traffic = $state(null);
  let loading = $state(true);
  let error = $state(null);

  // Network speed tracking
  let prevNetStats = $state(null);
  let netSpeed = $state({ rxMbps: 0, txMbps: 0 });

  let addonsStatus = $state({});

  async function fetchData() {
    try {
      // First fetch addons status to know what's installed
      const addonsRes = await fetch("/api/addons/status");
      if (addonsRes.ok) {
        addonsStatus = await addonsRes.json();
      }

      // Build list of fetches - only include adguard if installed
      const fetches = [
        fetch("/api/dashboard"),
        fetch("/api/tools/traffic")
      ];

      const adguardInstalled = addonsStatus.adguard?.installed;
      if (adguardInstalled) {
        fetches.push(fetch("/api/adguard/overview"));
      }

      const results = await Promise.all(fetches);
      const [dashRes, trafficRes] = results;
      const adRes = adguardInstalled ? results[2] : null;

      if (!dashRes.ok) throw new Error("Failed to fetch dashboard");
      const newDashboard = await dashRes.json();

      // Calculate network speed
      const wan = newDashboard.interfaces?.find(i => i.name === "enp1s0");
      if (wan && prevNetStats) {
        const timeDelta = (Date.now() - prevNetStats.timestamp) / 1000;
        if (timeDelta > 0) {
          const rxDelta = wan.rx_bytes - prevNetStats.rx_bytes;
          const txDelta = wan.tx_bytes - prevNetStats.tx_bytes;
          netSpeed = {
            rxMbps: Math.max(0, (rxDelta * 8) / (timeDelta * 1000000)),
            txMbps: Math.max(0, (txDelta * 8) / (timeDelta * 1000000))
          };
        }
      }

      if (wan) {
        prevNetStats = {
          rx_bytes: wan.rx_bytes,
          tx_bytes: wan.tx_bytes,
          timestamp: Date.now()
        };
      }

      dashboard = newDashboard;

      if (adRes?.ok) {
        adguard = await adRes.json();
      }

      if (trafficRes.ok) {
        traffic = await trafficRes.json();
      }
    } catch (e) {
      error = e.message;
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    fetchData();
    const interval = setInterval(fetchData, 3000);
    return () => clearInterval(interval);
  });

  function formatBytes(bytes) {
    if (!bytes || bytes === 0) return "0 B";
    const k = 1024;
    const sizes = ["B", "KB", "MB", "GB", "TB"];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + " " + sizes[i];
  }

  function getInterfaceLabel(name) {
    switch (name) {
      case "enp1s0": return "WAN";
      case "enp2s0": return "LAN";
      case "wlo1": return "WiFi";
      case "tailscale0": return "Tailscale";
      default: return name;
    }
  }

  let wanInterface = $derived(dashboard?.interfaces?.find(i => i.name === "enp1s0"));
  let wanTraffic = $derived(traffic?.interfaces?.find(i => i.name === "enp1s0"));
</script>

<svelte:head>
  <title>Dashboard - RouterUI</title>
</svelte:head>

<div class="space-y-6">
  <div class="flex items-center justify-between">
    <div>
      <h2 class="text-2xl font-bold">System Overview</h2>
      <p class="text-sm text-gray-500">Overview of the key system metrics.</p>
    </div>
  </div>

  {#if loading}
    <div class="text-gray-400">Loading...</div>
  {:else if error}
    <div class="card bg-red-900/20 border-red-700 text-red-400">{error}</div>
  {:else if dashboard}
    <!-- System Gauges -->
    <div class="card">
      <div class="flex flex-wrap items-center justify-center gap-6">
        <Gauge value={dashboard.system.memory.percent_used} max={100} label="Memory" unit="%" size={100} type="ring" />
        <Gauge value={dashboard.system.storage.percent_used} max={100} label="Storage" unit="%" size={100} type="ring" />
        <Gauge value={dashboard.system.cpu_usage} max={100} label="CPU" unit="%" size={160} type="speedometer" />
        <Gauge value={netSpeed.rxMbps} max={100} label="Download" unit="Mbps" size={100} type="ring" />
        <Gauge value={netSpeed.txMbps} max={100} label="Upload" unit="Mbps" size={100} type="ring" />
        <Gauge value={dashboard.system.memory.used_mb} max={dashboard.system.memory.total_mb} label="Used RAM" unit="MB" size={100} type="ring" />
      </div>
    </div>

    <!-- Traffic Stats -->
    {#if traffic?.interfaces?.length > 0}
    <div class="card">
      <h3 class="text-lg font-semibold mb-3">Bandwidth Usage</h3>
      <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
        {#each traffic.interfaces.filter(i => ["enp1s0", "enp2s0", "wlo1", "tailscale0"].includes(i.name)) as iface}
          <div class="bg-gray-700/50 rounded p-4">
            <div class="flex items-center justify-between mb-2">
              <span class="font-medium">{getInterfaceLabel(iface.name)}</span>
              <span class="text-xs text-gray-500">{iface.name}</span>
            </div>
            <div class="grid grid-cols-2 gap-2 text-sm">
              <div>
                <p class="text-gray-400 text-xs">Download</p>
                <p class="text-green-400 font-medium">{formatBytes(iface.total_rx)}</p>
              </div>
              <div>
                <p class="text-gray-400 text-xs">Upload</p>
                <p class="text-blue-400 font-medium">{formatBytes(iface.total_tx)}</p>
              </div>
            </div>
            {#if iface.daily?.length > 0}
              <div class="mt-2 pt-2 border-t border-gray-600">
                <p class="text-gray-400 text-xs mb-1">Last 24h</p>
                <div class="flex h-8 gap-px">
                  {#each iface.hourly.slice(-24) as hour, i}
                    {@const maxVal = Math.max(...iface.hourly.slice(-24).map(h => h.rx + h.tx), 1)}
                    {@const height = ((hour.rx + hour.tx) / maxVal) * 100}
                    <div class="flex-1 bg-gray-600 rounded-t relative" title="{hour.timestamp}">
                      <div
                        class="absolute bottom-0 left-0 right-0 bg-gradient-to-t from-blue-500 to-green-500 rounded-t"
                        style="height: {height}%"
                      ></div>
                    </div>
                  {/each}
                </div>
              </div>
            {/if}
          </div>
        {/each}
      </div>
      <p class="text-xs text-gray-500 mt-3">Traffic data collected by vnstat. Statistics accumulate over time.</p>
    </div>
    {/if}

    <!-- AdGuard Stats -->
    {#if adguard}
    <div class="card">
      <div class="flex items-center justify-between mb-3">
        <h3 class="text-lg font-semibold">AdGuard Home</h3>
        <span class={adguard.protection_enabled ? "status-active" : "status-inactive"}>
          {adguard.protection_enabled ? "Protected" : "Unprotected"}
        </span>
      </div>
      <div class="grid grid-cols-2 md:grid-cols-4 gap-4">
        <div class="bg-gray-700/50 rounded p-3 text-center">
          <p class="text-2xl font-bold">{adguard.dns_queries.toLocaleString()}</p>
          <p class="text-xs text-gray-400">DNS Queries</p>
        </div>
        <div class="bg-gray-700/50 rounded p-3 text-center">
          <p class="text-2xl font-bold text-red-400">{adguard.blocked_filtering.toLocaleString()}</p>
          <p class="text-xs text-gray-400">Blocked</p>
        </div>
        <div class="bg-gray-700/50 rounded p-3 text-center">
          <p class="text-2xl font-bold">{adguard.blocked_percentage.toFixed(1)}%</p>
          <p class="text-xs text-gray-400">Block Rate</p>
        </div>
        <div class="bg-gray-700/50 rounded p-3 text-center">
          <p class="text-2xl font-bold">{adguard.avg_processing_time.toFixed(1)}ms</p>
          <p class="text-xs text-gray-400">Avg Response</p>
        </div>
      </div>
    </div>
    {/if}

    <!-- WAN Status -->
    <div class="card">
      <h3 class="text-lg font-semibold mb-3">WAN Status</h3>
      <div class="flex flex-wrap items-center gap-4">
        <span class={dashboard.wan_status.connected ? "status-active" : "status-inactive"}>
          {dashboard.wan_status.connected ? "Connected" : "Disconnected"}
        </span>
        <span class="text-gray-400">|</span>
        <span>Interface: <span class="text-blue-400">{dashboard.wan_status.interface}</span></span>
        <span class="text-gray-400">|</span>
        <span>IP: <span class="text-blue-400">{dashboard.wan_status.ip_address}</span></span>
        <span class="text-gray-400">|</span>
        <span>Gateway: <span class="text-blue-400">{dashboard.wan_status.gateway}</span></span>
      </div>
    </div>

    <!-- Network Interfaces -->
    <div class="card">
      <h3 class="text-lg font-semibold mb-3">Network Interfaces</h3>
      <div class="overflow-x-auto">
        <table class="w-full text-sm">
          <thead>
            <tr class="text-left text-gray-400 border-b border-gray-700">
              <th class="pb-2">Interface</th>
              <th class="pb-2">State</th>
              <th class="pb-2">IPv4</th>
              <th class="pb-2">MAC</th>
              <th class="pb-2">RX Total</th>
              <th class="pb-2">TX Total</th>
            </tr>
          </thead>
          <tbody>
            {#each dashboard.interfaces as iface}
              <tr class="border-b border-gray-700/50">
                <td class="py-2 font-medium">{iface.name}</td>
                <td class={iface.state === "UP" || iface.state === "Active" ? "status-active" : "status-inactive"}>{iface.state}</td>
                <td>{iface.ipv4 || "-"}</td>
                <td class="text-gray-400">{iface.mac_address}</td>
                <td>{formatBytes(iface.rx_bytes)}</td>
                <td>{formatBytes(iface.tx_bytes)}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    </div>

    <!-- Services -->
    <div class="card">
      <h3 class="text-lg font-semibold mb-3">Services</h3>
      <div class="grid grid-cols-2 md:grid-cols-4 gap-3">
        {#each dashboard.services as service}
          <div class="flex items-center gap-2 p-2 bg-gray-700/50 rounded">
            <span class={service.status === "active" ? "status-active" : "status-inactive"}>*</span>
            <span class="text-sm">{service.display_name}</span>
          </div>
        {/each}
      </div>
    </div>
  {/if}
</div>

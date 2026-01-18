<script>
  import { onMount } from "svelte";

  // State
  let loading = $state(true);
  let activeTab = $state("interfaces");
  let interfaces = $state([]);
  let dhcp = $state({ config: {}, leases: [], static_leases: [] });
  let wifi = $state({});
  let dns = $state({ upstream_servers: [], local_entries: [] });
  let routes = $state([]);
  let wolDevices = $state([]);

  // Form states
  let newStaticLease = $state({ mac_address: "", ip_address: "", hostname: "" });
  let newLocalDns = $state({ hostname: "", ip_address: "" });
  let newRoute = $state({ destination: "", gateway: "", interface: "" });
  let newWolDevice = $state({ name: "", mac_address: "", ip_address: "" });
  let wifiEdit = $state({ ssid: "", password: "", channel: 0, hidden: false });
  let dhcpEdit = $state({ range_start: "", range_end: "", lease_time: "" });
  let showWifiPassword = $state(false);

  // Diagnostics state
  let pingHost = $state("");
  let pingResult = $state(null);
  let pingRunning = $state(false);
  let tracerouteHost = $state("");
  let tracerouteResult = $state(null);
  let tracerouteRunning = $state(false);
  let dnsHostname = $state("");
  let dnsRecordType = $state("A");
  let dnsResult = $state(null);
  let dnsRunning = $state(false);
  let speedTestResult = $state(null);
  let speedTestRunning = $state(false);

  async function fetchData() {
    try {
      const [ifRes, dhcpRes, wifiRes, dnsRes, routesRes, wolRes] = await Promise.all([
        fetch("/api/network/interfaces"),
        fetch("/api/network/dhcp"),
        fetch("/api/network/wifi"),
        fetch("/api/network/dns"),
        fetch("/api/network/routes"),
        fetch("/api/network/wol")
      ]);

      if (ifRes.ok) interfaces = await ifRes.json();
      if (dhcpRes.ok) {
        dhcp = await dhcpRes.json();
        dhcpEdit = {
          range_start: dhcp.config.range_start,
          range_end: dhcp.config.range_end,
          lease_time: dhcp.config.lease_time
        };
      }
      if (wifiRes.ok) {
        wifi = await wifiRes.json();
        wifiEdit = {
          ssid: wifi.ssid,
          password: wifi.password,
          channel: wifi.channel,
          hidden: wifi.hidden
        };
      }
      if (dnsRes.ok) dns = await dnsRes.json();
      if (routesRes.ok) routes = await routesRes.json();
      if (wolRes.ok) wolDevices = await wolRes.json();
    } catch (e) {
      console.error(e);
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    fetchData();
    const interval = setInterval(fetchData, 15000);
    return () => clearInterval(interval);
  });

  // DHCP functions
  async function addStaticLease() {
    if (!newStaticLease.mac_address || !newStaticLease.ip_address) return;
    const res = await fetch("/api/network/dhcp/static/add", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(newStaticLease)
    });
    if (res.ok) {
      newStaticLease = { mac_address: "", ip_address: "", hostname: "" };
      await fetchData();
    }
  }

  async function removeStaticLease(mac) {
    const res = await fetch("/api/network/dhcp/static/remove", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ mac_address: mac })
    });
    if (res.ok) await fetchData();
  }

  async function updateDhcpConfig() {
    const res = await fetch("/api/network/dhcp/config", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(dhcpEdit)
    });
    if (res.ok) await fetchData();
  }

  // WiFi functions
  async function updateWifi() {
    const res = await fetch("/api/network/wifi/update", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(wifiEdit)
    });
    if (res.ok) await fetchData();
  }

  async function toggleWifi() {
    const res = await fetch("/api/network/wifi/toggle", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ enabled: !wifi.enabled })
    });
    if (res.ok) await fetchData();
  }

  // DNS functions
  async function addLocalDns() {
    if (!newLocalDns.hostname || !newLocalDns.ip_address) return;
    const res = await fetch("/api/network/dns/local/add", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(newLocalDns)
    });
    if (res.ok) {
      newLocalDns = { hostname: "", ip_address: "" };
      await fetchData();
    }
  }

  async function removeLocalDns(hostname) {
    const res = await fetch("/api/network/dns/local/remove", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ hostname })
    });
    if (res.ok) await fetchData();
  }

  // Route functions
  async function addRoute() {
    if (!newRoute.destination || !newRoute.gateway) return;
    const res = await fetch("/api/network/routes/add", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(newRoute)
    });
    if (res.ok) {
      newRoute = { destination: "", gateway: "", interface: "" };
      await fetchData();
    }
  }

  async function removeRoute(destination) {
    const res = await fetch("/api/network/routes/remove", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ destination })
    });
    if (res.ok) await fetchData();
  }

  // WoL functions
  async function addWolDevice() {
    if (!newWolDevice.name || !newWolDevice.mac_address) return;
    const res = await fetch("/api/network/wol/add", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(newWolDevice)
    });
    if (res.ok) {
      newWolDevice = { name: "", mac_address: "", ip_address: "" };
      await fetchData();
    }
  }

  async function removeWolDevice(mac) {
    const res = await fetch("/api/network/wol/remove", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ mac_address: mac })
    });
    if (res.ok) await fetchData();
  }

  async function wakeDevice(mac) {
    const res = await fetch("/api/network/wol/wake", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ mac_address: mac })
    });
    if (res.ok) {
      alert("Wake packet sent!");
    }
  }

  // Helpers
  function formatBytes(bytes) {
    if (bytes === 0) return "0 B";
    const k = 1024;
    const sizes = ["B", "KB", "MB", "GB", "TB"];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + " " + sizes[i];
  }

  function getInterfaceIcon(type) {
    switch (type) {
      case "wan": return "🌐";
      case "lan": return "🔌";
      case "wifi": return "📶";
      case "loopback": return "🔄";
      default: return "🔗";
    }
  }

  // Diagnostics functions
  async function runPing() {
    if (!pingHost) return;
    pingRunning = true;
    pingResult = null;
    try {
      const res = await fetch("/api/tools/ping", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ host: pingHost, count: 4 })
      });
      if (res.ok) pingResult = await res.json();
    } finally {
      pingRunning = false;
    }
  }

  async function runTraceroute() {
    if (!tracerouteHost) return;
    tracerouteRunning = true;
    tracerouteResult = null;
    try {
      const res = await fetch("/api/tools/traceroute", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ host: tracerouteHost })
      });
      if (res.ok) tracerouteResult = await res.json();
    } finally {
      tracerouteRunning = false;
    }
  }

  async function runDnsLookup() {
    if (!dnsHostname) return;
    dnsRunning = true;
    dnsResult = null;
    try {
      const res = await fetch("/api/tools/dns-lookup", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ hostname: dnsHostname, record_type: dnsRecordType })
      });
      if (res.ok) dnsResult = await res.json();
    } finally {
      dnsRunning = false;
    }
  }

  async function runSpeedTest() {
    speedTestRunning = true;
    speedTestResult = null;
    try {
      const res = await fetch("/api/tools/speed-test", { method: "POST" });
      if (res.ok) speedTestResult = await res.json();
    } finally {
      speedTestRunning = false;
    }
  }
</script>

<svelte:head>
  <title>Network - RouterUI</title>
</svelte:head>

<div class="space-y-6">
  <div class="flex items-center justify-between">
    <div>
      <h2 class="text-2xl font-bold">Network</h2>
      <p class="text-sm text-gray-500">Configure network interfaces, DHCP, WiFi, and more.</p>
    </div>
  </div>

  {#if loading}
    <div class="text-gray-400">Loading...</div>
  {:else}
    <!-- Tabs -->
    <div class="border-b border-gray-700">
      <nav class="flex gap-4 overflow-x-auto">
        {#each [
          { id: "interfaces", label: "Interfaces" },
          { id: "dhcp", label: "DHCP" },
          { id: "wifi", label: "WiFi" },
          { id: "dns", label: "DNS" },
          { id: "routes", label: "Routes" },
          { id: "wol", label: "Wake-on-LAN" },
          { id: "diagnostics", label: "Diagnostics" }
        ] as tab}
          <button
            onclick={() => activeTab = tab.id}
            class="tab-btn {activeTab === tab.id ? 'tab-active' : ''}"
          >
            {tab.label}
          </button>
        {/each}
      </nav>
    </div>

    <!-- Interfaces Tab -->
    {#if activeTab === "interfaces"}
      <div class="card">
        <h3 class="text-lg font-semibold mb-4">Network Interfaces</h3>
        <div class="space-y-3">
          {#each interfaces as iface}
            <div class="p-4 bg-gray-700/50 rounded">
              <div class="flex items-center justify-between mb-2">
                <div class="flex items-center gap-2">
                  <span class="text-xl">{getInterfaceIcon(iface.interface_type)}</span>
                  <span class="font-medium">{iface.name}</span>
                  <span class="text-xs px-2 py-0.5 bg-blue-500/20 text-blue-400 rounded uppercase">
                    {iface.interface_type}
                  </span>
                </div>
                <span class={iface.state === "UP" ? "text-green-400" : "text-gray-500"}>
                  {iface.state}
                </span>
              </div>
              <div class="grid grid-cols-2 md:grid-cols-4 gap-4 text-sm">
                <div>
                  <span class="text-gray-400">IPv4:</span>
                  <span class="ml-1">{iface.ipv4 || "-"}</span>
                </div>
                <div>
                  <span class="text-gray-400">MAC:</span>
                  <span class="ml-1 font-mono text-xs">{iface.mac_address}</span>
                </div>
                <div>
                  <span class="text-gray-400">RX:</span>
                  <span class="ml-1">{formatBytes(iface.rx_bytes)}</span>
                </div>
                <div>
                  <span class="text-gray-400">TX:</span>
                  <span class="ml-1">{formatBytes(iface.tx_bytes)}</span>
                </div>
              </div>
            </div>
          {/each}
        </div>
      </div>

    <!-- DHCP Tab -->
    {:else if activeTab === "dhcp"}
      <div class="space-y-4">
        <!-- DHCP Config -->
        <div class="card">
          <h3 class="text-lg font-semibold mb-4">DHCP Configuration</h3>
          <div class="grid grid-cols-1 md:grid-cols-3 gap-4 mb-4">
            <div>
              <label class="block text-sm text-gray-400 mb-1">Range Start</label>
              <input type="text" bind:value={dhcpEdit.range_start} class="input w-full" />
            </div>
            <div>
              <label class="block text-sm text-gray-400 mb-1">Range End</label>
              <input type="text" bind:value={dhcpEdit.range_end} class="input w-full" />
            </div>
            <div>
              <label class="block text-sm text-gray-400 mb-1">Lease Time</label>
              <input type="text" bind:value={dhcpEdit.lease_time} class="input w-full" />
            </div>
          </div>
          <button onclick={updateDhcpConfig} class="btn-primary">Save DHCP Settings</button>
        </div>

        <!-- Active Leases -->
        <div class="card">
          <h3 class="text-lg font-semibold mb-4">Active Leases ({dhcp.leases.length})</h3>
          {#if dhcp.leases.length === 0}
            <p class="text-gray-500">No active DHCP leases.</p>
          {:else}
            <div class="overflow-x-auto">
              <table class="w-full text-sm">
                <thead>
                  <tr class="text-left text-gray-400 border-b border-gray-700">
                    <th class="pb-2">Hostname</th>
                    <th class="pb-2">IP Address</th>
                    <th class="pb-2">MAC Address</th>
                    <th class="pb-2">Expires</th>
                    <th class="pb-2">Type</th>
                  </tr>
                </thead>
                <tbody>
                  {#each dhcp.leases as lease}
                    <tr class="border-b border-gray-700/50">
                      <td class="py-2">{lease.hostname || "-"}</td>
                      <td class="py-2 font-mono">{lease.ip_address}</td>
                      <td class="py-2 font-mono text-xs">{lease.mac_address}</td>
                      <td class="py-2 text-gray-400">{lease.expires}</td>
                      <td class="py-2">
                        {#if lease.is_static}
                          <span class="text-xs px-2 py-0.5 bg-purple-500/20 text-purple-400 rounded">Static</span>
                        {:else}
                          <span class="text-xs px-2 py-0.5 bg-gray-500/20 text-gray-400 rounded">Dynamic</span>
                        {/if}
                      </td>
                    </tr>
                  {/each}
                </tbody>
              </table>
            </div>
          {/if}
        </div>

        <!-- Static Leases -->
        <div class="card">
          <h3 class="text-lg font-semibold mb-4">Static Leases (Reservations)</h3>
          <p class="text-sm text-gray-400 mb-4">Reserve specific IP addresses for devices based on their MAC address.</p>

          <div class="flex gap-2 mb-4">
            <input type="text" placeholder="MAC Address" bind:value={newStaticLease.mac_address} class="input flex-1" />
            <input type="text" placeholder="IP Address" bind:value={newStaticLease.ip_address} class="input flex-1" />
            <input type="text" placeholder="Hostname (optional)" bind:value={newStaticLease.hostname} class="input flex-1" />
            <button onclick={addStaticLease} class="btn-primary">Add</button>
          </div>

          {#if dhcp.static_leases.length > 0}
            <div class="space-y-2">
              {#each dhcp.static_leases as lease}
                <div class="flex items-center justify-between p-3 bg-gray-700/50 rounded">
                  <div>
                    <span class="font-mono">{lease.mac_address}</span>
                    <span class="text-gray-400 mx-2">→</span>
                    <span class="font-mono text-blue-400">{lease.ip_address}</span>
                    {#if lease.hostname}
                      <span class="text-gray-400 ml-2">({lease.hostname})</span>
                    {/if}
                  </div>
                  <button onclick={() => removeStaticLease(lease.mac_address)} class="text-red-400 hover:text-red-300 text-sm">
                    Remove
                  </button>
                </div>
              {/each}
            </div>
          {/if}
        </div>
      </div>

    <!-- WiFi Tab -->
    {:else if activeTab === "wifi"}
      <div class="card">
        <div class="flex items-center justify-between mb-4">
          <h3 class="text-lg font-semibold">WiFi Access Point</h3>
          <label class="toggle">
            <input type="checkbox" checked={wifi.enabled} onchange={toggleWifi} />
            <span class="toggle-slider"></span>
          </label>
        </div>

        <div class="grid grid-cols-1 md:grid-cols-2 gap-4 mb-4">
          <div>
            <label class="block text-sm text-gray-400 mb-1">Network Name (SSID)</label>
            <input type="text" bind:value={wifiEdit.ssid} class="input w-full" />
          </div>
          <div>
            <label class="block text-sm text-gray-400 mb-1">Password</label>
            <div class="relative">
              <input
                type={showWifiPassword ? "text" : "password"}
                bind:value={wifiEdit.password}
                class="input w-full pr-10"
              />
              <button
                onclick={() => showWifiPassword = !showWifiPassword}
                class="absolute right-2 top-1/2 -translate-y-1/2 text-gray-400 hover:text-white"
              >
                {showWifiPassword ? "🙈" : "👁"}
              </button>
            </div>
          </div>
          <div>
            <label class="block text-sm text-gray-400 mb-1">Channel</label>
            <select bind:value={wifiEdit.channel} class="input w-full">
              {#each [1,2,3,4,5,6,7,8,9,10,11] as ch}
                <option value={ch}>{ch}</option>
              {/each}
            </select>
          </div>
          <div class="flex items-center gap-2 pt-6">
            <label class="toggle">
              <input type="checkbox" bind:checked={wifiEdit.hidden} />
              <span class="toggle-slider"></span>
            </label>
            <span>Hide Network (Hidden SSID)</span>
          </div>
        </div>

        <div class="flex items-center justify-between pt-4 border-t border-gray-700">
          <div class="text-sm text-gray-400">
            Security: {wifi.security} | Country: {wifi.country_code} | Mode: {wifi.hw_mode}
          </div>
          <button onclick={updateWifi} class="btn-primary">Save WiFi Settings</button>
        </div>
      </div>

    <!-- DNS Tab -->
    {:else if activeTab === "dns"}
      <div class="space-y-4">
        <div class="card">
          <h3 class="text-lg font-semibold mb-4">Upstream DNS Servers</h3>
          <div class="space-y-2">
            {#each dns.upstream_servers as server}
              <div class="p-2 bg-gray-700/50 rounded font-mono">{server}</div>
            {/each}
          </div>
          <p class="text-sm text-gray-500 mt-2">DNS is forwarded to AdGuard Home for filtering.</p>
        </div>

        <div class="card">
          <h3 class="text-lg font-semibold mb-4">Local DNS Entries</h3>
          <p class="text-sm text-gray-400 mb-4">Map hostnames to IP addresses on your local network.</p>

          <div class="flex gap-2 mb-4">
            <input type="text" placeholder="Hostname (e.g., nas.lan)" bind:value={newLocalDns.hostname} class="input flex-1" />
            <input type="text" placeholder="IP Address" bind:value={newLocalDns.ip_address} class="input flex-1" />
            <button onclick={addLocalDns} class="btn-primary">Add</button>
          </div>

          {#if dns.local_entries.length > 0}
            <div class="space-y-2">
              {#each dns.local_entries as entry}
                <div class="flex items-center justify-between p-3 bg-gray-700/50 rounded">
                  <div>
                    <span class="font-mono">{entry.hostname}</span>
                    <span class="text-gray-400 mx-2">→</span>
                    <span class="font-mono text-blue-400">{entry.ip_address}</span>
                  </div>
                  <button onclick={() => removeLocalDns(entry.hostname)} class="text-red-400 hover:text-red-300 text-sm">
                    Remove
                  </button>
                </div>
              {/each}
            </div>
          {:else}
            <p class="text-gray-500">No local DNS entries configured.</p>
          {/if}
        </div>
      </div>

    <!-- Routes Tab -->
    {:else if activeTab === "routes"}
      <div class="card">
        <h3 class="text-lg font-semibold mb-4">Static Routes</h3>
        <p class="text-sm text-gray-400 mb-4">Add custom routes for specific networks.</p>

        <div class="flex gap-2 mb-4">
          <input type="text" placeholder="Destination (e.g., 192.168.10.0/24)" bind:value={newRoute.destination} class="input flex-1" />
          <input type="text" placeholder="Gateway" bind:value={newRoute.gateway} class="input flex-1" />
          <input type="text" placeholder="Interface (optional)" bind:value={newRoute.interface} class="input w-32" />
          <button onclick={addRoute} class="btn-primary">Add</button>
        </div>

        <div class="overflow-x-auto">
          <table class="w-full text-sm">
            <thead>
              <tr class="text-left text-gray-400 border-b border-gray-700">
                <th class="pb-2">Destination</th>
                <th class="pb-2">Gateway</th>
                <th class="pb-2">Interface</th>
                <th class="pb-2">Metric</th>
                <th class="pb-2"></th>
              </tr>
            </thead>
            <tbody>
              {#each routes as route}
                <tr class="border-b border-gray-700/50">
                  <td class="py-2 font-mono">{route.destination}</td>
                  <td class="py-2 font-mono">{route.gateway || "-"}</td>
                  <td class="py-2">{route.interface || "-"}</td>
                  <td class="py-2 text-gray-400">{route.metric || "-"}</td>
                  <td class="py-2">
                    {#if route.destination !== "default" && !route.destination.startsWith("10.22.22")}
                      <button onclick={() => removeRoute(route.destination)} class="text-red-400 hover:text-red-300 text-sm">
                        Remove
                      </button>
                    {/if}
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      </div>

    <!-- Wake-on-LAN Tab -->
    {:else if activeTab === "wol"}
      <div class="card">
        <h3 class="text-lg font-semibold mb-4">Wake-on-LAN</h3>
        <p class="text-sm text-gray-400 mb-4">Send magic packets to wake up devices on your network.</p>

        <div class="flex gap-2 mb-4">
          <input type="text" placeholder="Device Name" bind:value={newWolDevice.name} class="input flex-1" />
          <input type="text" placeholder="MAC Address" bind:value={newWolDevice.mac_address} class="input flex-1" />
          <input type="text" placeholder="IP (optional)" bind:value={newWolDevice.ip_address} class="input w-32" />
          <button onclick={addWolDevice} class="btn-primary">Add</button>
        </div>

        {#if wolDevices.length > 0}
          <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-3">
            {#each wolDevices as device}
              <div class="p-4 bg-gray-700/50 rounded">
                <div class="flex items-center justify-between mb-2">
                  <span class="font-medium">{device.name}</span>
                  <button onclick={() => removeWolDevice(device.mac_address)} class="text-red-400 hover:text-red-300 text-xs">
                    Remove
                  </button>
                </div>
                <div class="text-sm text-gray-400 font-mono mb-3">{device.mac_address}</div>
                <button
                  onclick={() => wakeDevice(device.mac_address)}
                  class="w-full btn-wake"
                >
                  ⚡ Wake Up
                </button>
              </div>
            {/each}
          </div>
        {:else}
          <div class="text-center py-8 text-gray-500">
            <p>No Wake-on-LAN devices configured.</p>
            <p class="text-sm">Add a device above to get started.</p>
          </div>
        {/if}
      </div>

    <!-- Diagnostics Tab -->
    {:else if activeTab === "diagnostics"}
      <div class="space-y-4">
        <!-- Ping -->
        <div class="card">
          <h3 class="text-lg font-semibold mb-3">Ping</h3>
          <div class="flex gap-2 mb-3">
            <input
              type="text"
              placeholder="Host or IP (e.g., 8.8.8.8, google.com)"
              bind:value={pingHost}
              onkeypress={(e) => e.key === 'Enter' && runPing()}
              class="input flex-1"
            />
            <button onclick={runPing} disabled={pingRunning} class="btn-primary">
              {pingRunning ? "Pinging..." : "Ping"}
            </button>
          </div>
          {#if pingResult}
            <div class="bg-gray-700/50 rounded p-3">
              <div class="grid grid-cols-4 gap-4 mb-3 text-center">
                <div>
                  <p class="text-2xl font-bold {pingResult.success ? 'text-green-400' : 'text-red-400'}">
                    {pingResult.packets_received}/{pingResult.packets_sent}
                  </p>
                  <p class="text-xs text-gray-400">Packets</p>
                </div>
                <div>
                  <p class="text-2xl font-bold">{pingResult.packet_loss.toFixed(0)}%</p>
                  <p class="text-xs text-gray-400">Loss</p>
                </div>
                <div>
                  <p class="text-2xl font-bold text-blue-400">
                    {pingResult.avg_latency ? pingResult.avg_latency.toFixed(1) : '-'}
                  </p>
                  <p class="text-xs text-gray-400">Avg ms</p>
                </div>
                <div>
                  <p class="text-2xl font-bold {pingResult.success ? 'text-green-400' : 'text-red-400'}">
                    {pingResult.success ? "OK" : "FAIL"}
                  </p>
                  <p class="text-xs text-gray-400">Status</p>
                </div>
              </div>
              <pre class="text-xs text-gray-400 overflow-x-auto whitespace-pre-wrap max-h-32 overflow-y-auto">{pingResult.output}</pre>
            </div>
          {/if}
        </div>

        <!-- Traceroute -->
        <div class="card">
          <h3 class="text-lg font-semibold mb-3">Traceroute</h3>
          <div class="flex gap-2 mb-3">
            <input
              type="text"
              placeholder="Host or IP"
              bind:value={tracerouteHost}
              onkeypress={(e) => e.key === 'Enter' && runTraceroute()}
              class="input flex-1"
            />
            <button onclick={runTraceroute} disabled={tracerouteRunning} class="btn-primary">
              {tracerouteRunning ? "Tracing..." : "Traceroute"}
            </button>
          </div>
          {#if tracerouteRunning}
            <p class="text-gray-400">Running traceroute (this may take a minute)...</p>
          {/if}
          {#if tracerouteResult}
            <div class="bg-gray-700/50 rounded p-3">
              <div class="space-y-1 max-h-64 overflow-y-auto">
                {#each tracerouteResult.hops as hop}
                  <div class="flex items-center gap-2 text-sm font-mono">
                    <span class="w-6 text-gray-500">{hop.hop}</span>
                    <span class="{hop.host === '*' ? 'text-gray-500' : 'text-white'}">{hop.host}</span>
                    {#if hop.ip}
                      <span class="text-gray-400">({hop.ip})</span>
                    {/if}
                    {#if hop.latency}
                      <span class="text-blue-400">{hop.latency}</span>
                    {/if}
                  </div>
                {/each}
              </div>
            </div>
          {/if}
        </div>

        <!-- DNS Lookup -->
        <div class="card">
          <h3 class="text-lg font-semibold mb-3">DNS Lookup</h3>
          <div class="flex gap-2 mb-3">
            <input
              type="text"
              placeholder="Hostname (e.g., google.com)"
              bind:value={dnsHostname}
              onkeypress={(e) => e.key === 'Enter' && runDnsLookup()}
              class="input flex-1"
            />
            <select bind:value={dnsRecordType} class="input">
              <option value="A">A</option>
              <option value="AAAA">AAAA</option>
              <option value="MX">MX</option>
              <option value="NS">NS</option>
              <option value="TXT">TXT</option>
              <option value="CNAME">CNAME</option>
            </select>
            <button onclick={runDnsLookup} disabled={dnsRunning} class="btn-primary">
              {dnsRunning ? "Looking up..." : "Lookup"}
            </button>
          </div>
          {#if dnsResult}
            <div class="bg-gray-700/50 rounded p-3">
              <p class="text-sm text-gray-400 mb-2">{dnsResult.hostname} ({dnsResult.record_type})</p>
              {#if dnsResult.results.length > 0}
                <div class="space-y-1">
                  {#each dnsResult.results as result}
                    <p class="font-mono text-green-400">{result}</p>
                  {/each}
                </div>
              {:else}
                <p class="text-gray-500">No records found</p>
              {/if}
            </div>
          {/if}
        </div>

        <!-- Speed Test -->
        <div class="card">
          <h3 class="text-lg font-semibold mb-3">Speed Test</h3>
          <button onclick={runSpeedTest} disabled={speedTestRunning} class="btn-primary mb-3">
            {speedTestRunning ? "Testing... (this takes about 30 seconds)" : "Run Speed Test"}
          </button>
          {#if speedTestResult}
            <div class="grid grid-cols-3 gap-4">
              <div class="bg-gray-700/50 rounded p-4 text-center">
                <p class="text-3xl font-bold text-green-400">
                  {speedTestResult.download_mbps ? speedTestResult.download_mbps.toFixed(1) : '-'}
                </p>
                <p class="text-sm text-gray-400">Download Mbps</p>
              </div>
              <div class="bg-gray-700/50 rounded p-4 text-center">
                <p class="text-3xl font-bold text-blue-400">
                  {speedTestResult.upload_mbps ? speedTestResult.upload_mbps.toFixed(1) : '-'}
                </p>
                <p class="text-sm text-gray-400">Upload Mbps</p>
              </div>
              <div class="bg-gray-700/50 rounded p-4 text-center">
                <p class="text-3xl font-bold text-yellow-400">
                  {speedTestResult.ping_ms ? speedTestResult.ping_ms.toFixed(0) : '-'}
                </p>
                <p class="text-sm text-gray-400">Ping ms</p>
              </div>
            </div>
          {/if}
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
    white-space: nowrap;
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

  .btn-wake {
    background: #059669;
    color: white;
    padding: 0.5rem 1rem;
    border-radius: 0.375rem;
    font-weight: 500;
    transition: all 0.2s;
  }

  .btn-wake:hover {
    background: #047857;
    transform: scale(1.02);
  }
</style>

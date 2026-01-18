<script>
  import { onMount } from "svelte";

  let loading = $state(true);
  let activeTab = $state("remote");

  // Tailscale state
  let tailscale = $state(null);
  let devices = $state([]);
  let netcheck = $state(null);
  let showNetcheck = $state(false);
  let connecting = $state(false);
  let loginUrl = $state("");

  // Gluetun state
  let gluetun = $state(null);

  // Connect form
  let advertiseRoutes = $state("10.22.22.0/24");
  let advertiseExitNode = $state(true);
  let hostname = $state("router");

  async function fetchData() {
    try {
      const res = await fetch("/api/vpn/overview");
      if (res.ok) {
        const data = await res.json();
        tailscale = data.tailscale;
        gluetun = data.gluetun;

        // If logged in, fetch devices
        if (tailscale?.logged_in) {
          const devRes = await fetch("/api/vpn/tailscale/devices");
          if (devRes.ok) {
            devices = await devRes.json();
          }
        }
      }
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

  async function connectTailscale() {
    connecting = true;
    loginUrl = "";
    try {
      const res = await fetch("/api/vpn/tailscale/connect", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          advertise_routes: advertiseRoutes || null,
          advertise_exit_node: advertiseExitNode,
          hostname: hostname || null,
          accept_routes: true
        })
      });
      if (res.ok) {
        const data = await res.json();
        if (data.url) {
          loginUrl = data.url;
        }
        // Poll for connection status
        setTimeout(fetchData, 3000);
      }
    } finally {
      connecting = false;
    }
  }

  async function disconnectTailscale() {
    await fetch("/api/vpn/tailscale/disconnect", { method: "POST" });
    await fetchData();
  }

  async function logoutTailscale() {
    if (confirm("This will remove this device from the Tailscale network. Continue?")) {
      await fetch("/api/vpn/tailscale/logout", { method: "POST" });
      devices = [];
      await fetchData();
    }
  }

  async function toggleExitNode() {
    const newState = !tailscale.exit_node_advertised;
    await fetch("/api/vpn/tailscale/exit-node", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ enable: newState })
    });
    await fetchData();
  }

  async function runNetcheck() {
    showNetcheck = true;
    netcheck = null;
    try {
      const res = await fetch("/api/vpn/tailscale/netcheck");
      if (res.ok) {
        netcheck = await res.json();
      }
    } catch (e) {
      console.error(e);
    }
  }

  async function restartGluetun() {
    await fetch("/api/vpn/gluetun/restart", { method: "POST" });
    await new Promise(r => setTimeout(r, 3000));
    await fetchData();
  }

  function formatBytes(bytes) {
    if (!bytes) return "0 B";
    const k = 1024;
    const sizes = ["B", "KB", "MB", "GB"];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + " " + sizes[i];
  }

  function getOsIcon(os) {
    if (!os) return "device";
    const lower = os.toLowerCase();
    if (lower.includes("android")) return "android";
    if (lower.includes("ios") || lower.includes("iphone")) return "iphone";
    if (lower.includes("windows")) return "windows";
    if (lower.includes("macos") || lower.includes("darwin")) return "apple";
    if (lower.includes("linux")) return "linux";
    return "device";
  }
</script>

<svelte:head>
  <title>VPN - RouterUI</title>
</svelte:head>

<div class="space-y-6">
  <div>
    <h2 class="text-2xl font-bold">VPN</h2>
    <p class="text-sm text-gray-500">Remote access and download VPN connections.</p>
  </div>

  <!-- Tabs -->
  <div class="flex gap-2 border-b border-gray-700 pb-2">
    <button
      onclick={() => activeTab = "remote"}
      class="px-4 py-2 rounded-t {activeTab === 'remote' ? 'bg-gray-700 text-white' : 'text-gray-400 hover:text-white'}"
    >
      Remote Access
    </button>
    <button
      onclick={() => activeTab = "download"}
      class="px-4 py-2 rounded-t {activeTab === 'download' ? 'bg-gray-700 text-white' : 'text-gray-400 hover:text-white'}"
    >
      Download VPN
    </button>
  </div>

  {#if loading}
    <div class="text-gray-400">Loading...</div>
  {:else if activeTab === "remote"}
    <!-- TAILSCALE SECTION -->
    <div class="space-y-4">
      <!-- Status Card -->
      <div class="card">
        <div class="flex items-center justify-between mb-4">
          <h3 class="text-lg font-semibold flex items-center gap-2">
            <svg class="w-5 h-5" fill="currentColor" viewBox="0 0 24 24">
              <path d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm-1 17.93c-3.95-.49-7-3.85-7-7.93 0-.62.08-1.21.21-1.79L9 15v1c0 1.1.9 2 2 2v1.93zm6.9-2.54c-.26-.81-1-1.39-1.9-1.39h-1v-3c0-.55-.45-1-1-1H8v-2h2c.55 0 1-.45 1-1V7h2c1.1 0 2-.9 2-2v-.41c2.93 1.19 5 4.06 5 7.41 0 2.08-.8 3.97-2.1 5.39z"/>
            </svg>
            Tailscale
            {#if tailscale?.logged_in}
              <span class="text-xs px-2 py-0.5 bg-green-500/20 text-green-400 rounded">Connected</span>
            {:else if tailscale?.running}
              <span class="text-xs px-2 py-0.5 bg-yellow-500/20 text-yellow-400 rounded">Not Logged In</span>
            {:else}
              <span class="text-xs px-2 py-0.5 bg-red-500/20 text-red-400 rounded">Not Running</span>
            {/if}
          </h3>
          {#if tailscale?.version}
            <span class="text-xs text-gray-500">v{tailscale.version}</span>
          {/if}
        </div>

        {#if !tailscale?.installed}
          <p class="text-gray-400">Tailscale is not installed.</p>
        {:else if !tailscale?.logged_in}
          <!-- Login Form -->
          <div class="space-y-4">
            <p class="text-gray-400">Connect this router to your Tailscale network for remote access.</p>

            <div class="grid grid-cols-2 gap-4">
              <div>
                <label class="block text-sm text-gray-400 mb-1">Hostname</label>
                <input
                  type="text"
                  bind:value={hostname}
                  placeholder="router"
                  class="w-full bg-gray-700 text-white rounded px-3 py-2 text-sm"
                />
              </div>
              <div>
                <label class="block text-sm text-gray-400 mb-1">Advertise Routes (LAN)</label>
                <input
                  type="text"
                  bind:value={advertiseRoutes}
                  placeholder="10.22.22.0/24"
                  class="w-full bg-gray-700 text-white rounded px-3 py-2 text-sm"
                />
              </div>
            </div>

            <label class="flex items-center gap-2">
              <input type="checkbox" bind:checked={advertiseExitNode} class="rounded" />
              <span class="text-sm">Advertise as exit node (route all phone traffic through home)</span>
            </label>

            <button
              onclick={connectTailscale}
              disabled={connecting}
              class="btn-primary"
            >
              {connecting ? "Connecting..." : "Connect to Tailscale"}
            </button>

            {#if loginUrl}
              <div class="mt-4 p-4 bg-blue-500/10 border border-blue-500/30 rounded">
                <p class="text-sm text-blue-400 mb-2">Open this URL to authenticate:</p>
                <a href={loginUrl} target="_blank" class="text-blue-400 underline break-all text-sm">
                  {loginUrl}
                </a>
                <p class="text-xs text-gray-500 mt-2">After logging in, this page will update automatically.</p>
              </div>
            {/if}
          </div>
        {:else}
          <!-- Connected Status -->
          <div class="grid grid-cols-2 md:grid-cols-4 gap-4 mb-4">
            <div class="bg-gray-700/50 rounded p-3">
              <p class="text-xs text-gray-400">Tailscale IP</p>
              <p class="font-mono text-green-400">{tailscale.tailscale_ip || "N/A"}</p>
            </div>
            <div class="bg-gray-700/50 rounded p-3">
              <p class="text-xs text-gray-400">Hostname</p>
              <p class="font-medium">{tailscale.hostname || "N/A"}</p>
            </div>
            <div class="bg-gray-700/50 rounded p-3">
              <p class="text-xs text-gray-400">DNS Name</p>
              <p class="font-mono text-xs truncate">{tailscale.dns_name || "N/A"}</p>
            </div>
            <div class="bg-gray-700/50 rounded p-3">
              <p class="text-xs text-gray-400">Exit Node</p>
              <p class="{tailscale.exit_node_advertised ? 'text-green-400' : 'text-gray-400'}">
                {tailscale.exit_node_advertised ? "Enabled" : "Disabled"}
              </p>
            </div>
          </div>

          {#if tailscale.advertised_routes?.length > 0}
            <div class="mb-4">
              <p class="text-xs text-gray-400 mb-1">Advertised Routes</p>
              <div class="flex gap-2">
                {#each tailscale.advertised_routes as route}
                  <span class="text-xs px-2 py-1 bg-gray-700 rounded font-mono">{route}</span>
                {/each}
              </div>
            </div>
          {/if}

          <div class="flex gap-2">
            <button onclick={toggleExitNode} class="btn-secondary">
              {tailscale.exit_node_advertised ? "Disable" : "Enable"} Exit Node
            </button>
            <button onclick={runNetcheck} class="btn-secondary">
              Network Diagnostics
            </button>
            <button onclick={disconnectTailscale} class="btn-secondary">
              Disconnect
            </button>
            <button onclick={logoutTailscale} class="btn-danger">
              Logout
            </button>
          </div>
        {/if}
      </div>

      <!-- Devices List -->
      {#if tailscale?.logged_in && devices.length > 0}
        <div class="card">
          <h3 class="text-lg font-semibold mb-4">Connected Devices</h3>
          <div class="space-y-2">
            {#each devices as device}
              <div class="flex items-center justify-between p-3 bg-gray-700/50 rounded">
                <div class="flex items-center gap-3">
                  <div class="w-8 h-8 rounded-full bg-gray-600 flex items-center justify-center text-xs">
                    {#if getOsIcon(device.os) === "android"}
                      <svg class="w-4 h-4" fill="currentColor" viewBox="0 0 24 24"><path d="M17.6 9.48l1.84-3.18c.16-.31.04-.69-.26-.85-.29-.15-.65-.06-.83.22l-1.88 3.24c-1.44-.66-3.05-1.03-4.77-1.03-1.72 0-3.33.37-4.77 1.03L5.05 5.67c-.18-.28-.54-.37-.83-.22-.31.16-.43.54-.26.85L5.8 9.48C2.65 11.25.84 14.28.84 17.66h22.32c0-3.38-1.81-6.41-4.96-8.18zM7.52 14c-.68 0-1.23-.54-1.23-1.21s.55-1.21 1.23-1.21c.68 0 1.23.54 1.23 1.21S8.2 14 7.52 14zm8.96 0c-.68 0-1.23-.54-1.23-1.21s.55-1.21 1.23-1.21c.68 0 1.23.54 1.23 1.21S17.16 14 16.48 14z"/></svg>
                    {:else if getOsIcon(device.os) === "linux"}
                      <svg class="w-4 h-4" fill="currentColor" viewBox="0 0 24 24"><path d="M12.504 0c-.155 0-.315.008-.48.021-4.226.333-3.105 4.807-3.17 6.298-.076 1.092-.3 1.953-1.05 3.02-.885 1.051-2.127 2.75-2.716 4.521-.278.832-.41 1.684-.287 2.489.117.779.456 1.511 1.038 2.1.49.51 1.064.875 1.647 1.16.547.27 1.095.48 1.565.66.415.159.752.291.97.407.293.156.515.292.64.456.114.154.18.335.18.553 0 .345-.105.615-.293.848-.188.23-.442.396-.738.515-.597.236-1.37.367-2.242.367-1.282 0-2.52-.214-3.592-.612-1.072-.397-1.97-.96-2.596-1.67-.313-.354-.556-.749-.693-1.17-.14-.423-.17-.872-.082-1.305.17-.893.758-1.79 1.58-2.528.823-.74 1.855-1.314 2.84-1.61.99-.297 1.92-.31 2.507-.068.595.245.933.68.895 1.175-.038.5-.44.92-1.02 1.2-.58.278-1.35.418-2.16.364-.45-.03-.855-.125-1.17-.27-.315-.145-.54-.338-.63-.56-.09-.22-.04-.475.133-.73.174-.256.48-.495.884-.65.404-.154.894-.222 1.38-.195.486.027.97.15 1.37.36.4.21.71.51.87.87.16.36.17.77.04 1.15-.13.38-.4.73-.78 1-.38.27-.86.45-1.38.53-.52.08-1.07.06-1.57-.06-.5-.12-.95-.34-1.31-.66-.36-.32-.62-.74-.75-1.22-.13-.48-.12-1.01.04-1.53.16-.52.46-1.01.88-1.43.42-.42.96-.77 1.58-1.02.62-.25 1.32-.39 2.06-.39.74 0 1.44.14 2.06.39.62.25 1.16.6 1.58 1.02.42.42.72.91.88 1.43.16.52.17 1.05.04 1.53-.13.48-.39.9-.75 1.22-.36.32-.81.54-1.31.66-.5.12-1.05.14-1.57.06-.52-.08-1-.26-1.38-.53-.38-.27-.65-.62-.78-1-.13-.38-.12-.79.04-1.15.16-.36.47-.66.87-.87.4-.21.88-.33 1.37-.36.49-.03.98.04 1.38.195.4.155.71.394.88.65.17.255.22.51.13.73-.09.22-.32.415-.63.56-.32.145-.72.24-1.17.27-.81.054-1.58-.086-2.16-.364-.58-.28-.98-.7-1.02-1.2-.04-.495.3-.93.895-1.175.587-.242 1.517-.23 2.507.068.985.296 2.017.87 2.84 1.61.822.738 1.41 1.635 1.58 2.528.088.433.058.882-.082 1.305-.137.421-.38.816-.693 1.17-.626.71-1.524 1.273-2.596 1.67-1.072.398-2.31.612-3.592.612-.872 0-1.645-.131-2.242-.367-.296-.12-.55-.285-.738-.515-.188-.233-.293-.503-.293-.848 0-.218.066-.399.18-.553.125-.164.347-.3.64-.456.218-.116.555-.248.97-.407.47-.18 1.018-.39 1.565-.66.583-.285 1.157-.65 1.647-1.16.582-.589.921-1.321 1.038-2.1.123-.805-.009-1.657-.287-2.489-.59-1.771-1.831-3.47-2.716-4.521-.75-1.067-.974-1.928-1.05-3.02-.065-1.491-1.056-5.965-3.17-6.298C12.82.008 12.66 0 12.504 0z"/></svg>
                    {:else}
                      <svg class="w-4 h-4" fill="currentColor" viewBox="0 0 24 24"><path d="M4 6h18V4H4c-1.1 0-2 .9-2 2v11H0v3h14v-3H4V6zm19 2h-6c-.55 0-1 .45-1 1v10c0 .55.45 1 1 1h6c.55 0 1-.45 1-1V9c0-.55-.45-1-1-1zm-1 9h-4v-7h4v7z"/></svg>
                    {/if}
                  </div>
                  <div>
                    <div class="flex items-center gap-2">
                      <span class="font-medium">{device.name}</span>
                      {#if device.is_current}
                        <span class="text-xs px-1.5 py-0.5 bg-blue-500/20 text-blue-400 rounded">This device</span>
                      {/if}
                      {#if device.is_exit_node}
                        <span class="text-xs px-1.5 py-0.5 bg-purple-500/20 text-purple-400 rounded">Exit Node</span>
                      {/if}
                    </div>
                    <div class="text-xs text-gray-400">
                      {device.tailscale_ip}
                      {#if device.os}
                        <span class="mx-1">•</span> {device.os}
                      {/if}
                    </div>
                  </div>
                </div>
                <div class="text-right">
                  <div class="flex items-center gap-2">
                    {#if device.relay}
                      <span class="text-xs px-1.5 py-0.5 bg-yellow-500/20 text-yellow-400 rounded">
                        Relay: {device.relay}
                      </span>
                    {:else if device.online && !device.is_current}
                      <span class="text-xs px-1.5 py-0.5 bg-green-500/20 text-green-400 rounded">Direct</span>
                    {/if}
                    <div class="w-2 h-2 rounded-full {device.online ? 'bg-green-500' : 'bg-gray-500'}"></div>
                  </div>
                  {#if device.rx_bytes || device.tx_bytes}
                    <div class="text-xs text-gray-500 mt-1">
                      ↓{formatBytes(device.rx_bytes)} ↑{formatBytes(device.tx_bytes)}
                    </div>
                  {/if}
                </div>
              </div>
            {/each}
          </div>
        </div>
      {/if}

      <!-- Netcheck Modal -->
      {#if showNetcheck}
        <div class="fixed inset-0 bg-black/70 flex items-center justify-center z-50 p-4" onclick={() => showNetcheck = false}>
          <div class="bg-gray-800 rounded-lg w-full max-w-lg" onclick={(e) => e.stopPropagation()}>
            <div class="flex items-center justify-between p-4 border-b border-gray-700">
              <h3 class="text-lg font-semibold">Network Diagnostics</h3>
              <button onclick={() => showNetcheck = false} class="text-gray-400 hover:text-white text-2xl">&times;</button>
            </div>
            <div class="p-4">
              {#if !netcheck}
                <div class="text-gray-400">Running diagnostics...</div>
              {:else}
                <div class="space-y-3">
                  <div class="grid grid-cols-2 gap-3">
                    <div class="bg-gray-700/50 rounded p-2">
                      <span class="text-xs text-gray-400">UDP</span>
                      <p class="{netcheck.udp ? 'text-green-400' : 'text-red-400'}">{netcheck.udp ? "Available" : "Blocked"}</p>
                    </div>
                    <div class="bg-gray-700/50 rounded p-2">
                      <span class="text-xs text-gray-400">IPv4</span>
                      <p class="{netcheck.ipv4 ? 'text-green-400' : 'text-gray-400'}">{netcheck.ipv4 ? "Yes" : "No"}</p>
                    </div>
                    <div class="bg-gray-700/50 rounded p-2">
                      <span class="text-xs text-gray-400">IPv6</span>
                      <p class="{netcheck.ipv6 ? 'text-green-400' : 'text-gray-400'}">{netcheck.ipv6 ? "Yes" : "No"}</p>
                    </div>
                    <div class="bg-gray-700/50 rounded p-2">
                      <span class="text-xs text-gray-400">Hair Pinning</span>
                      <p class="{netcheck.hair_pinning ? 'text-green-400' : 'text-yellow-400'}">{netcheck.hair_pinning ? "Yes" : "No"}</p>
                    </div>
                  </div>

                  <div class="bg-gray-700/50 rounded p-2">
                    <span class="text-xs text-gray-400">Preferred DERP Relay</span>
                    <p class="text-blue-400">{netcheck.preferred_derp || "None"}</p>
                  </div>

                  {#if netcheck.derp_latencies?.length > 0}
                    <div>
                      <span class="text-xs text-gray-400">DERP Relay Latencies</span>
                      <div class="mt-1 space-y-1 max-h-32 overflow-y-auto">
                        {#each netcheck.derp_latencies.slice(0, 5) as derp}
                          <div class="flex justify-between text-sm">
                            <span>Region {derp.region}</span>
                            <span class="text-gray-400">{derp.latency_ms.toFixed(1)} ms</span>
                          </div>
                        {/each}
                      </div>
                    </div>
                  {/if}
                </div>
              {/if}
            </div>
          </div>
        </div>
      {/if}
    </div>

  {:else if activeTab === "download"}
    <!-- GLUETUN/NORDVPN SECTION -->
    <div class="card">
      <div class="flex items-center justify-between mb-4">
        <h3 class="text-lg font-semibold flex items-center gap-2">
          <svg class="w-5 h-5" fill="currentColor" viewBox="0 0 24 24">
            <path d="M12 1L3 5v6c0 5.55 3.84 10.74 9 12 5.16-1.26 9-6.45 9-12V5l-9-4zm0 10.99h7c-.53 4.12-3.28 7.79-7 8.94V12H5V6.3l7-3.11v8.8z"/>
          </svg>
          Download VPN (NordVPN)
          {#if gluetun?.container_running && gluetun?.vpn_connected}
            <span class="text-xs px-2 py-0.5 bg-green-500/20 text-green-400 rounded">Connected</span>
          {:else if gluetun?.container_running}
            <span class="text-xs px-2 py-0.5 bg-yellow-500/20 text-yellow-400 rounded">Connecting...</span>
          {:else}
            <span class="text-xs px-2 py-0.5 bg-gray-500/20 text-gray-400 rounded">Not Running</span>
          {/if}
        </h3>
      </div>

      {#if !gluetun?.container_running}
        <div class="text-center py-8">
          <svg class="w-16 h-16 mx-auto text-gray-600 mb-4" fill="currentColor" viewBox="0 0 24 24">
            <path d="M12 1L3 5v6c0 5.55 3.84 10.74 9 12 5.16-1.26 9-6.45 9-12V5l-9-4zm0 10.99h7c-.53 4.12-3.28 7.79-7 8.94V12H5V6.3l7-3.11v8.8z"/>
          </svg>
          <h4 class="text-lg font-medium text-gray-400 mb-2">Download VPN Not Configured</h4>
          <p class="text-sm text-gray-500 max-w-md mx-auto">
            The Gluetun container with NordVPN is not set up yet. This will be used to route
            torrent traffic through a VPN for privacy.
          </p>
          <p class="text-xs text-gray-600 mt-4">
            Set up the media stack (gluetun + qBittorrent + *arr apps) to enable this feature.
          </p>
        </div>
      {:else}
        <div class="grid grid-cols-2 md:grid-cols-4 gap-4 mb-4">
          <div class="bg-gray-700/50 rounded p-3">
            <p class="text-xs text-gray-400">VPN IP</p>
            <p class="font-mono text-green-400">{gluetun.vpn_ip || "Connecting..."}</p>
          </div>
          <div class="bg-gray-700/50 rounded p-3">
            <p class="text-xs text-gray-400">Provider</p>
            <p class="font-medium">{gluetun.vpn_provider}</p>
          </div>
          <div class="bg-gray-700/50 rounded p-3">
            <p class="text-xs text-gray-400">Location</p>
            <p>{gluetun.vpn_country || "Unknown"} {gluetun.vpn_city || ""}</p>
          </div>
          <div class="bg-gray-700/50 rounded p-3">
            <p class="text-xs text-gray-400">Forwarded Port</p>
            <p class="{gluetun.port_forwarded ? 'text-green-400' : 'text-gray-400'}">
              {gluetun.port_forwarded || "None"}
            </p>
          </div>
        </div>

        <div class="flex gap-2">
          <button onclick={restartGluetun} class="btn-secondary">
            Restart VPN
          </button>
        </div>
      {/if}
    </div>
  {/if}
</div>

<style>
  .card {
    background: #1f2937;
    border-radius: 0.5rem;
    padding: 1.5rem;
  }

  .btn-primary {
    background: #3b82f6;
    color: white;
    padding: 0.5rem 1rem;
    border-radius: 0.375rem;
    font-weight: 500;
    transition: background 0.15s;
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
    transition: all 0.15s;
  }

  .btn-secondary:hover {
    background: rgba(107, 114, 128, 0.3);
    color: white;
  }

  .btn-danger {
    background: rgba(239, 68, 68, 0.2);
    color: #ef4444;
    padding: 0.5rem 1rem;
    border-radius: 0.375rem;
    font-weight: 500;
    transition: all 0.15s;
  }

  .btn-danger:hover {
    background: rgba(239, 68, 68, 0.3);
  }
</style>

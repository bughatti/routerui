<script>
  import { onMount } from "svelte";
  import { goto } from "$app/navigation";

  // State
  let currentStep = $state(1);
  let loading = $state(true);
  let configuring = $state(false);
  let error = $state(null);

  // Data
  let interfaces = $state([]);

  // Form data
  let adminForm = $state({
    username: "",
    password: "",
    confirmPassword: ""
  });

  let networkForm = $state({
    wan_interface: "",
    lan_interface: ""
  });

  // Configuration progress
  let configProgress = $state([]);

  const steps = [
    { num: 1, title: "Welcome" },
    { num: 2, title: "Admin" },
    { num: 3, title: "Network" },
    { num: 4, title: "Complete" }
  ];

  onMount(async () => {
    try {
      const statusRes = await fetch("/api/setup/status");
      const status = await statusRes.json();

      if (status.is_complete) {
        goto("/");
        return;
      }

      // Load interfaces
      const ifaceRes = await fetch("/api/setup/interfaces");
      if (ifaceRes.ok) {
        interfaces = await ifaceRes.json();
        // Auto-select interfaces if obvious
        const wan = interfaces.find(i => i.ip && (i.name.includes("enp0s3") || i.name.includes("eth0")));
        const lan = interfaces.find(i => !i.ip || i.name.includes("enp0s8") || i.name.includes("eth1"));
        if (wan) networkForm.wan_interface = wan.name;
        if (lan && lan.name !== wan?.name) networkForm.lan_interface = lan.name;
      }
    } catch (e) {
      error = e.message;
    } finally {
      loading = false;
    }
  });

  async function createAdmin() {
    if (adminForm.password !== adminForm.confirmPassword) {
      error = "Passwords do not match";
      return;
    }
    if (adminForm.password.length < 6) {
      error = "Password must be at least 6 characters";
      return;
    }

    try {
      loading = true;
      error = null;
      const res = await fetch("/api/setup/admin", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          username: adminForm.username,
          password: adminForm.password
        })
      });

      if (!res.ok) {
        const data = await res.json();
        throw new Error(data.message || "Failed to create admin");
      }

      currentStep = 3;
    } catch (e) {
      error = e.message;
    } finally {
      loading = false;
    }
  }

  async function configureRouter() {
    if (networkForm.wan_interface === networkForm.lan_interface) {
      error = "WAN and LAN must be different interfaces";
      return;
    }

    configuring = true;
    error = null;
    configProgress = [];

    try {
      // Step 1: Save network config
      configProgress = [...configProgress, { text: "Saving network configuration...", done: false }];
      const res = await fetch("/api/setup/configure-router", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          wan_interface: networkForm.wan_interface,
          lan_interface: networkForm.lan_interface
        })
      });

      if (!res.ok) {
        const data = await res.json();
        throw new Error(data.message || "Configuration failed");
      }

      const result = await res.json();

      // Update progress with results
      configProgress = result.steps.map(s => ({ text: s.name, done: s.success, error: s.error }));

      if (result.success) {
        // Mark setup complete
        await fetch("/api/setup/complete", { method: "POST" });
        currentStep = 4;
      } else {
        throw new Error("Some configuration steps failed. Check the details below.");
      }
    } catch (e) {
      error = e.message;
    } finally {
      configuring = false;
    }
  }
</script>

<svelte:head>
  <title>Setup - RouterUI</title>
</svelte:head>

{#if loading && currentStep === 1}
  <div class="card text-center py-12">
    <div class="animate-spin rounded-full h-12 w-12 border-b-2 border-blue-400 mx-auto"></div>
    <p class="mt-4 text-gray-400">Loading setup wizard...</p>
  </div>
{:else}
  <!-- Progress Steps -->
  <div class="flex items-center justify-center mb-8">
    {#each steps as step, i}
      <div class="flex items-center">
        <div class="flex flex-col items-center">
          <div
            class="w-10 h-10 rounded-full flex items-center justify-center font-bold transition-colors
              {currentStep >= step.num ? 'bg-blue-600 text-white' : 'bg-gray-700 text-gray-400'}"
          >
            {#if currentStep > step.num}
              <svg class="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7"></path>
              </svg>
            {:else}
              {step.num}
            {/if}
          </div>
          <span class="text-sm mt-2 {currentStep >= step.num ? 'text-blue-400' : 'text-gray-500'}">{step.title}</span>
        </div>
        {#if i < steps.length - 1}
          <div class="w-16 h-0.5 mx-2 {currentStep > step.num ? 'bg-blue-600' : 'bg-gray-700'}"></div>
        {/if}
      </div>
    {/each}
  </div>

  {#if error}
    <div class="card bg-red-900/20 border-red-700 text-red-400 mb-4">
      {error}
      <button onclick={() => error = null} class="ml-4 underline">Dismiss</button>
    </div>
  {/if}

  <div class="card max-w-2xl mx-auto">
    {#if currentStep === 1}
      <!-- Welcome -->
      <div class="text-center py-8">
        <div class="text-6xl mb-4">🌐</div>
        <h2 class="text-2xl font-bold mb-4">Welcome to RouterUI</h2>
        <p class="text-gray-400 mb-6 max-w-lg mx-auto">
          Let's configure your Linux box as a router. This will take about 2 minutes.
        </p>
        <div class="bg-gray-700/50 rounded-lg p-4 max-w-md mx-auto text-left mb-6">
          <h3 class="font-semibold mb-3">What we'll configure:</h3>
          <ul class="text-sm text-gray-400 space-y-2">
            <li class="flex items-center gap-2">
              <span class="text-green-400">✓</span>
              <span>IP forwarding between interfaces</span>
            </li>
            <li class="flex items-center gap-2">
              <span class="text-green-400">✓</span>
              <span>NAT/Masquerade for internet sharing</span>
            </li>
            <li class="flex items-center gap-2">
              <span class="text-green-400">✓</span>
              <span>DHCP server for your local network</span>
            </li>
            <li class="flex items-center gap-2">
              <span class="text-green-400">✓</span>
              <span>DNS forwarding</span>
            </li>
          </ul>
        </div>
        <button onclick={() => currentStep = 2} class="btn btn-primary px-8 py-3 text-lg">
          Get Started
        </button>
      </div>

    {:else if currentStep === 2}
      <!-- Admin Account -->
      <h2 class="text-xl font-bold mb-2">Create Admin Account</h2>
      <p class="text-gray-400 mb-6">This account will be used to log into RouterUI.</p>

      <div class="space-y-4 max-w-md">
        <div>
          <label class="block text-sm font-medium mb-1">Username</label>
          <input
            type="text"
            bind:value={adminForm.username}
            class="w-full bg-gray-700 border border-gray-600 rounded-lg px-4 py-3 focus:border-blue-500 focus:outline-none"
            placeholder="admin"
          />
        </div>
        <div>
          <label class="block text-sm font-medium mb-1">Password</label>
          <input
            type="password"
            bind:value={adminForm.password}
            class="w-full bg-gray-700 border border-gray-600 rounded-lg px-4 py-3 focus:border-blue-500 focus:outline-none"
            placeholder="Minimum 6 characters"
          />
        </div>
        <div>
          <label class="block text-sm font-medium mb-1">Confirm Password</label>
          <input
            type="password"
            bind:value={adminForm.confirmPassword}
            class="w-full bg-gray-700 border border-gray-600 rounded-lg px-4 py-3 focus:border-blue-500 focus:outline-none"
          />
        </div>
      </div>

      <div class="flex justify-between mt-8">
        <button onclick={() => currentStep = 1} class="btn bg-gray-700 hover:bg-gray-600">Back</button>
        <button
          onclick={createAdmin}
          disabled={loading || !adminForm.username || !adminForm.password}
          class="btn btn-primary disabled:opacity-50"
        >
          {loading ? "Creating..." : "Continue"}
        </button>
      </div>

    {:else if currentStep === 3}
      <!-- Network Configuration -->
      <h2 class="text-xl font-bold mb-2">Select Network Interfaces</h2>
      <p class="text-gray-400 mb-6">Choose which interface connects to the internet (WAN) and which serves your local network (LAN).</p>

      <div class="space-y-6">
        <div>
          <label class="block text-sm font-medium mb-2">WAN Interface (Internet/Upstream)</label>
          <div class="grid gap-2">
            {#each interfaces as iface}
              <label
                class="flex items-center gap-3 p-4 rounded-lg cursor-pointer transition-colors
                  {networkForm.wan_interface === iface.name ? 'bg-blue-600/20 border-2 border-blue-500' : 'bg-gray-700/50 border-2 border-transparent hover:bg-gray-700'}"
              >
                <input
                  type="radio"
                  name="wan"
                  value={iface.name}
                  bind:group={networkForm.wan_interface}
                  class="w-4 h-4"
                />
                <div class="flex-1">
                  <div class="font-medium">{iface.name}</div>
                  <div class="text-sm text-gray-400">
                    {iface.ip || "No IP"}
                    {iface.is_up ? "" : "(down)"}
                    <span class="text-gray-500 ml-2">{iface.mac}</span>
                  </div>
                </div>
                {#if iface.ip}
                  <span class="text-xs bg-green-600/30 text-green-400 px-2 py-1 rounded">Has IP</span>
                {/if}
              </label>
            {/each}
          </div>
        </div>

        <div>
          <label class="block text-sm font-medium mb-2">LAN Interface (Local Network)</label>
          <div class="grid gap-2">
            {#each interfaces as iface}
              <label
                class="flex items-center gap-3 p-4 rounded-lg cursor-pointer transition-colors
                  {networkForm.lan_interface === iface.name ? 'bg-blue-600/20 border-2 border-blue-500' : 'bg-gray-700/50 border-2 border-transparent hover:bg-gray-700'}
                  {networkForm.wan_interface === iface.name ? 'opacity-50 cursor-not-allowed' : ''}"
              >
                <input
                  type="radio"
                  name="lan"
                  value={iface.name}
                  bind:group={networkForm.lan_interface}
                  disabled={networkForm.wan_interface === iface.name}
                  class="w-4 h-4"
                />
                <div class="flex-1">
                  <div class="font-medium">{iface.name}</div>
                  <div class="text-sm text-gray-400">
                    {iface.ip || "No IP"}
                    {iface.is_up ? "" : "(down)"}
                    <span class="text-gray-500 ml-2">{iface.mac}</span>
                  </div>
                </div>
                {#if networkForm.wan_interface === iface.name}
                  <span class="text-xs text-gray-500">Selected as WAN</span>
                {/if}
              </label>
            {/each}
          </div>
        </div>

        <div class="bg-blue-900/20 border border-blue-700 rounded-lg p-4">
          <h4 class="font-medium text-blue-400 mb-2">Configuration Preview</h4>
          <ul class="text-sm text-gray-300 space-y-1">
            <li>LAN IP: <span class="text-white">192.168.1.1</span></li>
            <li>DHCP Range: <span class="text-white">192.168.1.100 - 192.168.1.250</span></li>
            <li>Subnet: <span class="text-white">255.255.255.0</span></li>
          </ul>
        </div>
      </div>

      {#if configProgress.length > 0}
        <div class="mt-6 space-y-2">
          <h4 class="font-medium mb-2">Configuration Progress:</h4>
          {#each configProgress as step}
            <div class="flex items-center gap-2 text-sm">
              {#if step.done}
                <span class="text-green-400">✓</span>
              {:else if step.error}
                <span class="text-red-400">✗</span>
              {:else}
                <div class="animate-spin w-4 h-4 border-2 border-blue-400 border-t-transparent rounded-full"></div>
              {/if}
              <span class="{step.error ? 'text-red-400' : ''}">{step.text}</span>
              {#if step.error}
                <span class="text-red-400 text-xs">- {step.error}</span>
              {/if}
            </div>
          {/each}
        </div>
      {/if}

      <div class="flex justify-between mt-8">
        <button onclick={() => currentStep = 2} class="btn bg-gray-700 hover:bg-gray-600" disabled={configuring}>Back</button>
        <button
          onclick={configureRouter}
          disabled={configuring || !networkForm.wan_interface || !networkForm.lan_interface}
          class="btn btn-primary disabled:opacity-50"
        >
          {#if configuring}
            <span class="flex items-center gap-2">
              <div class="animate-spin w-4 h-4 border-2 border-white border-t-transparent rounded-full"></div>
              Configuring...
            </span>
          {:else}
            Configure Router
          {/if}
        </button>
      </div>

    {:else if currentStep === 4}
      <!-- Complete -->
      <div class="text-center py-8">
        <div class="text-6xl mb-4">🎉</div>
        <h2 class="text-2xl font-bold mb-4 text-green-400">Router Configured!</h2>
        <p class="text-gray-400 mb-6 max-w-lg mx-auto">
          Your router is now ready. Devices connected to the LAN interface will receive IP addresses automatically.
        </p>

        <div class="bg-gray-700/50 rounded-lg p-4 max-w-md mx-auto text-left mb-6">
          <h3 class="font-semibold mb-3">Configuration Summary</h3>
          <div class="space-y-2 text-sm">
            <div class="flex justify-between">
              <span class="text-gray-400">WAN Interface:</span>
              <span>{networkForm.wan_interface}</span>
            </div>
            <div class="flex justify-between">
              <span class="text-gray-400">LAN Interface:</span>
              <span>{networkForm.lan_interface}</span>
            </div>
            <div class="flex justify-between">
              <span class="text-gray-400">Router IP:</span>
              <span>192.168.1.1</span>
            </div>
            <div class="flex justify-between">
              <span class="text-gray-400">DHCP Range:</span>
              <span>192.168.1.100-250</span>
            </div>
          </div>
        </div>

        <div class="bg-blue-900/20 border border-blue-700 rounded-lg p-4 max-w-md mx-auto text-left mb-6">
          <h4 class="font-medium text-blue-400 mb-2">Optional Add-ons</h4>
          <p class="text-sm text-gray-400">
            You can install additional features like AdGuard Home, Tailscale VPN, or media servers from the main dashboard after logging in.
          </p>
        </div>

        <a href="/" class="btn btn-primary px-8 py-3 text-lg">Go to Dashboard</a>
      </div>
    {/if}
  </div>
{/if}

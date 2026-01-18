<script>
  import { onMount } from "svelte";
  import { goto } from "$app/navigation";

  // State
  let currentStep = $state(1);
  let loading = $state(true);
  let error = $state(null);
  let installing = $state(false);
  let installProgress = $state({});

  // Data
  let interfaces = $state([]);
  let features = $state([]);

  // Form data
  let adminForm = $state({
    username: "",
    password: "",
    confirmPassword: "",
    email: ""
  });

  let networkForm = $state({
    wan_interface: "",
    lan_interface: "",
    wifi_interface: ""
  });

  let selectedFeatures = $state({});

  const steps = [
    { num: 1, title: "Welcome" },
    { num: 2, title: "Admin Account" },
    { num: 3, title: "Network" },
    { num: 4, title: "Features" },
    { num: 5, title: "Complete" }
  ];

  const featureCategories = {
    "Core": { description: "Essential services for router operation", color: "blue" },
    "DNS": { description: "DNS and ad-blocking services", color: "green" },
    "VPN": { description: "VPN services for secure connectivity", color: "purple" },
    "Security": { description: "Security and protection tools", color: "red" },
    "Media": { description: "Media management and streaming", color: "yellow" }
  };

  onMount(async () => {
    try {
      // Check if setup already complete
      const statusRes = await fetch("/api/setup/status");
      const status = await statusRes.json();

      if (status.is_complete) {
        goto("/");
        return;
      }

      // Load interfaces and features
      const [ifaceRes, featuresRes] = await Promise.all([
        fetch("/api/setup/interfaces"),
        fetch("/api/setup/features")
      ]);

      if (ifaceRes.ok) {
        interfaces = await ifaceRes.json();
        // Auto-select interfaces based on common patterns
        const wan = interfaces.find(i => i.name.includes("enp1") || i.name.includes("eth0"));
        const lan = interfaces.find(i => i.name.includes("enp2") || i.name.includes("eth1"));
        const wifi = interfaces.find(i => i.is_wireless);
        if (wan) networkForm.wan_interface = wan.name;
        if (lan) networkForm.lan_interface = lan.name;
        if (wifi) networkForm.wifi_interface = wifi.name;
      }

      if (featuresRes.ok) {
        features = await featuresRes.json();
        // Initialize selected state for all features
        features.forEach(f => {
          selectedFeatures[f.id] = {
            enabled: f.is_installed,
            install: false
          };
        });
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
          password: adminForm.password,
          email: adminForm.email || null
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

  async function saveNetwork() {
    try {
      loading = true;
      error = null;
      const res = await fetch("/api/setup/network", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          wan_interface: networkForm.wan_interface,
          lan_interface: networkForm.lan_interface,
          wifi_interface: networkForm.wifi_interface || null
        })
      });

      if (!res.ok) {
        throw new Error("Failed to save network config");
      }

      currentStep = 4;
    } catch (e) {
      error = e.message;
    } finally {
      loading = false;
    }
  }

  async function saveFeatures() {
    try {
      loading = true;
      error = null;

      // Save feature selection
      const featureList = Object.entries(selectedFeatures).map(([id, state]) => ({
        id,
        enabled: state.enabled,
        install: state.install
      }));

      const res = await fetch("/api/setup/features/save", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ features: featureList })
      });

      if (!res.ok) {
        throw new Error("Failed to save features");
      }

      const data = await res.json();

      // Install features that need installation
      if (data.to_install?.length > 0) {
        installing = true;
        for (const featureId of data.to_install) {
          installProgress[featureId] = { status: "installing", message: "Installing..." };

          const installRes = await fetch("/api/setup/features/install", {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify({ feature_id: featureId })
          });

          const result = await installRes.json();
          installProgress[featureId] = result;
        }
        installing = false;
      }

      // Complete setup
      await fetch("/api/setup/complete", { method: "POST" });
      currentStep = 5;
    } catch (e) {
      error = e.message;
    } finally {
      loading = false;
    }
  }

  function toggleFeature(featureId, field) {
    selectedFeatures[featureId] = {
      ...selectedFeatures[featureId],
      [field]: !selectedFeatures[featureId][field]
    };
  }

  function getFeaturesByCategory(category) {
    return features.filter(f => f.category === category);
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
          <span class="text-xs mt-1 {currentStep >= step.num ? 'text-blue-400' : 'text-gray-500'}">{step.title}</span>
        </div>
        {#if i < steps.length - 1}
          <div class="w-12 h-1 mx-2 {currentStep > step.num ? 'bg-blue-600' : 'bg-gray-700'}"></div>
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

  <!-- Step Content -->
  <div class="card">
    {#if currentStep === 1}
      <!-- Welcome -->
      <div class="text-center py-8">
        <div class="text-6xl mb-4">🌐</div>
        <h2 class="text-2xl font-bold mb-4">Welcome to RouterUI</h2>
        <p class="text-gray-400 mb-6 max-w-lg mx-auto">
          This wizard will help you set up your network management system.
          We'll configure your admin account, network interfaces, and optional features.
        </p>
        <div class="bg-gray-700/50 rounded-lg p-4 max-w-md mx-auto text-left mb-6">
          <h3 class="font-semibold mb-2">What we'll configure:</h3>
          <ul class="text-sm text-gray-400 space-y-1">
            <li>✓ Administrator account</li>
            <li>✓ Network interfaces (WAN, LAN, WiFi)</li>
            <li>✓ Optional features and services</li>
          </ul>
        </div>
        <button onclick={() => currentStep = 2} class="btn btn-primary px-8">
          Get Started
        </button>
      </div>

    {:else if currentStep === 2}
      <!-- Admin Account -->
      <h2 class="text-xl font-bold mb-4">Create Admin Account</h2>
      <p class="text-gray-400 mb-6">Set up the administrator account for RouterUI.</p>

      <div class="space-y-4 max-w-md">
        <div>
          <label class="block text-sm font-medium mb-1">Username</label>
          <input
            type="text"
            bind:value={adminForm.username}
            class="w-full bg-gray-700 border border-gray-600 rounded-lg px-4 py-2 focus:border-blue-500 focus:outline-none"
            placeholder="admin"
          />
        </div>
        <div>
          <label class="block text-sm font-medium mb-1">Password</label>
          <input
            type="password"
            bind:value={adminForm.password}
            class="w-full bg-gray-700 border border-gray-600 rounded-lg px-4 py-2 focus:border-blue-500 focus:outline-none"
            placeholder="Minimum 6 characters"
          />
        </div>
        <div>
          <label class="block text-sm font-medium mb-1">Confirm Password</label>
          <input
            type="password"
            bind:value={adminForm.confirmPassword}
            class="w-full bg-gray-700 border border-gray-600 rounded-lg px-4 py-2 focus:border-blue-500 focus:outline-none"
          />
        </div>
        <div>
          <label class="block text-sm font-medium mb-1">Email (optional)</label>
          <input
            type="email"
            bind:value={adminForm.email}
            class="w-full bg-gray-700 border border-gray-600 rounded-lg px-4 py-2 focus:border-blue-500 focus:outline-none"
            placeholder="admin@example.com"
          />
        </div>
      </div>

      <div class="flex justify-between mt-8">
        <button onclick={() => currentStep = 1} class="btn bg-gray-700 hover:bg-gray-600">
          Back
        </button>
        <button
          onclick={createAdmin}
          disabled={loading || !adminForm.username || !adminForm.password}
          class="btn btn-primary disabled:opacity-50"
        >
          {loading ? "Creating..." : "Create Account"}
        </button>
      </div>

    {:else if currentStep === 3}
      <!-- Network Configuration -->
      <h2 class="text-xl font-bold mb-4">Network Configuration</h2>
      <p class="text-gray-400 mb-6">Select your network interfaces. We've auto-detected available interfaces.</p>

      {#if interfaces.length === 0}
        <div class="text-center py-8 text-gray-400">
          <p>No network interfaces detected.</p>
          <p class="text-sm mt-2">This may be normal in mock mode.</p>
        </div>
      {:else}
        <div class="space-y-4">
          <div>
            <label class="block text-sm font-medium mb-1">WAN Interface (Internet)</label>
            <select
              bind:value={networkForm.wan_interface}
              class="w-full bg-gray-700 border border-gray-600 rounded-lg px-4 py-2 focus:border-blue-500 focus:outline-none"
            >
              <option value="">Select interface...</option>
              {#each interfaces as iface}
                <option value={iface.name}>
                  {iface.name} {iface.ip ? `(${iface.ip})` : ""} {iface.is_up ? "●" : "○"}
                </option>
              {/each}
            </select>
          </div>
          <div>
            <label class="block text-sm font-medium mb-1">LAN Interface (Local Network)</label>
            <select
              bind:value={networkForm.lan_interface}
              class="w-full bg-gray-700 border border-gray-600 rounded-lg px-4 py-2 focus:border-blue-500 focus:outline-none"
            >
              <option value="">Select interface...</option>
              {#each interfaces as iface}
                <option value={iface.name}>
                  {iface.name} {iface.ip ? `(${iface.ip})` : ""} {iface.is_up ? "●" : "○"}
                </option>
              {/each}
            </select>
          </div>
          <div>
            <label class="block text-sm font-medium mb-1">WiFi Interface (optional)</label>
            <select
              bind:value={networkForm.wifi_interface}
              class="w-full bg-gray-700 border border-gray-600 rounded-lg px-4 py-2 focus:border-blue-500 focus:outline-none"
            >
              <option value="">None</option>
              {#each interfaces.filter(i => i.is_wireless) as iface}
                <option value={iface.name}>
                  {iface.name} {iface.ip ? `(${iface.ip})` : ""} {iface.is_up ? "●" : "○"}
                </option>
              {/each}
            </select>
          </div>
        </div>

        <!-- Interface Preview -->
        <div class="mt-6 bg-gray-700/50 rounded-lg p-4">
          <h3 class="font-semibold mb-3">Detected Interfaces</h3>
          <div class="grid grid-cols-1 md:grid-cols-2 gap-2 text-sm">
            {#each interfaces as iface}
              <div class="flex items-center gap-2 p-2 bg-gray-800 rounded">
                <span class={iface.is_up ? "text-green-400" : "text-gray-500"}>●</span>
                <span class="font-mono">{iface.name}</span>
                {#if iface.is_wireless}
                  <span class="text-xs bg-purple-600/30 text-purple-400 px-2 rounded">WiFi</span>
                {/if}
                {#if iface.ip}
                  <span class="text-gray-400 ml-auto">{iface.ip}</span>
                {/if}
              </div>
            {/each}
          </div>
        </div>
      {/if}

      <div class="flex justify-between mt-8">
        <button onclick={() => currentStep = 2} class="btn bg-gray-700 hover:bg-gray-600">
          Back
        </button>
        <button
          onclick={saveNetwork}
          disabled={loading}
          class="btn btn-primary disabled:opacity-50"
        >
          {loading ? "Saving..." : "Continue"}
        </button>
      </div>

    {:else if currentStep === 4}
      <!-- Features Selection -->
      <h2 class="text-xl font-bold mb-4">Select Features</h2>
      <p class="text-gray-400 mb-6">Choose which features to enable. You can install missing services now or later.</p>

      {#if installing}
        <div class="text-center py-8">
          <div class="animate-spin rounded-full h-12 w-12 border-b-2 border-blue-400 mx-auto"></div>
          <p class="mt-4 text-gray-400">Installing features...</p>
          <div class="mt-4 max-w-md mx-auto text-left space-y-2">
            {#each Object.entries(installProgress) as [id, progress]}
              <div class="flex items-center gap-2 text-sm">
                {#if progress.status === "installing"}
                  <span class="text-yellow-400">⏳</span>
                {:else if progress.status === "complete"}
                  <span class="text-green-400">✓</span>
                {:else}
                  <span class="text-red-400">✗</span>
                {/if}
                <span>{id}</span>
                <span class="text-gray-500 ml-auto">{progress.message}</span>
              </div>
            {/each}
          </div>
        </div>
      {:else}
        <div class="space-y-6">
          {#each Object.entries(featureCategories) as [category, meta]}
            {@const categoryFeatures = getFeaturesByCategory(category)}
            {#if categoryFeatures.length > 0}
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <h3 class="font-semibold text-{meta.color}-400">{category}</h3>
                  <span class="text-xs text-gray-500">{meta.description}</span>
                </div>
                <div class="grid grid-cols-1 md:grid-cols-2 gap-3">
                  {#each categoryFeatures as feature}
                    {@const needsDocker = feature.install_method === "docker" && !feature.can_install}
                    {@const dockerFeature = features.find(f => f.id === "docker")}
                    {@const dockerInstalled = dockerFeature?.is_installed}
                    <div class="bg-gray-700/50 rounded-lg p-4">
                      <div class="flex items-start justify-between">
                        <div class="flex-1">
                          <div class="flex items-center gap-2 flex-wrap">
                            <span class="font-medium">{feature.name}</span>
                            {#if feature.is_installed}
                              <span class="text-xs bg-green-600/30 text-green-400 px-2 rounded">Installed</span>
                            {:else if feature.can_install}
                              <span class="text-xs bg-yellow-600/30 text-yellow-400 px-2 rounded">Not Installed</span>
                            {:else if needsDocker}
                              <span class="text-xs bg-blue-600/30 text-blue-400 px-2 rounded">Requires Docker</span>
                            {:else}
                              <span class="text-xs bg-gray-600/30 text-gray-400 px-2 rounded">Requires {feature.install_method}</span>
                            {/if}
                          </div>
                          <p class="text-xs text-gray-400 mt-1">{feature.description}</p>
                          {#if feature.is_running}
                            <p class="text-xs text-green-400 mt-1">● Running</p>
                          {/if}
                          {#if needsDocker && !dockerInstalled}
                            <p class="text-xs text-blue-400 mt-1">Docker will be installed automatically</p>
                          {/if}
                        </div>
                        <div class="flex flex-col gap-2 ml-4">
                          <label class="flex items-center gap-2 text-sm cursor-pointer">
                            <input
                              type="checkbox"
                              checked={selectedFeatures[feature.id]?.enabled}
                              onchange={() => toggleFeature(feature.id, "enabled")}
                              class="rounded bg-gray-600 border-gray-500"
                            />
                            <span>Enable</span>
                          </label>
                          {#if !feature.is_installed}
                            <label class="flex items-center gap-2 text-sm cursor-pointer">
                              <input
                                type="checkbox"
                                checked={selectedFeatures[feature.id]?.install}
                                onchange={() => toggleFeature(feature.id, "install")}
                                class="rounded bg-gray-600 border-gray-500"
                              />
                              <span class="text-blue-400">Install</span>
                            </label>
                          {/if}
                        </div>
                      </div>
                    </div>
                  {/each}
                </div>
              </div>
            {/if}
          {/each}
        </div>

        <div class="flex justify-between mt-8">
          <button onclick={() => currentStep = 3} class="btn bg-gray-700 hover:bg-gray-600">
            Back
          </button>
          <button
            onclick={saveFeatures}
            disabled={loading}
            class="btn btn-primary disabled:opacity-50"
          >
            {loading ? "Saving..." : "Complete Setup"}
          </button>
        </div>
      {/if}

    {:else if currentStep === 5}
      <!-- Complete -->
      <div class="text-center py-8">
        <div class="text-6xl mb-4">🎉</div>
        <h2 class="text-2xl font-bold mb-4 text-green-400">Setup Complete!</h2>
        <p class="text-gray-400 mb-6 max-w-lg mx-auto">
          RouterUI has been configured successfully. You can now log in with your admin account
          and start managing your network.
        </p>

        {#if Object.keys(installProgress).length > 0}
          <div class="bg-gray-700/50 rounded-lg p-4 max-w-md mx-auto text-left mb-6">
            <h3 class="font-semibold mb-2">Installation Results</h3>
            <div class="space-y-1 text-sm">
              {#each Object.entries(installProgress) as [id, progress]}
                <div class="flex items-center gap-2">
                  {#if progress.status === "complete"}
                    <span class="text-green-400">✓</span>
                  {:else}
                    <span class="text-red-400">✗</span>
                  {/if}
                  <span>{id}</span>
                </div>
              {/each}
            </div>
          </div>
        {/if}

        <a href="/" class="btn btn-primary px-8">
          Go to Dashboard
        </a>
      </div>
    {/if}
  </div>
{/if}

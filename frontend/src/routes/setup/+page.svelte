<script>
  import { onMount } from "svelte";
  import { goto } from "$app/navigation";

  // State
  let currentStep = $state(1);
  let loading = $state(true);
  let error = $state(null);

  // Data
  let interfaces = $state([]);
  let coreServices = $state([]);
  let features = $state([]);

  // Installation state
  let installingCore = $state(false);
  let coreInstallIndex = $state(0);
  let installingFeature = $state(null);
  let selectedFeatures = $state(new Set());

  // Form data
  let adminForm = $state({
    username: "",
    password: "",
    confirmPassword: ""
  });

  let networkForm = $state({
    wan_interface: "",
    lan_interface: "",
    wifi_interface: ""
  });

  const steps = [
    { num: 1, title: "Welcome" },
    { num: 2, title: "Admin" },
    { num: 3, title: "Network" },
    { num: 4, title: "Core Services" },
    { num: 5, title: "Features" },
    { num: 6, title: "Complete" }
  ];

  onMount(async () => {
    try {
      const statusRes = await fetch("/api/setup/status");
      const status = await statusRes.json();

      if (status.is_complete) {
        goto("/");
        return;
      }

      // Load all data upfront
      const [ifaceRes, coreRes, featuresRes] = await Promise.all([
        fetch("/api/setup/interfaces"),
        fetch("/api/setup/core-services"),
        fetch("/api/setup/features")
      ]);

      if (ifaceRes.ok) {
        interfaces = await ifaceRes.json();
        // Auto-select interfaces
        const wan = interfaces.find(i => i.name.includes("enp0s3") || i.name.includes("enp1") || i.name.includes("eth0"));
        const lan = interfaces.find(i => i.name.includes("enp0s8") || i.name.includes("enp2") || i.name.includes("eth1"));
        const wifi = interfaces.find(i => i.is_wireless);
        if (wan) networkForm.wan_interface = wan.name;
        if (lan) networkForm.lan_interface = lan.name;
        if (wifi) networkForm.wifi_interface = wifi.name;
      }

      if (coreRes.ok) {
        coreServices = await coreRes.json();
      }

      if (featuresRes.ok) {
        features = await featuresRes.json();
      }
    } catch (e) {
      error = e.message;
    } finally {
      loading = false;
    }
  });

  // Refresh features list (to update Docker status after install)
  async function refreshFeatures() {
    try {
      const res = await fetch("/api/setup/features");
      if (res.ok) {
        features = await res.json();
      }
    } catch (e) {
      console.error("Failed to refresh features:", e);
    }
  }

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

  // Install core services one by one
  async function installCoreServices() {
    installingCore = true;
    coreInstallIndex = 0;

    for (let i = 0; i < coreServices.length; i++) {
      const service = coreServices[i];
      if (service.is_installed) {
        coreInstallIndex = i + 1;
        continue;
      }

      coreInstallIndex = i;

      try {
        const res = await fetch("/api/setup/core-services/install", {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ id: service.id })
        });

        const result = await res.json();

        // Update service status
        coreServices[i] = {
          ...service,
          is_installed: result.success,
          is_running: result.success,
          message: result.message
        };
        coreServices = [...coreServices]; // trigger reactivity
      } catch (e) {
        coreServices[i] = {
          ...service,
          is_installed: false,
          message: e.message
        };
        coreServices = [...coreServices];
      }

      coreInstallIndex = i + 1;
    }

    installingCore = false;
    // Auto-advance after a short delay
    setTimeout(() => {
      currentStep = 5;
    }, 1500);
  }

  // Install a single optional feature
  async function installFeature(featureId) {
    installingFeature = featureId;
    error = null;

    try {
      const res = await fetch("/api/setup/features/install", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ id: featureId })
      });

      const result = await res.json();

      // Update feature status
      const idx = features.findIndex(f => f.id === featureId);
      if (idx !== -1) {
        features[idx] = {
          ...features[idx],
          is_installed: result.success,
          is_running: result.success,
          message: result.message
        };
        features = [...features];
      }

      // If Docker was installed, refresh to enable dependent features
      if (featureId === "docker" && result.success) {
        await refreshFeatures();
      }

      if (!result.success) {
        error = result.message;
      }
    } catch (e) {
      error = e.message;
    } finally {
      installingFeature = null;
    }
  }

  // Install all selected features
  async function installAllSelected() {
    const toInstall = Array.from(selectedFeatures);

    // If any Docker-dependent features are selected, ensure Docker is first
    const dockerDependent = ["gluetun", "radarr", "sonarr", "jellyfin", "transmission"];
    const needsDocker = toInstall.some(id => dockerDependent.includes(id));
    const dockerFeature = features.find(f => f.id === "docker");

    if (needsDocker && dockerFeature && !dockerFeature.is_installed && !toInstall.includes("docker")) {
      toInstall.unshift("docker");
    }

    // Install in order
    for (const featureId of toInstall) {
      const feature = features.find(f => f.id === featureId);
      if (feature && !feature.is_installed) {
        await installFeature(featureId);
      }
      selectedFeatures.delete(featureId);
      selectedFeatures = new Set(selectedFeatures);
    }
  }

  function toggleFeatureSelection(featureId) {
    if (selectedFeatures.has(featureId)) {
      selectedFeatures.delete(featureId);
    } else {
      selectedFeatures.add(featureId);
    }
    selectedFeatures = new Set(selectedFeatures);
  }

  async function completeSetup() {
    try {
      await fetch("/api/setup/complete", { method: "POST" });
      currentStep = 6;
    } catch (e) {
      error = e.message;
    }
  }

  // Check if Docker is installed (for enabling dependent features)
  let dockerInstalled = $derived(features.find(f => f.id === "docker")?.is_installed ?? false);
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
  <div class="flex items-center justify-center mb-8 overflow-x-auto">
    {#each steps as step, i}
      <div class="flex items-center">
        <div class="flex flex-col items-center">
          <div
            class="w-8 h-8 rounded-full flex items-center justify-center font-bold text-sm transition-colors
              {currentStep >= step.num ? 'bg-blue-600 text-white' : 'bg-gray-700 text-gray-400'}"
          >
            {#if currentStep > step.num}
              <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7"></path>
              </svg>
            {:else}
              {step.num}
            {/if}
          </div>
          <span class="text-xs mt-1 {currentStep >= step.num ? 'text-blue-400' : 'text-gray-500'}">{step.title}</span>
        </div>
        {#if i < steps.length - 1}
          <div class="w-8 h-0.5 mx-1 {currentStep > step.num ? 'bg-blue-600' : 'bg-gray-700'}"></div>
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

  <div class="card">
    {#if currentStep === 1}
      <!-- Welcome -->
      <div class="text-center py-8">
        <div class="text-6xl mb-4">🌐</div>
        <h2 class="text-2xl font-bold mb-4">Welcome to RouterUI</h2>
        <p class="text-gray-400 mb-6 max-w-lg mx-auto">
          This wizard will set up your router with core services and optional features.
        </p>
        <div class="bg-gray-700/50 rounded-lg p-4 max-w-md mx-auto text-left mb-6">
          <h3 class="font-semibold mb-2">Setup Steps:</h3>
          <ul class="text-sm text-gray-400 space-y-1">
            <li>1. Create administrator account</li>
            <li>2. Configure network interfaces</li>
            <li>3. Install core router services (DHCP, Firewall)</li>
            <li>4. Choose optional features (Docker, AdGuard, etc.)</li>
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
      </div>

      <div class="flex justify-between mt-8">
        <button onclick={() => currentStep = 1} class="btn bg-gray-700 hover:bg-gray-600">Back</button>
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
      <p class="text-gray-400 mb-6">Select your network interfaces.</p>

      <div class="space-y-4 max-w-md">
        <div>
          <label class="block text-sm font-medium mb-1">WAN Interface (Internet)</label>
          <select bind:value={networkForm.wan_interface} class="w-full bg-gray-700 border border-gray-600 rounded-lg px-4 py-2">
            <option value="">Select interface...</option>
            {#each interfaces as iface}
              <option value={iface.name}>{iface.name} {iface.ip ? `(${iface.ip})` : ""}</option>
            {/each}
          </select>
        </div>
        <div>
          <label class="block text-sm font-medium mb-1">LAN Interface (Local Network)</label>
          <select bind:value={networkForm.lan_interface} class="w-full bg-gray-700 border border-gray-600 rounded-lg px-4 py-2">
            <option value="">Select interface...</option>
            {#each interfaces as iface}
              <option value={iface.name}>{iface.name} {iface.ip ? `(${iface.ip})` : ""}</option>
            {/each}
          </select>
        </div>
        <div>
          <label class="block text-sm font-medium mb-1">WiFi Interface (optional)</label>
          <select bind:value={networkForm.wifi_interface} class="w-full bg-gray-700 border border-gray-600 rounded-lg px-4 py-2">
            <option value="">None</option>
            {#each interfaces.filter(i => i.is_wireless) as iface}
              <option value={iface.name}>{iface.name}</option>
            {/each}
          </select>
        </div>
      </div>

      <div class="flex justify-between mt-8">
        <button onclick={() => currentStep = 2} class="btn bg-gray-700 hover:bg-gray-600">Back</button>
        <button onclick={saveNetwork} disabled={loading} class="btn btn-primary disabled:opacity-50">
          {loading ? "Saving..." : "Continue"}
        </button>
      </div>

    {:else if currentStep === 4}
      <!-- Core Services Installation -->
      <h2 class="text-xl font-bold mb-4">Installing Core Services</h2>
      <p class="text-gray-400 mb-6">Setting up essential router components...</p>

      <div class="space-y-4 max-w-lg">
        {#each coreServices as service, i}
          <div class="flex items-center gap-4 p-4 bg-gray-700/50 rounded-lg">
            <div class="w-8 h-8 flex items-center justify-center">
              {#if service.is_installed || (installingCore && i < coreInstallIndex)}
                <span class="text-green-400 text-xl">✓</span>
              {:else if installingCore && i === coreInstallIndex}
                <div class="animate-spin rounded-full h-6 w-6 border-b-2 border-blue-400"></div>
              {:else}
                <span class="text-gray-500 text-xl">○</span>
              {/if}
            </div>
            <div class="flex-1">
              <div class="font-medium">{service.name}</div>
              <div class="text-sm text-gray-400">{service.description}</div>
              {#if service.message}
                <div class="text-xs text-blue-400 mt-1">{service.message}</div>
              {/if}
            </div>
          </div>
        {/each}
      </div>

      <div class="flex justify-between mt-8">
        <button onclick={() => currentStep = 3} class="btn bg-gray-700 hover:bg-gray-600" disabled={installingCore}>Back</button>
        {#if !installingCore && coreServices.every(s => s.is_installed)}
          <button onclick={() => currentStep = 5} class="btn btn-primary">Continue</button>
        {:else if !installingCore}
          <button onclick={installCoreServices} class="btn btn-primary">
            Install Core Services
          </button>
        {:else}
          <button disabled class="btn btn-primary opacity-50">Installing...</button>
        {/if}
      </div>

    {:else if currentStep === 5}
      <!-- Optional Features -->
      <h2 class="text-xl font-bold mb-4">Optional Features</h2>
      <p class="text-gray-400 mb-6">Install additional features. You can install one at a time or select multiple.</p>

      <div class="space-y-6">
        <!-- Non-Docker Features -->
        <div>
          <h3 class="text-sm font-semibold text-gray-400 mb-3 uppercase tracking-wide">Available Features</h3>
          <div class="space-y-2">
            {#each features.filter(f => !f.requires_docker) as feature}
              <div class="flex items-center gap-3 p-3 bg-gray-700/50 rounded-lg">
                <input
                  type="checkbox"
                  checked={selectedFeatures.has(feature.id)}
                  onchange={() => toggleFeatureSelection(feature.id)}
                  disabled={feature.is_installed || installingFeature}
                  class="rounded bg-gray-600 border-gray-500 w-5 h-5"
                />
                <div class="flex-1">
                  <div class="flex items-center gap-2">
                    <span class="font-medium">{feature.name}</span>
                    {#if feature.is_installed}
                      <span class="text-xs bg-green-600/30 text-green-400 px-2 rounded">Installed</span>
                    {/if}
                    <span class="text-xs text-gray-500">{feature.category}</span>
                  </div>
                  <div class="text-sm text-gray-400">{feature.description}</div>
                  {#if feature.message}
                    <div class="text-xs text-blue-400 mt-1">{feature.message}</div>
                  {/if}
                </div>
                <button
                  onclick={() => installFeature(feature.id)}
                  disabled={feature.is_installed || installingFeature !== null}
                  class="btn btn-primary text-sm px-4 py-1 disabled:opacity-50"
                >
                  {#if installingFeature === feature.id}
                    <span class="animate-spin">⏳</span>
                  {:else if feature.is_installed}
                    ✓
                  {:else}
                    Install
                  {/if}
                </button>
              </div>
            {/each}
          </div>
        </div>

        <!-- Docker-dependent Features -->
        <div>
          <h3 class="text-sm font-semibold text-gray-400 mb-3 uppercase tracking-wide">
            Requires Docker
            {#if !dockerInstalled}
              <span class="text-yellow-400 normal-case font-normal">(install Docker first)</span>
            {/if}
          </h3>
          <div class="space-y-2">
            {#each features.filter(f => f.requires_docker) as feature}
              <div class="flex items-center gap-3 p-3 bg-gray-700/50 rounded-lg {!dockerInstalled ? 'opacity-50' : ''}">
                <input
                  type="checkbox"
                  checked={selectedFeatures.has(feature.id)}
                  onchange={() => toggleFeatureSelection(feature.id)}
                  disabled={!dockerInstalled || feature.is_installed || installingFeature}
                  class="rounded bg-gray-600 border-gray-500 w-5 h-5"
                />
                <div class="flex-1">
                  <div class="flex items-center gap-2">
                    <span class="font-medium">{feature.name}</span>
                    {#if feature.is_installed}
                      <span class="text-xs bg-green-600/30 text-green-400 px-2 rounded">Installed</span>
                    {/if}
                    <span class="text-xs text-gray-500">{feature.category}</span>
                  </div>
                  <div class="text-sm text-gray-400">{feature.description}</div>
                  {#if feature.message}
                    <div class="text-xs text-blue-400 mt-1">{feature.message}</div>
                  {/if}
                </div>
                <button
                  onclick={() => installFeature(feature.id)}
                  disabled={!dockerInstalled || feature.is_installed || installingFeature !== null}
                  class="btn btn-primary text-sm px-4 py-1 disabled:opacity-50"
                >
                  {#if installingFeature === feature.id}
                    <span class="animate-spin">⏳</span>
                  {:else if feature.is_installed}
                    ✓
                  {:else}
                    Install
                  {/if}
                </button>
              </div>
            {/each}
          </div>
        </div>
      </div>

      <div class="flex justify-between mt-8 gap-4">
        <button onclick={() => currentStep = 4} class="btn bg-gray-700 hover:bg-gray-600" disabled={installingFeature}>Back</button>
        <div class="flex gap-2">
          {#if selectedFeatures.size > 0}
            <button
              onclick={installAllSelected}
              disabled={installingFeature !== null}
              class="btn bg-purple-600 hover:bg-purple-700 disabled:opacity-50"
            >
              Install Selected ({selectedFeatures.size})
            </button>
          {/if}
          <button
            onclick={completeSetup}
            disabled={installingFeature !== null}
            class="btn btn-primary disabled:opacity-50"
          >
            Finish Setup
          </button>
        </div>
      </div>

    {:else if currentStep === 6}
      <!-- Complete -->
      <div class="text-center py-8">
        <div class="text-6xl mb-4">🎉</div>
        <h2 class="text-2xl font-bold mb-4 text-green-400">Setup Complete!</h2>
        <p class="text-gray-400 mb-6 max-w-lg mx-auto">
          RouterUI has been configured. You can now log in with your admin account.
        </p>

        <div class="bg-gray-700/50 rounded-lg p-4 max-w-md mx-auto text-left mb-6">
          <h3 class="font-semibold mb-2">Installed Services:</h3>
          <div class="space-y-1 text-sm">
            {#each coreServices.filter(s => s.is_installed) as service}
              <div class="flex items-center gap-2">
                <span class="text-green-400">✓</span>
                <span>{service.name}</span>
              </div>
            {/each}
            {#each features.filter(f => f.is_installed) as feature}
              <div class="flex items-center gap-2">
                <span class="text-green-400">✓</span>
                <span>{feature.name}</span>
              </div>
            {/each}
          </div>
        </div>

        <a href="/" class="btn btn-primary px-8">Go to Dashboard</a>
      </div>
    {/if}
  </div>
{/if}

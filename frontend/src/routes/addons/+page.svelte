<script>
  import { onMount } from 'svelte';

  let addons = $state([]);
  let loading = $state(true);
  let installing = $state(null);
  let installResult = $state(null);

  onMount(async () => {
    await fetchAddons();
  });

  async function fetchAddons() {
    loading = true;
    try {
      const res = await fetch('/api/addons/list');
      if (res.ok) {
        addons = await res.json();
      }
    } catch (e) {
      console.error('Failed to fetch addons:', e);
    }
    loading = false;
  }

  async function installAddon(id) {
    installing = id;
    installResult = null;
    try {
      const res = await fetch('/api/addons/install', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ id })
      });
      const result = await res.json();
      installResult = { id, ...result };
      // Refresh addon list to update status
      await fetchAddons();
    } catch (e) {
      installResult = { id, success: false, message: e.message };
    }
    installing = null;
  }
</script>

<div class="space-y-6">
  <div class="flex items-center justify-between">
    <div>
      <h1 class="text-2xl font-bold">Add-ons</h1>
      <p class="text-gray-400 mt-1">Install optional services to extend your router's functionality</p>
    </div>
    <button
      onclick={fetchAddons}
      class="px-4 py-2 bg-gray-700 hover:bg-gray-600 rounded-lg transition-colors"
      disabled={loading}
    >
      Refresh
    </button>
  </div>

  {#if installResult}
    <div class="p-4 rounded-lg {installResult.success ? 'bg-green-900/50 border border-green-700' : 'bg-red-900/50 border border-red-700'}">
      <div class="flex items-center gap-2">
        <span>{installResult.success ? '✓' : '✗'}</span>
        <span class="font-medium">{installResult.success ? 'Installation Successful' : 'Installation Failed'}</span>
      </div>
      <p class="mt-1 text-sm text-gray-300">{installResult.message}</p>
    </div>
  {/if}

  {#if loading}
    <div class="flex items-center justify-center py-12">
      <div class="animate-spin rounded-full h-8 w-8 border-b-2 border-blue-400"></div>
    </div>
  {:else}
    <div class="grid gap-4 md:grid-cols-2 lg:grid-cols-3">
      {#each addons as addon}
        <div class="bg-gray-800 rounded-lg p-5 border border-gray-700">
          <div class="flex items-start justify-between">
            <div>
              <h3 class="font-semibold text-lg">{addon.name}</h3>
              <p class="text-sm text-gray-400 mt-1">{addon.description}</p>
            </div>
            {#if addon.status.running}
              <span class="px-2 py-1 text-xs bg-green-900/50 text-green-400 rounded-full">Running</span>
            {:else if addon.status.installed}
              <span class="px-2 py-1 text-xs bg-yellow-900/50 text-yellow-400 rounded-full">Installed</span>
            {:else}
              <span class="px-2 py-1 text-xs bg-gray-700 text-gray-400 rounded-full">Not Installed</span>
            {/if}
          </div>

          <div class="mt-4 pt-4 border-t border-gray-700">
            {#if addon.status.installed}
              <div class="flex items-center gap-2 text-sm text-gray-400">
                <span class="w-2 h-2 rounded-full {addon.status.running ? 'bg-green-400' : 'bg-yellow-400'}"></span>
                <span>{addon.status.running ? 'Service is active' : 'Service is stopped'}</span>
              </div>
            {:else if addon.install_command}
              <button
                onclick={() => installAddon(addon.id)}
                disabled={installing !== null}
                class="w-full px-4 py-2 bg-blue-600 hover:bg-blue-500 disabled:bg-gray-600 disabled:cursor-not-allowed rounded-lg transition-colors flex items-center justify-center gap-2"
              >
                {#if installing === addon.id}
                  <div class="animate-spin rounded-full h-4 w-4 border-b-2 border-white"></div>
                  <span>Installing...</span>
                {:else}
                  <span>Install</span>
                {/if}
              </button>
            {:else}
              <p class="text-sm text-gray-500">Requires Docker - install Docker first</p>
            {/if}
          </div>
        </div>
      {/each}
    </div>

    {#if addons.length === 0}
      <div class="text-center py-12 text-gray-400">
        <p>No add-ons available</p>
      </div>
    {/if}
  {/if}

  <div class="bg-gray-800/50 rounded-lg p-4 border border-gray-700">
    <h3 class="font-medium mb-2">About Add-ons</h3>
    <ul class="text-sm text-gray-400 space-y-1">
      <li>- Add-ons extend your router with additional functionality</li>
      <li>- Once installed and running, add-ons will appear in the sidebar</li>
      <li>- Some add-ons require Docker to be installed first</li>
      <li>- Installation may take several minutes depending on your connection</li>
    </ul>
  </div>
</div>

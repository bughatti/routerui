<script>
  import { onMount } from "svelte";

  let loading = $state(true);
  let activeTab = $state("containers");
  let status = $state(null);
  let containers = $state([]);
  let images = $state([]);
  let volumes = $state([]);
  let networks = $state([]);

  let selectedContainer = $state(null);
  let containerLogs = $state("");
  let loadingLogs = $state(false);
  let actionInProgress = $state(null);

  let pullImageName = $state("");
  let pulling = $state(false);

  async function fetchData() {
    try {
      const [statusRes, containersRes, imagesRes, volumesRes, networksRes] = await Promise.all([
        fetch("/api/docker/status"),
        fetch("/api/docker/containers"),
        fetch("/api/docker/images"),
        fetch("/api/docker/volumes"),
        fetch("/api/docker/networks")
      ]);

      if (statusRes.ok) status = await statusRes.json();
      if (containersRes.ok) containers = await containersRes.json();
      if (imagesRes.ok) images = await imagesRes.json();
      if (volumesRes.ok) volumes = await volumesRes.json();
      if (networksRes.ok) networks = await networksRes.json();
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

  async function containerAction(id, action) {
    actionInProgress = `${id}-${action}`;
    try {
      const res = await fetch("/api/docker/containers/action", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ id, action })
      });
      if (res.ok) {
        await new Promise(r => setTimeout(r, 1000));
        await fetchData();
      }
    } finally {
      actionInProgress = null;
    }
  }

  async function viewLogs(container) {
    selectedContainer = container;
    loadingLogs = true;
    containerLogs = "";
    try {
      const res = await fetch("/api/docker/containers/logs", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ id: container.id, lines: 200 })
      });
      if (res.ok) {
        const data = await res.json();
        containerLogs = data.logs;
      }
    } finally {
      loadingLogs = false;
    }
  }

  function closeLogs() {
    selectedContainer = null;
    containerLogs = "";
  }

  async function removeImage(id) {
    if (!confirm("Are you sure you want to remove this image?")) return;
    const res = await fetch("/api/docker/images/action", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ id, action: "remove" })
    });
    if (res.ok) {
      await fetchData();
    }
  }

  async function pullImage() {
    if (!pullImageName) return;
    pulling = true;
    try {
      const res = await fetch("/api/docker/images/pull", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ image: pullImageName })
      });
      if (res.ok) {
        pullImageName = "";
        await fetchData();
      }
    } finally {
      pulling = false;
    }
  }

  function getStateColor(state) {
    switch (state) {
      case "running": return "bg-green-500";
      case "exited": return "bg-gray-500";
      case "paused": return "bg-yellow-500";
      case "restarting": return "bg-blue-500";
      default: return "bg-gray-500";
    }
  }
</script>

<svelte:head>
  <title>Docker - RouterUI</title>
</svelte:head>

<div class="space-y-6">
  <div class="flex items-center justify-between">
    <div>
      <h2 class="text-2xl font-bold">Docker</h2>
      <p class="text-sm text-gray-500">Manage containers, images, and volumes.</p>
    </div>
  </div>

  {#if loading}
    <div class="text-gray-400">Loading...</div>
  {:else if !status?.installed}
    <div class="card bg-red-900/20 border-red-700">
      <p class="text-red-400">Docker is not installed.</p>
    </div>
  {:else if !status?.running}
    <div class="card bg-yellow-900/20 border-yellow-700">
      <p class="text-yellow-400">Docker is installed but not running. Start it from the Services tab.</p>
    </div>
  {:else}
    <!-- Status Overview -->
    <div class="card">
      <div class="grid grid-cols-2 md:grid-cols-5 gap-4">
        <div class="bg-gray-700/50 rounded p-3 text-center">
          <p class="text-xl font-bold text-blue-400">v{status.version}</p>
          <p class="text-xs text-gray-400">Docker Version</p>
        </div>
        <div class="bg-gray-700/50 rounded p-3 text-center">
          <p class="text-2xl font-bold text-green-400">{status.containers_running}</p>
          <p class="text-xs text-gray-400">Running</p>
        </div>
        <div class="bg-gray-700/50 rounded p-3 text-center">
          <p class="text-2xl font-bold text-gray-400">{status.containers_stopped}</p>
          <p class="text-xs text-gray-400">Stopped</p>
        </div>
        <div class="bg-gray-700/50 rounded p-3 text-center">
          <p class="text-2xl font-bold text-purple-400">{status.images_count}</p>
          <p class="text-xs text-gray-400">Images</p>
        </div>
        <div class="bg-gray-700/50 rounded p-3 text-center">
          <p class="text-2xl font-bold text-orange-400">{status.volumes_count}</p>
          <p class="text-xs text-gray-400">Volumes</p>
        </div>
      </div>
    </div>

    <!-- Tabs -->
    <div class="border-b border-gray-700">
      <nav class="flex gap-4">
        {#each [
          { id: "containers", label: "Containers", count: containers.length },
          { id: "images", label: "Images", count: images.length },
          { id: "volumes", label: "Volumes", count: volumes.length },
          { id: "networks", label: "Networks", count: networks.length }
        ] as tab}
          <button
            onclick={() => activeTab = tab.id}
            class="tab-btn {activeTab === tab.id ? 'tab-active' : ''}"
          >
            {tab.label}
            <span class="ml-1 text-xs text-gray-500">({tab.count})</span>
          </button>
        {/each}
      </nav>
    </div>

    <!-- Containers Tab -->
    {#if activeTab === "containers"}
      <div class="card">
        <h3 class="text-lg font-semibold mb-4">Containers</h3>

        {#if containers.length === 0}
          <p class="text-gray-500">No containers found.</p>
        {:else}
          <div class="space-y-3">
            {#each containers as container}
              <div class="p-4 bg-gray-700/50 rounded">
                <div class="flex items-center justify-between mb-2">
                  <div class="flex items-center gap-3">
                    <div class="w-3 h-3 rounded-full {getStateColor(container.state)}"></div>
                    <span class="font-medium">{container.name}</span>
                    <span class="text-xs text-gray-500">{container.id.slice(0, 12)}</span>
                  </div>
                  <div class="flex items-center gap-2">
                    {#if container.state === "running"}
                      <button
                        onclick={() => containerAction(container.id, 'restart')}
                        disabled={actionInProgress === `${container.id}-restart`}
                        class="btn-action btn-yellow"
                      >
                        Restart
                      </button>
                      <button
                        onclick={() => containerAction(container.id, 'stop')}
                        disabled={actionInProgress === `${container.id}-stop`}
                        class="btn-action btn-red"
                      >
                        Stop
                      </button>
                    {:else if container.state === "paused"}
                      <button
                        onclick={() => containerAction(container.id, 'unpause')}
                        disabled={actionInProgress === `${container.id}-unpause`}
                        class="btn-action btn-green"
                      >
                        Unpause
                      </button>
                    {:else}
                      <button
                        onclick={() => containerAction(container.id, 'start')}
                        disabled={actionInProgress === `${container.id}-start`}
                        class="btn-action btn-green"
                      >
                        Start
                      </button>
                    {/if}
                    <button
                      onclick={() => containerAction(container.id, 'remove')}
                      disabled={actionInProgress === `${container.id}-remove`}
                      class="btn-action btn-red"
                    >
                      Remove
                    </button>
                    <button onclick={() => viewLogs(container)} class="btn-action btn-gray">
                      Logs
                    </button>
                  </div>
                </div>

                <div class="grid grid-cols-2 md:grid-cols-4 gap-4 text-sm mt-2">
                  <div>
                    <span class="text-gray-400">Image:</span>
                    <span class="ml-1 text-blue-400">{container.image}</span>
                  </div>
                  <div>
                    <span class="text-gray-400">Status:</span>
                    <span class="ml-1">{container.status}</span>
                  </div>
                  {#if container.cpu_percent !== null}
                    <div>
                      <span class="text-gray-400">CPU:</span>
                      <span class="ml-1">{container.cpu_percent?.toFixed(1)}%</span>
                    </div>
                  {/if}
                  {#if container.memory_usage}
                    <div>
                      <span class="text-gray-400">Memory:</span>
                      <span class="ml-1">{container.memory_usage}</span>
                    </div>
                  {/if}
                </div>

                {#if container.ports.length > 0}
                  <div class="mt-2 text-sm">
                    <span class="text-gray-400">Ports:</span>
                    <span class="ml-1 font-mono text-xs">{container.ports.join(', ')}</span>
                  </div>
                {/if}
              </div>
            {/each}
          </div>
        {/if}
      </div>

    <!-- Images Tab -->
    {:else if activeTab === "images"}
      <div class="card">
        <h3 class="text-lg font-semibold mb-4">Images</h3>

        <!-- Pull Image -->
        <div class="flex gap-2 mb-4">
          <input
            type="text"
            placeholder="Image name (e.g., nginx:latest)"
            bind:value={pullImageName}
            class="input flex-1"
          />
          <button
            onclick={pullImage}
            disabled={pulling}
            class="btn-primary"
          >
            {pulling ? "Pulling..." : "Pull Image"}
          </button>
        </div>

        {#if images.length === 0}
          <p class="text-gray-500">No images found.</p>
        {:else}
          <div class="overflow-x-auto">
            <table class="w-full text-sm">
              <thead>
                <tr class="text-left text-gray-400 border-b border-gray-700">
                  <th class="pb-2">Repository</th>
                  <th class="pb-2">Tag</th>
                  <th class="pb-2">ID</th>
                  <th class="pb-2">Size</th>
                  <th class="pb-2">Created</th>
                  <th class="pb-2"></th>
                </tr>
              </thead>
              <tbody>
                {#each images as image}
                  <tr class="border-b border-gray-700/50">
                    <td class="py-2 text-blue-400">{image.repository}</td>
                    <td class="py-2">{image.tag}</td>
                    <td class="py-2 font-mono text-xs text-gray-500">{image.id.slice(0, 12)}</td>
                    <td class="py-2">{image.size}</td>
                    <td class="py-2 text-gray-400">{image.created}</td>
                    <td class="py-2">
                      <button
                        onclick={() => removeImage(image.id)}
                        class="text-red-400 hover:text-red-300 text-xs"
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
      </div>

    <!-- Volumes Tab -->
    {:else if activeTab === "volumes"}
      <div class="card">
        <h3 class="text-lg font-semibold mb-4">Volumes</h3>

        {#if volumes.length === 0}
          <p class="text-gray-500">No volumes found.</p>
        {:else}
          <div class="space-y-2">
            {#each volumes as volume}
              <div class="p-3 bg-gray-700/50 rounded">
                <div class="font-medium">{volume.name}</div>
                <div class="text-sm text-gray-400">
                  Driver: {volume.driver}
                </div>
              </div>
            {/each}
          </div>
        {/if}
      </div>

    <!-- Networks Tab -->
    {:else if activeTab === "networks"}
      <div class="card">
        <h3 class="text-lg font-semibold mb-4">Networks</h3>

        {#if networks.length === 0}
          <p class="text-gray-500">No networks found.</p>
        {:else}
          <div class="overflow-x-auto">
            <table class="w-full text-sm">
              <thead>
                <tr class="text-left text-gray-400 border-b border-gray-700">
                  <th class="pb-2">Name</th>
                  <th class="pb-2">Driver</th>
                  <th class="pb-2">Scope</th>
                  <th class="pb-2">ID</th>
                </tr>
              </thead>
              <tbody>
                {#each networks as network}
                  <tr class="border-b border-gray-700/50">
                    <td class="py-2 font-medium">{network.name}</td>
                    <td class="py-2">{network.driver}</td>
                    <td class="py-2">{network.scope}</td>
                    <td class="py-2 font-mono text-xs text-gray-500">{network.id.slice(0, 12)}</td>
                  </tr>
                {/each}
              </tbody>
            </table>
          </div>
        {/if}
      </div>
    {/if}
  {/if}
</div>

<!-- Logs Modal -->
{#if selectedContainer}
  <div class="fixed inset-0 bg-black/70 flex items-center justify-center z-50 p-4" onclick={closeLogs}>
    <div class="bg-gray-800 rounded-lg w-full max-w-4xl max-h-[80vh] flex flex-col" onclick={(e) => e.stopPropagation()}>
      <div class="flex items-center justify-between p-4 border-b border-gray-700">
        <h3 class="text-lg font-semibold">
          Logs: {selectedContainer.name}
        </h3>
        <button onclick={closeLogs} class="text-gray-400 hover:text-white text-2xl">&times;</button>
      </div>
      <div class="flex-1 overflow-auto p-4">
        {#if loadingLogs}
          <div class="text-gray-400">Loading logs...</div>
        {:else if containerLogs}
          <pre class="text-xs font-mono text-gray-300 whitespace-pre-wrap">{containerLogs}</pre>
        {:else}
          <p class="text-gray-500">No logs available.</p>
        {/if}
      </div>
      <div class="p-4 border-t border-gray-700 flex justify-end">
        <button onclick={closeLogs} class="btn-primary">Close</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .tab-btn {
    padding: 0.75rem 1rem;
    border-bottom: 2px solid transparent;
    color: #9ca3af;
    transition: all 0.15s;
  }

  .tab-btn:hover {
    color: #f3f4f6;
  }

  .tab-active {
    color: #60a5fa;
    border-bottom-color: #60a5fa;
  }

  .btn-action {
    padding: 0.25rem 0.75rem;
    border-radius: 0.375rem;
    font-size: 0.75rem;
    font-weight: 500;
    transition: all 0.15s;
  }

  .btn-action:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .btn-green {
    background: rgba(34, 197, 94, 0.2);
    color: #22c55e;
  }
  .btn-green:hover:not(:disabled) {
    background: rgba(34, 197, 94, 0.3);
  }

  .btn-red {
    background: rgba(239, 68, 68, 0.2);
    color: #ef4444;
  }
  .btn-red:hover:not(:disabled) {
    background: rgba(239, 68, 68, 0.3);
  }

  .btn-yellow {
    background: rgba(234, 179, 8, 0.2);
    color: #eab308;
  }
  .btn-yellow:hover:not(:disabled) {
    background: rgba(234, 179, 8, 0.3);
  }

  .btn-gray {
    background: rgba(107, 114, 128, 0.2);
    color: #9ca3af;
  }
  .btn-gray:hover:not(:disabled) {
    background: rgba(107, 114, 128, 0.3);
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

  .btn-primary:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
</style>

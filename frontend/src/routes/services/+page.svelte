<script>
  import { onMount } from "svelte";

  let loading = $state(true);
  let services = $state([]);
  let totalRunning = $state(0);
  let totalFailed = $state(0);
  let showAllServices = $state(false);
  let selectedService = $state(null);
  let serviceLogs = $state("");
  let loadingLogs = $state(false);
  let actionInProgress = $state(null);

  async function fetchServices() {
    try {
      const endpoint = showAllServices ? "/api/services/all" : "/api/services";
      const res = await fetch(endpoint);
      if (res.ok) {
        const data = await res.json();
        services = data.services;
        totalRunning = data.total_running;
        totalFailed = data.total_failed;
      }
    } catch (e) {
      console.error(e);
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    fetchServices();
    const interval = setInterval(fetchServices, 10000);
    return () => clearInterval(interval);
  });

  async function performAction(serviceName, action) {
    actionInProgress = `${serviceName}-${action}`;
    try {
      const res = await fetch("/api/services/action", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ name: serviceName, action })
      });
      if (res.ok) {
        // Wait a moment for service to change state
        await new Promise(r => setTimeout(r, 1000));
        await fetchServices();
      }
    } finally {
      actionInProgress = null;
    }
  }

  async function viewLogs(service) {
    selectedService = service;
    loadingLogs = true;
    serviceLogs = "";
    try {
      const res = await fetch("/api/services/logs", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ name: service.name, lines: 100 })
      });
      if (res.ok) {
        const data = await res.json();
        serviceLogs = data.logs;
      }
    } finally {
      loadingLogs = false;
    }
  }

  function closeLogs() {
    selectedService = null;
    serviceLogs = "";
  }

  function getStatusColor(status) {
    switch (status) {
      case "active": return "text-green-400";
      case "inactive": return "text-gray-400";
      case "failed": return "text-red-400";
      default: return "text-yellow-400";
    }
  }

  function getStatusBg(status) {
    switch (status) {
      case "active": return "bg-green-500/20";
      case "inactive": return "bg-gray-500/20";
      case "failed": return "bg-red-500/20";
      default: return "bg-yellow-500/20";
    }
  }
</script>

<svelte:head>
  <title>Services - RouterUI</title>
</svelte:head>

<div class="space-y-6">
  <div class="flex items-center justify-between">
    <div>
      <h2 class="text-2xl font-bold">Services</h2>
      <p class="text-sm text-gray-500">Manage system services and daemons.</p>
    </div>
    <label class="flex items-center gap-2 text-sm">
      <input type="checkbox" bind:checked={showAllServices} onchange={fetchServices} class="rounded" />
      <span>Show all services</span>
    </label>
  </div>

  {#if loading}
    <div class="text-gray-400">Loading...</div>
  {:else}
    <!-- Status Summary -->
    <div class="card">
      <div class="grid grid-cols-3 gap-4">
        <div class="bg-gray-700/50 rounded p-3 text-center">
          <p class="text-2xl font-bold">{services.length}</p>
          <p class="text-xs text-gray-400">Total Services</p>
        </div>
        <div class="bg-gray-700/50 rounded p-3 text-center">
          <p class="text-2xl font-bold text-green-400">{totalRunning}</p>
          <p class="text-xs text-gray-400">Running</p>
        </div>
        <div class="bg-gray-700/50 rounded p-3 text-center">
          <p class="text-2xl font-bold text-red-400">{totalFailed}</p>
          <p class="text-xs text-gray-400">Failed</p>
        </div>
      </div>
    </div>

    <!-- Services List -->
    <div class="card">
      <h3 class="text-lg font-semibold mb-4">
        {showAllServices ? "All Running Services" : "Router Services"}
      </h3>

      <div class="space-y-3">
        {#each services as service}
          <div class="p-4 bg-gray-700/50 rounded">
            <div class="flex items-center justify-between mb-2">
              <div class="flex items-center gap-3">
                <div class="w-3 h-3 rounded-full {service.is_running ? 'bg-green-500' : service.status === 'failed' ? 'bg-red-500' : 'bg-gray-500'}"></div>
                <div>
                  <span class="font-medium">{service.display_name}</span>
                  {#if service.display_name !== service.name}
                    <span class="text-gray-500 text-sm ml-2">({service.name})</span>
                  {/if}
                </div>
                <span class="text-xs px-2 py-0.5 rounded {getStatusBg(service.status)} {getStatusColor(service.status)}">
                  {service.status}
                </span>
                {#if service.is_enabled}
                  <span class="text-xs px-2 py-0.5 bg-blue-500/20 text-blue-400 rounded">auto-start</span>
                {/if}
              </div>

              <div class="flex items-center gap-2">
                <!-- Action Buttons -->
                {#if service.is_running}
                  <button
                    onclick={() => performAction(service.name, 'restart')}
                    disabled={actionInProgress === `${service.name}-restart`}
                    class="btn-action btn-yellow"
                  >
                    {actionInProgress === `${service.name}-restart` ? "..." : "Restart"}
                  </button>
                  <button
                    onclick={() => performAction(service.name, 'stop')}
                    disabled={actionInProgress === `${service.name}-stop`}
                    class="btn-action btn-red"
                  >
                    {actionInProgress === `${service.name}-stop` ? "..." : "Stop"}
                  </button>
                {:else}
                  <button
                    onclick={() => performAction(service.name, 'start')}
                    disabled={actionInProgress === `${service.name}-start`}
                    class="btn-action btn-green"
                  >
                    {actionInProgress === `${service.name}-start` ? "..." : "Start"}
                  </button>
                {/if}

                <!-- Enable/Disable -->
                {#if service.is_enabled}
                  <button
                    onclick={() => performAction(service.name, 'disable')}
                    disabled={actionInProgress === `${service.name}-disable`}
                    class="btn-action btn-gray"
                    title="Disable auto-start"
                  >
                    Disable
                  </button>
                {:else}
                  <button
                    onclick={() => performAction(service.name, 'enable')}
                    disabled={actionInProgress === `${service.name}-enable`}
                    class="btn-action btn-blue"
                    title="Enable auto-start"
                  >
                    Enable
                  </button>
                {/if}

                <!-- Logs -->
                <button
                  onclick={() => viewLogs(service)}
                  class="btn-action btn-gray"
                >
                  Logs
                </button>
              </div>
            </div>

            {#if service.description}
              <p class="text-sm text-gray-400 ml-6">{service.description}</p>
            {/if}

            <div class="flex gap-4 text-xs text-gray-500 mt-2 ml-6">
              {#if service.pid}
                <span>PID: {service.pid}</span>
              {/if}
              {#if service.memory}
                <span>Memory: {service.memory}</span>
              {/if}
              {#if service.uptime && service.is_running}
                <span>Started: {service.uptime}</span>
              {/if}
            </div>
          </div>
        {/each}
      </div>
    </div>
  {/if}
</div>

<!-- Logs Modal -->
{#if selectedService}
  <div class="fixed inset-0 bg-black/70 flex items-center justify-center z-50 p-4" onclick={closeLogs}>
    <div class="bg-gray-800 rounded-lg w-full max-w-4xl max-h-[80vh] flex flex-col" onclick={(e) => e.stopPropagation()}>
      <div class="flex items-center justify-between p-4 border-b border-gray-700">
        <h3 class="text-lg font-semibold">
          Logs: {selectedService.display_name}
        </h3>
        <button onclick={closeLogs} class="text-gray-400 hover:text-white text-2xl">&times;</button>
      </div>
      <div class="flex-1 overflow-auto p-4">
        {#if loadingLogs}
          <div class="text-gray-400">Loading logs...</div>
        {:else if serviceLogs}
          <pre class="text-xs font-mono text-gray-300 whitespace-pre-wrap">{serviceLogs}</pre>
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

  .btn-blue {
    background: rgba(59, 130, 246, 0.2);
    color: #3b82f6;
  }
  .btn-blue:hover:not(:disabled) {
    background: rgba(59, 130, 246, 0.3);
  }

  .btn-gray {
    background: rgba(107, 114, 128, 0.2);
    color: #9ca3af;
  }
  .btn-gray:hover:not(:disabled) {
    background: rgba(107, 114, 128, 0.3);
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
</style>

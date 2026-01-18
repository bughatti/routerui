<script>
  import { onMount } from "svelte";

  let loading = $state(true);
  let data = $state(null);
  let error = $state(null);

  async function fetchData() {
    try {
      const res = await fetch("/api/media/overview");
      if (res.ok) {
        data = await res.json();
      } else {
        error = "Failed to load media data";
      }
    } catch (e) {
      error = e.message;
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    fetchData();
    const interval = setInterval(fetchData, 30000);
    return () => clearInterval(interval);
  });

  function formatGB(gb) {
    if (gb >= 1000) return (gb / 1000).toFixed(2) + " TB";
    return gb.toFixed(1) + " GB";
  }
</script>

<svelte:head>
  <title>Media - RouterUI</title>
</svelte:head>

<div class="space-y-6">
  <div class="flex items-center justify-between">
    <div>
      <h2 class="text-2xl font-bold">Media Center</h2>
      <p class="text-sm text-gray-500">Storage, library stats, and recent downloads</p>
    </div>
    <button onclick={fetchData} class="text-sm text-blue-400 hover:text-blue-300">
      Refresh
    </button>
  </div>

  {#if loading}
    <div class="text-gray-400">Loading media data...</div>
  {:else if error}
    <div class="card bg-red-900/20 border-red-700 text-red-400">{error}</div>
  {:else if data}
    <!-- Storage Card -->
    <div class="card">
      <h3 class="text-lg font-semibold mb-4">External Storage</h3>
      <div class="mb-2 flex justify-between text-sm">
        <span>Used: {formatGB(data.storage.used_gb)}</span>
        <span>Free: {formatGB(data.storage.free_gb)}</span>
        <span>Total: {formatGB(data.storage.total_gb)}</span>
      </div>
      <div class="w-full bg-gray-700 rounded-full h-4">
        <div
          class="h-4 rounded-full {data.storage.percent_used > 90 ? 'bg-red-500' : data.storage.percent_used > 70 ? 'bg-yellow-500' : 'bg-blue-500'}"
          style="width: {data.storage.percent_used}%"
        ></div>
      </div>
      <p class="text-xs text-gray-500 mt-2">{data.storage.mount_point} - {data.storage.percent_used.toFixed(1)}% used</p>
    </div>

    <!-- Stats Grid -->
    <div class="grid grid-cols-2 md:grid-cols-4 gap-4">
      <div class="card text-center">
        <p class="text-3xl font-bold text-blue-400">{data.library.movies}</p>
        <p class="text-sm text-gray-400">Movies</p>
      </div>
      <div class="card text-center">
        <p class="text-3xl font-bold text-purple-400">{data.library.tv_shows}</p>
        <p class="text-sm text-gray-400">TV Shows</p>
      </div>
      {#if data.jellyfin}
        <div class="card text-center">
          <p class="text-3xl font-bold text-green-400">{data.jellyfin.episode_count}</p>
          <p class="text-sm text-gray-400">Episodes</p>
        </div>
        <div class="card text-center">
          <p class="text-3xl font-bold {data.jellyfin.active_streams > 0 ? 'text-yellow-400' : 'text-gray-500'}">{data.jellyfin.active_streams}</p>
          <p class="text-sm text-gray-400">Streaming</p>
        </div>
      {/if}
    </div>

    <!-- Jellyfin Server Info -->
    {#if data.jellyfin}
      <div class="card">
        <div class="flex items-center justify-between">
          <div class="flex items-center gap-3">
            <span class="text-2xl">📺</span>
            <div>
              <h3 class="font-semibold">Jellyfin Server</h3>
              <p class="text-sm text-gray-400">{data.jellyfin.server_name}</p>
            </div>
          </div>
          <div class="text-right">
            <span class="text-xs px-2 py-1 bg-green-500/20 text-green-400 rounded">Online</span>
            <p class="text-xs text-gray-500 mt-1">v{data.jellyfin.version}</p>
          </div>
        </div>
      </div>
    {/if}

    <!-- Recent Downloads -->
    <div class="grid grid-cols-1 md:grid-cols-2 gap-6">
      <!-- Recent Movies -->
      <div class="card">
        <h3 class="text-lg font-semibold mb-4">Recent Movies</h3>
        {#if data.recent_movies.length === 0}
          <p class="text-gray-500 text-sm">No recent movies</p>
        {:else}
          <div class="space-y-2">
            {#each data.recent_movies as movie}
              <div class="p-3 bg-gray-700/50 rounded">
                <div class="flex items-center justify-between">
                  <span class="font-medium text-sm truncate flex-1 mr-2">{movie.title}</span>
                  <span class="text-xs px-2 py-0.5 rounded {movie.status === 'Imported' ? 'bg-green-500/20 text-green-400' : movie.status === 'Downloading' ? 'bg-yellow-500/20 text-yellow-400' : 'bg-gray-500/20 text-gray-400'}">
                    {movie.status}
                  </span>
                </div>
                <div class="flex items-center gap-2 mt-1 text-xs text-gray-400">
                  <span>{movie.date}</span>
                  {#if movie.quality}
                    <span class="px-1 bg-gray-600 rounded">{movie.quality}</span>
                  {/if}
                </div>
              </div>
            {/each}
          </div>
        {/if}
      </div>

      <!-- Recent TV Shows -->
      <div class="card">
        <h3 class="text-lg font-semibold mb-4">Recent TV Episodes</h3>
        {#if data.recent_shows.length === 0}
          <p class="text-gray-500 text-sm">No recent episodes</p>
        {:else}
          <div class="space-y-2">
            {#each data.recent_shows as show}
              <div class="p-3 bg-gray-700/50 rounded">
                <div class="flex items-center justify-between">
                  <span class="font-medium text-sm truncate flex-1 mr-2">{show.title}</span>
                  <span class="text-xs px-2 py-0.5 rounded {show.status === 'Imported' ? 'bg-green-500/20 text-green-400' : show.status === 'Downloading' ? 'bg-yellow-500/20 text-yellow-400' : 'bg-gray-500/20 text-gray-400'}">
                    {show.status}
                  </span>
                </div>
                <div class="flex items-center gap-2 mt-1 text-xs text-gray-400">
                  <span>{show.date}</span>
                  {#if show.quality}
                    <span class="px-1 bg-gray-600 rounded">{show.quality}</span>
                  {/if}
                </div>
              </div>
            {/each}
          </div>
        {/if}
      </div>
    </div>
  {/if}
</div>

<style>
  .card {
    background: #1f2937;
    border: 1px solid #374151;
    border-radius: 0.5rem;
    padding: 1.5rem;
  }
</style>

<script>
  import { onMount } from "svelte";
  import { goto } from "$app/navigation";

  let username = $state("");
  let password = $state("");
  let error = $state(null);
  let loading = $state(false);

  onMount(async () => {
    // If setup has not run yet, the first admin is created there, not here.
    try {
      const s = await fetch("/api/setup/status");
      if (s.ok && !(await s.json()).is_complete) { goto("/setup"); return; }
    } catch (e) { /* offline: stay on login */ }
    // Already logged in? skip to the dashboard.
    try {
      const me = await fetch("/api/auth/me");
      if (me.ok) goto("/");
    } catch (e) { /* ignore */ }
  });

  async function submit(e) {
    e.preventDefault();
    error = null;
    loading = true;
    try {
      const res = await fetch("/api/auth/login", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ username, password }),
      });
      if (res.ok) {
        // Full reload so the layout re-runs its auth + addons load.
        window.location.href = "/";
      } else if (res.status === 401 || res.status === 403) {
        error = "Incorrect username or password.";
      } else {
        error = "Login failed. Please try again.";
      }
    } catch (e) {
      error = "Could not reach the router.";
    } finally {
      loading = false;
    }
  }
</script>

<div class="min-h-screen bg-gray-900 flex items-center justify-center p-4">
  <form on:submit={submit} class="w-full max-w-sm bg-gray-800 border border-gray-700 rounded-xl p-6 space-y-4">
    <div>
      <h1 class="text-2xl font-bold text-blue-400">RouterUI</h1>
      <p class="text-sm text-gray-400">Sign in to manage your router.</p>
    </div>

    {#if error}
      <p class="text-sm text-red-400 bg-red-900/30 border border-red-800 rounded px-3 py-2">{error}</p>
    {/if}

    <div>
      <label class="block text-xs text-gray-400 mb-1" for="u">Username</label>
      <input id="u" bind:value={username} autocomplete="username" required
        class="w-full bg-gray-900 border border-gray-700 rounded px-3 py-2 text-gray-100 focus:border-blue-500 outline-none" />
    </div>

    <div>
      <label class="block text-xs text-gray-400 mb-1" for="p">Password</label>
      <input id="p" type="password" bind:value={password} autocomplete="current-password" required
        class="w-full bg-gray-900 border border-gray-700 rounded px-3 py-2 text-gray-100 focus:border-blue-500 outline-none" />
    </div>

    <button type="submit" disabled={loading || !username || !password}
      class="w-full bg-blue-600 hover:bg-blue-500 disabled:opacity-50 text-white rounded px-3 py-2 font-medium">
      {loading ? "Signing in…" : "Sign in"}
    </button>
  </form>
</div>

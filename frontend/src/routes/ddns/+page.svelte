<script>
  import { onMount } from "svelte";

  let cfg = $state(null);
  let loading = $state(true);
  let saving = $state(false);
  let updating = $state(false);
  let message = $state("");

  // Form state. Secret fields start blank; the backend never echoes secrets and
  // preserves the stored value when a secret field is submitted empty.
  let form = $state({
    enabled: false,
    provider: "duckdns",
    hostname: "",
    token: "",
    username: "",
    password: "",
    zone_id: "",
    record_id: "",
    custom_url: ""
  });

  async function fetchData() {
    try {
      const res = await fetch("/api/ddns/config");
      if (res.ok) {
        cfg = await res.json();
        form.enabled = cfg.enabled;
        form.provider = cfg.provider || "duckdns";
        form.hostname = cfg.hostname || "";
        form.zone_id = cfg.zone_id || "";
        form.record_id = cfg.record_id || "";
        form.custom_url = cfg.custom_url || "";
      }
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

  async function save() {
    saving = true;
    message = "";
    try {
      const res = await fetch("/api/ddns/config", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(form)
      });
      if (res.ok) {
        message = "Saved.";
        // Clear entered secrets from the form after a successful save.
        form.token = "";
        form.password = "";
        await fetchData();
      } else {
        message = "Error: " + (await res.text());
      }
    } catch (e) {
      message = "Error: " + e;
    } finally {
      saving = false;
    }
  }

  async function updateNow() {
    updating = true;
    message = "";
    try {
      const res = await fetch("/api/ddns/update", { method: "POST" });
      const data = await res.json().catch(() => ({}));
      if (res.ok) {
        message = `Update: ${data.status} (${data.ip || "?"})`;
        await fetchData();
      } else {
        message = "Error: " + (data.error || (await res.text().catch(() => "update failed")));
      }
    } catch (e) {
      message = "Error: " + e;
    } finally {
      updating = false;
    }
  }
</script>

<svelte:head>
  <title>Dynamic DNS - RouterUI</title>
</svelte:head>

<div class="space-y-6">
  <h2 class="text-2xl font-bold">Dynamic DNS</h2>
  <p class="text-sm text-gray-400">
    Keep a hostname pointed at your changing WAN IP so port forwards and VPN stay reachable.
  </p>

  {#if loading}
    <div class="text-gray-400">Loading...</div>
  {:else}
    <!-- Status -->
    <div class="card">
      <div class="flex items-center justify-between">
        <div>
          <h3 class="text-lg font-semibold">Status</h3>
          <p class="text-sm text-gray-400">
            Last IP: <span class="font-mono">{cfg?.last_ip || "—"}</span>
            &nbsp;•&nbsp; Last update: <span class="font-mono">{cfg?.last_update || "never"}</span>
          </p>
          {#if cfg?.last_status}
            <p class="text-sm {cfg.last_status === 'ok' ? 'text-green-400' : 'text-yellow-400'} mt-1">
              {cfg.last_status}
            </p>
          {/if}
        </div>
        <div class="flex items-center gap-4">
          <span class={cfg?.enabled ? "status-active" : "status-inactive"}>
            {cfg?.enabled ? "Enabled" : "Disabled"}
          </span>
          <button onclick={updateNow} disabled={updating} class="btn btn-primary">
            {updating ? "Updating..." : "Update now"}
          </button>
        </div>
      </div>
    </div>

    <!-- Configuration -->
    <div class="card">
      <h3 class="text-lg font-semibold mb-4">Configuration</h3>

      <div class="space-y-4">
        <label class="flex items-center gap-3">
          <input type="checkbox" bind:checked={form.enabled} class="w-4 h-4" />
          <span class="text-sm">Enable automatic updates</span>
        </label>

        <div>
          <label class="block text-sm text-gray-400 mb-1" for="ddns-provider">Provider</label>
          <select
            id="ddns-provider"
            bind:value={form.provider}
            class="bg-gray-700 border border-gray-600 rounded px-3 py-2 text-sm w-64"
          >
            <option value="duckdns">DuckDNS</option>
            <option value="cloudflare">Cloudflare</option>
            <option value="noip">No-IP</option>
            <option value="custom">Custom URL</option>
          </select>
        </div>

        <div>
          <label class="block text-sm text-gray-400 mb-1" for="ddns-host">Hostname</label>
          <input
            id="ddns-host"
            type="text"
            bind:value={form.hostname}
            placeholder="myhome.duckdns.org"
            class="w-80 bg-gray-700 border border-gray-600 rounded px-3 py-2 text-sm"
          />
        </div>

        {#if form.provider === "duckdns"}
          <div>
            <label class="block text-sm text-gray-400 mb-1" for="ddns-token">DuckDNS token</label>
            <input
              id="ddns-token"
              type="password"
              bind:value={form.token}
              placeholder={cfg?.has_credentials ? "•••••••• (saved)" : "token"}
              autocomplete="off"
              class="w-80 bg-gray-700 border border-gray-600 rounded px-3 py-2 text-sm"
            />
          </div>
        {:else if form.provider === "cloudflare"}
          <div>
            <label class="block text-sm text-gray-400 mb-1" for="cf-zone">Zone ID</label>
            <input id="cf-zone" type="text" bind:value={form.zone_id}
              class="w-80 bg-gray-700 border border-gray-600 rounded px-3 py-2 text-sm" />
          </div>
          <div>
            <label class="block text-sm text-gray-400 mb-1" for="cf-record">Record ID</label>
            <input id="cf-record" type="text" bind:value={form.record_id}
              class="w-80 bg-gray-700 border border-gray-600 rounded px-3 py-2 text-sm" />
          </div>
          <div>
            <label class="block text-sm text-gray-400 mb-1" for="cf-token">API token</label>
            <input id="cf-token" type="password" bind:value={form.token}
              placeholder={cfg?.has_credentials ? "•••••••• (saved)" : "API token"}
              autocomplete="off"
              class="w-80 bg-gray-700 border border-gray-600 rounded px-3 py-2 text-sm" />
          </div>
        {:else if form.provider === "noip"}
          <div>
            <label class="block text-sm text-gray-400 mb-1" for="noip-user">Username</label>
            <input id="noip-user" type="text" bind:value={form.username}
              autocomplete="off"
              class="w-80 bg-gray-700 border border-gray-600 rounded px-3 py-2 text-sm" />
          </div>
          <div>
            <label class="block text-sm text-gray-400 mb-1" for="noip-pass">Password</label>
            <input id="noip-pass" type="password" bind:value={form.password}
              placeholder={cfg?.has_credentials ? "•••••••• (saved)" : "password"}
              autocomplete="off"
              class="w-80 bg-gray-700 border border-gray-600 rounded px-3 py-2 text-sm" />
          </div>
        {:else if form.provider === "custom"}
          <div>
            <label class="block text-sm text-gray-400 mb-1" for="custom-url">Update URL (use {"{ip}"} placeholder)</label>
            <input id="custom-url" type="text" bind:value={form.custom_url}
              placeholder="https://example.com/update?ip={'{ip}'}"
              class="w-full bg-gray-700 border border-gray-600 rounded px-3 py-2 text-sm" />
          </div>
        {/if}

        <div class="flex items-center gap-4">
          <button onclick={save} disabled={saving} class="btn btn-primary">
            {saving ? "Saving..." : "Save"}
          </button>
          {#if message}
            <span class="text-sm text-gray-300">{message}</span>
          {/if}
        </div>
      </div>
    </div>
  {/if}
</div>

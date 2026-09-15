<script>
  import { onMount } from "svelte";

  let loading = $state(true);
  let status = $state({ enabled: false, ssid: "", subnet: "", has_wifi: false, has_passphrase: false });
  let ssid = $state("");
  let passphrase = $state("");
  let error = $state("");
  let busy = $state(false);

  async function load() {
    try {
      const res = await fetch("/api/guest/status");
      if (res.ok) { status = await res.json(); ssid = status.ssid || ""; }
    } catch (e) { console.error(e); } finally { loading = false; }
  }

  async function save(enabled) {
    error = ""; busy = true;
    try {
      const res = await fetch("/api/guest/config", {
        method: "POST", headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ enabled, ssid, passphrase })
      });
      if (res.ok) { passphrase = ""; await load(); }
      else error = await res.text();
    } catch (e) { error = String(e); } finally { busy = false; }
  }

  onMount(load);
</script>

<div class="space-y-6">
  <div>
    <h2 class="text-2xl font-bold">Guest Network</h2>
    <p class="text-gray-400 text-sm">An isolated network for visitors. Guests reach the internet but not your main LAN, other networks, or each other.</p>
  </div>

  {#if error}<div class="card border border-red-600 text-red-300 text-sm">{error}</div>{/if}

  {#if loading}
    <p class="text-gray-400">Loading…</p>
  {:else}
    <div class="card">
      <div class="flex items-center justify-between">
        <div>
          <p class="font-semibold">Status: <span class={status.enabled ? "status-active" : "status-inactive"}>{status.enabled ? "Enabled" : "Disabled"}</span></p>
          {#if status.subnet}<p class="text-gray-500 text-xs mt-1">Subnet <code>{status.subnet}</code>, isolated</p>{/if}
        </div>
        {#if status.enabled}
          <button class="btn btn-danger" onclick={() => save(false)} disabled={busy}>Turn off</button>
        {:else}
          <button class="btn btn-primary" onclick={() => save(true)} disabled={busy}>Turn on</button>
        {/if}
      </div>
    </div>

    <div class="card">
      <h3 class="text-lg font-semibold mb-4">Wi-Fi (optional)</h3>
      {#if status.has_wifi}
        <p class="text-gray-400 text-sm mb-3">Broadcast a separate guest SSID with client isolation. Leave blank for a wired-only guest network.</p>
        <label class="block text-sm text-gray-400 mb-1">Guest SSID
          <input type="text" bind:value={ssid} placeholder="MyNetwork-Guest" class="input w-full mt-1" />
        </label>
        <label class="block text-sm text-gray-400 mb-1">Passphrase {#if status.has_passphrase}<span class="text-gray-500">(leave blank to keep current)</span>{/if}
          <input type="password" bind:value={passphrase} placeholder="8–63 characters" class="input w-full mt-1" />
        </label>
        <button class="btn btn-primary" onclick={() => save(status.enabled)} disabled={busy}>Save Wi-Fi settings</button>
      {:else}
        <p class="text-gray-500 text-sm">No Wi-Fi radio detected on this router, so the guest network is wired/VLAN only. Put guest ports on VLAN 90 to use it.</p>
      {/if}
    </div>
  {/if}
</div>


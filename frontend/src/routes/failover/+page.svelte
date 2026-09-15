<script>
  import { onMount } from "svelte";

  let loading = $state(true);
  let status = $state({ enabled: false, primary_iface: "", backup_iface: "", ping_target: "1.1.1.1", active_iface: "", primary_up: false, backup_up: false });
  let interfaces = $state([]);
  let form = $state({ enabled: false, primary_iface: "", backup_iface: "", ping_target: "1.1.1.1" });
  let error = $state("");
  let busy = $state(false);

  async function load() {
    try {
      const [s, ifs] = await Promise.all([fetch("/api/failover/status"), fetch("/api/network/interfaces")]);
      if (s.ok) {
        status = await s.json();
        form = { enabled: status.enabled, primary_iface: status.primary_iface, backup_iface: status.backup_iface, ping_target: status.ping_target || "1.1.1.1" };
      }
      if (ifs.ok) interfaces = (await ifs.json()).filter(i => i.name !== "lo");
    } catch (e) { console.error(e); } finally { loading = false; }
  }

  async function save() {
    error = ""; busy = true;
    try {
      const res = await fetch("/api/failover/config", {
        method: "POST", headers: { "Content-Type": "application/json" },
        body: JSON.stringify(form)
      });
      if (res.ok) await load(); else error = await res.text();
    } catch (e) { error = String(e); } finally { busy = false; }
  }

  onMount(() => { load(); const t = setInterval(load, 10000); return () => clearInterval(t); });
</script>

<div class="max-w-2xl">
  <div class="mb-6">
    <h2 class="text-2xl font-bold">Dual-WAN Failover</h2>
    <p class="text-gray-400 text-sm">If your primary internet uplink goes down, the router automatically switches to a backup uplink (e.g. an LTE/USB modem) and fails back when the primary returns. Requires two WAN interfaces.</p>
  </div>

  {#if error}<div class="card border-red-600 mb-4 text-red-300 text-sm">{error}</div>{/if}

  {#if loading}
    <p class="text-gray-400">Loading…</p>
  {:else}
    <div class="card mb-4">
      <div class="grid grid-cols-3 gap-4 text-center">
        <div>
          <p class="text-gray-400 text-xs">Active uplink</p>
          <p class="font-mono">{status.active_iface || "—"}</p>
        </div>
        <div>
          <p class="text-gray-400 text-xs">Primary</p>
          <p class={status.primary_up ? "status-active" : "status-inactive"}>{status.primary_iface || "—"} {status.primary_up ? "▲" : "▼"}</p>
        </div>
        <div>
          <p class="text-gray-400 text-xs">Backup</p>
          <p class={status.backup_up ? "status-active" : "status-inactive"}>{status.backup_iface || "—"} {status.backup_up ? "▲" : "▼"}</p>
        </div>
      </div>
    </div>

    <div class="card">
      <label class="text-sm flex items-center gap-2 mb-4">
        <input type="checkbox" bind:checked={form.enabled} /> Enable failover
      </label>
      <div class="grid grid-cols-2 gap-3">
        <label class="text-sm">Primary WAN
          <select bind:value={form.primary_iface} class="input w-full mt-1">
            <option value="">Select…</option>
            {#each interfaces as i}<option value={i.name}>{i.name}</option>{/each}
          </select>
        </label>
        <label class="text-sm">Backup WAN
          <select bind:value={form.backup_iface} class="input w-full mt-1">
            <option value="">Select…</option>
            {#each interfaces as i}<option value={i.name}>{i.name}</option>{/each}
          </select>
        </label>
        <label class="text-sm">Probe target (IP)
          <input type="text" bind:value={form.ping_target} class="input w-full mt-1" />
        </label>
      </div>
      <button class="btn btn-primary mt-4" onclick={save} disabled={busy}>Save</button>
    </div>
  {/if}
</div>


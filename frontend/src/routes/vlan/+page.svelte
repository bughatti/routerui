<script>
  import { onMount } from "svelte";

  let loading = $state(true);
  let vlans = $state([]);
  let lanIface = $state("");
  let error = $state("");
  let busy = $state(false);
  let form = $state({ id: 20, name: "", subnet: "192.168.20", dhcp_start: "192.168.20.100", dhcp_end: "192.168.20.200", isolated: true });

  async function load() {
    try {
      const res = await fetch("/api/vlan/list");
      if (res.ok) {
        const d = await res.json();
        vlans = d.vlans || [];
        lanIface = d.lan_interface || "";
      }
    } catch (e) { console.error(e); } finally { loading = false; }
  }

  // Keep the subnet-derived defaults in sync as the operator edits the subnet.
  function syncSubnet() {
    form.dhcp_start = `${form.subnet}.100`;
    form.dhcp_end = `${form.subnet}.200`;
  }

  async function addVlan() {
    error = ""; busy = true;
    try {
      const res = await fetch("/api/vlan/add", {
        method: "POST", headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ ...form, id: Number(form.id) })
      });
      if (res.ok) { await load(); form.name = ""; }
      else error = await res.text();
    } catch (e) { error = String(e); } finally { busy = false; }
  }

  async function removeVlan(id) {
    if (!confirm(`Delete VLAN ${id}? Devices on it will lose their network.`)) return;
    busy = true;
    try {
      const res = await fetch("/api/vlan/remove", {
        method: "POST", headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ id })
      });
      if (res.ok) await load(); else error = await res.text();
    } catch (e) { error = String(e); } finally { busy = false; }
  }

  onMount(load);
</script>

<div class="max-w-4xl">
  <div class="mb-6">
    <h2 class="text-2xl font-bold">Networks (VLANs)</h2>
    <p class="text-gray-400 text-sm">Segment your LAN into separate networks. Each VLAN gets its own subnet and DHCP; an <em>isolated</em> VLAN can reach the internet but not your main LAN or other VLANs.</p>
    {#if lanIface}<p class="text-gray-500 text-xs mt-1">Tagged on LAN interface: <code>{lanIface}</code> — your switch/AP must pass the VLAN tag to use it beyond the router.</p>{/if}
  </div>

  {#if error}<div class="card border-red-600 mb-4 text-red-300 text-sm">{error}</div>{/if}

  {#if loading}
    <p class="text-gray-400">Loading…</p>
  {:else}
    <div class="card mb-6">
      <h3 class="font-semibold mb-3">Configured networks</h3>
      {#if vlans.length === 0}
        <p class="text-gray-500 text-sm">No VLANs yet. Add one below.</p>
      {:else}
        <table class="w-full text-sm">
          <thead class="text-gray-400 text-left">
            <tr><th class="py-1">VLAN</th><th>Name</th><th>Subnet</th><th>DHCP range</th><th>Isolated</th><th></th></tr>
          </thead>
          <tbody>
            {#each vlans as v}
              <tr class="border-t border-gray-700">
                <td class="py-2">{v.id}</td>
                <td>{v.name}</td>
                <td><code>{v.subnet}.0/24</code></td>
                <td class="text-gray-400">{v.dhcp_start}–{v.dhcp_end}</td>
                <td>{v.isolated ? "🔒 yes" : "no"}</td>
                <td class="text-right"><button class="btn-danger text-xs px-2 py-1" onclick={() => removeVlan(v.id)} disabled={busy}>Delete</button></td>
              </tr>
            {/each}
          </tbody>
        </table>
      {/if}
    </div>

    <div class="card">
      <h3 class="font-semibold mb-3">Add a network</h3>
      <div class="grid grid-cols-2 gap-3">
        <label class="text-sm">VLAN ID (1–4094)
          <input type="number" min="1" max="4094" bind:value={form.id} class="input w-full mt-1" />
        </label>
        <label class="text-sm">Name
          <input type="text" bind:value={form.name} placeholder="IoT" class="input w-full mt-1" />
        </label>
        <label class="text-sm">Subnet (first 3 octets)
          <input type="text" bind:value={form.subnet} oninput={syncSubnet} placeholder="192.168.20" class="input w-full mt-1" />
        </label>
        <label class="text-sm flex items-center gap-2 mt-6">
          <input type="checkbox" bind:checked={form.isolated} /> Isolated (guest-style)
        </label>
        <label class="text-sm">DHCP start
          <input type="text" bind:value={form.dhcp_start} class="input w-full mt-1" />
        </label>
        <label class="text-sm">DHCP end
          <input type="text" bind:value={form.dhcp_end} class="input w-full mt-1" />
        </label>
      </div>
      <button class="btn-primary mt-4" onclick={addVlan} disabled={busy || !form.name}>Add network</button>
    </div>
  {/if}
</div>

<style>
  .input { @apply bg-gray-900 border border-gray-700 rounded px-3 py-2 text-gray-100; }
</style>

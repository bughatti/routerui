<script>
  import '../app.css';
  import { goto } from '$app/navigation';
  import { page } from '$app/stores';

  let { children } = $props();
  let setupChecked = $state(false);
  let isSetupRoute = $derived($page.url.pathname.startsWith('/setup'));
  let installedAddons = $state({});
  let hasCheckedSetup = $state(false);

  // Core navigation - always visible
  const coreNavItems = [
    { href: '/', label: 'Dashboard', icon: '📊' },
    { href: '/network', label: 'Network', icon: '🌐' },
    { href: '/firewall', label: 'Firewall', icon: '🔥' },
    { href: '/services', label: 'Services', icon: '⚙️' },
    { href: '/users', label: 'Users', icon: '👥' },
    { href: '/system', label: 'System', icon: '🖥️' },
  ];

  // Optional navigation - only visible when addon is installed
  const optionalNavItems = [
    { href: '/adguard', label: 'AdGuard', icon: '🛡️', addonId: 'adguard' },
    { href: '/vpn', label: 'VPN', icon: '🔐', addonId: 'vpn' },
    { href: '/docker', label: 'Docker', icon: '🐳', addonId: 'docker' },
    { href: '/media', label: 'Media', icon: '🎬', addonId: 'media' },
    { href: '/antivirus', label: 'Antivirus', icon: '🦠', addonId: 'antivirus' },
    { href: '/protection', label: 'Protection', icon: '🔒', addonId: 'protection' },
    { href: '/security', label: 'Security', icon: '🚨', addonId: 'security' },
  ];

  // Compute visible optional items based on installed addons
  let visibleOptionalItems = $derived(
    optionalNavItems.filter(item => installedAddons[item.addonId]?.installed)
  );

  // Effect to check setup status and fetch addons when route changes
  $effect(() => {
    // On setup routes, just mark as checked
    if (isSetupRoute) {
      setupChecked = true;
      return;
    }

    // Not on setup route - need to verify setup and load addons
    if (!hasCheckedSetup) {
      hasCheckedSetup = true;
      checkSetupAndLoadAddons();
    } else {
      // Already checked, just mark as ready
      setupChecked = true;
    }
  });

  async function checkSetupAndLoadAddons() {
    try {
      const res = await fetch('/api/setup/status');
      if (res.ok) {
        const status = await res.json();
        if (!status.is_complete) {
          goto('/setup');
          return;
        }
      }
    } catch (e) {
      console.warn('Setup check failed:', e);
    }

    // Fetch installed addons
    try {
      const addonsRes = await fetch('/api/addons/status');
      if (addonsRes.ok) {
        installedAddons = await addonsRes.json();
      }
    } catch (e) {
      console.warn('Failed to fetch addons status:', e);
    }

    setupChecked = true;
  }
</script>

{#if !setupChecked && !isSetupRoute}
  <!-- Loading state while checking setup -->
  <div class="min-h-screen bg-gray-900 flex items-center justify-center">
    <div class="text-center">
      <div class="animate-spin rounded-full h-12 w-12 border-b-2 border-blue-400 mx-auto"></div>
      <p class="mt-4 text-gray-400">Loading RouterUI...</p>
    </div>
  </div>
{:else if isSetupRoute}
  <!-- Setup pages have their own layout -->
  {@render children()}
{:else}
  <div class="flex h-screen">
    <!-- Sidebar -->
    <aside class="w-64 bg-gray-800 border-r border-gray-700 flex flex-col">
      <div class="p-4 border-b border-gray-700">
        <h1 class="text-xl font-bold text-blue-400">RouterUI</h1>
        <p class="text-xs text-gray-500">Network Management</p>
      </div>

      <nav class="flex-1 p-4 overflow-y-auto">
        <!-- Core Navigation -->
        <ul class="space-y-1">
          {#each coreNavItems as item}
            <li>
              <a
                href={item.href}
                class="flex items-center gap-3 px-3 py-2 rounded-lg hover:bg-gray-700 transition-colors
                  {$page.url.pathname === item.href ? 'bg-gray-700 text-blue-400' : ''}"
              >
                <span>{item.icon}</span>
                <span>{item.label}</span>
              </a>
            </li>
          {/each}
        </ul>

        <!-- Optional Navigation (only if addons installed) -->
        {#if visibleOptionalItems.length > 0}
          <div class="mt-4 pt-4 border-t border-gray-700">
            <p class="text-xs text-gray-500 uppercase tracking-wide mb-2 px-3">Add-ons</p>
            <ul class="space-y-1">
              {#each visibleOptionalItems as item}
                <li>
                  <a
                    href={item.href}
                    class="flex items-center gap-3 px-3 py-2 rounded-lg hover:bg-gray-700 transition-colors
                      {$page.url.pathname === item.href ? 'bg-gray-700 text-blue-400' : ''}"
                  >
                    <span>{item.icon}</span>
                    <span>{item.label}</span>
                  </a>
                </li>
              {/each}
            </ul>
          </div>
        {/if}

        <!-- Add-ons Link -->
        <div class="mt-4 pt-4 border-t border-gray-700">
          <a
            href="/addons"
            class="flex items-center gap-3 px-3 py-2 rounded-lg hover:bg-gray-700 transition-colors
              {$page.url.pathname === '/addons' ? 'bg-gray-700 text-blue-400' : ''}"
          >
            <span>➕</span>
            <span>Add-ons</span>
          </a>
        </div>
      </nav>

      <div class="p-4 border-t border-gray-700 text-xs text-gray-500">
        <p>Logged in as: admin</p>
      </div>
    </aside>

    <!-- Main content -->
    <main class="flex-1 overflow-auto p-6">
      {@render children()}
    </main>
  </div>
{/if}

<script>
  import '../app.css';

  let { children } = $props();

  const navItems = [
    { href: '/', label: 'Dashboard', icon: '📊' },
    { href: '/adguard', label: 'AdGuard', icon: '🛡️' },
    { href: '/network', label: 'Network', icon: '🌐' },
    { href: '/security', label: 'Security', icon: '🚨' },
    { href: '/firewall', label: 'Firewall', icon: '🔥' },
    { href: '/protection', label: 'Protection', icon: '🔒' },
    { href: '/antivirus', label: 'Antivirus', icon: '🦠' },
    { href: '/vpn', label: 'VPN', icon: '🔐' },
    { href: '/services', label: 'Services', icon: '⚙️' },
    { href: '/docker', label: 'Docker', icon: '🐳' },
    { href: '/media', label: 'Media', icon: '🎬' },
    { href: '/users', label: 'Users', icon: '👥' },
    { href: '/system', label: 'System', icon: '🖥️' },
  ];
</script>

<div class="flex h-screen">
  <!-- Sidebar -->
  <aside class="w-64 bg-gray-800 border-r border-gray-700 flex flex-col">
    <div class="p-4 border-b border-gray-700">
      <h1 class="text-xl font-bold text-blue-400">RouterUI</h1>
      <p class="text-xs text-gray-500">Network Management</p>
    </div>

    <nav class="flex-1 p-4">
      <ul class="space-y-2">
        {#each navItems as item}
          <li>
            <a
              href={item.href}
              class="flex items-center gap-3 px-3 py-2 rounded-lg hover:bg-gray-700 transition-colors"
            >
              <span>{item.icon}</span>
              <span>{item.label}</span>
            </a>
          </li>
        {/each}
      </ul>
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

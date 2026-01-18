<script>
  import { onMount } from "svelte";

  // State
  let loading = $state(true);
  let users = $state([]);
  let currentUser = $state(null);
  let error = $state("");
  let success = $state("");

  // Form states
  let showAddForm = $state(false);
  let editingUser = $state(null);
  let newUser = $state({ username: "", password: "", confirmPassword: "", role: "viewer" });
  let editForm = $state({ password: "", confirmPassword: "", role: "" });
  let showPassword = $state(false);

  async function fetchUsers() {
    try {
      const res = await fetch("/api/users");
      if (res.ok) {
        users = await res.json();
      }
    } catch (e) {
      console.error(e);
    } finally {
      loading = false;
    }
  }

  async function fetchCurrentUser() {
    try {
      const res = await fetch("/api/auth/me");
      if (res.ok) {
        currentUser = await res.json();
      }
    } catch (e) {
      console.error(e);
    }
  }

  onMount(() => {
    fetchUsers();
    fetchCurrentUser();
  });

  async function addUser() {
    error = "";
    success = "";

    if (!newUser.username || !newUser.password) {
      error = "Username and password are required";
      return;
    }

    if (newUser.password !== newUser.confirmPassword) {
      error = "Passwords do not match";
      return;
    }

    if (newUser.password.length < 8) {
      error = "Password must be at least 8 characters";
      return;
    }

    try {
      const res = await fetch("/api/users", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          username: newUser.username,
          password: newUser.password,
          role: newUser.role
        })
      });

      if (res.ok) {
        success = "User created successfully";
        newUser = { username: "", password: "", confirmPassword: "", role: "viewer" };
        showAddForm = false;
        await fetchUsers();
      } else {
        const data = await res.json().catch(() => ({}));
        error = data.message || "Failed to create user";
      }
    } catch (e) {
      error = "Network error";
    }
  }

  function startEdit(user) {
    editingUser = user;
    editForm = { password: "", confirmPassword: "", role: user.role };
  }

  function cancelEdit() {
    editingUser = null;
    editForm = { password: "", confirmPassword: "", role: "" };
  }

  async function saveEdit() {
    error = "";
    success = "";

    if (editForm.password && editForm.password !== editForm.confirmPassword) {
      error = "Passwords do not match";
      return;
    }

    if (editForm.password && editForm.password.length < 8) {
      error = "Password must be at least 8 characters";
      return;
    }

    const updates = {};
    if (editForm.password) updates.password = editForm.password;
    if (editForm.role !== editingUser.role) updates.role = editForm.role;

    if (Object.keys(updates).length === 0) {
      cancelEdit();
      return;
    }

    try {
      const res = await fetch(`/api/users/${editingUser.id}`, {
        method: "PUT",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(updates)
      });

      if (res.ok) {
        success = "User updated successfully";
        cancelEdit();
        await fetchUsers();
      } else {
        error = "Failed to update user";
      }
    } catch (e) {
      error = "Network error";
    }
  }

  async function deleteUser(user) {
    if (!confirm(`Are you sure you want to delete user "${user.username}"?`)) {
      return;
    }

    error = "";
    success = "";

    try {
      const res = await fetch(`/api/users/${user.id}`, {
        method: "DELETE"
      });

      if (res.ok) {
        success = "User deleted successfully";
        await fetchUsers();
      } else {
        const data = await res.json().catch(() => ({}));
        error = data.message || "Failed to delete user";
      }
    } catch (e) {
      error = "Network error";
    }
  }

  function getRoleBadgeClass(role) {
    switch (role) {
      case "admin": return "bg-red-500/20 text-red-400";
      case "operator": return "bg-yellow-500/20 text-yellow-400";
      case "viewer": return "bg-blue-500/20 text-blue-400";
      default: return "bg-gray-500/20 text-gray-400";
    }
  }
</script>

<svelte:head>
  <title>Users - RouterUI</title>
</svelte:head>

<div class="space-y-6">
  <div class="flex items-center justify-between">
    <div>
      <h2 class="text-2xl font-bold">Users</h2>
      <p class="text-sm text-gray-500">Manage RouterUI user accounts</p>
    </div>
    <button
      onclick={() => showAddForm = !showAddForm}
      class="btn-primary"
    >
      {showAddForm ? "Cancel" : "+ Add User"}
    </button>
  </div>

  {#if error}
    <div class="p-4 bg-red-500/20 border border-red-500/50 rounded text-red-400">
      {error}
    </div>
  {/if}

  {#if success}
    <div class="p-4 bg-green-500/20 border border-green-500/50 rounded text-green-400">
      {success}
    </div>
  {/if}

  <!-- Add User Form -->
  {#if showAddForm}
    <div class="card">
      <h3 class="text-lg font-semibold mb-4">Add New User</h3>
      <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
        <div>
          <label class="block text-sm text-gray-400 mb-1">Username</label>
          <input
            type="text"
            bind:value={newUser.username}
            class="input w-full"
            placeholder="Enter username"
          />
        </div>
        <div>
          <label class="block text-sm text-gray-400 mb-1">Role</label>
          <select bind:value={newUser.role} class="input w-full">
            <option value="viewer">Viewer (read-only)</option>
            <option value="operator">Operator (can make changes)</option>
            <option value="admin">Admin (full access)</option>
          </select>
        </div>
        <div>
          <label class="block text-sm text-gray-400 mb-1">Password</label>
          <input
            type={showPassword ? "text" : "password"}
            bind:value={newUser.password}
            class="input w-full"
            placeholder="Enter password"
          />
        </div>
        <div>
          <label class="block text-sm text-gray-400 mb-1">Confirm Password</label>
          <input
            type={showPassword ? "text" : "password"}
            bind:value={newUser.confirmPassword}
            class="input w-full"
            placeholder="Confirm password"
          />
        </div>
      </div>
      <div class="flex items-center gap-4 mt-4">
        <label class="flex items-center gap-2 text-sm text-gray-400">
          <input type="checkbox" bind:checked={showPassword} />
          Show password
        </label>
        <button onclick={addUser} class="btn-primary">Create User</button>
      </div>
    </div>
  {/if}

  {#if loading}
    <div class="text-gray-400">Loading...</div>
  {:else}
    <!-- Users List -->
    <div class="card">
      <h3 class="text-lg font-semibold mb-4">User Accounts ({users.length})</h3>

      <div class="space-y-3">
        {#each users as user}
          <div class="p-4 bg-gray-700/50 rounded flex items-center justify-between">
            {#if editingUser?.id === user.id}
              <!-- Edit Mode -->
              <div class="flex-1 grid grid-cols-1 md:grid-cols-4 gap-4">
                <div>
                  <span class="text-lg font-medium">{user.username}</span>
                  {#if currentUser?.id === user.id}
                    <span class="text-xs text-blue-400 ml-2">(you)</span>
                  {/if}
                </div>
                <div>
                  <label class="block text-xs text-gray-400 mb-1">New Password (optional)</label>
                  <input
                    type="password"
                    bind:value={editForm.password}
                    class="input w-full text-sm"
                    placeholder="Leave blank to keep"
                  />
                </div>
                <div>
                  <label class="block text-xs text-gray-400 mb-1">Confirm Password</label>
                  <input
                    type="password"
                    bind:value={editForm.confirmPassword}
                    class="input w-full text-sm"
                    placeholder="Confirm new password"
                  />
                </div>
                <div>
                  <label class="block text-xs text-gray-400 mb-1">Role</label>
                  <select bind:value={editForm.role} class="input w-full text-sm">
                    <option value="viewer">Viewer</option>
                    <option value="operator">Operator</option>
                    <option value="admin">Admin</option>
                  </select>
                </div>
              </div>
              <div class="flex gap-2 ml-4">
                <button onclick={saveEdit} class="btn-primary text-sm">Save</button>
                <button onclick={cancelEdit} class="btn-secondary text-sm">Cancel</button>
              </div>
            {:else}
              <!-- View Mode -->
              <div class="flex items-center gap-4">
                <div>
                  <span class="text-lg font-medium">{user.username}</span>
                  {#if currentUser?.id === user.id}
                    <span class="text-xs text-blue-400 ml-2">(you)</span>
                  {/if}
                </div>
                <span class="text-xs px-2 py-1 rounded uppercase {getRoleBadgeClass(user.role)}">
                  {user.role}
                </span>
              </div>
              <div class="flex gap-2">
                <button
                  onclick={() => startEdit(user)}
                  class="text-blue-400 hover:text-blue-300 text-sm"
                >
                  Edit
                </button>
                {#if currentUser?.id !== user.id}
                  <button
                    onclick={() => deleteUser(user)}
                    class="text-red-400 hover:text-red-300 text-sm"
                  >
                    Delete
                  </button>
                {/if}
              </div>
            {/if}
          </div>
        {/each}
      </div>
    </div>

    <!-- Role Descriptions -->
    <div class="card">
      <h3 class="text-lg font-semibold mb-4">Role Permissions</h3>
      <div class="grid grid-cols-1 md:grid-cols-3 gap-4">
        <div class="p-4 bg-gray-700/50 rounded">
          <div class="flex items-center gap-2 mb-2">
            <span class="text-xs px-2 py-1 rounded uppercase bg-blue-500/20 text-blue-400">Viewer</span>
          </div>
          <ul class="text-sm text-gray-400 space-y-1">
            <li>View dashboard and status</li>
            <li>View network configuration</li>
            <li>View logs and statistics</li>
            <li>Cannot make changes</li>
          </ul>
        </div>
        <div class="p-4 bg-gray-700/50 rounded">
          <div class="flex items-center gap-2 mb-2">
            <span class="text-xs px-2 py-1 rounded uppercase bg-yellow-500/20 text-yellow-400">Operator</span>
          </div>
          <ul class="text-sm text-gray-400 space-y-1">
            <li>All Viewer permissions</li>
            <li>Modify network settings</li>
            <li>Manage firewall rules</li>
            <li>Cannot manage users</li>
          </ul>
        </div>
        <div class="p-4 bg-gray-700/50 rounded">
          <div class="flex items-center gap-2 mb-2">
            <span class="text-xs px-2 py-1 rounded uppercase bg-red-500/20 text-red-400">Admin</span>
          </div>
          <ul class="text-sm text-gray-400 space-y-1">
            <li>All Operator permissions</li>
            <li>Create and delete users</li>
            <li>Change user roles</li>
            <li>Full system access</li>
          </ul>
        </div>
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

  select.input {
    appearance: none;
    background-image: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' fill='none' viewBox='0 0 24 24' stroke='%239ca3af'%3E%3Cpath stroke-linecap='round' stroke-linejoin='round' stroke-width='2' d='M19 9l-7 7-7-7'%3E%3C/path%3E%3C/svg%3E");
    background-repeat: no-repeat;
    background-position: right 0.5rem center;
    background-size: 1.5em 1.5em;
    padding-right: 2.5rem;
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

  .btn-secondary {
    background: #4b5563;
    color: white;
    padding: 0.5rem 1rem;
    border-radius: 0.375rem;
    font-weight: 500;
  }

  .btn-secondary:hover {
    background: #6b7280;
  }
</style>

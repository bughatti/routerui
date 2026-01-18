<script>
  import '../../app.css';
  let { children } = $props();
</script>

<div class="min-h-screen bg-gradient-to-br from-gray-900 via-gray-800 to-gray-900 flex items-center justify-center p-4">
  <div class="w-full max-w-4xl">
    <div class="text-center mb-8">
      <h1 class="text-4xl font-bold text-blue-400">RouterUI</h1>
      <p class="text-gray-400 mt-2">Network Management System</p>
    </div>
    {@render children()}
  </div>
</div>

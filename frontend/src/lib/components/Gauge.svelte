<script>
  let { value = 0, max = 100, label = "", unit = "%", size = 120, type = "ring" } = $props();

  // Ensure value is a number
  let numValue = $derived(typeof value === 'number' ? value : parseFloat(value) || 0);
  let percentage = $derived(Math.min(100, Math.max(0, (numValue / max) * 100)));
  let radius = $derived(size / 2 - 8);
  let centerX = $derived(size / 2);
  let centerY = $derived(size / 2);
  const strokeWidth = 6;

  // Color based on percentage
  function getColor(pct) {
    if (pct < 50) return "#22c55e"; // green
    if (pct < 75) return "#eab308"; // yellow
    if (pct < 90) return "#f97316"; // orange
    return "#ef4444"; // red
  }

  let color = $derived(getColor(percentage));

  // Calculate stroke-dasharray for ring gauge
  let circumference = $derived(2 * Math.PI * radius);
  let dashOffset = $derived(circumference - (percentage / 100) * circumference);

  // Speedometer calculations
  // Arc goes from left (0%) to right (100%), about 180 degrees
  // Needle: -90 degrees = points up (center), sweep from -180 (left) to 0 (right)
  let needleAngle = $derived(-180 + (percentage * 1.8));
</script>

{#if type === "speedometer"}
  <!-- Speedometer style gauge (like CPU) -->
  <div class="gauge-wrapper">
    <div class="gauge-label">{label}</div>
    <svg width={size * 1.2} height={size * 0.8} viewBox="0 0 {size * 1.2} {size * 0.8}">
      <defs>
        <linearGradient id="speed-gradient-{label}" x1="0%" y1="0%" x2="100%" y2="0%">
          <stop offset="0%" stop-color="#22c55e"/>
          <stop offset="60%" stop-color="#eab308"/>
          <stop offset="100%" stop-color="#ef4444"/>
        </linearGradient>
      </defs>

      <!-- Background arc -->
      <path
        d="M {size * 0.15} {size * 0.65} A {radius} {radius} 0 0 1 {size * 1.05} {size * 0.65}"
        fill="none"
        stroke="#374151"
        stroke-width={strokeWidth + 2}
        stroke-linecap="round"
      />

      <!-- Colored arc -->
      <path
        d="M {size * 0.15} {size * 0.65} A {radius} {radius} 0 0 1 {size * 1.05} {size * 0.65}"
        fill="none"
        stroke="url(#speed-gradient-{label})"
        stroke-width={strokeWidth}
        stroke-linecap="round"
      />

      <!-- Needle -->
      <g transform="translate({size * 0.6}, {size * 0.65}) rotate({needleAngle})">
        <line x1="0" y1="0" x2={radius - 10} y2="0" stroke="#f9fafb" stroke-width="2" stroke-linecap="round"/>
        <circle cx="0" cy="0" r="4" fill="#f9fafb"/>
      </g>

      <!-- Scale labels -->
      <text x={size * 0.1} y={size * 0.78} fill="#6b7280" font-size="10" font-family="system-ui">0</text>
      <text x={size * 1.0} y={size * 0.78} fill="#6b7280" font-size="10" font-family="system-ui">100.0</text>
    </svg>
    <div class="speed-value">
      <span class="value" style="color: {color}">{numValue.toFixed(1)}</span>
      <span class="unit">{unit}</span>
    </div>
  </div>
{:else}
  <!-- Ring style gauge -->
  <div class="gauge-wrapper">
    <div class="gauge-label">{label}</div>
    <svg width={size} height={size} viewBox="0 0 {size} {size}">
      <!-- Background circle -->
      <circle
        cx={centerX}
        cy={centerY}
        r={radius}
        fill="none"
        stroke="#374151"
        stroke-width={strokeWidth}
        transform="rotate(-90 {centerX} {centerY})"
      />

      <!-- Progress arc -->
      <circle
        cx={centerX}
        cy={centerY}
        r={radius}
        fill="none"
        stroke={color}
        stroke-width={strokeWidth}
        stroke-linecap="round"
        stroke-dasharray={circumference}
        stroke-dashoffset={dashOffset}
        transform="rotate(-90 {centerX} {centerY})"
      />

      <!-- Center text -->
      <text
        x={centerX}
        y={centerY - 2}
        text-anchor="middle"
        dominant-baseline="middle"
        fill="#f9fafb"
        font-size={size / 5}
        font-weight="bold"
        font-family="system-ui, -apple-system, sans-serif"
      >
        {numValue.toFixed(numValue >= 100 ? 0 : numValue >= 10 ? 1 : 2)}
      </text>
      <text
        x={centerX}
        y={centerY + size / 6}
        text-anchor="middle"
        fill="#6b7280"
        font-size={size / 10}
        font-family="system-ui, -apple-system, sans-serif"
      >
        {unit}
      </text>
    </svg>
  </div>
{/if}

<style>
  .gauge-wrapper {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
  }

  .gauge-label {
    font-size: 0.75rem;
    font-weight: 500;
    color: #9ca3af;
    text-transform: capitalize;
  }

  .speed-value {
    display: flex;
    align-items: baseline;
    gap: 4px;
    margin-top: -10px;
  }

  .speed-value .value {
    font-size: 2rem;
    font-weight: bold;
  }

  .speed-value .unit {
    font-size: 0.875rem;
    color: #6b7280;
  }
</style>

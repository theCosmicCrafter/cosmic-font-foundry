<script>
  let { filled = false, size = 17 } = $props();
</script>

<svg width={size} height={size} viewBox="0 0 18 18" aria-hidden="true">
  <path
    d="M9 14.9C5 12.1 2.2 9.4 2.2 6.2a3.5 3.5 0 0 1 6.8-1.1 3.5 3.5 0 0 1 6.8 1.1c0 3.2-2.8 5.9-6.8 8.7Z"
    fill={filled ? "currentColor" : "none"}
    stroke="currentColor"
    stroke-width="1.5"
    stroke-linejoin="round"
  />
</svg>

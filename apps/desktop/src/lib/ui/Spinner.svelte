<!-- Indeterminate spinner (decorative; the owner announces the busy state). -->
<script lang="ts">
  import Icon from './Icon.svelte';
  import { LoaderCircle } from './icons';

  let { size = 16 }: { size?: number } = $props();
</script>

<span class="fc-spinner" aria-hidden="true"><Icon icon={LoaderCircle} {size} /></span>

<style>
  .fc-spinner {
    display: inline-flex;
    animation: fc-spin 0.8s linear infinite;
  }
  @keyframes fc-spin {
    to {
      transform: rotate(360deg);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .fc-spinner {
      animation-duration: 2.4s;
    }
  }
</style>

<!-- Loading placeholder shaped like the content to come. Hidden from screen readers (announce loading elsewhere). -->
<script lang="ts">
  interface Props {
    width?: string;
    height?: string;
    shape?: 'text' | 'control' | 'card' | 'circle';
    /** Several text lines (the last one shorter). */
    lines?: number;
    class?: string;
  }

  let { width = '100%', height, shape = 'text', lines = 1, class: klass = '' }: Props = $props();

  const h = $derived(height ?? { text: '14px', control: 'var(--control-h-md)', card: '120px', circle: width }[shape]);
</script>

<span class="fc-skeleton-group {klass}" aria-hidden="true">
  {#each { length: lines } as _, i (i)}
    <span class="fc-skeleton {shape}" style:width={lines > 1 && i === lines - 1 ? '60%' : width} style:height={h}></span>
  {/each}
</span>

<style>
  .fc-skeleton-group {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }
  .fc-skeleton {
    display: block;
    background: var(--surface-2);
    border-radius: 4px;
    animation: fc-pulse 1.4s ease-in-out infinite;
  }
  .control {
    border-radius: var(--radius-control);
  }
  .card {
    border-radius: var(--radius-card);
  }
  .circle {
    border-radius: 50%;
  }
  @keyframes fc-pulse {
    50% {
      opacity: 0.55;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .fc-skeleton {
      animation: none;
    }
  }
</style>

<!-- Draws `toast.items` (mount once, in the app shell). -->
<script lang="ts">
  import { t } from '$lib/i18n/index.svelte';
  import Toast from './Toast.svelte';
  import { toast } from './toast-store.svelte';
</script>

<section class="fc-toasts" aria-label={t('ui.notifications')}>
  {#each toast.items as item (item.id)}
    <Toast {item} ondismiss={() => toast.dismiss(item.id)} />
  {/each}
</section>

<style>
  .fc-toasts {
    position: fixed;
    z-index: 200;
    bottom: var(--space-4);
    right: var(--space-4);
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: var(--space-2);
    pointer-events: none;
  }
  .fc-toasts > :global(*) {
    pointer-events: auto;
  }
</style>

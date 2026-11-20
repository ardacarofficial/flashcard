<script lang="ts">
  import { onMount } from 'svelte';
  import { api, type ApiError } from '$lib/api';
  import type { UiContribution } from '$lib/api/types';
  import { td } from '$lib/i18n/index.svelte';
  import { contributions, parseCommandMethod, uiText } from './texts.svelte';

  /** Buttons plugins add under a card (`card_toolbar` slot); each runs its command with `{card_id}`. */
  let { cardId }: { cardId: string } = $props();
  let buttons = $state<UiContribution[]>([]);
  let message = $state<string | null>(null);

  onMount(() => {
    api.modules.list().then((m) => (buttons = contributions(m.modules, 'card_toolbar')), () => {});
  });

  async function run(c: UiContribution) {
    const target = parseCommandMethod(c.method);
    if (!target) return;
    message = null;
    try {
      const r = await api.call('plugins.run', { plugin_id: target.pluginId, command: target.command, params: { card_id: cardId } });
      message = r.notices.map((n) => uiText(n.plugin_id, n.text)).join(' ') || null;
    } catch (e) {
      message = td((e as ApiError).messageKey, (e as ApiError).details as Record<string, string>);
    }
  }
</script>

{#if buttons.length}
  <div class="toolbar">
    {#each buttons as b (b.id)}<button onclick={() => run(b)}>{td(b.label_key)}</button>{/each}
    {#if message}<span class="muted" role="status">{message}</span>{/if}
  </div>
{/if}

<style>
  .toolbar {
    display: flex;
    gap: var(--fc-space-2);
    align-items: center;
    margin-top: var(--fc-space-2);
  }
</style>

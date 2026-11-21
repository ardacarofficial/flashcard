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
    {#each buttons as b (b.id)}<button type="button" class="tb" onclick={() => run(b)}>{td(b.label_key)}</button>{/each}
    {#if message}<span class="muted" role="status">{message}</span>{/if}
  </div>
{/if}

<style>
  .toolbar {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1);
    align-items: center;
    justify-content: center;
    margin-top: var(--space-2);
  }
  .tb {
    height: var(--control-h-sm);
    padding: 0 var(--space-3);
    border: 1px solid var(--border);
    border-radius: var(--radius-control);
    background: var(--surface-1);
    color: var(--text-2);
    font: inherit;
    font-size: var(--text-xs);
    cursor: pointer;
  }
  .tb:hover {
    background: var(--surface-2);
    color: var(--text-1);
  }
  .tb:focus-visible {
    outline: 2px solid var(--focus-ring);
    outline-offset: 2px;
  }
  .muted {
    color: var(--text-2);
    font-size: var(--text-xs);
  }
</style>

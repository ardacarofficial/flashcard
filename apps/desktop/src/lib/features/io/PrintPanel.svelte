<script lang="ts">
  import { api, type ApiError } from '$lib/api';
  import type { PrintDocument, PrintLayoutDto } from '$lib/api/types';
  import { t, td } from '$lib/i18n/index.svelte';
  import type { ScreenCtx } from '../types';

  let { ctx }: { ctx: ScreenCtx } = $props();

  const LAYOUTS: Record<string, PrintLayoutDto> = {
    cards_2x4: { kind: 'cards', columns: 2, rows: 4 },
    cards_3x5: { kind: 'cards', columns: 3, rows: 5 },
    list: { kind: 'list' },
  };
  type LayoutKey = 'cards_2x4' | 'cards_3x5' | 'list';
  let layout = $state<LayoutKey>('cards_2x4');
  let doc = $state<PrintDocument | null>(null);
  let error = $state<string | null>(null);
  let frame = $state<HTMLIFrameElement | null>(null);

  async function render() {
    if (!ctx.selectedDeck) return;
    error = null;
    try {
      doc = await api.call('io.print.render', { deck_id: ctx.selectedDeck, layout: LAYOUTS[layout]! });
    } catch (e) {
      error = td((e as ApiError).messageKey);
    }
  }
</script>

<div class="panel">
  {#if !ctx.selectedDeck}
    <p class="muted">{t('io.pick_deck')}</p>
  {:else}
    <p class="muted">{t('io.print.hint')}</p>
    <div class="row">
      <label>
        {t('io.print.layout')}
        <select bind:value={layout} onchange={() => (doc = null)}>
          <option value="cards_2x4">{t('io.print.cards_2x4')}</option>
          <option value="cards_3x5">{t('io.print.cards_3x5')}</option>
          <option value="list">{t('io.print.list')}</option>
        </select>
      </label>
      <button onclick={render}>{t('io.print.render')}</button>
      {#if doc}
        <span class="muted">{t('io.print.count', { count: doc.cards })}</span>
        <button class="primary" onclick={() => frame?.contentWindow?.print()}>{t('io.print.print')}</button>
      {/if}
    </div>
    {#if doc}
      <!-- Text-only HTML from the backend; no scripts may run in it. -->
      <iframe bind:this={frame} title={t('io.tabs.print')} sandbox="allow-modals allow-same-origin" srcdoc={doc.html}></iframe>
    {/if}
  {/if}
  {#if error}<p class="error" role="alert">{error}</p>{/if}
</div>

<style>
  .panel {
    display: flex;
    flex-direction: column;
    gap: var(--fc-space-2);
    align-items: flex-start;
  }
  .row {
    display: flex;
    gap: var(--fc-space-2);
    align-items: center;
  }
  iframe {
    width: 100%;
    height: 60vh;
    border: var(--fc-border-width) solid var(--fc-border);
    background: white;
  }
</style>

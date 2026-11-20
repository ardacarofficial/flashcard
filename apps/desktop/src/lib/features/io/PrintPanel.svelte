<script lang="ts">
  import { api, type ApiError } from '$lib/api';
  import type { PrintDocument, PrintLayoutDto } from '$lib/api/types';
  import { t, td } from '$lib/i18n/index.svelte';
  import type { ScreenCtx } from '../types';
  import { Button, CircleAlert, EmptyState, FileText, Icon, Printer, Select } from '$lib/ui';
  import { FormRow, SettingsSection } from '../core/kit';

  let { ctx }: { ctx: ScreenCtx } = $props();

  const LAYOUTS: Record<string, PrintLayoutDto> = {
    cards_2x4: { kind: 'cards', columns: 2, rows: 4 },
    cards_3x5: { kind: 'cards', columns: 3, rows: 5 },
    list: { kind: 'list' },
  };
  type LayoutKey = 'cards_2x4' | 'cards_3x5' | 'list';
  let layout = $state<LayoutKey>('cards_2x4');
  const layouts = $derived(
    (['cards_2x4', 'cards_3x5', 'list'] as const).map((k) => ({ value: k as LayoutKey, label: t(`io.print.${k}`) })),
  );
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

<SettingsSection title={t('io.tabs.print')} description={t('io.print.hint')} icon={Printer} body="rows">
  {#if !ctx.selectedDeck}
    <EmptyState size="sm" icon={FileText} title={t('io.pick_deck')} />
  {:else}
    <FormRow label={t('io.print.layout')} description={ctx.selectedPath ?? ''} labelFor="fc-print-layout">
      <Select id="fc-print-layout" hideLabel label={t('io.print.layout')} options={layouts} bind:value={layout} onchange={() => (doc = null)} />
    </FormRow>
    <div class="foot">
      <Button onclick={render}>{t('io.print.render')}</Button>
      {#if doc}
        <span class="count">{t('io.print.count', { count: doc.cards })}</span>
        <Button variant="primary" icon={Printer} onclick={() => frame?.contentWindow?.print()}>{t('io.print.print')}</Button>
      {/if}
    </div>
    {#if doc}
      <div class="frame">
        <!-- Text-only HTML from the backend; no scripts may run in it. -->
        <iframe bind:this={frame} title={t('io.tabs.print')} sandbox="allow-modals allow-same-origin" srcdoc={doc.html}></iframe>
      </div>
    {/if}
  {/if}
  {#if error}<p class="bad" role="alert"><Icon icon={CircleAlert} size={16} /><span>{error}</span></p>{/if}
</SettingsSection>

<style>
  .foot {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    flex-wrap: wrap;
    padding: var(--space-4) var(--space-6);
    border-top: 1px solid var(--border);
  }
  .count {
    color: var(--text-2);
    font-size: var(--text-sm);
    margin-inline-start: auto;
  }
  .frame {
    padding: 0 var(--space-6) var(--space-6);
  }
  iframe {
    display: block;
    width: 100%;
    height: 60vh;
    border: 1px solid var(--border);
    border-radius: var(--radius-control);
    /* The printed page is white paper in both themes. */
    background: var(--gray-50);
  }
  .bad {
    display: flex;
    gap: var(--space-2);
    margin: 0;
    padding: 0 var(--space-6) var(--space-4);
    color: var(--danger);
    font-size: var(--text-sm);
  }
  @media (max-width: 1023.98px) {
    .foot,
    .frame,
    .bad {
      padding-inline: var(--space-4);
    }
  }
</style>

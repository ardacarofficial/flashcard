<!--
  One settings row inside a `SettingsSection body="rows"` (v1.3): label + a body-size
  description that says what happens when you change it (+ an optional concrete example) on
  the left, the control on the right, a 1px line below.
  - `stack` puts wide controls (lists, text areas, swatches) under the text; every row also
    stacks when the page is narrow (≤ 900 px window, or a page column under 560 px).
  - `isDefault` (known default) shows a quiet "default" marker; with `onReset`, a non-default
    value gets a "Reset to default" button (the caller runs `settings.reset`).
  - The row lights up while the settings search matches its texts (`search.svelte.ts`).
  Pass `labelFor` when the control is a plain lib/ui control with `hideLabel` and its own id,
  so clicking the label focuses it; Switch / Checkbox carry their own label (`ariaLabel`).
-->
<script lang="ts">
  import type { Snippet } from 'svelte';
  import { t } from '$lib/i18n/index.svelte';
  import { Button, RotateCcw } from '$lib/ui';
  import { matchesQuery, settingsSearch } from './search.svelte';

  interface Props {
    label: string;
    /** What happens if you change it (body size). */
    description?: string;
    /** A concrete example ("20 a day → at most 20 new words are asked each day"). */
    example?: string;
    /** id of the control the label names (`<label for>`). */
    labelFor?: string;
    /** Control below the text (wide controls: lists, text areas). */
    stack?: boolean;
    /** `true`: the value is the default (marker); `false`: changed; omitted: unknown. */
    isDefault?: boolean;
    /** Puts the default back (shown when `isDefault === false`). */
    onReset?: () => void;
    /** Extra words the settings search should find this row by. */
    keywords?: string;
    children?: Snippet;
    class?: string;
  }

  let { label, description, example, labelFor, stack = false, isDefault, onReset, keywords, children, class: klass = '' }: Props = $props();

  const hit = $derived(settingsSearch.active && matchesQuery(settingsSearch.query, label, description, example, keywords));
</script>

<div class="fc-form-row {klass}" class:stack class:hit data-search-hit={hit ? '' : undefined}>
  <div class="text">
    <div class="head">
      {#if labelFor}
        <label class="label" for={labelFor}>{label}</label>
      {:else}
        <span class="label">{label}</span>
      {/if}
      {#if isDefault === true}<span class="default">{t('kit.default')}</span>{/if}
    </div>
    {#if description}<p class="desc">{description}</p>{/if}
    {#if example}<p class="example"><span class="ex-label">{t('kit.example')}</span> {example}</p>{/if}
    {#if isDefault === false && onReset}
      <Button variant="ghost" size="sm" icon={RotateCcw} class="reset" onclick={onReset}>{t('kit.reset')}</Button>
    {/if}
  </div>
  {#if children}<div class="control">{@render children()}</div>{/if}
</div>

<style>
  .fc-form-row {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, auto);
    align-items: center;
    gap: var(--space-2) var(--space-6);
    min-height: 48px;
    padding: var(--space-3) 0;
    border-bottom: 1px solid var(--border);
  }
  .stack {
    grid-template-columns: minmax(0, 1fr);
  }
  .hit {
    background: color-mix(in srgb, var(--accent) 10%, transparent);
    box-shadow: inset 2px 0 0 var(--accent);
    padding-inline: var(--space-3);
    margin-inline: calc(-1 * var(--space-3));
  }
  .text {
    min-width: 0;
    max-width: 62ch;
  }
  .head {
    display: flex;
    align-items: baseline;
    flex-wrap: wrap;
    gap: 2px var(--space-2);
  }
  .label {
    display: block;
    font-size: var(--text-md);
    font-weight: var(--weight-medium);
    line-height: var(--leading-tight);
    color: var(--text-1);
  }
  .default {
    font-size: var(--text-2xs);
    color: var(--text-2);
    border: 1px solid var(--border);
    border-radius: 999px;
    padding: 0 6px;
    line-height: 16px;
  }
  .desc,
  .example {
    margin: 4px 0 0;
    font-size: var(--text-sm);
    color: var(--text-2);
    line-height: var(--leading);
  }
  .ex-label {
    color: var(--text-2);
    font-weight: var(--weight-medium);
  }
  .text :global(.reset) {
    margin: var(--space-1) 0 0 -8px;
  }
  .control {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: var(--space-2);
    flex-wrap: wrap;
    min-width: 0;
  }
  .control :global(.fc-field) {
    min-width: 14rem;
  }
  .stack .control {
    justify-content: flex-start;
  }
  .stack .control :global(.fc-field) {
    flex: 1 1 100%;
  }
  /* Narrow: every row puts its control under the text (labels never squeeze into a column). */
  @media (max-width: 900px) {
    .fc-form-row {
      grid-template-columns: minmax(0, 1fr);
    }
    .control {
      justify-content: flex-start;
    }
  }
  @container fc-settings (max-width: 560px) {
    .fc-form-row {
      grid-template-columns: minmax(0, 1fr);
    }
    .control {
      justify-content: flex-start;
    }
  }
</style>

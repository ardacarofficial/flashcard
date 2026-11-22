<!--
  Query syntax cheat sheet of the Search screen: a compact panel under the query field, one
  line per operator (mono example → what it does). Clicking an example runs it; Esc closes.
-->
<script lang="ts">
  import { t, type MessageKey } from '$lib/i18n/index.svelte';
  import { IconButton, X } from '$lib/ui';

  let { onpick, onclose }: { onpick: (query: string) => void; onclose?: () => void } = $props();

  const EXAMPLES: { q: MessageKey; d: MessageKey }[] = [
    { q: 'browse.syntax.text_q', d: 'browse.syntax.text_d' },
    { q: 'browse.syntax.deck_q', d: 'browse.syntax.deck_d' },
    { q: 'browse.syntax.tag_q', d: 'browse.syntax.tag_d' },
    { q: 'browse.syntax.is_q', d: 'browse.syntax.is_d' },
    { q: 'browse.syntax.level_q', d: 'browse.syntax.level_d' },
    { q: 'browse.syntax.added_q', d: 'browse.syntax.added_d' },
    { q: 'browse.syntax.prop_q', d: 'browse.syntax.prop_d' },
    { q: 'browse.syntax.not_q', d: 'browse.syntax.not_d' },
    { q: 'browse.syntax.or_q', d: 'browse.syntax.or_d' },
    { q: 'browse.syntax.phrase_q', d: 'browse.syntax.phrase_d' },
    // v1.4 language-learning keys (docs/02 §9.1)
    { q: 'browse.syntax.word_q', d: 'browse.syntax.word_d' },
    { q: 'browse.syntax.seen_q', d: 'browse.syntax.seen_d' },
    { q: 'browse.syntax.from_q', d: 'browse.syntax.from_d' },
    { q: 'browse.syntax.media_q', d: 'browse.syntax.media_d' },
  ];
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<section
  class="help"
  aria-label={t('browse.syntax.heading')}
  data-testid="query-help"
  onkeydown={(e) => {
    if (e.key === 'Escape' && onclose) {
      e.stopPropagation();
      onclose();
    }
  }}
>
  <header>
    <h2>{t('browse.syntax.heading')}</h2>
    <p class="intro">{t('browse.help')}</p>
    {#if onclose}<IconButton size="sm" icon={X} label={t('browse.info.close')} onclick={onclose} />{/if}
  </header>
  <dl>
    {#each EXAMPLES as ex (ex.q)}
      <div class="ex">
        <dt><button type="button" class="q" onclick={() => onpick(t(ex.q))}>{t(ex.q)}</button></dt>
        <dd>{t(ex.d)}</dd>
      </div>
    {/each}
  </dl>
</section>

<style>
  .help {
    padding: var(--space-2) var(--space-3) var(--space-3);
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
    background: var(--surface-1);
  }
  header {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    min-height: var(--control-h-sm);
    margin-bottom: var(--space-1);
  }
  h2 {
    margin: 0;
    flex: none;
    font-size: var(--text-sm);
    font-weight: var(--weight-semibold);
  }
  .intro {
    flex: 1;
    min-width: 0;
    margin: 0;
    color: var(--text-2);
    font-size: var(--text-xs);
  }
  dl {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(380px, 1fr));
    gap: 0 var(--space-6);
    margin: 0;
  }
  .ex {
    display: grid;
    grid-template-columns: 9.5rem minmax(0, 1fr);
    gap: var(--space-3);
    align-items: center;
    min-height: 28px;
    border-bottom: 1px solid var(--border);
  }
  dt {
    min-width: 0;
  }
  dd {
    margin: 0;
    color: var(--text-2);
    font-size: var(--text-xs);
  }
  .q {
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: var(--font-mono);
    font-variant-ligatures: none;
    font-size: var(--text-xs);
    color: var(--accent-text);
    background: transparent;
    border: none;
    border-radius: 4px;
    padding: 2px 4px;
    margin-inline-start: -4px;
    cursor: pointer;
  }
  .q:hover {
    background: var(--accent-soft);
  }
  .q:focus-visible {
    outline: 2px solid var(--focus-ring);
    outline-offset: 2px;
  }
</style>

<!-- Query syntax cheat sheet of the Search screen; clicking an example runs it. -->
<script lang="ts">
  import { t, type MessageKey } from '$lib/i18n/index.svelte';
  import { Card } from '$lib/ui';

  let { onpick }: { onpick: (query: string) => void } = $props();

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
  ];
</script>

<Card padding="md">
  <section class="help" aria-label={t('browse.syntax.heading')}>
    <h2>{t('browse.syntax.heading')}</h2>
    <p class="intro">{t('browse.help')}</p>
    <dl>
      {#each EXAMPLES as ex (ex.q)}
        <div class="ex">
          <dt><button type="button" class="q" onclick={() => onpick(t(ex.q))}>{t(ex.q)}</button></dt>
          <dd>{t(ex.d)}</dd>
        </div>
      {/each}
    </dl>
  </section>
</Card>

<style>
  h2 {
    margin: 0;
    font-size: var(--text-sm);
    font-weight: var(--weight-semibold);
  }
  .intro {
    margin: var(--space-1) 0 var(--space-3);
    color: var(--text-2);
    font-size: var(--text-sm);
  }
  dl {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
    gap: var(--space-2) var(--space-6);
    margin: 0;
  }
  .ex {
    display: flex;
    gap: var(--space-3);
    align-items: baseline;
  }
  dt {
    flex: none;
  }
  dd {
    margin: 0;
    color: var(--text-2);
    font-size: var(--text-sm);
  }
  .q {
    font-family: var(--font-mono);
    font-size: var(--text-sm);
    color: var(--accent-text);
    background: var(--accent-soft);
    border: none;
    border-radius: var(--radius-control);
    padding: 2px var(--space-2);
    cursor: pointer;
  }
  .q:hover {
    text-decoration: underline;
  }
  .q:focus-visible {
    outline: 2px solid var(--focus-ring);
    outline-offset: 2px;
  }
</style>

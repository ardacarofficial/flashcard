<!--
  Ways into "Ekle" (v1.4 E3, prototype olustur-ara-istatistik.html › Ekle): a rail of two
  groups — "Kendin yaz" (one word, a pasted list, row by row) and "Bir yerden getir"
  (captured words, a text, a file, AI, starter decks). Ways shown on this page are tabs of a
  vertical tab list (↑/↓ Home/End move and activate, one tab stop); ways that live on their own
  page (Inbox, Reading, AI, starters) are plain buttons that open it, listed after the tabs of
  their group (a tab list holds tabs only). Each way says in one line
  what it does. A way whose module is off is not drawn. The inbox count comes from
  `inbox.count`.
-->
<script lang="ts">
  import type { IconNode } from 'lucide';
  import { api } from '$lib/api';
  import { t, type MessageKey } from '$lib/i18n/index.svelte';
  import { formatCount } from '$lib/format';
  import { BookOpen, ClipboardPaste, Columns3, ExternalLink, FileText, Icon, Inbox, Library, Plus, Sparkles } from '$lib/ui';
  import { moduleOn } from '../cards/modules.svelte';
  import type { AddWay } from './add-way.svelte';

  interface Props {
    way: AddWay;
    onchoose: (way: AddWay) => void;
    /** Opens a route (`ctx.navigate`); without it the page-link ways are hidden. */
    navigate?: (route: string) => void;
  }

  let { way, onchoose, navigate }: Props = $props();

  type Item =
    | { kind: 'tab'; id: AddWay; icon: IconNode; label: MessageKey; desc: MessageKey }
    | { kind: 'link'; id: string; route: string; icon: IconNode; label: MessageKey; desc: MessageKey; count?: number };

  let inboxCount = $state(0);
  $effect(() => {
    if (!moduleOn('inbox')) return;
    void api.call('inbox.count', {}).then(
      (c) => (inboxCount = c.pending),
      () => {},
    );
  });

  const own = $derived<Item[]>([
    { kind: 'tab', id: 'single', icon: Plus, label: 'add.way.single', desc: 'add.way_desc.single' },
    { kind: 'tab', id: 'paste', icon: ClipboardPaste, label: 'add.way.paste', desc: 'add.way_desc.paste' },
    { kind: 'tab', id: 'bulk', icon: Columns3, label: 'add.way.bulk', desc: 'add.way_desc.bulk' },
  ]);
  const bring = $derived<Item[]>(
    (
      [
      navigate && moduleOn('inbox')
        ? ({ kind: 'link', id: 'inbox', route: '/inbox', icon: Inbox, label: 'add.way.inbox', desc: 'add.way_desc.inbox', count: inboxCount } as const)
        : null,
      navigate && moduleOn('reading')
        ? ({ kind: 'link', id: 'text', route: '/reading', icon: BookOpen, label: 'add.way.text', desc: 'add.way_desc.text' } as const)
        : null,
      { kind: 'tab', id: 'file', icon: FileText, label: 'add.way.file', desc: 'add.way_desc.file' } as const,
      navigate && moduleOn('ai')
        ? ({ kind: 'link', id: 'ai', route: '/ai', icon: Sparkles, label: 'add.way.ai', desc: 'add.way_desc.ai' } as const)
        : null,
      navigate && moduleOn('onboarding')
        ? ({ kind: 'link', id: 'starters', route: '/starters', icon: Library, label: 'add.way.starters', desc: 'add.way_desc.starters' } as const)
        : null,
      ] as (Item | null)[]
    ).filter((x): x is Item => x !== null),
  );

  const tabs = $derived([...own, ...bring].filter((i) => i.kind === 'tab') as Extract<Item, { kind: 'tab' }>[]);
  let refs = $state<Record<string, HTMLButtonElement | null>>({});

  function onkeydown(e: KeyboardEvent, id: AddWay) {
    const i = tabs.findIndex((x) => x.id === id);
    const n = tabs.length;
    const to: Record<string, number> = { ArrowDown: (i + 1) % n, ArrowUp: (i - 1 + n) % n, Home: 0, End: n - 1 };
    const j = to[e.key];
    if (j === undefined) return;
    e.preventDefault();
    const next = tabs[j]!;
    refs[next.id]?.focus();
    onchoose(next.id);
  }
</script>

{#snippet row(item: Item)}
  {#if item.kind === 'tab'}
    <button
      type="button"
      role="tab"
      class="way"
      id="add-way-{item.id}"
      aria-selected={way === item.id}
      aria-label={t(item.label)}
      aria-describedby="add-way-{item.id}-d"
      tabindex={way === item.id ? 0 : -1}
      bind:this={refs[item.id]}
      onclick={() => onchoose(item.id)}
      onkeydown={(e) => onkeydown(e, item.id)}
    >
      <Icon icon={item.icon} size={16} />
      <span class="txt">
        <span class="t">{t(item.label)}</span>
        <span class="d" id="add-way-{item.id}-d">{t(item.desc)}</span>
      </span>
    </button>
  {:else}
    <button type="button" class="way link" aria-describedby="add-way-{item.id}-d" onclick={() => navigate?.(item.route)}>
      <Icon icon={item.icon} size={16} />
      <span class="txt">
        <span class="t">
          {t(item.label)}
          {#if item.count}<span class="badge num">{formatCount(item.count)}</span>{/if}
          <span class="out" aria-hidden="true"><Icon icon={ExternalLink} size={12} /></span>
        </span>
        <span class="d" id="add-way-{item.id}-d">{t(item.desc)}</span>
      </span>
    </button>
  {/if}
{/snippet}

<nav class="ways" aria-label={t('add.way.label')}>
  <p class="cap" id="add-ways-own">{t('add.way_group.own')}</p>
  <div role="tablist" aria-orientation="vertical" aria-labelledby="add-ways-own" class="group">
    {#each own as item (item.id)}{@render row(item)}{/each}
  </div>
  <p class="cap" id="add-ways-bring">{t('add.way_group.bring')}</p>
  <div role="tablist" aria-orientation="vertical" aria-labelledby="add-ways-bring" class="group">
    {#each bring.filter((i) => i.kind === 'tab') as item (item.id)}{@render row(item)}{/each}
  </div>
  <div class="group">
    {#each bring.filter((i) => i.kind === 'link') as item (item.id)}{@render row(item)}{/each}
  </div>
</nav>

<style>
  .ways {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    min-width: 0;
  }
  .cap {
    margin: var(--space-3) 0 var(--space-1);
    font-size: var(--text-2xs);
    font-weight: var(--weight-semibold);
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--text-2);
  }
  .cap:first-child {
    margin-top: 0;
  }
  .group {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .way {
    display: flex;
    align-items: flex-start;
    gap: var(--space-2);
    width: 100%;
    padding: var(--space-2) var(--space-3);
    border: 1px solid transparent;
    border-radius: var(--radius-control);
    background: none;
    color: var(--text-2);
    text-align: left;
    font: inherit;
    cursor: pointer;
  }
  .way :global(svg) {
    flex: none;
    margin-top: 2px;
  }
  .way:hover {
    background: var(--surface-2);
    color: var(--text-1);
  }
  .way:focus-visible {
    outline: none;
    box-shadow: var(--focus-ring);
  }
  .way[aria-selected='true'] {
    background: var(--surface-2);
    border-color: var(--border);
    color: var(--text-1);
  }
  .way[aria-selected='true'] > :global(svg) {
    color: var(--accent-text);
  }
  .txt {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
  }
  .t {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    font-size: var(--text-sm);
    font-weight: var(--weight-medium);
  }
  .d {
    font-size: var(--text-xs);
    color: var(--text-2);
    line-height: var(--leading-tight);
  }
  .badge {
    padding: 0 6px;
    border-radius: var(--radius-pill);
    background: var(--accent-soft);
    color: var(--accent-text);
    font-family: var(--font-mono);
    font-size: var(--text-2xs);
  }
  .out {
    display: inline-flex;
    color: var(--text-3);
  }
  /* Narrow window: the rail becomes a wrapping row of compact ways (descriptions hidden). */
  @media (max-width: 1000px) {
    .ways {
      flex-direction: row;
      flex-wrap: wrap;
      align-items: center;
      gap: var(--space-1) var(--space-2);
    }
    .cap {
      display: none;
    }
    .group {
      flex-direction: row;
      flex-wrap: wrap;
    }
    .way {
      width: auto;
      padding: var(--space-1) var(--space-3);
      align-items: center;
    }
    .way :global(svg) {
      margin-top: 0;
    }
    .d {
      display: none;
    }
  }
</style>

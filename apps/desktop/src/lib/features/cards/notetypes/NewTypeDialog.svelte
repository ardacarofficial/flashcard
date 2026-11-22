<!--
  "New note type" (v1.3): pick a ready-made starting point (one line each says what it is),
  name it, create. The request comes from `starters.ts`; the backend creates the type.
-->
<script lang="ts">
  import type { IconNode } from 'lucide';
  import type { CreateNoteType, NoteTypeDto } from '$lib/api/types';
  import { t } from '$lib/i18n/index.svelte';
  import { ArrowLeftRight, BookOpen, Button, Check, Dialog, FileText, Icon, Input, Languages, Monitor, Pencil, Plus } from '$lib/ui';
  import { STARTERS, starterAvailable, starterRequest, type StarterKey } from './starters';

  interface Props {
    types: NoteTypeDto[];
    oncreate: (req: CreateNoteType) => Promise<void> | void;
    onclose: () => void;
  }
  let { types, oncreate, onclose }: Props = $props();

  const ICONS: Record<StarterKey, IconNode> = {
    word: Languages,
    word_example: BookOpen,
    reverse: ArrowLeftRight,
    cloze: Pencil,
    subtitle: Monitor,
    blank: FileText,
  };

  let open = $state(true);
  let picked = $state<StarterKey>('word');
  let name = $state(t('notetype.start.word.name'));
  let nameTouched = $state(false);
  let busy = $state(false);

  const request = $derived(starterRequest(picked, name, types, t));

  function pick(k: StarterKey) {
    picked = k;
    if (!nameTouched) name = t(`notetype.start.${k}.name`);
  }

  async function create() {
    if (!request || !name.trim() || busy) return;
    busy = true;
    try {
      await oncreate(request);
    } finally {
      busy = false;
    }
  }
</script>

<Dialog bind:open size="lg" title={t('notetype.new')} description={t('notetype.start.intro')} {onclose}>
  <form
    id="fc-new-notetype"
    onsubmit={(e) => {
      e.preventDefault();
      void create();
    }}
  >
    <div class="starts" role="group" aria-label={t('notetype.start.label')}>
      {#each STARTERS as k (k)}
        {@const ok = starterAvailable(k, types)}
        <button
          type="button"
          class="start"
          class:on={picked === k}
          aria-pressed={picked === k}
          disabled={!ok}
          data-testid="starter-{k}"
          onclick={() => pick(k)}
        >
          <span class="icon"><Icon icon={ICONS[k]} size={18} /></span>
          <span class="text">
            <span class="title">{t(`notetype.start.${k}.title`)}</span>
            <span class="desc">{ok ? t(`notetype.start.${k}.desc`) : t('notetype.start.missing')}</span>
          </span>
          {#if picked === k}<span class="tick"><Icon icon={Check} size={16} /></span>{/if}
        </button>
      {/each}
    </div>
    <div class="name">
      <Input bind:value={name} label={t('notetype.start.name_label')} hint={t('notetype.start.name_hint')} oninput={() => (nameTouched = true)} />
    </div>
  </form>
  {#snippet footer()}
    <Button variant="ghost" onclick={() => ((open = false), onclose())}>{t('common.cancel')}</Button>
    <Button variant="primary" icon={Plus} type="submit" form="fc-new-notetype" loading={busy} disabled={!request || !name.trim()}>{t('notetype.start.create')}</Button>
  {/snippet}
</Dialog>

<style>
  .starts {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(15rem, 1fr));
    gap: var(--space-2);
  }
  .start {
    position: relative;
    display: flex;
    align-items: flex-start;
    gap: var(--space-3);
    padding: var(--space-3);
    padding-inline-end: var(--space-6);
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
    background: var(--surface-1);
    color: var(--text-1);
    font: inherit;
    text-align: start;
    cursor: pointer;
  }
  .start:hover:not(:disabled) {
    background: var(--surface-2);
  }
  .start.on {
    border-color: var(--accent);
    box-shadow: inset 0 0 0 1px var(--accent);
  }
  .start:focus-visible {
    outline: 2px solid var(--focus-ring);
    outline-offset: 1px;
  }
  .start:disabled {
    cursor: not-allowed;
    opacity: 0.6;
  }
  .icon {
    display: inline-flex;
    margin-top: 1px;
    color: var(--text-2);
  }
  .on .icon {
    color: var(--accent-text);
  }
  .text {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .title {
    font-size: var(--text-md);
    font-weight: var(--weight-medium);
  }
  .desc {
    font-size: var(--text-sm);
    color: var(--text-2);
    line-height: var(--leading);
  }
  .tick {
    position: absolute;
    top: var(--space-3);
    right: var(--space-3);
    color: var(--accent-text);
  }
  .name {
    margin-top: var(--space-4);
    max-width: 28rem;
  }
</style>

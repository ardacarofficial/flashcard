<!-- Library → Notes: the selected deck's notes (edit, delete). -->
<script lang="ts">
  import NoteList from '$lib/components/NoteList.svelte';
  import { t } from '$lib/i18n/index.svelte';
  import { EmptyState, Library, PageHeader } from '$lib/ui';
  import type { ScreenCtx } from '../types';

  let { ctx }: { ctx: ScreenCtx } = $props();
</script>

<PageHeader title={t('note.list_heading')} subtitle={ctx.selectedPath ?? undefined} />
{#if ctx.selectedDeck}
  <NoteList deckId={ctx.selectedDeck} />
{:else}
  <EmptyState icon={Library} title={t('note.pick_deck')} />
{/if}

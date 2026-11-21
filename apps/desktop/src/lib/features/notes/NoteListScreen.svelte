<!-- Library → Notes: the selected deck's notes (edit, delete); "Add card" opens Quick Add on it. -->
<script lang="ts">
  import NoteList from '$lib/components/NoteList.svelte';
  import { t } from '$lib/i18n/index.svelte';
  import { openQuickAdd, shellCommands } from '$lib/shell/commands.svelte';
  import { Button, EmptyState, Library, PageHeader, Plus } from '$lib/ui';
  import type { ScreenCtx } from '../types';

  let { ctx }: { ctx: ScreenCtx } = $props();
</script>

<PageHeader title={t('note.list_heading')} subtitle={ctx.selectedPath ?? undefined}>
  {#snippet actions()}
    {#if shellCommands.canQuickAdd}
      <Button icon={Plus} kbd="N" onclick={() => void openQuickAdd()} data-testid="open-dialog-quick-add">{t('note.add_card')}</Button>
    {/if}
  {/snippet}
</PageHeader>
{#if ctx.selectedDeck}
  <NoteList deckId={ctx.selectedDeck} />
{:else}
  <EmptyState icon={Library} title={t('note.pick_deck')} />
{/if}

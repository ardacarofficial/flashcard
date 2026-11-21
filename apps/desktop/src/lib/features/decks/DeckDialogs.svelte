<!--
  The deck ⋯ menu's dialogs (v1.2), shared by the Library rows and the deck page: rename,
  new sub-deck and delete (to the trash, undoable). Set `request` to open one; every write is a
  backend command (`deck.rename` / `deck.create` / `deck.remove`).
-->
<script lang="ts">
  import { api } from '$lib/api';
  import type { DeckTreeNode } from '$lib/api/types';
  import { t } from '$lib/i18n/index.svelte';
  import { Button, Dialog, Trash2 } from '$lib/ui';
  import NameDialog from './NameDialog.svelte';
  import type { DeckRequest } from './deck-menu';

  interface Props {
    request: DeckRequest | null;
    guarded: (fn: () => Promise<unknown>) => Promise<boolean>;
    /** After a successful delete (the deck page goes back to the Library). */
    ondeleted?: (node: DeckTreeNode) => void;
  }
  let { request = $bindable(), guarded, ondeleted }: Props = $props();

  // Kept while a dialog animates closed, so its title does not blank out.
  let node = $state<DeckTreeNode | null>(null);
  let renaming = $state(false);
  let addingSub = $state(false);
  let deleting = $state(false);

  $effect(() => {
    if (!request) return;
    node = request.node;
    renaming = request.kind === 'rename';
    addingSub = request.kind === 'sub';
    deleting = request.kind === 'delete';
    request = null;
  });

  async function remove() {
    const n = node;
    if (!n) return;
    if (await guarded(() => api.deck.remove(n.deck.id))) {
      deleting = false;
      ondeleted?.(n);
    }
  }
</script>

<NameDialog
  bind:open={renaming}
  title={t('deck_page.rename_title')}
  initial={node?.deck.name ?? ''}
  onsubmit={(name) => guarded(() => api.deck.rename(node!.deck.id, name))}
/>
<NameDialog
  bind:open={addingSub}
  title={t('deck_page.sub_title', { name: node?.deck.name ?? '' })}
  onsubmit={(name) => guarded(() => api.deck.create(name, node!.deck.id))}
/>
<Dialog
  bind:open={deleting}
  role="alertdialog"
  size="sm"
  title={t('deck_page.delete_title', { name: node?.deck.name ?? '' })}
  description={t('deck_page.delete_body')}
>
  {#snippet footer()}
    <Button variant="ghost" onclick={() => (deleting = false)}>{t('common.cancel')}</Button>
    <Button variant="danger" icon={Trash2} onclick={() => void remove()}>{t('deck_page.delete')}</Button>
  {/snippet}
</Dialog>

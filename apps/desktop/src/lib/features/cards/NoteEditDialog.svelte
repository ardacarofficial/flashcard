<script lang="ts">
  import { t } from '$lib/i18n/index.svelte';
  import NoteEditor from './NoteEditor.svelte';
  import { Dialog } from '$lib/ui';

  interface Props {
    noteId: string;
    deckId: string;
    deckName: string;
    onclose: () => void;
  }
  let { noteId, deckId, deckName, onclose }: Props = $props();

  // Following a [[link]] opens the linked note in the same dialog.
  let current = $state<string | null>(null);
  const shown = $derived(current ?? noteId);
</script>

<Dialog open title={t('note_editor.edit_heading')} size="lg" onclose={onclose}>
  {#key shown}
    <NoteEditor bare noteId={shown} {deckId} {deckName} onopennote={(id) => (current = id)} />
  {/key}
</Dialog>

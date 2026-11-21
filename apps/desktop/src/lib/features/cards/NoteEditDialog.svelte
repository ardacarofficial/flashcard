<script lang="ts">
  /**
   * Edit one note (Search, note lists). Keyboard flow (v1.2 flow 3): opens with the caret in
   * the first field, Tab moves between fields, Ctrl+Enter saves and closes (a toast confirms),
   * Esc closes without saving.
   */
  import { t } from '$lib/i18n/index.svelte';
  import NoteEditor from './NoteEditor.svelte';
  import { Dialog, toast } from '$lib/ui';

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
    <NoteEditor
      bare
      autofocus
      noteId={shown}
      {deckId}
      {deckName}
      onopennote={(id) => (current = id)}
      onsaved={() => {
        toast.show(t('note_editor.saved'), { tone: 'success' });
        onclose();
      }}
    />
  {/key}
</Dialog>

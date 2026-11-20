<script lang="ts">
  import { t } from '$lib/i18n/index.svelte';
  import NoteEditor from './NoteEditor.svelte';

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

<div class="backdrop" role="presentation" onkeydown={(e) => e.key === 'Escape' && onclose()}>
  <div class="dialog" role="dialog" aria-modal="true" aria-label={t('note_editor.edit_heading')} tabindex="-1">
    <div class="bar">
      <button onclick={onclose}>{t('note_editor.close')}</button>
    </div>
    {#key shown}
      <NoteEditor noteId={shown} {deckId} {deckName} onopennote={(id) => (current = id)} />
    {/key}
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgb(0 0 0 / 35%);
    display: flex;
    align-items: flex-start;
    justify-content: center;
    padding: var(--fc-space-6);
    overflow: auto;
    z-index: 10;
  }
  .dialog {
    background: var(--fc-bg);
    border-radius: var(--fc-radius);
    padding: var(--fc-space-4);
    width: min(1100px, 100%);
  }
  .bar {
    display: flex;
    justify-content: flex-end;
  }
</style>

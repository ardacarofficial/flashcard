<script lang="ts">
  import NoteEditor from '$lib/features/cards/NoteEditor.svelte';
  import type { ScreenCtx } from '../types';

  let { ctx }: { ctx: ScreenCtx } = $props();
</script>

{#if ctx.selectedDeck && ctx.selectedPath}
  <NoteEditor deckId={ctx.selectedDeck} deckName={ctx.selectedPath} />
{/if}

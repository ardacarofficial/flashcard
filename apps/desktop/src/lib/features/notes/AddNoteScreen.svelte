<script lang="ts">
  import NoteAddForm from '$lib/components/NoteAddForm.svelte';
  import type { ScreenCtx } from '../types';

  let { ctx }: { ctx: ScreenCtx } = $props();
</script>

{#if ctx.selectedDeck && ctx.selectedPath}
  <NoteAddForm deckId={ctx.selectedDeck} deckName={ctx.selectedPath} />
{/if}

<script lang="ts">
  import NoteList from '$lib/components/NoteList.svelte';
  import type { ScreenCtx } from '../types';

  let { ctx }: { ctx: ScreenCtx } = $props();
</script>

{#if ctx.selectedDeck}
  <NoteList deckId={ctx.selectedDeck} />
{/if}

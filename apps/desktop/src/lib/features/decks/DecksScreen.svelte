<script lang="ts">
  import { api } from '$lib/api';
  import DeckTree from '$lib/components/DeckTree.svelte';
  import type { ScreenCtx } from '../types';

  let { ctx }: { ctx: ScreenCtx } = $props();
</script>

<DeckTree
  decks={ctx.decks}
  selectedId={ctx.selectedDeck}
  onselect={ctx.selectDeck}
  oncreate={(name, parentId) => ctx.guarded(() => api.deck.create(name, parentId))}
  onrename={(id, name) => ctx.guarded(() => api.deck.rename(id, name))}
  ondelete={(id) => ctx.guarded(() => api.deck.remove(id))}
/>

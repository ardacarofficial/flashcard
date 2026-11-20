<!-- The deck picker the shell draws above screens that work on one deck (`deckPicker: true`). -->
<script lang="ts">
  import type { DeckPath } from '$lib/components/deck-paths';
  import { t } from '$lib/i18n/index.svelte';
  import { Select } from '$lib/ui';

  interface Props {
    paths: DeckPath[];
    value: string | null;
    onchange: (id: string | null) => void;
  }

  let { paths, value, onchange }: Props = $props();
  const options = $derived([{ value: null as string | null, label: t('deck.select_prompt') }, ...paths.map((p) => ({ value: p.id as string | null, label: p.path }))]);
</script>

<div class="deck-pick">
  <Select {options} {value} label={t('deck.current')} {onchange} />
</div>

<style>
  .deck-pick {
    max-width: 360px;
    margin-bottom: var(--space-6);
  }
</style>

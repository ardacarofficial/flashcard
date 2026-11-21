<!--
  The deck picker of screens that work on one deck (`deckPicker: true`, empty choice "Choose a
  deck") or on one deck or all of them (`deckPicker: 'all'`, empty choice "All decks"). The
  shell hands it to the screen's PageHeader (page-chrome.ts): a compact select among the
  header actions.
-->
<script lang="ts">
  import type { DeckPath } from '$lib/components/deck-paths';
  import { t } from '$lib/i18n/index.svelte';
  import Select from '$lib/ui/Select.svelte';

  interface Props {
    paths: DeckPath[];
    value: string | null;
    onchange: (id: string | null) => void;
    /** No selection means every deck (study sessions). */
    all?: boolean;
  }

  let { paths, value, onchange, all = false }: Props = $props();
  const options = $derived([
    { value: null as string | null, label: t(all ? 'deck.all' : 'deck.pick') },
    ...paths.map((p) => ({ value: p.id as string | null, label: p.path })),
  ]);
</script>

<div class="deck-pick" data-testid="deck-picker">
  <Select {options} {value} label={t('deck.current')} hideLabel {onchange} />
</div>

<style>
  .deck-pick {
    width: 240px;
    max-width: 100%;
  }
</style>

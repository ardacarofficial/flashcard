<!--
  "Meaning in this sentence" as a dialog (v1.4): the study card's "More" menu opens the same
  component the note editor shows inline. Shown by the caller only when `aiReady()`.
-->
<script lang="ts">
  import { t } from '$lib/i18n/index.svelte';
  import { Dialog } from '$lib/ui';
  import MeaningInContext from './MeaningInContext.svelte';

  interface Props {
    noteId: string;
    onclose: () => void;
  }
  let { noteId, onclose }: Props = $props();
</script>

<Dialog open title={t('ai.meaning.title')} size="md" {onclose}>
  <MeaningInContext {noteId} bare />
</Dialog>

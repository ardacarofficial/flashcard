<!-- A one-field dialog for a deck name (new deck, new subdeck, rename). Enter saves. -->
<script lang="ts">
  import { t } from '$lib/i18n/index.svelte';
  import { Button, Dialog, Input } from '$lib/ui';

  interface Props {
    open: boolean;
    title: string;
    hint?: string;
    initial?: string;
    /** Resolves to whether it worked (the dialog then closes). */
    onsubmit: (name: string) => Promise<boolean>;
  }
  let { open = $bindable(), title, hint, initial = '', onsubmit }: Props = $props();

  let name = $state('');
  let busy = $state(false);

  $effect(() => {
    if (open) name = initial;
  });

  async function save(e?: SubmitEvent) {
    e?.preventDefault();
    if (!name.trim() || busy) return;
    busy = true;
    try {
      if (await onsubmit(name.trim())) open = false;
    } finally {
      busy = false;
    }
  }
</script>

<Dialog bind:open {title} size="sm" initialFocus="input">
  <form id="deck-name-form" onsubmit={save}>
    <Input label={t('deck.name')} {hint} bind:value={name} />
  </form>
  {#snippet footer()}
    <Button variant="ghost" onclick={() => (open = false)}>{t('common.cancel')}</Button>
    <Button variant="primary" type="submit" form="deck-name-form" loading={busy} disabled={!name.trim()}>{t('common.save')}</Button>
  {/snippet}
</Dialog>

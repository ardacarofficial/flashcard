<!--
  A destructive confirmation (v1.2: the design system's alert dialog). Mounted while asked;
  focus starts on Cancel, Esc / backdrop / × cancel. Same props as before.
-->
<script lang="ts">
  import { t } from '$lib/i18n/index.svelte';
  import { Button, Dialog, Trash2 } from '$lib/ui';

  interface Props {
    message: string;
    hint?: string;
    confirmLabel: string;
    onconfirm: () => void;
    oncancel: () => void;
  }
  let { message, hint, confirmLabel, onconfirm, oncancel }: Props = $props();

  let open = $state(true);
</script>

<Dialog bind:open role="alertdialog" size="sm" title={message} description={hint} initialFocus="[data-cancel]" onclose={oncancel}>
  {#snippet footer()}
    <Button variant="ghost" data-cancel onclick={oncancel}>{t('common.cancel')}</Button>
    <Button variant="danger" icon={Trash2} onclick={onconfirm}>{confirmLabel}</Button>
  {/snippet}
</Dialog>

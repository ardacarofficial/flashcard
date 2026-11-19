<script lang="ts">
  import { t } from '$lib/i18n/index.svelte';

  interface Props {
    message: string;
    hint?: string;
    confirmLabel: string;
    onconfirm: () => void;
    oncancel: () => void;
  }
  let { message, hint, confirmLabel, onconfirm, oncancel }: Props = $props();

  function focusCancel(node: HTMLElement) {
    node.focus();
  }
  function onkeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') oncancel();
  }
</script>

<div class="backdrop" role="presentation" {onkeydown}>
  <div class="dialog" role="alertdialog" aria-modal="true" aria-labelledby="confirm-msg" tabindex="-1">
    <p id="confirm-msg">{message}</p>
    {#if hint}<p class="muted">{hint}</p>{/if}
    <div class="actions">
      <button use:focusCancel onclick={oncancel}>{t('common.cancel')}</button>
      <button class="danger" onclick={onconfirm}>{confirmLabel}</button>
    </div>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    display: grid;
    place-items: center;
    background: color-mix(in srgb, var(--fc-text) 40%, transparent);
  }
  .dialog {
    background: var(--fc-surface);
    border: var(--fc-border-width) solid var(--fc-border);
    border-radius: var(--fc-radius);
    padding: var(--fc-space-4);
    max-width: 360px;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: var(--fc-space-2);
  }
</style>

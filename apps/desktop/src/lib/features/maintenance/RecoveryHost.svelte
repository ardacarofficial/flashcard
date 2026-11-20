<script lang="ts">
  /**
   * After a crash or an unclean shutdown (v1.0): offers what can be picked up again — the
   * unfinished study session, the unsaved form drafts — with the integrity verdict and the
   * crash report. Also the "new version" notice, and the opt-in update check at start (the
   * backend says when it is due). Mounted once by the shell; draws nothing otherwise.
   */
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import type { RecoveryDto, UpdateInfoDto } from '$lib/api/types';
  import { t } from '$lib/i18n/index.svelte';

  interface Props {
    /** Opens the menu entry with this route. */
    open: (route: string) => void;
  }
  let { open }: Props = $props();

  let offer = $state<RecoveryDto | null>(null);
  let available = $state<UpdateInfoDto | null>(null);

  async function dismiss() {
    offer = null;
    try {
      await api.call('maintenance.recovery.dismiss', {});
    } catch {
      // Nothing to do: the offer is gone for this window either way.
    }
  }

  async function go(route: string) {
    await dismiss();
    open(route);
  }

  async function updates() {
    try {
      let s = await api.call('updates.status', {});
      if (s.check_due) s = await api.call('updates.check', {});
      available = s.available;
    } catch {
      // Updates off, unsupported or unreachable: the maintenance screen shows why.
    }
  }

  onMount(() => {
    void api
      .call('maintenance.recovery.get', {})
      .then((r) => (offer = r.reason ? r : null))
      .catch(() => {});
    void updates();
  });
</script>

{#if offer?.reason}
  <div class="backdrop">
    <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="recovery-title">
      <h2 id="recovery-title">
        {offer.reason === 'crash' ? t('app_maintenance.recovery.title_crash') : t('app_maintenance.recovery.title_unclean')}
      </h2>
      <p>{t('app_maintenance.recovery.intro')}</p>
      {#if offer.integrity}
        <p class={offer.integrity.ok ? '' : 'error'}>
          {offer.integrity.ok ? t('app_maintenance.recovery.integrity_ok') : t('app_maintenance.recovery.integrity_bad')}
        </p>
      {/if}
      <ul>
        {#if offer.session}<li>{t('app_maintenance.recovery.session')}</li>{/if}
        {#if offer.drafts.length > 0}<li>{t('app_maintenance.recovery.drafts', { count: offer.drafts.length })}</li>{/if}
        {#if offer.crash}<li class="muted">{t('app_maintenance.recovery.crash', { message: offer.crash.message })}</li>{/if}
      </ul>
      <div class="actions">
        {#if offer.session}<button onclick={() => go('/study')}>{t('app_maintenance.recovery.resume')}</button>{/if}
        <button onclick={() => go('/app-maintenance')}>{t('app_maintenance.recovery.details')}</button>
        <button onclick={dismiss}>{t('app_maintenance.recovery.dismiss')}</button>
      </div>
    </div>
  </div>
{/if}

{#if available && !offer?.reason}
  <div class="banner" role="status">
    {t('app_maintenance.updates.banner', { version: available.version })}
    <button onclick={() => { available = null; open('/app-maintenance'); }}>{t('app_maintenance.updates.open')}</button>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgb(0 0 0 / 40%);
    display: grid;
    place-items: center;
    z-index: 50;
  }
  .dialog {
    background: var(--fc-surface);
    color: var(--fc-text);
    border-radius: var(--fc-radius);
    padding: var(--fc-space-4);
    max-width: 32rem;
  }
  .actions {
    display: flex;
    gap: var(--fc-space-2);
    flex-wrap: wrap;
  }
  .banner {
    position: fixed;
    right: var(--fc-space-3);
    bottom: var(--fc-space-3);
    background: var(--fc-surface);
    border: var(--fc-border-width) solid var(--fc-border);
    border-radius: var(--fc-radius);
    padding: var(--fc-space-2) var(--fc-space-3);
    display: flex;
    gap: var(--fc-space-2);
    align-items: center;
  }
  .error {
    color: var(--fc-danger);
  }
</style>

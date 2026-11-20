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
  import { Button, CircleAlert, CircleCheck, Dialog, FileText, GraduationCap, Icon, IconButton, RefreshCw, TriangleAlert, X } from '$lib/ui';

  interface Props {
    /** Opens the menu entry with this route. */
    open: (route: string) => void;
  }
  let { open }: Props = $props();

  let offer = $state<RecoveryDto | null>(null);
  let available = $state<UpdateInfoDto | null>(null);
  let dialogOpen = $state(false);
  $effect(() => {
    dialogOpen = !!offer?.reason;
  });

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
  {@const o = offer}
  <Dialog
    bind:open={dialogOpen}
    title={o.reason === 'crash' ? t('app_maintenance.recovery.title_crash') : t('app_maintenance.recovery.title_unclean')}
    description={t('app_maintenance.recovery.intro')}
    initialFocus="[data-recovery-primary]"
    onclose={() => void dismiss()}
  >
    <ul class="items">
      {#if o.integrity}
        <li class={o.integrity.ok ? 'ok' : 'bad'}>
          <Icon icon={o.integrity.ok ? CircleCheck : CircleAlert} size={20} />
          <span>{o.integrity.ok ? t('app_maintenance.recovery.integrity_ok') : t('app_maintenance.recovery.integrity_bad')}</span>
        </li>
      {/if}
      {#if o.session}<li><Icon icon={GraduationCap} size={20} /><span>{t('app_maintenance.recovery.session')}</span></li>{/if}
      {#if o.drafts.length > 0}<li><Icon icon={FileText} size={20} /><span>{t('app_maintenance.recovery.drafts', { count: o.drafts.length })}</span></li>{/if}
      {#if o.crash}<li class="muted"><Icon icon={TriangleAlert} size={20} /><span>{t('app_maintenance.recovery.crash', { message: o.crash.message })}</span></li>{/if}
    </ul>
    {#snippet footer()}
      <Button variant="ghost" onclick={() => go('/app-maintenance')}>{t('app_maintenance.recovery.details')}</Button>
      {#if o.session}
        <Button onclick={dismiss}>{t('app_maintenance.recovery.dismiss')}</Button>
        <Button variant="primary" data-recovery-primary onclick={() => go('/study')}>{t('app_maintenance.recovery.resume')}</Button>
      {:else}
        <Button variant="primary" data-recovery-primary onclick={dismiss}>{t('app_maintenance.recovery.dismiss')}</Button>
      {/if}
    {/snippet}
  </Dialog>
{/if}

{#if available && !offer?.reason}
  <div class="update" role="status">
    <span class="icon" aria-hidden="true"><Icon icon={RefreshCw} size={20} /></span>
    <span class="text">{t('app_maintenance.updates.banner', { version: available.version })}</span>
    <Button size="sm" variant="primary" data-testid="update-open" onclick={() => { available = null; open('/app-maintenance'); }}>{t('app_maintenance.updates.open')}</Button>
    <IconButton size="sm" icon={X} label={t('app_maintenance.updates.later')} onclick={() => (available = null)} />
  </div>
{/if}

<style>
  .items {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }
  .items li {
    display: flex;
    align-items: flex-start;
    gap: var(--space-3);
    font-size: var(--text-sm);
    color: var(--text-1);
  }
  .items :global(svg) {
    flex: none;
    color: var(--text-2);
  }
  .items .ok,
  .items .ok :global(svg) {
    color: var(--success);
  }
  .items .bad,
  .items .bad :global(svg) {
    color: var(--danger);
  }
  .items .muted {
    color: var(--text-2);
  }
  .update {
    position: fixed;
    right: var(--space-4);
    bottom: var(--space-4);
    z-index: 40;
    display: flex;
    align-items: center;
    gap: var(--space-3);
    max-width: min(28rem, calc(100vw - 2 * var(--space-4)));
    padding: var(--space-3) var(--space-3) var(--space-3) var(--space-4);
    background: var(--surface-1);
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
    box-shadow: var(--shadow-2);
    font-size: var(--text-sm);
  }
  .icon {
    display: inline-flex;
    color: var(--accent);
  }
  .text {
    flex: 1 1 auto;
  }
</style>

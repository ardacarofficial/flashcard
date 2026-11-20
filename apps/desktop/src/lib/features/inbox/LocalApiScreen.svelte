<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import type { ApiClientDto, ApiClientList } from '$lib/api/types';
  import { t, td } from '$lib/i18n/index.svelte';
  import type { ScreenCtx } from '../types';
  import { Button, Checkbox, Chip, EmptyState, Globe, Icon, Plug, Skeleton } from '$lib/ui';
  import type { Tone } from '$lib/ui';
  import { SettingsPage, SettingsSection } from '../core/kit';

  let { ctx }: { ctx: ScreenCtx } = $props();

  let list = $state<ApiClientList | null>(null);
  /** Scopes ticked for each pending client (default: everything it asked for). */
  let grant = $state<Record<string, Record<string, boolean>>>({});

  async function load() {
    await ctx.guarded(async () => {
      list = await api.call('local_api.clients.list', {});
      const next: Record<string, Record<string, boolean>> = {};
      for (const c of list.clients) {
        if (c.status === 'pending') {
          next[c.id] = grant[c.id] ?? Object.fromEntries(c.requested_scopes.map((s) => [s, true]));
        }
      }
      grant = next;
    });
  }

  async function approve(c: ApiClientDto) {
    const scopes = c.requested_scopes.filter((s) => grant[c.id]?.[s]);
    await ctx.guarded(() => api.call('local_api.clients.approve', { id: c.id, scopes }));
    await load();
  }

  async function act(method: 'local_api.clients.deny' | 'local_api.clients.revoke' | 'local_api.clients.remove', c: ApiClientDto) {
    await ctx.guarded(() => api.call(method, { id: c.id }));
    await load();
  }

  onMount(() => {
    void load();
    const unsub = api.on(['local_api_', 'data_changed'], () => void load());
    return () => void unsub.then((u) => u());
  });
  const TONE: Record<string, Tone> = { pending: 'warning', approved: 'success', denied: 'neutral', revoked: 'neutral' };
</script>

<SettingsPage title={t('local_api.heading')} subtitle={list ? t('local_api.intro', { port: list.port }) : undefined}>
  <SettingsSection title={t('local_api.clients')} icon={Plug}>
    {#if !list}
      <Skeleton lines={3} />
    {:else if list.clients.length === 0}
      <EmptyState size="sm" icon={Plug} title={t('local_api.none')} />
    {:else}
      <ul class="clients">
        {#each list.clients as c (c.id)}
          <li class:pending={c.status === 'pending'}>
            <div class="head">
              <div class="who">
                <strong>{c.name}</strong>
                <span class="muted">{t(`local_api.kind.${c.kind}`)}</span>
              </div>
              <Chip size="sm" tone={TONE[c.status] ?? 'neutral'} label={t(`local_api.status.${c.status}`)} />
            </div>
            {#if c.origin}<p class="muted origin"><Icon icon={Globe} size={16} />{t('local_api.origin', { origin: c.origin })}</p>{/if}
            {#if c.status === 'pending'}
              <fieldset>
                <legend>{t('local_api.pairing.asks')}</legend>
                {#each c.requested_scopes as s (s)}
                  <Checkbox label={td(`local_api.scope.${s}`)} bind:checked={grant[c.id]![s]} />
                {/each}
              </fieldset>
              <div class="actions">
                <Button variant="ghost" onclick={() => act('local_api.clients.deny', c)}>{t('local_api.deny')}</Button>
                <Button variant="primary" onclick={() => approve(c)}>{t('local_api.approve')}</Button>
              </div>
            {:else}
              {#if c.scopes.length > 0}
                <p class="scopes">{t('local_api.scopes')}: {c.scopes.map((s) => td(`local_api.scope.${s}`)).join(', ')}</p>
              {/if}
              <div class="foot">
                <p class="muted">
                  {c.last_used_at ? t('local_api.last_used', { time: new Date(c.last_used_at).toLocaleString() }) : t('local_api.never_used')}
                </p>
                {#if c.status === 'approved'}
                  <Button size="sm" variant="danger" onclick={() => act('local_api.clients.revoke', c)}>{t('local_api.revoke')}</Button>
                {:else}
                  <Button size="sm" variant="ghost" onclick={() => act('local_api.clients.remove', c)}>{t('local_api.remove')}</Button>
                {/if}
              </div>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
  </SettingsSection>
</SettingsPage>

<style>
  .clients {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }
  .clients li {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    padding: var(--space-4);
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
    background: var(--surface-0);
  }
  .clients li.pending {
    border-color: var(--warning-solid);
  }
  .head,
  .foot {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    flex-wrap: wrap;
  }
  .who {
    display: flex;
    align-items: baseline;
    gap: var(--space-2);
    flex-wrap: wrap;
    min-width: 0;
  }
  .muted {
    margin: 0;
    color: var(--text-2);
    font-size: var(--text-sm);
  }
  .origin {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    overflow-wrap: anywhere;
  }
  .scopes {
    margin: 0;
    font-size: var(--text-sm);
  }
  fieldset {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    border: none;
    padding: 0;
    margin: var(--space-2) 0 0;
  }
  legend {
    padding: 0;
    margin-bottom: var(--space-2);
    font-size: var(--text-sm);
    font-weight: var(--weight-medium);
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: var(--space-2);
  }
</style>

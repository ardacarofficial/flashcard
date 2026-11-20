<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import type { ApiClientDto, ApiClientList } from '$lib/api/types';
  import { t, td } from '$lib/i18n/index.svelte';
  import type { ScreenCtx } from '../types';

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
</script>

<section>
  <h1>{t('local_api.heading')}</h1>
  {#if list}
    <p class="muted">{t('local_api.intro', { port: list.port })}</p>
    <h2>{t('local_api.clients')}</h2>
    {#if list.clients.length === 0}
      <p class="muted">{t('local_api.none')}</p>
    {:else}
      <ul class="clients">
        {#each list.clients as c (c.id)}
          <li>
            <div class="head">
              <strong>{c.name}</strong>
              <span class="muted">{t(`local_api.kind.${c.kind}`)}</span>
              <span class="status {c.status}">{t(`local_api.status.${c.status}`)}</span>
            </div>
            {#if c.origin}<p class="muted">{t('local_api.origin', { origin: c.origin })}</p>{/if}
            {#if c.status === 'pending'}
              <fieldset>
                <legend>{t('local_api.pairing.asks')}</legend>
                {#each c.requested_scopes as s (s)}
                  <label>
                    <input type="checkbox" bind:checked={grant[c.id]![s]} />
                    {td(`local_api.scope.${s}`)}
                  </label>
                {/each}
              </fieldset>
              <div class="actions">
                <button onclick={() => approve(c)}>{t('local_api.approve')}</button>
                <button onclick={() => act('local_api.clients.deny', c)}>{t('local_api.deny')}</button>
              </div>
            {:else}
              {#if c.scopes.length > 0}
                <p>{t('local_api.scopes')}: {c.scopes.map((s) => td(`local_api.scope.${s}`)).join(', ')}</p>
              {/if}
              <p class="muted">
                {c.last_used_at ? t('local_api.last_used', { time: new Date(c.last_used_at).toLocaleString() }) : t('local_api.never_used')}
              </p>
              <div class="actions">
                {#if c.status === 'approved'}
                  <button class="danger" onclick={() => act('local_api.clients.revoke', c)}>{t('local_api.revoke')}</button>
                {:else}
                  <button onclick={() => act('local_api.clients.remove', c)}>{t('local_api.remove')}</button>
                {/if}
              </div>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
  {/if}
</section>

<style>
  .clients {
    list-style: none;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--fc-space-3);
  }
  .clients li {
    border: var(--fc-border-width) solid var(--fc-border);
    border-radius: var(--fc-radius, 6px);
    padding: var(--fc-space-3);
  }
  .head,
  .actions {
    display: flex;
    gap: var(--fc-space-2);
    align-items: center;
  }
  fieldset {
    display: flex;
    flex-direction: column;
    gap: var(--fc-space-1);
    border: none;
    padding: 0;
    margin: var(--fc-space-2) 0;
  }
  .status.approved {
    color: var(--fc-success, inherit);
  }
</style>

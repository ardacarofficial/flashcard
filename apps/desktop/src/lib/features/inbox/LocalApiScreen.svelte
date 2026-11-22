<!--
  Settings → Local API (v1.2, explained in v1.3): what it is for and how approval works in
  plain words first; programs waiting for approval (what they ask for as checkboxes, Refuse /
  Allow); then every known program as one row: name, kind, origin, permissions, last use,
  status, Revoke / Remove. Technical details (port, grantable scopes, callable methods) sit
  under "Advanced". Port and per-program quotas are the module's settings (command palette).
-->
<script lang="ts">
  import { formatDateTime } from '$lib/format';
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import type { ApiClientDto, ApiClientList, LocalApiMethod } from '$lib/api/types';
  import { t, td } from '$lib/i18n/index.svelte';
  import type { ScreenCtx } from '../types';
  import { Button, Checkbox, Chip, CircleHelp, Clock, EmptyState, Icon, Inbox, Plug, Shield, Skeleton } from '$lib/ui';
  import type { Tone } from '$lib/ui';
  import { FormRow, SettingsAdvanced, SettingsPage, SettingsSection } from '../core/kit';

  let { ctx }: { ctx: ScreenCtx } = $props();

  let list = $state<ApiClientList | null>(null);
  /** What a program may call (technical details, under Advanced). */
  let methods = $state<LocalApiMethod[] | null>(null);
  /** Scopes ticked for each pending client (default: everything it asked for). */
  let grant = $state<Record<string, Record<string, boolean>>>({});

  const waiting = $derived(list?.clients.filter((c) => c.status === 'pending') ?? []);
  const known = $derived(list?.clients.filter((c) => c.status !== 'pending') ?? []);

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

  async function loadMethods() {
    try {
      methods = (await api.call('local_api.methods', {})).methods;
    } catch {
      methods = [];
    }
  }

  onMount(() => {
    void load();
    void loadMethods();
    const unsub = api.on(['local_api_', 'data_changed'], () => void load());
    return () => void unsub.then((u) => u());
  });
  /** How a program calls method `name` over HTTP (code, not UI text). */
  const endpoint = (name: string) => `POST /v1/${name}`;
  const TONE: Record<string, Tone> = { pending: 'warning', approved: 'success', denied: 'neutral', revoked: 'neutral' };
  const who = (c: ApiClientDto) => [t(`local_api.kind.${c.kind}`), c.origin ? t('local_api.origin', { origin: c.origin }) : ''].filter(Boolean).join(' · ');
</script>

<SettingsPage title={t('local_api.heading')} subtitle={t('local_api.intro')}>
  <SettingsSection title={t('local_api.how.title')} icon={CircleHelp}>
    <dl class="how">
      <dt><Icon icon={CircleHelp} size={14} />{t('local_api.how.ask_label')}</dt>
      <dd>{t('local_api.how.ask')}</dd>
      <dt><Icon icon={Inbox} size={14} />{t('local_api.how.cards_label')}</dt>
      <dd>{t('local_api.how.cards')}</dd>
      <dt><Icon icon={Shield} size={14} />{t('local_api.how.safe_label')}</dt>
      <dd>{t('local_api.how.safe', { port: list?.port ?? '…' })}</dd>
    </dl>
  </SettingsSection>

  {#if waiting.length > 0}
    <SettingsSection title={t('local_api.waiting')} description={t('local_api.waiting_hint')} icon={Clock} body="rows">
      {#each waiting as c (c.id)}
        <div class="row pending">
          <div class="text">
            <span class="name">{c.name}</span>
            <p class="desc">{who(c)}</p>
          </div>
          <div class="control">
            <Chip size="sm" tone={TONE[c.status] ?? 'neutral'} label={t(`local_api.status.${c.status}`)} />
          </div>
          <fieldset class="scopes-pick">
            <legend>{t('local_api.pairing.asks')}</legend>
            {#each c.requested_scopes as s (s)}
              <Checkbox label={td(`local_api.scope.${s}`)} bind:checked={grant[c.id]![s]} />
            {/each}
          </fieldset>
          <div class="actions">
            <Button size="sm" variant="ghost" onclick={() => act('local_api.clients.deny', c)}>{t('local_api.deny')}</Button>
            <Button size="sm" variant="primary" onclick={() => approve(c)}>{t('local_api.approve')}</Button>
          </div>
        </div>
      {/each}
    </SettingsSection>
  {/if}

  <SettingsSection title={t('local_api.clients')} description={t('local_api.clients_hint')} icon={Plug} body="rows">
    {#if !list}
      <div class="pad"><Skeleton lines={3} /></div>
    {:else if list.clients.length === 0}
      <div class="pad"><EmptyState size="sm" icon={Plug} title={t('local_api.none')} /></div>
    {:else if known.length === 0}
      <p class="pad desc">{t('local_api.none_allowed')}</p>
    {:else}
      {#each known as c (c.id)}
        <div class="row">
          <div class="text">
            <span class="name">{c.name}</span>
            <p class="desc">{who(c)}</p>
            {#if c.scopes.length > 0}
              <p class="desc">{t('local_api.scopes')}: {c.scopes.map((s) => td(`local_api.scope.${s}`)).join(', ')}</p>
            {/if}
            <p class="desc used">
              {c.last_used_at ? t('local_api.last_used', { time: formatDateTime(c.last_used_at) }) : t('local_api.never_used')}
            </p>
          </div>
          <div class="control">
            <Chip size="sm" tone={TONE[c.status] ?? 'neutral'} label={t(`local_api.status.${c.status}`)} />
            {#if c.status === 'approved'}
              <Button size="sm" variant="danger" onclick={() => act('local_api.clients.revoke', c)}>{t('local_api.revoke')}</Button>
            {:else}
              <Button size="sm" variant="ghost" onclick={() => act('local_api.clients.remove', c)}>{t('local_api.remove')}</Button>
            {/if}
          </div>
        </div>
      {/each}
    {/if}
  </SettingsSection>

  <SettingsAdvanced id="local_api" summary={t('local_api.advanced.summary')}>
    <SettingsSection body="rows">
      <FormRow label={t('local_api.advanced.port')} description={t('local_api.advanced.port_hint')}>
        <code class="value" data-testid="local-api-port">{list?.port ?? '…'}</code>
      </FormRow>
      <FormRow label={t('local_api.advanced.scopes')} description={t('local_api.advanced.scopes_hint')} stack>
        <ul class="tech">
          {#each list?.grantable_scopes ?? [] as s (s)}
            <li><code>{s}</code><span>{td(`local_api.scope.${s}`)}</span></li>
          {/each}
        </ul>
      </FormRow>
      <FormRow label={t('local_api.advanced.methods')} description={t('local_api.advanced.methods_hint')} stack>
        {#if methods === null}
          <Skeleton lines={2} />
        {:else if methods.length === 0}
          <p class="desc">{t('local_api.advanced.none')}</p>
        {:else}
          <ul class="tech">
            {#each methods as m (m.name)}
              <li><code>{endpoint(m.name)}</code><span>{td(`local_api.scope.${m.permission}`)}</span></li>
            {/each}
          </ul>
        {/if}
      </FormRow>
    </SettingsSection>
  </SettingsAdvanced>
</SettingsPage>

<style>
  .row {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    align-items: center;
    gap: var(--space-2) var(--space-6);
    padding: var(--space-3) 0;
    border-bottom: 1px solid var(--border);
  }
  /* A request waiting for a decision: marked by a warning bar, text indented past it. */
  .row.pending {
    padding-inline-start: var(--space-3);
    box-shadow: inset 2px 0 0 var(--warning-solid);
  }
  .text {
    min-width: 0;
  }
  .name {
    display: block;
    font-size: var(--text-sm);
    font-weight: var(--weight-medium);
    color: var(--text-1);
  }
  .desc {
    margin: 2px 0 0;
    font-size: var(--text-xs);
    color: var(--text-2);
    line-height: var(--leading);
    overflow-wrap: anywhere;
  }
  .used {
    color: var(--text-2);
  }
  .control {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: var(--space-2);
  }
  .pad {
    margin: 0;
    padding: var(--space-3) 0;
    border-bottom: 1px solid var(--border);
  }
  fieldset {
    grid-column: 1 / -1;
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2) var(--space-4);
    border: none;
    padding: 0;
    margin: 0;
  }
  legend {
    float: left;
    width: 100%;
    padding: 0;
    margin-bottom: var(--space-1);
    font-size: var(--text-xs);
    font-weight: var(--weight-medium);
    color: var(--text-2);
  }
  .how {
    display: grid;
    grid-template-columns: max-content minmax(0, 1fr);
    gap: var(--space-3) var(--space-4);
    margin: 0;
    font-size: var(--text-sm);
    line-height: var(--leading);
  }
  .how dt {
    align-self: start;
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    font-weight: var(--weight-medium);
    color: var(--text-1);
  }
  .how dd {
    margin: 0;
    color: var(--text-2);
  }
  .value {
    font-family: var(--font-mono);
    font-size: var(--text-sm);
  }
  .tech {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    grid-template-columns: max-content minmax(0, 1fr);
    gap: var(--space-1) var(--space-6);
    font-size: var(--text-sm);
    color: var(--text-2);
  }
  .tech li {
    display: contents;
  }
  .tech code {
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    line-height: 20px;
    color: var(--text-1);
  }
  @media (max-width: 639.98px) {
    .how {
      grid-template-columns: minmax(0, 1fr);
    }
  }
  .actions {
    grid-column: 1 / -1;
    display: flex;
    justify-content: flex-end;
    gap: var(--space-2);
  }
</style>

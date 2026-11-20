<script lang="ts">
  import { api, type ApiError } from '$lib/api';
  import type { AiFeature, AiPriceDto, AiProviderDto, AiProviderList, AiProviderKind, AiRouteDto } from '$lib/api/types';
  import { t, td } from '$lib/i18n/index.svelte';
  import type { ScreenCtx } from '../types';
  import {
    Button,
    Chip,
    EmptyState,
    Icon,
    Input,
    KeyRound,
    Menu,
    Plus,
    Bot,
    Select,
    Skeleton,
    TriangleAlert,
    Trash2,
    Save,
    CircleCheck,
  } from '$lib/ui';
  import { SettingsSection, FormRow } from '../core/kit';

  let { ctx }: { ctx: ScreenCtx } = $props();

  let list = $state.raw<AiProviderList | null>(null);
  let loadFailed = $state(false);
  let routes = $state.raw<AiRouteDto[]>([]);
  let prices = $state.raw<AiPriceDto[]>([]);
  let newKind = $state<AiProviderKind>('anthropic');
  let keys = $state<Record<string, string>>({});
  let notice = $state<string | null>(null);
  let error = $state<string | null>(null);
  let price = $state<AiPriceDto>({ model: '', input_per_mtok: 0, output_per_mtok: 0 });
  /** A new address typed for a provider with a stored key: waits for keep/remove. */
  let moving = $state<Record<string, string>>({});

  function fail(e: unknown) {
    error = td((e as ApiError).messageKey ?? 'errors.internal.unexpected');
  }

  async function load() {
    try {
      [list, routes, prices] = await Promise.all([
        api.call('ai.providers.list', {}),
        api.call('ai.routes.list', {}).then((r) => r.routes),
        api.call('ai.prices.list', {}).then((r) => r.prices),
      ]);
      loadFailed = false;
    } catch (e) {
      loadFailed = list === null;
      fail(e);
    }
  }

  $effect(() => {
    void load();
    const off = api.on('ai_config_changed', () => void load());
    return () => void off.then((u) => u());
  });

  async function act(fn: () => Promise<unknown>, done?: string) {
    error = null;
    notice = null;
    if (await ctx.guarded(fn)) {
      if (done) notice = done;
      await load();
    }
  }

  const add = () => act(() => api.call('ai.providers.create', { kind: newKind }));

  function update(p: AiProviderDto, patch: Partial<AiProviderDto>, keepKey: boolean | null = null) {
    const next = { ...p, ...patch };
    return act(() =>
      api.call('ai.providers.update', {
        id: p.id,
        name: next.name,
        base_url: next.base_url,
        default_model: next.default_model,
        models: next.models,
        is_default: next.is_default,
        keep_key: keepKey,
      }),
    );
  }

  /** A key is never sent to a new address unless the user says so. */
  function changeUrl(p: AiProviderDto, url: string) {
    if (url.trim().replace(/\/+$/, '') === p.base_url) return;
    if (p.has_key) moving[p.id] = url;
    else void update(p, { base_url: url });
  }

  async function confirmMove(p: AiProviderDto, keep: boolean) {
    const url = moving[p.id];
    delete moving[p.id];
    if (url !== undefined) await update(p, { base_url: url }, keep);
  }

  async function saveKey(p: AiProviderDto) {
    const key = keys[p.id]?.trim();
    if (!key) return;
    await act(() => api.call('ai.providers.set_key', { provider_id: p.id, key }), t('ai.providers.key_saved'));
    keys[p.id] = '';
  }

  const removeKey = (p: AiProviderDto) =>
    act(() => api.call('ai.providers.set_key', { provider_id: p.id, key: null }));

  const refresh = (p: AiProviderDto) =>
    act(() => api.call('ai.providers.refresh_models', { id: p.id }), t('ai.providers.refreshing'));

  const remove = (p: AiProviderDto) => act(() => api.call('ai.providers.delete', { id: p.id }));

  function setRoute(feature: AiFeature, provider_id: string | null, model: string | null) {
    return act(() => api.call('ai.routes.set', { feature, provider_id, model: model?.trim() || null }));
  }

  const savePrice = (p: AiPriceDto) => act(() => api.call('ai.prices.set', p));
  const deletePrice = (model: string) => act(() => api.call('ai.prices.delete', { model }));

  const kindOptions = $derived((list?.kinds ?? []).map((k) => ({ value: k.kind, label: td(`ai.kind.${k.kind}`) })));
  const routeProviders = $derived([
    { value: '', label: t('ai.routes.default') },
    ...(list?.providers ?? []).map((p) => ({ value: p.id, label: p.name })),
  ]);
</script>

<div class="panel">
  {#if error}
    <p class="banner danger" role="alert"><Icon icon={TriangleAlert} size={16} /><span>{error}</span></p>
  {/if}
  {#if notice}
    <p class="banner success" role="status"><Icon icon={CircleCheck} size={16} /><span>{notice}</span></p>
  {/if}
  {#if list && !list.keychain}
    <p class="banner warning"><Icon icon={TriangleAlert} size={16} /><span>{t('ai.providers.no_keychain')}</span></p>
  {:else if list?.keychain_session_only}
    <p class="banner warning" role="note"><Icon icon={TriangleAlert} size={16} /><span>{t('ai.providers.session_keychain')}</span></p>
  {/if}

  <SettingsSection title={t('ai.providers.heading')} description={t('ai.providers.intro')} icon={Bot}>
    {#snippet actions()}
      {#if list}
        <div class="add">
          <Select label={t('ai.providers.kind')} hideLabel size="sm" options={kindOptions} bind:value={newKind} />
          <Button size="sm" variant="primary" icon={Plus} onclick={() => void add()}>{t('ai.providers.add')}</Button>
        </div>
      {/if}
    {/snippet}
    {#if !list}
      {#if loadFailed}
        <p class="muted">{t('ai.providers.load_failed')}</p>
      {:else}
        <Skeleton shape="card" height="120px" />
      {/if}
    {:else if list.providers.length === 0}
      <EmptyState size="sm" icon={Bot} title={t('ai.providers.none')} />
    {:else}
      <ul class="providers">
        {#each list.providers as p (p.id)}
          <li class="provider">
            <div class="head">
              <div class="who">
                <strong>{p.name}</strong>
                <span class="muted">{td(`ai.kind.${p.kind}`)}</span>
                {#if p.is_default}<Chip size="sm" tone="accent" label={t('ai.providers.default')} />{/if}
                {#if p.local}<Chip size="sm" tone="success" label={t('ai.providers.local')} />{/if}
              </div>
              <Menu
                label={t('ai.providers.more', { name: p.name })}
                items={[
                  { id: 'refresh', label: t('ai.providers.refresh'), onselect: () => void refresh(p) },
                  ...(p.is_default ? [] : [{ id: 'default', label: t('ai.providers.make_default'), onselect: () => void update(p, { is_default: true }) }]),
                  { id: 'sep', separator: true as const },
                  { id: 'delete', label: t('ai.providers.delete'), icon: Trash2, danger: true, onselect: () => void remove(p) },
                ]}
              />
            </div>
            <div class="fields">
              <Input
                type="url"
                label={t('ai.providers.base_url')}
                value={moving[p.id] ?? p.base_url}
                onchange={(e: Event) => changeUrl(p, (e.currentTarget as HTMLInputElement).value)}
              />
              <Input
                label={t('ai.providers.model')}
                list={`models-${p.id}`}
                value={p.default_model}
                onchange={(e: Event) => void update(p, { default_model: (e.currentTarget as HTMLInputElement).value })}
              />
              <datalist id={`models-${p.id}`}>
                {#each p.models as m (m)}<option value={m}></option>{/each}
              </datalist>
            </div>
            {#if moving[p.id] !== undefined}
              <div class="banner warning move" role="alert">
                <p>{t('ai.providers.move_key', { url: moving[p.id] ?? '' })}</p>
                <div class="actions">
                  <Button size="sm" onclick={() => void confirmMove(p, false)}>{t('ai.providers.move_remove_key')}</Button>
                  <Button size="sm" onclick={() => void confirmMove(p, true)}>{t('ai.providers.move_keep_key')}</Button>
                  <Button size="sm" variant="ghost" onclick={() => delete moving[p.id]}>{t('ai.preview.cancel')}</Button>
                </div>
              </div>
            {/if}
            {#if p.needs_key || p.has_key}
              <div class="key">
                <p class="key-status" class:ok={p.has_key}>
                  <Icon icon={p.has_key ? CircleCheck : KeyRound} size={16} />
                  <span>{p.has_key ? t('ai.providers.key_set') : t('ai.providers.key_missing')}</span>
                </p>
                <div class="key-row">
                  <Input type="password" autocomplete="off" label={t('ai.providers.key')}
                    value={keys[p.id] ?? ''}
                    oninput={(e: Event) => (keys[p.id] = (e.currentTarget as HTMLInputElement).value)}
                  />
                  <Button icon={Save} disabled={!keys[p.id]?.trim()} onclick={() => void saveKey(p)}>{t('ai.providers.save_key')}</Button>
                  {#if p.has_key}
                    <Button variant="ghost" onclick={() => void removeKey(p)}>{t('ai.providers.remove_key')}</Button>
                  {/if}
                </div>
              </div>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
  </SettingsSection>

  {#if (list?.providers.length ?? 0) > 0}
    <SettingsSection title={t('ai.routes.heading')} description={t('ai.routes.intro')} body="rows">
      {#each routes as r (r.feature)}
        <FormRow label={td(`ai.feature.${r.feature}`)}>
          <div class="route">
            <Select
              label={t('ai.routes.provider')}
              hideLabel
              size="sm"
              options={routeProviders}
              value={r.provider_id ?? ''}
              onchange={(v: string) => void setRoute(r.feature, v || null, r.model)}
            />
            <Input
              size="sm"
              label={t('ai.routes.model')}
              hideLabel
              placeholder={t('ai.routes.model_default')}
              value={r.model ?? ''}
              onchange={(e: Event) => void setRoute(r.feature, r.provider_id, (e.currentTarget as HTMLInputElement).value)}
            />
          </div>
        </FormRow>
      {/each}
    </SettingsSection>
  {/if}

  <SettingsSection title={t('ai.prices.heading')} description={t('ai.prices.intro')}>
    <div class="prices" role="table" aria-label={t('ai.prices.heading')}>
      <div class="price-row head" role="row">
        <span role="columnheader">{t('ai.prices.model')}</span>
        <span role="columnheader">{t('ai.prices.input')}</span>
        <span role="columnheader">{t('ai.prices.output')}</span>
        <span role="columnheader"><span class="sr-only">{t('ai.prices.actions')}</span></span>
      </div>
      {#each prices as p (p.model)}
        <div class="price-row" role="row">
          <span role="cell" class="model">{p.model}</span>
          <span role="cell" class="num">{p.input_per_mtok}</span>
          <span role="cell" class="num">{p.output_per_mtok}</span>
          <span role="cell" class="end">
            <Button size="sm" variant="ghost" onclick={() => void deletePrice(p.model)}>{t('ai.prices.delete')}</Button>
          </span>
        </div>
      {/each}
      <div class="price-row new" role="row">
        <span role="cell"><Input size="sm" label={t('ai.prices.model')} hideLabel placeholder={t('ai.prices.model')} bind:value={price.model} /></span>
        <span role="cell"><Input size="sm" type="number" min="0" step="0.01" label={t('ai.prices.input')} hideLabel bind:value={price.input_per_mtok} /></span>
        <span role="cell"><Input size="sm" type="number" min="0" step="0.01" label={t('ai.prices.output')} hideLabel bind:value={price.output_per_mtok} /></span>
        <span role="cell" class="end">
          <Button size="sm" disabled={!price.model.trim()} onclick={() => void savePrice($state.snapshot(price))}>{t('ai.prices.save')}</Button>
        </span>
      </div>
    </div>
  </SettingsSection>
</div>

<style>
  .panel {
    display: flex;
    flex-direction: column;
    gap: var(--space-6);
  }
  .banner {
    display: flex;
    align-items: flex-start;
    gap: var(--space-2);
    margin: 0;
    padding: var(--space-3) var(--space-4);
    border-radius: var(--radius-control);
    font-size: var(--text-sm);
    color: var(--text-1);
  }
  .banner :global(svg) {
    flex: none;
    margin-top: 2px;
  }
  .banner.danger {
    background: var(--danger-soft);
    color: var(--danger);
  }
  .banner.success {
    background: var(--success-soft);
    color: var(--success);
  }
  .banner.warning {
    background: var(--warning-soft);
  }
  .banner.warning :global(svg) {
    color: var(--warning);
  }
  .move {
    flex-direction: column;
  }
  .move p {
    margin: 0;
  }
  .add,
  .actions,
  .key-row,
  .route {
    display: flex;
    gap: var(--space-2);
    align-items: flex-end;
    flex-wrap: wrap;
  }
  .add :global(.fc-field) {
    min-width: 11rem;
  }
  .providers {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }
  .provider {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    padding: var(--space-4);
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
    background: var(--surface-0);
  }
  .head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: var(--space-3);
  }
  .who {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    flex-wrap: wrap;
    min-width: 0;
  }
  .muted {
    color: var(--text-2);
    font-size: var(--text-sm);
  }
  .fields {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(14rem, 1fr));
    gap: var(--space-3);
  }
  .key {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    padding-top: var(--space-3);
    border-top: 1px solid var(--border);
  }
  .key-status {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    margin: 0;
    font-size: var(--text-sm);
    color: var(--text-2);
  }
  .key-status.ok {
    color: var(--success);
  }
  .key-row :global(.fc-field) {
    flex: 1 1 14rem;
  }
  .route :global(.fc-field) {
    min-width: 0;
    width: 11rem;
  }
  .prices {
    display: flex;
    flex-direction: column;
  }
  .price-row {
    display: grid;
    grid-template-columns: minmax(8rem, 2fr) minmax(5rem, 1fr) minmax(5rem, 1fr) minmax(5.5rem, auto);
    gap: var(--space-3);
    align-items: center;
    padding: var(--space-2) 0;
    font-size: var(--text-sm);
  }
  .price-row + .price-row {
    border-top: 1px solid var(--border);
  }
  .price-row.head {
    color: var(--text-2);
    font-size: var(--text-xs);
    font-weight: var(--weight-semibold);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  .model {
    font-family: var(--font-mono);
    overflow-wrap: anywhere;
  }
  .num {
    font-variant-numeric: tabular-nums;
  }
  .end {
    display: flex;
    justify-content: flex-end;
  }
  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip: rect(0 0 0 0);
    white-space: nowrap;
  }
</style>

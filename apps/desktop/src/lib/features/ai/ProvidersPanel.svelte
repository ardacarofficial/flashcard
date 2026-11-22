<!--
  AI providers (v1.3): what AI is for, the configured providers as dense rows with their status
  (ready / needs a key / needs a model / test failed), "add provider" (preset gallery + three
  steps, `AddProviderDialog`), the default provider, per-feature routing and prices. Every rule
  (which presets exist, whether a key is needed, the status, the test verdict) comes from Rust.
-->
<script lang="ts">
  import { api, type ApiError } from '$lib/api';
  import type { AiConnectionTest, AiFeature, AiPriceDto, AiProviderDto, AiProviderList, AiProviderStatus, AiRouteDto } from '$lib/api/types';
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
    ChevronDown,
    ChevronRight,
    type Tone,
  } from '$lib/ui';
  import { SettingsSection, FormRow } from '../core/kit';
  import { fetchModels, jobErrorText, testConnection } from './ai';
  import AddProviderDialog from './AddProviderDialog.svelte';
  import KeyLink from './KeyLink.svelte';
  import ModelPicker from './ModelPicker.svelte';
  import TestResult from './TestResult.svelte';

  let { ctx }: { ctx: ScreenCtx } = $props();

  let list = $state.raw<AiProviderList | null>(null);
  let loadFailed = $state(false);
  let routes = $state.raw<AiRouteDto[]>([]);
  let prices = $state.raw<AiPriceDto[]>([]);
  let keys = $state<Record<string, string>>({});
  let notice = $state<string | null>(null);
  let error = $state<string | null>(null);
  let price = $state<AiPriceDto>({ model: '', input_per_mtok: 0, output_per_mtok: 0 });
  /** A new address typed for a provider with a stored key: waits for keep/remove. */
  let moving = $state<Record<string, string>>({});
  let adding = $state(false);
  /** Providers whose settings are open. */
  let open = $state<Record<string, boolean>>({});
  /** Per provider: a test or model fetch is running / its last failure / the test just run. */
  let testing = $state<Record<string, boolean>>({});
  let fetching = $state<Record<string, boolean>>({});
  let fetchError = $state<Record<string, string | null>>({});
  let lastTest = $state<Record<string, AiConnectionTest | null>>({});

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

  async function refresh(p: AiProviderDto) {
    fetching[p.id] = true;
    fetchError[p.id] = null;
    try {
      const { job, list: models } = await fetchModels(p.id);
      if (!models) fetchError[p.id] = jobErrorText(job, t('ai.setup.models_unknown_error'));
      await load();
    } catch (e) {
      fetchError[p.id] = td((e as ApiError).messageKey ?? 'errors.internal.unexpected');
    } finally {
      fetching[p.id] = false;
    }
  }

  async function runTest(p: AiProviderDto) {
    testing[p.id] = true;
    error = null;
    try {
      const r = await testConnection(p.id);
      lastTest[p.id] = r.test;
      if (!r.test) error = jobErrorText(r.job, t('ai.test.job_failed'));
      await load();
    } catch (e) {
      fail(e);
    } finally {
      testing[p.id] = false;
    }
  }

  function pickModel(p: AiProviderDto, model: string) {
    if (model === p.default_model) return;
    const models = p.models.includes(model) ? p.models : [...p.models, model];
    void update(p, { default_model: model, models });
  }

  const remove = (p: AiProviderDto) => act(() => api.call('ai.providers.delete', { id: p.id }));

  function setRoute(feature: AiFeature, provider_id: string | null, model: string | null) {
    return act(() => api.call('ai.routes.set', { feature, provider_id, model: model?.trim() || null }));
  }

  const savePrice = (p: AiPriceDto) => act(() => api.call('ai.prices.set', p));
  const deletePrice = (model: string) => act(() => api.call('ai.prices.delete', { model }));

  const STATUS_TONE: Record<AiProviderStatus, Tone> = {
    ready: 'success',
    needs_key: 'warning',
    needs_model: 'warning',
    test_failed: 'danger',
  };

  const presetOf = (p: AiProviderDto) => list?.presets.find((x) => x.id === p.preset) ?? null;
  /** The test to show: the one just run here, else the provider's last one (this app run). */
  const testOf = (p: AiProviderDto) => lastTest[p.id] ?? p.last_test;

  const routeProviders = $derived([
    { value: '', label: t('ai.routes.default') },
    ...(list?.providers ?? []).map((p) => ({ value: p.id, label: p.name })),
  ]);
  const providerOptions = $derived((list?.providers ?? []).map((p) => ({ value: p.id, label: p.name })));
  const defaultId = $derived(list?.providers.find((p) => p.is_default)?.id ?? null);
</script>

<div class="panel">
  <p class="lead" data-testid="ai-what-for">{t('ai.providers.what_for')}</p>
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
        <Button size="sm" variant="primary" icon={Plus} onclick={() => (adding = true)} data-testid="open-dialog-add-provider">
          {t('ai.providers.add')}
        </Button>
      {/if}
    {/snippet}
    {#if !list}
      {#if loadFailed}
        <p class="muted">{t('ai.providers.load_failed')}</p>
      {:else}
        <Skeleton shape="card" height="120px" />
      {/if}
    {:else if list.providers.length === 0}
      <EmptyState size="sm" icon={Bot} title={t('ai.providers.none')} description={t('ai.providers.none_hint')} />
    {:else}
      <ul class="providers">
        {#each list.providers as p (p.id)}
          {@const preset = presetOf(p)}
          {@const test = testOf(p)}
          <li class="provider" data-testid="ai-provider-row" data-status={p.status}>
            <div class="head">
              <Button
                size="sm"
                variant="ghost"
                icon={open[p.id] ? ChevronDown : ChevronRight}
                aria-expanded={open[p.id] ?? false}
                onclick={() => (open[p.id] = !open[p.id])}
              >
                <strong>{p.name}</strong>
              </Button>
              <span class="model">{p.default_model || '—'}</span>
              <div class="chips">
                <Chip size="sm" tone={STATUS_TONE[p.status]} label={td(`ai.providers.status.${p.status}`)} />
                {#if p.is_default}<Chip size="sm" tone="accent" label={t('ai.providers.default')} />{/if}
                {#if p.local}<Chip size="sm" label={t('ai.providers.local')} />{/if}
              </div>
              <div class="row-actions">
                <Button size="sm" loading={testing[p.id] ?? false} disabled={testing[p.id] ?? false} onclick={() => void runTest(p)}>
                  {t('ai.providers.test')}
                </Button>
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
            </div>
            {#if test && (test.outcome !== 'ok' || lastTest[p.id])}
              <TestResult {test} provider={p} />
            {/if}
            {#if open[p.id] || p.status === 'needs_key'}
              <div class="details">
                <div class="key">
                  <p class="key-status" class:ok={p.has_key}>
                    <Icon icon={p.has_key ? CircleCheck : KeyRound} size={16} />
                    <span>
                      {p.has_key ? t('ai.providers.key_set') : p.needs_key ? t('ai.providers.key_missing') : t('ai.providers.key_not_needed')}
                    </span>
                  </p>
                  {#if preset?.key_url && !p.has_key}<KeyLink url={preset.key_url} label={t('ai.setup.key_here')} />{/if}
                  <div class="key-row">
                    <Input type="password" autocomplete="off" label={p.needs_key ? t('ai.providers.key') : t('ai.providers.key_optional')}
                      value={keys[p.id] ?? ''}
                      oninput={(e: Event) => (keys[p.id] = (e.currentTarget as HTMLInputElement).value)}
                    />
                    <Button icon={Save} disabled={!keys[p.id]?.trim()} onclick={() => void saveKey(p)}>{t('ai.providers.save_key')}</Button>
                    {#if p.has_key}
                      <Button variant="ghost" onclick={() => void removeKey(p)}>{t('ai.providers.remove_key')}</Button>
                    {/if}
                  </div>
                </div>
                {#if open[p.id]}
                  <div class="field">
                    <ModelPicker
                      models={p.models}
                      value={p.default_model}
                      onpick={(m) => pickModel(p, m)}
                      onrefresh={() => void refresh(p)}
                      loading={fetching[p.id] ?? false}
                      error={fetchError[p.id] ?? null}
                    />
                  </div>
                  <details class="advanced">
                    <summary>{t('ai.providers.advanced')}</summary>
                    <div class="fields">
                      <Input
                        type="url"
                        label={t('ai.providers.base_url')}
                        hint={t('ai.setup.address_hint')}
                        value={moving[p.id] ?? p.base_url}
                        onchange={(e: Event) => changeUrl(p, (e.currentTarget as HTMLInputElement).value)}
                      />
                      <Input
                        label={t('ai.providers.name')}
                        value={p.name}
                        onchange={(e: Event) => void update(p, { name: (e.currentTarget as HTMLInputElement).value })}
                      />
                    </div>
                    <p class="muted">{t('ai.providers.advanced_note')}</p>
                  </details>
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
                {/if}
              </div>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
  </SettingsSection>

  {#if (list?.providers.length ?? 0) > 1}
    <SettingsSection title={t('ai.providers.default_heading')} description={t('ai.providers.default_intro')} body="rows">
      <FormRow label={t('ai.providers.default_label')}>
        <Select
          label={t('ai.providers.default_label')}
          hideLabel
          size="sm"
          options={providerOptions}
          value={defaultId}
          onchange={(id: string) => {
            const p = list?.providers.find((x) => x.id === id);
            if (p && !p.is_default) void update(p, { is_default: true });
          }}
        />
      </FormRow>
    </SettingsSection>
  {/if}

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

  {#if list}
    <AddProviderDialog bind:open={adding} presets={list.presets} keychain={list.keychain} onclose={() => void load()} />
  {/if}
</div>

<style>
  .lead {
    margin: 0;
    max-width: 70ch;
    color: var(--text-2);
    font-size: var(--text-sm);
  }
  .provider .head {
    flex-wrap: wrap;
  }
  .head .model {
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    color: var(--text-2);
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .chips {
    display: flex;
    gap: var(--space-1);
    flex-wrap: wrap;
    margin-inline-end: auto;
  }
  .row-actions {
    display: flex;
    gap: var(--space-1);
    align-items: center;
  }
  .details {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    padding-top: var(--space-3);
    border-top: 1px solid var(--border);
  }
  .details .key {
    padding-top: 0;
    border-top: 0;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }
  .advanced summary {
    cursor: pointer;
    font-size: var(--text-sm);
    color: var(--text-2);
  }
  .advanced[open] summary {
    margin-bottom: var(--space-3);
  }
  .advanced .muted {
    margin: var(--space-2) 0 0;
  }
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
  .actions,
  .key-row,
  .route {
    display: flex;
    gap: var(--space-2);
    align-items: flex-end;
    flex-wrap: wrap;
  }
  .providers {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }
  .provider {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-3);
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
    background: var(--surface-0);
  }
  .head {
    display: flex;
    align-items: center;
    gap: var(--space-2);
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
    min-height: var(--row-h);
    padding: 2px 0;
    font-size: var(--text-sm);
  }
  .price-row + .price-row {
    border-top: 1px solid var(--border);
  }
  /* The app's table header: 30 px, 12 px, sentence case. */
  .price-row.head {
    min-height: 30px;
    padding: 0;
    color: var(--text-2);
    font-size: var(--text-xs);
    font-weight: var(--weight-medium);
  }
  .price-row.new {
    padding: var(--space-2) 0;
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

<script lang="ts">
  import { api, type ApiError } from '$lib/api';
  import type { AiFeature, AiPriceDto, AiProviderDto, AiProviderList, AiProviderKind, AiRouteDto } from '$lib/api/types';
  import { t, td } from '$lib/i18n/index.svelte';
  import type { ScreenCtx } from '../types';

  let { ctx }: { ctx: ScreenCtx } = $props();

  let list = $state.raw<AiProviderList | null>(null);
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
    } catch (e) {
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
</script>

<section>
  {#if error}
    <p class="error" role="alert">{error}</p>
  {/if}
  {#if notice}
    <p role="status">{notice}</p>
  {/if}
  {#if list && !list.keychain}
    <p class="warn">{t('ai.providers.no_keychain')}</p>
  {:else if list?.keychain_session_only}
    <p class="warn" role="note">{t('ai.providers.session_keychain')}</p>
  {/if}

  <h3>{t('ai.providers.heading')}</h3>
  {#if list && list.providers.length === 0}
    <p>{t('ai.providers.none')}</p>
  {/if}
  {#each list?.providers ?? [] as p (p.id)}
    <article class="provider">
      <header>
        <strong>{p.name}</strong>
        <span class="muted">{td(`ai.kind.${p.kind}`)}</span>
        {#if p.is_default}<span class="badge">{t('ai.providers.default')}</span>{/if}
        {#if p.local}<span class="badge">{t('ai.providers.local')}</span>{/if}
      </header>
      <label>
        {t('ai.providers.base_url')}
        <input
          type="url"
          value={moving[p.id] ?? p.base_url}
          onchange={(e) => changeUrl(p, (e.currentTarget as HTMLInputElement).value)}
        />
      </label>
      {#if moving[p.id] !== undefined}
        <div class="warn move" role="alert">
          <p>{t('ai.providers.move_key', { url: moving[p.id] ?? '' })}</p>
          <div class="actions">
            <button onclick={() => void confirmMove(p, false)}>{t('ai.providers.move_remove_key')}</button>
            <button onclick={() => void confirmMove(p, true)}>{t('ai.providers.move_keep_key')}</button>
            <button onclick={() => delete moving[p.id]}>{t('ai.preview.cancel')}</button>
          </div>
        </div>
      {/if}
      <label>
        {t('ai.providers.model')}
        <input
          type="text"
          list={`models-${p.id}`}
          value={p.default_model}
          onchange={(e) => void update(p, { default_model: (e.currentTarget as HTMLInputElement).value })}
        />
        <datalist id={`models-${p.id}`}>
          {#each p.models as m (m)}<option value={m}></option>{/each}
        </datalist>
      </label>
      {#if p.needs_key || p.has_key}
        <p class="muted">{p.has_key ? t('ai.providers.key_set') : t('ai.providers.key_missing')}</p>
        <label>
          {t('ai.providers.key')}
          <input type="password" autocomplete="off" bind:value={keys[p.id]} />
        </label>
      {/if}
      <div class="actions">
        {#if p.needs_key || p.has_key}
          <button disabled={!keys[p.id]?.trim()} onclick={() => void saveKey(p)}>{t('ai.providers.save_key')}</button>
          {#if p.has_key}
            <button onclick={() => void removeKey(p)}>{t('ai.providers.remove_key')}</button>
          {/if}
        {/if}
        <button onclick={() => void refresh(p)}>{t('ai.providers.refresh')}</button>
        {#if !p.is_default}
          <button onclick={() => void update(p, { is_default: true })}>{t('ai.providers.make_default')}</button>
        {/if}
        <button class="danger" onclick={() => void remove(p)}>{t('ai.providers.delete')}</button>
      </div>
    </article>
  {/each}
  <div class="actions">
    <label>
      {t('ai.providers.kind')}
      <select bind:value={newKind}>
        {#each list?.kinds ?? [] as k (k.kind)}
          <option value={k.kind}>{td(`ai.kind.${k.kind}`)}</option>
        {/each}
      </select>
    </label>
    <button onclick={() => void add()}>{t('ai.providers.add')}</button>
  </div>

  {#if (list?.providers.length ?? 0) > 0}
    <details>
      <summary>{t('ai.routes.heading')}</summary>
      <p class="muted">{t('ai.routes.intro')}</p>
      <table>
        <tbody>
          {#each routes as r (r.feature)}
            <tr>
              <th scope="row">{td(`ai.feature.${r.feature}`)}</th>
              <td>
                <select
                  aria-label={t('ai.routes.provider')}
                  value={r.provider_id ?? ''}
                  onchange={(e) => void setRoute(r.feature, (e.currentTarget as HTMLSelectElement).value || null, r.model)}
                >
                  <option value="">{t('ai.routes.default')}</option>
                  {#each list?.providers ?? [] as p (p.id)}<option value={p.id}>{p.name}</option>{/each}
                </select>
              </td>
              <td>
                <input
                  type="text"
                  aria-label={t('ai.routes.model')}
                  placeholder={t('ai.routes.model_default')}
                  value={r.model ?? ''}
                  onchange={(e) => void setRoute(r.feature, r.provider_id, (e.currentTarget as HTMLInputElement).value)}
                />
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    </details>
  {/if}

  <details>
    <summary>{t('ai.prices.heading')}</summary>
    <p class="muted">{t('ai.prices.intro')}</p>
    <table>
      <thead>
        <tr>
          <th scope="col">{t('ai.prices.model')}</th>
          <th scope="col">{t('ai.prices.input')}</th>
          <th scope="col">{t('ai.prices.output')}</th>
          <th></th>
        </tr>
      </thead>
      <tbody>
        {#each prices as p (p.model)}
          <tr>
            <td>{p.model}</td>
            <td>{p.input_per_mtok}</td>
            <td>{p.output_per_mtok}</td>
            <td><button onclick={() => void deletePrice(p.model)}>{t('ai.prices.delete')}</button></td>
          </tr>
        {/each}
        <tr>
          <td><input type="text" aria-label={t('ai.prices.model')} bind:value={price.model} /></td>
          <td><input type="number" min="0" step="0.01" aria-label={t('ai.prices.input')} bind:value={price.input_per_mtok} /></td>
          <td><input type="number" min="0" step="0.01" aria-label={t('ai.prices.output')} bind:value={price.output_per_mtok} /></td>
          <td>
            <button disabled={!price.model.trim()} onclick={() => void savePrice($state.snapshot(price))}>
              {t('ai.prices.save')}
            </button>
          </td>
        </tr>
      </tbody>
    </table>
  </details>
</section>

<style>
  .provider {
    display: grid;
    gap: var(--fc-space-2);
    border: var(--fc-border-width) solid var(--fc-border);
    border-radius: var(--fc-radius);
    padding: var(--fc-space-3);
    margin-bottom: var(--fc-space-3);
    max-width: 48rem;
  }
  header {
    display: flex;
    gap: var(--fc-space-2);
    align-items: baseline;
  }
  label {
    display: grid;
    gap: var(--fc-space-1);
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: var(--fc-space-2);
    align-items: end;
  }
  .badge {
    font-size: 0.85em;
    padding: 0 var(--fc-space-1);
    border: var(--fc-border-width) solid var(--fc-border);
    border-radius: var(--fc-radius);
  }
  .muted {
    color: var(--fc-text-muted);
  }
  .warn,
  .error,
  .danger {
    color: var(--fc-danger);
  }
</style>

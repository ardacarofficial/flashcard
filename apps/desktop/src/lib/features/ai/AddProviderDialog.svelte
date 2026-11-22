<!--
  "Add provider" (v1.3): a gallery of presets (drawn from `ai.providers.list` → `presets`,
  Rust's registry), then exactly three steps — 1 key (or address for local/custom servers),
  2 model, 3 connection test. The provider is created when step 1 is confirmed (its key can
  only live in the keychain under a provider id); leaving before step 3 removes it again, so
  "cancel" leaves nothing behind.
-->
<script lang="ts">
  import { api, type ApiError } from '$lib/api';
  import type { AiConnectionTest, AiProviderDto, AiProviderPreset } from '$lib/api/types';
  import { t, td } from '$lib/i18n/index.svelte';
  import { Button, Dialog, Globe, Input, Plug, Server, Tile, TriangleAlert, Icon, ChevronLeft } from '$lib/ui';
  import { fetchModels, jobErrorText, testConnection } from './ai';
  import KeyLink from './KeyLink.svelte';
  import ModelPicker from './ModelPicker.svelte';
  import TestResult from './TestResult.svelte';

  interface Props {
    open: boolean;
    presets: readonly AiProviderPreset[];
    /** Keys cannot be stored (no keychain): hosted presets cannot be finished. */
    keychain: boolean;
    onclose: () => void;
  }

  let { open = $bindable(), presets, keychain, onclose }: Props = $props();

  type Step = 'gallery' | 1 | 2 | 3;
  let step = $state<Step>('gallery');
  let preset = $state.raw<AiProviderPreset | null>(null);
  let created = $state.raw<AiProviderDto | null>(null);
  let key = $state('');
  let address = $state('');
  let showAddress = $state(false);
  let model = $state('');
  let models = $state.raw<string[]>([]);
  let modelsLoading = $state(false);
  let modelsError = $state<string | null>(null);
  let busy = $state(false);
  let error = $state<string | null>(null);
  let test = $state.raw<AiConnectionTest | null>(null);

  const isCustom = $derived(preset?.id === 'custom');
  const presetName = $derived(preset ? (preset.name || t('ai.preset.custom.name')) : '');

  function reset() {
    step = 'gallery';
    preset = null;
    created = null;
    key = '';
    address = '';
    showAddress = false;
    model = '';
    models = [];
    modelsLoading = false;
    modelsError = null;
    busy = false;
    error = null;
    test = null;
  }

  function messageOf(e: unknown): string {
    return td((e as ApiError).messageKey ?? 'errors.internal.unexpected');
  }

  function choose(p: AiProviderPreset) {
    preset = p;
    address = p.base_url;
    showAddress = p.local || p.id === 'custom';
    model = p.default_model;
    step = 1;
  }

  /** Every editable column of `p`, with `patch` applied (the update command replaces them all). */
  function update(p: AiProviderDto, patch: Partial<Pick<AiProviderDto, 'base_url' | 'default_model' | 'models'>>) {
    const next = { ...p, ...patch };
    return api.call('ai.providers.update', {
      id: p.id,
      name: p.name,
      base_url: next.base_url,
      default_model: next.default_model,
      models: next.models,
      is_default: p.is_default,
      keep_key: true,
    });
  }

  const step1Ready = $derived(
    !!preset && address.trim() !== '' && (!preset.needs_key || key.trim() !== '' || !!created?.has_key) && !busy,
  );

  async function confirmStep1() {
    if (!preset || !step1Ready) return;
    busy = true;
    error = null;
    try {
      if (!created) {
        created = await api.call('ai.providers.create', {
          kind: preset.kind,
          name: isCustom ? t('ai.preset.custom.name') : null,
          base_url: address.trim(),
          default_model: null,
          is_default: null,
        });
      } else if (address.trim().replace(/\/+$/, '') !== created.base_url) {
        created = await update(created, { base_url: address.trim() });
      }
      if (key.trim()) {
        created = await api.call('ai.providers.set_key', { provider_id: created.id, key: key.trim() });
        key = '';
      }
      model = model || created.default_model;
      models = created.models;
      step = 2;
      void loadModels();
    } catch (e) {
      error = messageOf(e);
    } finally {
      busy = false;
    }
  }

  async function loadModels() {
    if (!created) return;
    modelsLoading = true;
    modelsError = null;
    try {
      const { job, list } = await fetchModels(created.id);
      if (list) models = list.models.length ? list.models : models;
      else modelsError = jobErrorText(job, t('ai.setup.models_unknown_error'));
    } catch (e) {
      modelsError = messageOf(e);
    } finally {
      modelsLoading = false;
    }
  }

  async function confirmStep2() {
    if (!created || !model.trim()) return;
    busy = true;
    error = null;
    try {
      if (created.default_model !== model.trim()) {
        const list = models.includes(model.trim()) ? models : [...models, model.trim()];
        created = await update(created, { default_model: model.trim(), models: list });
      }
      test = null;
      step = 3;
    } catch (e) {
      error = messageOf(e);
    } finally {
      busy = false;
    }
  }

  async function runTest() {
    if (!created) return;
    busy = true;
    error = null;
    test = null;
    try {
      const r = await testConnection(created.id);
      test = r.test;
      if (!r.test) error = jobErrorText(r.job, t('ai.test.job_failed'));
    } catch (e) {
      error = messageOf(e);
    } finally {
      busy = false;
    }
  }

  /** Closing before the model is chosen leaves nothing behind. */
  async function close() {
    const leftover = created !== null && step !== 3 ? created : null;
    reset();
    open = false;
    if (leftover) {
      try {
        await api.call('ai.providers.delete', { id: leftover.id });
      } catch {
        // Already gone or the module was switched off: nothing to clean up.
      }
    }
    onclose();
  }

  function back() {
    error = null;
    if (step === 3) step = 2;
    else if (step === 2) step = 1;
    else if (step === 1 && !created) step = 'gallery';
  }

  const stepTitles = $derived([
    preset?.local || isCustom ? (isCustom ? t('ai.setup.step1_custom') : t('ai.setup.step1_local')) : t('ai.setup.step1_key'),
    t('ai.setup.step2'),
    t('ai.setup.step3'),
  ]);

  const tagsOf = (p: AiProviderPreset) =>
    p.local ? t('ai.preset_tag.local') : p.free ? t('ai.preset_tag.free') : undefined;
  const iconOf = (p: AiProviderPreset) => (p.local ? Server : p.id === 'custom' ? Plug : Globe);
</script>

<Dialog
  bind:open
  size="lg"
  title={step === 'gallery' ? t('ai.setup.title') : t('ai.setup.title_preset', { name: presetName })}
  description={step === 'gallery' ? t('ai.setup.gallery_intro') : undefined}
  onclose={() => void close()}
  footer={step === 'gallery' ? undefined : stepActions}
>
  {#if step === 'gallery'}
    <div class="gallery" data-testid="ai-preset-gallery">
      {#each presets as p (p.id)}
        <Tile
          icon={iconOf(p)}
          title={p.name || t('ai.preset.custom.name')}
          description={td(`ai.preset.${p.id}.description`)}
          meta={tagsOf(p)}
          testid={`ai-preset-${p.id}`}
          onclick={() => choose(p)}
        />
      {/each}
    </div>
  {:else if preset}
    <ol class="steps" aria-label={t('ai.setup.steps')}>
      {#each stepTitles as title, i (i)}
        <li class:current={step === i + 1} class:done={typeof step === 'number' && step > i + 1} aria-current={step === i + 1 ? 'step' : undefined}>
          <span class="n">{i + 1}</span>{title}
        </li>
      {/each}
    </ol>

    {#if step === 1}
      <div class="body">
        {#if preset.local}
          <p class="lead">{t('ai.setup.local_intro', { name: presetName })}</p>
          {#if preset.setup_url}<KeyLink url={preset.setup_url} label={t('ai.setup.download_here')} />{/if}
        {:else if isCustom}
          <p class="lead">{t('ai.setup.custom_intro')}</p>
        {:else}
          <p class="lead">{t('ai.setup.key_intro', { name: presetName })}</p>
          {#if preset.key_url}<KeyLink url={preset.key_url} label={t('ai.setup.key_here')} />{/if}
        {/if}
        {#if showAddress}
          <Input type="url" label={t('ai.providers.base_url')} hint={t('ai.setup.address_hint')} bind:value={address} required />
        {/if}
        {#if preset.needs_key && !keychain}
          <p class="warn" role="alert"><Icon icon={TriangleAlert} size={16} />{t('ai.providers.no_keychain')}</p>
        {/if}
        <Input
          type="password"
          autocomplete="off"
          label={preset.needs_key ? t('ai.providers.key') : t('ai.providers.key_optional')}
          hint={created?.has_key ? t('ai.providers.key_set') : t('ai.setup.key_hint')}
          bind:value={key}
          onkeydown={(e: KeyboardEvent) => e.key === 'Enter' && void confirmStep1()}
        />
        {#if !showAddress}
          <div class="start">
            <Button size="sm" variant="ghost" onclick={() => (showAddress = true)}>{t('ai.providers.advanced')}</Button>
          </div>
        {/if}
      </div>
    {:else if step === 2}
      <div class="body">
        <p class="lead">{t('ai.setup.model_intro')}</p>
        <ModelPicker
          {models}
          value={model}
          onpick={(m) => (model = m)}
          onrefresh={() => void loadModels()}
          loading={modelsLoading}
          error={modelsError}
        />
      </div>
    {:else if step === 3 && created}
      <div class="body">
        <p class="lead">{t('ai.setup.test_intro', { model: created.default_model })}</p>
        <div class="start">
          <Button variant="primary" loading={busy} disabled={busy} onclick={() => void runTest()} data-testid="ai-run-test">
            {t('ai.providers.test')}
          </Button>
        </div>
        {#if test}<TestResult {test} provider={created} />{/if}
      </div>
    {/if}
    {#if error}
      <p class="warn" role="alert"><Icon icon={TriangleAlert} size={16} />{error}</p>
    {/if}
  {/if}

</Dialog>

{#snippet stepActions()}
  {#if step !== 1 || !created}
    <Button variant="ghost" icon={ChevronLeft} disabled={busy} onclick={back}>{t('ai.setup.back')}</Button>
  {/if}
  <span class="spacer"></span>
  {#if step === 1}
    <Button variant="primary" loading={busy} disabled={!step1Ready} onclick={() => void confirmStep1()}>{t('ai.setup.next')}</Button>
  {:else if step === 2}
    <Button variant="primary" loading={busy} disabled={busy || !model.trim()} onclick={() => void confirmStep2()}>{t('ai.setup.next')}</Button>
  {:else}
    <Button variant={test?.outcome === 'ok' ? 'primary' : 'secondary'} disabled={busy} onclick={() => void close()}>{t('ai.setup.done')}</Button>
  {/if}
{/snippet}

<style>
  .gallery {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(15rem, 1fr));
    gap: var(--space-2);
  }
  .steps {
    display: flex;
    gap: var(--space-4);
    flex-wrap: wrap;
    margin: 0 0 var(--space-4);
    padding: 0 0 var(--space-3);
    list-style: none;
    border-bottom: 1px solid var(--border);
    font-size: var(--text-sm);
    color: var(--text-3);
  }
  .steps li {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  .steps li.current {
    color: var(--text-1);
    font-weight: var(--weight-medium);
  }
  .steps li.done {
    color: var(--text-2);
  }
  .n {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 20px;
    height: 20px;
    border: 1px solid var(--border);
    border-radius: 50%;
    font-family: var(--font-mono);
    font-size: var(--text-xs);
  }
  .current .n {
    border-color: var(--accent);
    color: var(--accent-text);
  }
  .body {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }
  .start {
    display: flex;
  }
  .lead {
    margin: 0;
    font-size: var(--text-sm);
    color: var(--text-2);
  }
  .warn {
    display: flex;
    gap: var(--space-2);
    align-items: flex-start;
    margin: var(--space-3) 0 0;
    padding: var(--space-2) var(--space-3);
    border-radius: var(--radius-control);
    background: var(--warning-soft);
    font-size: var(--text-sm);
  }
  .warn :global(svg) {
    flex: none;
    margin-top: 2px;
    color: var(--warning);
  }
  .spacer {
    flex: 1;
  }
</style>

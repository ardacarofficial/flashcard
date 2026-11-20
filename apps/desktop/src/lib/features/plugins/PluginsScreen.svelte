<script lang="ts">
  import { formatDecimal } from '$lib/format';
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import type { ModelList, PackageInfo, PluginDto, PluginLogEntry, PluginSettings } from '$lib/api/types';
  import { t, td } from '$lib/i18n/index.svelte';
  import { pickFile } from '../io/files';
  import type { ScreenCtx } from '../types';
  import { loadPluginTexts, uiText } from './texts.svelte';
  import {
    Button,
    Card,
    Checkbox,
    ChevronDown,
    ChevronUp,
    Chip,
    Dialog,
    Download,
    EmptyState,
    HardDrive,
    Icon,
    Input,
    Play,
    Puzzle,
    ScrollText,
    Select,
    Shield,
    Switch,
    Trash2,
    Upload,
    type Tone,
  } from '$lib/ui';
  import { FormRow, LoadState, SettingsPage, SettingsSection } from '../core/kit';

  let { ctx }: { ctx: ScreenCtx } = $props();

  let plugins = $state<PluginDto[]>([]);
  let models = $state<ModelList | null>(null);
  let loaded = $state(false);
  // Install dialog.
  let pending = $state<{ path: string; info: PackageInfo; grant: Record<string, boolean> } | null>(null);
  // Per-plugin panels.
  let open = $state<string | null>(null);
  let grants = $state<Record<string, boolean>>({});
  let settings = $state<PluginSettings | null>(null);
  let logs = $state<PluginLogEntry[] | null>(null);
  let removing = $state<{ id: string; deleteData: boolean } | null>(null);
  let notice = $state<string | null>(null);

  const STATUS_TONE: Record<PluginDto['status'], Tone> = {
    active: 'success',
    disabled: 'neutral',
    auto_disabled: 'warning',
    broken: 'danger',
  };
  let failed = $state(false);
  let installOpen = $state(false);
  let removeOpen = $state(false);
  $effect(() => {
    installOpen = pending !== null;
  });
  $effect(() => {
    removeOpen = removing !== null;
  });

  /** The plugin's translated name, else its manifest name. */
  const nameOf = (p: { id: string; name: string }) => {
    const key = `plugin.${p.id}.name`;
    return td(key) === key ? p.name : td(key);
  };
  /** A plugin text with its manifest value as fallback. */
  const optionLabel = (id: string, labelKey: string, o: string) => {
    const key = `plugin.${id}.${labelKey}.${o}`;
    return td(key) === key ? o : td(key);
  };
  const removingPlugin = $derived(removing ? plugins.find((p) => p.id === removing?.id) ?? null : null);

  const mb = (bytes: number | bigint) => formatDecimal(Number(bytes) / 1_000_000, 1);

  async function load() {
    const ok = await ctx.guarded(async () => {
      plugins = (await api.call('plugins.list', {})).plugins;
      models = await api.call('plugins.models.list', {});
      loaded = true;
    });
    failed = !ok && !loaded;
  }

  async function chooseFile() {
    const path = await pickFile([{ name: t('plugins.file_filter'), extensions: ['fcplugin'] }], t('plugins.pick_prompt'));
    if (!path) return;
    await ctx.guarded(async () => {
      const info = await api.call('plugins.inspect', { path });
      // Sensitive permissions start unticked: they are asked for separately.
      const grant = Object.fromEntries(info.permissions.map((p) => [p.permission, p.granted || !p.sensitive]));
      pending = { path, info, grant };
    });
  }

  async function install() {
    const p = pending;
    if (!p) return;
    const grant = Object.entries(p.grant).filter(([, on]) => on).map(([k]) => k);
    if (await ctx.guarded(() => api.call('plugins.install', { path: p.path, grant }))) {
      pending = null;
      await loadPluginTexts().catch(() => {});
      await load();
    }
  }

  function toggleOpen(p: PluginDto) {
    if (open === p.id) {
      open = null;
      return;
    }
    open = p.id;
    grants = Object.fromEntries(p.permissions.map((x) => [x.permission, x.granted]));
    settings = null;
    logs = null;
    removing = null;
    notice = null;
    if (p.has_settings) {
      void ctx.guarded(async () => {
        settings = await api.call('plugins.settings.get', { id: p.id });
      });
    }
  }

  async function setEnabled(p: PluginDto, enabled: boolean) {
    if (await ctx.guarded(() => api.call('plugins.set_enabled', { id: p.id, enabled }))) await load();
  }

  async function savePermissions(p: PluginDto) {
    const granted = Object.entries(grants).filter(([, on]) => on).map(([k]) => k);
    if (await ctx.guarded(() => api.call('plugins.set_permissions', { id: p.id, granted }))) {
      notice = t('plugins.saved');
      await load();
    }
  }

  async function setSetting(id: string, key: string, value: unknown) {
    await ctx.guarded(async () => {
      settings = await api.call('plugins.settings.set', { id, key, value });
    });
  }

  async function showLogs(id: string) {
    await ctx.guarded(async () => {
      logs = (await api.call('plugins.logs', { id })).entries;
    });
  }

  async function run(p: PluginDto, command: string) {
    notice = null;
    await ctx.guarded(async () => {
      const r = await api.call('plugins.run', { plugin_id: p.id, command, params: null });
      notice = r.notices.map((n) => uiText(n.plugin_id, n.text)).join(' ') || t('plugins.saved');
    });
  }

  async function uninstall() {
    const r = removing;
    if (!r) return;
    if (await ctx.guarded(() => api.call('plugins.uninstall', { id: r.id, delete_data: r.deleteData }))) {
      removing = null;
      open = null;
      await load();
    }
  }

  async function model(kind: 'download' | 'delete', plugin_id: string, id: string) {
    await ctx.guarded(() =>
      kind === 'download'
        ? api.call('plugins.models.download', { plugin_id, id })
        : api.call('plugins.models.delete', { plugin_id, id }),
    );
    await load();
  }

  onMount(() => {
    void load();
    const unsub = api.on(['plugins_changed', 'job_progress'], () => void load());
    return () => void unsub.then((u) => u());
  });
</script>

{#snippet perm(p: { permission: string; sensitive: boolean }, checked: boolean, set: (v: boolean) => void)}
  <li>
    <Checkbox {checked} onchange={set}>
      <span class="perm"><code>{p.permission}</code>
        {#if p.sensitive}<Chip size="sm" tone="warning" icon={Shield} label={t('plugins.sensitive')} />{/if}</span>
    </Checkbox>
  </li>
{/snippet}

<SettingsPage title={t('plugins.heading')} subtitle={t('plugins.intro')}>
  {#snippet actions()}
    <Button variant="primary" icon={Upload} onclick={chooseFile}>{t('plugins.install')}</Button>
  {/snippet}

  {#if !loaded}
    <Card padding="lg">
      {#if failed}<LoadState state="error" retry={() => void load()} />{:else}<LoadState state="loading" />{/if}
    </Card>
  {:else if plugins.length === 0}
    <Card padding="lg">
      <EmptyState icon={Puzzle} title={t('plugins.none')} description={t('plugins.empty_hint')}
        action={{ label: t('plugins.install'), onclick: () => void chooseFile(), icon: Upload }} />
    </Card>
  {:else}
    <ul class="plugins" aria-label={t('plugins.installed')}>
      {#each plugins as p (p.id)}
        {@const expanded = open === p.id}
        <Card as="li" padding="none" class="plugin">
          <div class="head">
            <span class="badge" aria-hidden="true"><Icon icon={Puzzle} size={20} /></span>
            <div class="titles">
              <button class="name" aria-expanded={expanded} aria-controls={`plugin-${p.id}`} onclick={() => toggleOpen(p)}>
                <span>{nameOf(p)}</span>
                <Icon icon={expanded ? ChevronUp : ChevronDown} size={16} />
              </button>
              <p class="sub">
                <span>{t('plugins.version', { version: p.version })}</span>
                {#if p.description}<span class="dot" aria-hidden="true">·</span><span>{p.description}</span>{/if}
              </p>
            </div>
            <div class="side">
              <!-- On / off is the switch's job; the chip only flags trouble. -->
              {#if p.status === 'auto_disabled' || p.status === 'broken'}
                <Chip size="sm" tone={STATUS_TONE[p.status]} label={t(`plugins.status.${p.status}`)} />
              {/if}
              {#if p.status === 'auto_disabled'}
                <Button size="sm" onclick={() => setEnabled(p, true)}>{t('plugins.enable')}</Button>
              {/if}
              <Switch checked={p.enabled} ariaLabel={p.enabled ? t('plugins.disable') : t('plugins.enable')}
                onchange={(on) => void setEnabled(p, on)} />
            </div>
          </div>
          {#if p.last_error && p.status !== 'active'}
            <p class="error" role="alert">{t('plugins.last_error', { error: p.last_error })}</p>
          {/if}

          {#if expanded}
            <div class="details" id={`plugin-${p.id}`}>
              {#if notice}<p class="notice" role="status">{notice}</p>{/if}
              {#if p.contributes.commands.length}
                <div class="block">
                  <h3>{t('plugins.contrib.commands')}</h3>
                  <div class="row">
                    {#each p.contributes.commands as c (c)}
                      <Button size="sm" icon={Play} onclick={() => run(p, c)}>{td(`plugin.${p.id}.commands.${c}`)}</Button>
                    {/each}
                  </div>
                </div>
              {/if}

              <div class="block">
                <h3>{t('plugins.permissions')}</h3>
                {#if p.permissions.length === 0}
                  <p class="muted">{t('plugins.no_permissions')}</p>
                {:else}
                  <ul class="perms">
                    {#each p.permissions as x (x.permission)}
                      {@render perm(x, grants[x.permission] ?? false, (v) => (grants[x.permission] = v))}
                    {/each}
                  </ul>
                  <div><Button size="sm" onclick={() => savePermissions(p)}>{t('plugins.save_permissions')}</Button></div>
                {/if}
              </div>

              {#if settings && settings.id === p.id && settings.schema.length}
                <div class="block flush">
                  <h3>{t('plugins.settings')}</h3>
                  {#each settings.schema as def (def.key)}
                    {@const value = settings.values[def.key]}
                    {@const label = td(`plugin.${p.id}.${def.label_key}`)}
                    <FormRow {label}>
                      {#if def.kind.type === 'bool'}
                        <Switch checked={value === true} ariaLabel={label} onchange={(v) => setSetting(p.id, def.key, v)} />
                      {:else if def.kind.type === 'choice'}
                        <Select hideLabel {label} value={String(value)}
                          options={def.kind.options.map((o) => ({ value: o, label: optionLabel(p.id, def.label_key, o) }))}
                          onchange={(v) => setSetting(p.id, def.key, v)} />
                      {:else if def.kind.type === 'integer' || def.kind.type === 'float'}
                        <Input hideLabel {label} type="number" value={Number(value)}
                          onchange={(e: Event) => setSetting(p.id, def.key, Number((e.currentTarget as HTMLInputElement).value))} />
                      {:else}
                        <Input hideLabel {label} value={String(value ?? '')}
                          onchange={(e: Event) => setSetting(p.id, def.key, (e.currentTarget as HTMLInputElement).value)} />
                      {/if}
                    </FormRow>
                  {/each}
                </div>
              {/if}

              <div class="block">
                <h3>{t('plugins.logs')}</h3>
                {#if logs === null}
                  <div><Button size="sm" variant="ghost" icon={ScrollText} onclick={() => showLogs(p.id)}>{t('plugins.show_logs')}</Button></div>
                {:else if logs.length === 0}
                  <p class="muted">{t('plugins.logs_empty')}</p>
                {:else}
                  <pre class="logs">{logs.map((l) => `${l.level}  ${l.message}`).join('\n')}</pre>
                {/if}
              </div>

              <div class="block footer">
                <Button size="sm" variant="danger" icon={Trash2} onclick={() => (removing = { id: p.id, deleteData: false })}>{t('plugins.uninstall')}</Button>
              </div>
            </div>
          {/if}
        </Card>
      {/each}
    </ul>
  {/if}

  {#if models && models.models.length}
    <SettingsSection title={t('plugins.models.heading')} icon={HardDrive} description={t('plugins.models.usage', { size: mb(models.disk_usage) })} body="rows">
      {#each models.models as m (`${m.plugin_id}/${m.id}`)}
        <FormRow label={m.name} description={`${t('plugins.models.size', { size: mb(m.size) })} · ${m.license} · ${t(`plugins.models.state.${m.state}`)}`}>
          {#if m.state === 'ready'}
            <Button size="sm" variant="ghost" icon={Trash2} onclick={() => model('delete', m.plugin_id, m.id)}>{t('plugins.models.delete')}</Button>
          {:else}
            <Button size="sm" icon={Download} onclick={() => model('download', m.plugin_id, m.id)}>{t('plugins.models.download')}</Button>
          {/if}
        </FormRow>
      {/each}
    </SettingsSection>
  {/if}
</SettingsPage>

{#if pending}
  {@const info = pending.info}
  <Dialog bind:open={installOpen} title={t('plugins.install_heading', { name: info.name, version: info.version })}
    description={info.description ?? undefined} onclose={() => (pending = null)}>
    {#if info.installed_version}
      <p class="muted">{t('plugins.installed_version', { version: info.installed_version })}</p>
    {/if}
    <p class="muted">{t('plugins.grant_hint')}</p>
    <ul class="perms">
      {#each info.permissions as x (x.permission)}
        {@render perm(x, pending.grant[x.permission] ?? false, (v) => pending && (pending.grant[x.permission] = v))}
      {/each}
    </ul>
    {#snippet footer()}
      <Button onclick={() => (pending = null)}>{t('plugins.cancel')}</Button>
      <Button variant="primary" onclick={install}>
        {info.installed_version ? t('plugins.update_confirm') : t('plugins.install_confirm')}
      </Button>
    {/snippet}
  </Dialog>
{/if}

{#if removing && removingPlugin}
  <Dialog bind:open={removeOpen} role="alertdialog" size="sm" title={t('plugins.uninstall')}
    description={t('plugins.confirm_uninstall', { name: removingPlugin.name })} onclose={() => (removing = null)}>
    <Checkbox label={t('plugins.delete_data')} checked={removing.deleteData}
      onchange={(v) => removing && (removing.deleteData = v)} />
    {#snippet footer()}
      <Button onclick={() => (removing = null)}>{t('plugins.cancel')}</Button>
      <Button variant="danger" onclick={uninstall}>{t('plugins.uninstall')}</Button>
    {/snippet}
  </Dialog>
{/if}

<style>
  .plugins {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }
  .head {
    display: flex;
    align-items: center;
    gap: var(--space-4);
    padding: var(--space-4) var(--space-6);
    flex-wrap: wrap;
  }
  .badge {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 40px;
    height: 40px;
    flex: none;
    border-radius: var(--radius-control);
    background: var(--accent-soft);
    color: var(--accent);
  }
  .titles {
    flex: 1 1 14rem;
    min-width: 0;
  }
  .name {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    padding: 0;
    border: none;
    background: none;
    color: var(--text-1);
    font: inherit;
    font-size: var(--text-md);
    font-weight: var(--weight-semibold);
    cursor: pointer;
    border-radius: var(--radius-control);
  }
  .name :global(svg) {
    color: var(--text-2);
  }
  .name:hover span {
    text-decoration: underline;
  }
  .name:focus-visible {
    outline: 2px solid var(--focus-ring);
    outline-offset: 2px;
  }
  .sub {
    margin: 2px 0 0;
    color: var(--text-2);
    font-size: var(--text-sm);
    display: flex;
    flex-wrap: wrap;
    gap: 0 var(--space-1);
  }
  .side {
    display: flex;
    align-items: center;
    gap: var(--space-3);
  }
  .error {
    margin: 0;
    padding: 0 var(--space-6) var(--space-4);
    color: var(--danger);
    font-size: var(--text-sm);
  }
  .details {
    border-top: 1px solid var(--border);
    padding: var(--space-2) 0;
  }
  .block {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    padding: var(--space-3) var(--space-6);
  }
  .block.flush {
    padding-inline: 0;
  }
  .block.flush h3 {
    padding-inline: var(--space-6);
  }
  .block h3 {
    margin: 0;
    font-size: var(--text-xs);
    font-weight: var(--weight-semibold);
    color: var(--text-2);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  .block.footer {
    align-items: flex-start;
  }
  .row {
    display: flex;
    gap: var(--space-2);
    flex-wrap: wrap;
  }
  .perms {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }
  .perm {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    flex-wrap: wrap;
  }
  code {
    font-family: var(--font-mono);
    font-size: var(--text-sm);
  }
  .muted {
    margin: 0;
    color: var(--text-2);
    font-size: var(--text-sm);
  }
  .notice {
    margin: var(--space-2) var(--space-6) 0;
    padding: var(--space-2) var(--space-3);
    border-radius: var(--radius-control);
    background: var(--success-soft);
    color: var(--success);
    font-size: var(--text-sm);
  }
  .logs {
    margin: 0;
    max-height: 12rem;
    overflow: auto;
    padding: var(--space-3);
    border-radius: var(--radius-control);
    background: var(--surface-2);
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    color: var(--text-1);
  }
  @media (max-width: 1023.98px) {
    .head,
    .block {
      padding-inline: var(--space-4);
    }
    .block.flush h3 {
      padding-inline: var(--space-4);
    }
  }
</style>

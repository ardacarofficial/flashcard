<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import type { ModelList, PackageInfo, PluginDto, PluginLogEntry, PluginSettings } from '$lib/api/types';
  import { t, td } from '$lib/i18n/index.svelte';
  import { pickFile } from '../io/files';
  import type { ScreenCtx } from '../types';
  import { loadPluginTexts, uiText } from './texts.svelte';

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

  const mb = (bytes: number | bigint) => (Number(bytes) / 1_000_000).toFixed(1);

  async function load() {
    await ctx.guarded(async () => {
      plugins = (await api.call('plugins.list', {})).plugins;
      models = await api.call('plugins.models.list', {});
      loaded = true;
    });
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

<h2>{t('plugins.heading')}</h2>
<p class="muted">{t('plugins.intro')}</p>
<button onclick={chooseFile}>{t('plugins.install')}</button>

{#if pending}
  <section class="dialog" aria-labelledby="install-heading">
    <h3 id="install-heading">{t('plugins.install_heading', { name: pending.info.name, version: pending.info.version })}</h3>
    {#if pending.info.description}<p>{pending.info.description}</p>{/if}
    {#if pending.info.installed_version}
      <p class="muted">{t('plugins.installed_version', { version: pending.info.installed_version })}</p>
    {/if}
    <p class="muted">{t('plugins.grant_hint')}</p>
    <ul class="perms">
      {#each pending.info.permissions as perm (perm.permission)}
        <li>
          <label>
            <input type="checkbox" bind:checked={pending.grant[perm.permission]} />
            <code>{perm.permission}</code>
            {#if perm.sensitive}<small class="sensitive">{t('plugins.sensitive')}</small>{/if}
          </label>
        </li>
      {/each}
    </ul>
    <div class="row">
      <button class="primary" onclick={install}>
        {pending.info.installed_version ? t('plugins.update_confirm') : t('plugins.install_confirm')}
      </button>
      <button onclick={() => (pending = null)}>{t('plugins.cancel')}</button>
    </div>
  </section>
{/if}

{#if loaded && plugins.length === 0}
  <p class="muted">{t('plugins.none')}</p>
{/if}

<ul class="plugins">
  {#each plugins as p (p.id)}
    <li class="plugin">
      <div class="head">
        <button class="link" aria-expanded={open === p.id} onclick={() => toggleOpen(p)}>
          <strong>{td(`plugin.${p.id}.name`) === `plugin.${p.id}.name` ? p.name : td(`plugin.${p.id}.name`)}</strong>
        </button>
        <span class="muted">{t('plugins.version', { version: p.version })}</span>
        <span class="status {p.status}">{t(`plugins.status.${p.status}`)}</span>
        {#if p.enabled}
          <button onclick={() => setEnabled(p, false)}>{t('plugins.disable')}</button>
        {:else}
          <button onclick={() => setEnabled(p, true)}>{t('plugins.enable')}</button>
        {/if}
        {#if p.status === 'auto_disabled'}
          <button onclick={() => setEnabled(p, true)}>{t('plugins.enable')}</button>
        {/if}
      </div>
      {#if p.description}<p class="muted">{p.description}</p>{/if}
      {#if p.last_error && p.status !== 'active'}<p class="error">{t('plugins.last_error', { error: p.last_error })}</p>{/if}

      {#if open === p.id}
        <div class="details">
          {#if notice}<p role="status">{notice}</p>{/if}
          {#if p.contributes.commands.length}
            <div class="row">
              {#each p.contributes.commands as c (c)}
                <button onclick={() => run(p, c)}>{td(`plugin.${p.id}.commands.${c}`)}</button>
              {/each}
            </div>
          {/if}

          <h4>{t('plugins.permissions')}</h4>
          <ul class="perms">
            {#each p.permissions as perm (perm.permission)}
              <li>
                <label>
                  <input type="checkbox" bind:checked={grants[perm.permission]} />
                  <code>{perm.permission}</code>
                  {#if perm.sensitive}<small class="sensitive">{t('plugins.sensitive')}</small>{/if}
                </label>
              </li>
            {/each}
          </ul>
          <button onclick={() => savePermissions(p)}>{t('plugins.save_permissions')}</button>

          {#if settings && settings.id === p.id && settings.schema.length}
            <h4>{t('plugins.settings')}</h4>
            {#each settings.schema as def (def.key)}
              {@const value = settings.values[def.key]}
              <label class="setting">
                {td(`plugin.${p.id}.${def.label_key}`)}
                {#if def.kind.type === 'bool'}
                  <input type="checkbox" checked={value === true} onchange={(e) => setSetting(p.id, def.key, e.currentTarget.checked)} />
                {:else if def.kind.type === 'choice'}
                  <select value={String(value)} onchange={(e) => setSetting(p.id, def.key, e.currentTarget.value)}>
                    {#each def.kind.options as o (o)}<option value={o}>{td(`plugin.${p.id}.${def.label_key}.${o}`) === `plugin.${p.id}.${def.label_key}.${o}` ? o : td(`plugin.${p.id}.${def.label_key}.${o}`)}</option>{/each}
                  </select>
                {:else if def.kind.type === 'integer' || def.kind.type === 'float'}
                  <input type="number" value={Number(value)} onchange={(e) => setSetting(p.id, def.key, Number(e.currentTarget.value))} />
                {:else}
                  <input value={String(value ?? '')} onchange={(e) => setSetting(p.id, def.key, e.currentTarget.value)} />
                {/if}
              </label>
            {/each}
          {/if}

          <h4>{t('plugins.logs')}</h4>
          {#if logs === null}
            <button onclick={() => showLogs(p.id)}>{t('plugins.logs')}</button>
          {:else if logs.length === 0}
            <p class="muted">{t('plugins.logs_empty')}</p>
          {:else}
            <pre class="logs">{logs.map((l) => `${l.level}  ${l.message}`).join('\n')}</pre>
          {/if}

          {#if removing?.id === p.id}
            <div class="row danger-zone">
              <p>{t('plugins.confirm_uninstall', { name: p.name })}</p>
              <label><input type="checkbox" bind:checked={removing.deleteData} /> {t('plugins.delete_data')}</label>
              <button class="danger" onclick={uninstall}>{t('plugins.uninstall')}</button>
              <button onclick={() => (removing = null)}>{t('plugins.cancel')}</button>
            </div>
          {:else}
            <button class="danger" onclick={() => (removing = { id: p.id, deleteData: false })}>{t('plugins.uninstall')}</button>
          {/if}
        </div>
      {/if}
    </li>
  {/each}
</ul>

{#if models && models.models.length}
  <h3>{t('plugins.models.heading')}</h3>
  <p class="muted">{t('plugins.models.usage', { size: mb(models.disk_usage) })}</p>
  <ul>
    {#each models.models as m (`${m.plugin_id}/${m.id}`)}
      <li class="row">
        <span>{m.name}</span>
        <span class="muted">{t('plugins.models.size', { size: mb(m.size) })} · {m.license}</span>
        <span>{t(`plugins.models.state.${m.state}`)}</span>
        {#if m.state === 'ready'}
          <button onclick={() => model('delete', m.plugin_id, m.id)}>{t('plugins.models.delete')}</button>
        {:else}
          <button onclick={() => model('download', m.plugin_id, m.id)}>{t('plugins.models.download')}</button>
        {/if}
      </li>
    {/each}
  </ul>
{/if}

<style>
  .plugins,
  .perms {
    list-style: none;
    padding: 0;
  }
  .plugin {
    border-bottom: var(--fc-border-width) solid var(--fc-border);
    padding: var(--fc-space-3) 0;
  }
  .head,
  .row {
    display: flex;
    gap: var(--fc-space-2);
    align-items: center;
    flex-wrap: wrap;
  }
  .link {
    border: none;
    background: none;
    padding: 0;
  }
  .status.auto_disabled,
  .status.broken {
    color: var(--fc-danger, firebrick);
  }
  .sensitive {
    color: var(--fc-danger, firebrick);
  }
  .details {
    padding-left: var(--fc-space-4);
  }
  .setting {
    display: block;
    margin: var(--fc-space-2) 0;
  }
  .logs {
    max-height: 12rem;
    overflow: auto;
    font-size: 0.85em;
  }
  .dialog {
    border: var(--fc-border-width) solid var(--fc-border);
    padding: var(--fc-space-4);
    margin: var(--fc-space-4) 0;
  }
</style>

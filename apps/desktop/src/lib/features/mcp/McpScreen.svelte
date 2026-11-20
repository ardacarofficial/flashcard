<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import ConfirmDialog from '$lib/components/ConfirmDialog.svelte';
  import type { McpServerDto, McpToolList, McpTransport } from '$lib/api/types';
  import { t, td } from '$lib/i18n/index.svelte';
  import type { ScreenCtx } from '../types';
  import { claudeCodeCommand, claudeDesktopConfig } from './snippets';

  let { ctx }: { ctx: ScreenCtx } = $props();

  let tools = $state<McpToolList | null>(null);
  let servers = $state<McpServerDto[]>([]);

  // Add-server form.
  let name = $state('');
  let transport = $state<McpTransport>('stdio');
  let command = $state('');
  let args = $state('');
  let url = $state('');

  /** What the user is asked to approve before the app may launch a program. */
  let pending = $state<{ server: McpServerDto | null } | null>(null);

  const launches = (s: { transport: McpTransport; command: string; args: string[]; url: string | null }) =>
    s.transport === 'stdio' ? [s.command, ...s.args].join(' ') : (s.url ?? '');

  const lines = (s: string) =>
    s
      .split('\n')
      .map((l) => l.trim())
      .filter(Boolean);

  async function load() {
    await ctx.guarded(async () => {
      tools = await api.call('mcp.tools.list', {});
      servers = (await api.call('mcp.servers.list', {})).servers;
    });
  }

  async function setWrites(allowed: boolean) {
    await ctx.guarded(() =>
      api.call('settings.set', { scope: { kind: 'app' }, key: 'mcp.server.allow_writes', value: allowed }),
    );
    await load();
  }

  async function add() {
    pending = null;
    const ok = await ctx.guarded(() =>
      api.call('mcp.servers.add', {
        name,
        transport,
        command: transport === 'stdio' ? command : null,
        args: transport === 'stdio' ? lines(args) : [],
        url: transport === 'http' ? url : null,
        enabled: true,
      }),
    );
    if (ok) {
      name = command = args = url = '';
      await load();
    }
  }

  /** Saving a row again is the user's approval of exactly what it now says. */
  async function approve(s: McpServerDto) {
    pending = null;
    await ctx.guarded(() =>
      api.call('mcp.servers.update', {
        id: s.id,
        name: s.name,
        transport: s.transport,
        command: s.command,
        args: s.args,
        url: s.url,
        enabled: s.enabled,
      }),
    );
    await load();
  }

  async function toggle(s: McpServerDto) {
    await ctx.guarded(() =>
      api.call('mcp.servers.update', {
        id: s.id,
        name: s.name,
        transport: s.transport,
        command: s.command,
        args: s.args,
        url: s.url,
        enabled: !s.enabled,
      }),
    );
    await load();
  }

  async function remove(s: McpServerDto) {
    await ctx.guarded(() => api.call('mcp.servers.remove', { id: s.id }));
    await load();
  }

  onMount(() => {
    void load();
    // Claude (or the CLI) may have changed things from outside; undo/redo changes the list too.
    const unsub = api.on(['data_changed', 'undone', 'redone'], () => void load());
    return () => void unsub.then((u) => u());
  });
</script>

<h2>{t('mcp.heading')}</h2>

<section aria-labelledby="mcp-server">
  <h3 id="mcp-server">{t('mcp.server.heading')}</h3>
  <p class="muted">{t('mcp.server.intro')}</p>
  {#if tools}
    <label class="check">
      <input
        type="checkbox"
        checked={tools.writes_allowed}
        onchange={(e) => void setWrites(e.currentTarget.checked)}
      />
      {t('mcp.server.allow_writes')}
    </label>
    {#if !tools.writes_allowed}<p class="muted" role="status">{t('mcp.server.read_only_note')}</p>{/if}

    <details>
      <summary>{t('mcp.server.setup')}</summary>
      <p>{t('mcp.server.claude_code')}</p>
      <pre><code>{claudeCodeCommand()}</code></pre>
      <p>{t('mcp.server.claude_desktop')}</p>
      <pre><code>{claudeDesktopConfig()}</code></pre>
      <p class="muted">{t('mcp.server.db_hint')}</p>
    </details>

    <h4>{t('mcp.server.tools')}</h4>
    <ul class="tools">
      {#each tools.tools as tool (tool.name)}
        <li class:off={tool.access === 'write' && !tools.writes_allowed}>
          {td(`mcp.tools.${tool.name}`)}
          <span class="muted">
            · {t(tool.access === 'read' ? 'mcp.server.read' : 'mcp.server.write')}{#if tool.destructive}
              · {t('mcp.server.changes_content')}{/if}
          </span>
        </li>
      {/each}
    </ul>
  {/if}
</section>

<section aria-labelledby="mcp-client">
  <h3 id="mcp-client">{t('mcp.client.heading')}</h3>
  <p class="muted">{t('mcp.client.intro')}</p>
  {#if servers.length === 0}
    <p class="muted">{t('mcp.client.none')}</p>
  {:else}
    <ul>
      {#each servers as s (s.id)}
        <li>
          <label class="check">
            <input
              type="checkbox"
              checked={s.enabled}
              disabled={!s.approved}
              onchange={() => void toggle(s)}
            />
            <strong>{s.name}</strong>
          </label>
          <code>{launches(s)}</code>
          {#if !s.approved}
            <span class="warn" role="status">{t('mcp.client.not_approved')}</span>
            <button type="button" onclick={() => (pending = { server: s })}>{t('mcp.client.approve')}</button>
          {/if}
          <button type="button" class="danger" onclick={() => void remove(s)}>{t('mcp.client.remove')}</button>
        </li>
      {/each}
    </ul>
  {/if}

  <form
    onsubmit={(e) => {
      e.preventDefault();
      pending = { server: null };
    }}
  >
    <label>{t('mcp.client.name')} <input bind:value={name} /></label>
    <label>
      {t('mcp.client.transport')}
      <select bind:value={transport}>
        <option value="stdio">{t('mcp.client.stdio')}</option>
        <option value="http">{t('mcp.client.http')}</option>
      </select>
    </label>
    {#if transport === 'stdio'}
      <label>{t('mcp.client.command')} <input bind:value={command} /></label>
      <label>{t('mcp.client.args')} <textarea rows="3" bind:value={args}></textarea></label>
    {:else}
      <label>{t('mcp.client.url')} <input type="url" bind:value={url} /></label>
    {/if}
    <button type="submit" disabled={!name.trim()}>{t('mcp.client.add')}</button>
  </form>
</section>

{#if pending}
  {@const target = pending.server}
  <ConfirmDialog
    message={t(target ? 'mcp.client.confirm_approve' : 'mcp.client.confirm_add', {
      command: target ? launches(target) : launches({ transport, command, args: lines(args), url }),
    })}
    confirmLabel={t(target ? 'mcp.client.trust_approve' : 'mcp.client.trust_add')}
    onconfirm={() => void (target ? approve(target) : add())}
    oncancel={() => (pending = null)}
  />
{/if}

<style>
  .warn {
    color: var(--fc-danger, inherit);
  }
  section {
    margin-bottom: var(--fc-space-6);
  }
  ul {
    list-style: none;
    padding: 0;
  }
  li {
    margin: var(--fc-space-1) 0;
    display: flex;
    gap: var(--fc-space-2);
    align-items: center;
    flex-wrap: wrap;
  }
  .tools li.off {
    opacity: 0.5;
  }
  label {
    display: block;
    margin: var(--fc-space-2) 0;
  }
  label input:not([type='checkbox']),
  label select,
  label textarea {
    display: block;
    width: 100%;
    max-width: 24rem;
  }
  li label.check {
    margin: 0;
  }
  pre {
    overflow-x: auto;
    padding: var(--fc-space-2);
    background: var(--fc-surface-2, transparent);
  }
</style>

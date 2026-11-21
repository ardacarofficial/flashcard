<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import type { McpServerDto, McpToolList, McpTransport } from '$lib/api/types';
  import { t, td } from '$lib/i18n/index.svelte';
  import type { ScreenCtx } from '../types';
  import { claudeCodeCommand, claudeDesktopConfig } from './snippets';
  import {
    Button,
    ChevronDown,
    ChevronUp,
    Chip,
    Dialog,
    Icon,
    Input,
    Plus,
    SegmentedControl,
    Server,
    Skeleton,
    Switch,
    Terminal,
    Textarea,
    TriangleAlert,
    Bot,
  } from '$lib/ui';
  import { FormRow, SettingsPage, SettingsSection } from '../core/kit';

  let { ctx }: { ctx: ScreenCtx } = $props();

  let tools = $state<McpToolList | null>(null);
  let servers = $state<McpServerDto[] | null>(null);
  let showSetup = $state(false);

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
      const [nextTools, list] = await Promise.all([api.call('mcp.tools.list', {}), api.call('mcp.servers.list', {})]);
      tools = nextTools;
      servers = list.servers;
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
  const transports = $derived([
    { value: 'stdio' as McpTransport, label: t('mcp.client.stdio') },
    { value: 'http' as McpTransport, label: t('mcp.client.http') },
  ]);
</script>

<SettingsPage title={t('mcp.heading')} subtitle={t('mcp.subtitle')}>
  <SettingsSection title={t('mcp.server.heading')} description={t('mcp.server.intro')} icon={Bot} body="rows">
    {#if !tools}
      <div class="pad"><Skeleton lines={3} /></div>
    {:else}
      <FormRow
        label={t('mcp.server.allow_writes')}
        description={tools.writes_allowed ? t('settings.mcp.server.allow_writes.description') : t('mcp.server.read_only_note')}
      >
        <Switch ariaLabel={t('mcp.server.allow_writes')} checked={tools.writes_allowed} onchange={(on) => void setWrites(on)} />
      </FormRow>
      <FormRow label={t('mcp.server.setup')} description={t('mcp.server.setup_hint')} stack={showSetup}>
        <Button
          size="sm"
          variant="secondary"
          iconEnd={showSetup ? ChevronUp : ChevronDown}
          aria-expanded={showSetup}
          aria-controls="mcp-setup"
          onclick={() => (showSetup = !showSetup)}
        >
          {showSetup ? t('mcp.server.hide_setup') : t('mcp.server.show_setup')}
        </Button>
        {#if showSetup}
          <div id="mcp-setup" class="setup">
            <p class="sub">{t('mcp.server.claude_code')}</p>
            <pre><code>{claudeCodeCommand()}</code></pre>
            <p class="sub">{t('mcp.server.claude_desktop')}</p>
            <pre><code>{claudeDesktopConfig()}</code></pre>
            <p class="muted">{t('mcp.server.db_hint')}</p>
          </div>
        {/if}
      </FormRow>
      <FormRow label={t('mcp.server.tools')} stack>
        <ul class="tools">
          {#each tools.tools as tool (tool.name)}
            {@const off = tool.access === 'write' && !tools.writes_allowed}
            <li class:off>
              <span class="tool-name">{td(`mcp.tools.${tool.name}`)}</span>
              <span class="tags">
                <Chip size="sm" tone={tool.access === 'read' ? 'neutral' : 'accent'} label={t(tool.access === 'read' ? 'mcp.server.read' : 'mcp.server.write')} />
                {#if tool.destructive}<Chip size="sm" tone="warning" label={t('mcp.server.changes_content')} />{/if}
              </span>
            </li>
          {/each}
        </ul>
      </FormRow>
    {/if}
  </SettingsSection>

  <SettingsSection title={t('mcp.client.heading')} description={t('mcp.client.intro')} icon={Server} body="rows">
    {#if servers === null}
      <div class="pad"><Skeleton lines={2} /></div>
    {:else if servers.length === 0}
      <p class="empty"><Icon icon={Server} size={16} /><span>{t('mcp.client.none')}</span></p>
    {:else}
      <ul class="servers">
        {#each servers as s (s.id)}
          <li>
            <div class="server-text">
              <span class="server-name">{s.name}</span>
              <code>{launches(s)}</code>
            </div>
            <div class="server-actions">
              {#if !s.approved}
                <span class="warn" role="status"><Icon icon={TriangleAlert} size={16} />{t('mcp.client.not_approved')}</span>
                <Button size="sm" onclick={() => (pending = { server: s })}>{t('mcp.client.approve')}</Button>
              {/if}
              <Button size="sm" variant="ghost" onclick={() => void remove(s)}>{t('mcp.client.remove')}</Button>
              <Switch ariaLabel={s.name} checked={s.enabled} disabled={!s.approved} onchange={() => void toggle(s)} />
            </div>
          </li>
        {/each}
      </ul>
    {/if}

    <form
      class="add"
      onsubmit={(e) => {
        e.preventDefault();
        pending = { server: null };
      }}
    >
      <h3>{t('mcp.client.add_heading')}</h3>
      <div class="grid">
        <Input size="sm" label={t('mcp.client.name')} bind:value={name} />
        <div class="transport">
          <span class="label" aria-hidden="true">{t('mcp.client.transport')}</span>
          <SegmentedControl size="sm" label={t('mcp.client.transport')} options={transports} bind:value={transport} />
        </div>
      </div>
      {#if transport === 'stdio'}
        <Input size="sm" label={t('mcp.client.command')} icon={Terminal} bind:value={command} />
        <Textarea label={t('mcp.client.args')} rows={2} autoGrow bind:value={args} />
      {:else}
        <Input size="sm" type="url" label={t('mcp.client.url')} bind:value={url} />
      {/if}
      <div class="row-end">
        <Button size="sm" type="submit" variant="primary" icon={Plus} disabled={!name.trim()}>{t('mcp.client.add')}</Button>
      </div>
    </form>
  </SettingsSection>
</SettingsPage>

{#if pending}
  {@const target = pending.server}
  <Dialog
    open
    role="alertdialog"
    size="sm"
    title={t(target ? 'mcp.client.approve_title' : 'mcp.client.add_title')}
    onclose={() => (pending = null)}
  >
    <p class="confirm">
      {t(target ? 'mcp.client.confirm_approve' : 'mcp.client.confirm_add', {
        command: target ? launches(target) : launches({ transport, command, args: lines(args), url }),
      })}
    </p>
    {#snippet footer()}
      <Button variant="ghost" onclick={() => (pending = null)}>{t('common.cancel')}</Button>
      <Button variant="danger" onclick={() => void (target ? approve(target) : add())}>
        {t(target ? 'mcp.client.trust_approve' : 'mcp.client.trust_add')}
      </Button>
    {/snippet}
  </Dialog>
{/if}


<style>
  .pad {
    padding: var(--space-3) 0;
  }
  .setup {
    width: 100%;
  }
  .sub {
    margin: var(--space-3) 0 var(--space-1);
    font-size: var(--text-xs);
    font-weight: var(--weight-medium);
    color: var(--text-2);
  }
  pre {
    margin: 0;
    padding: var(--space-2) var(--space-3);
    overflow-x: auto;
    background: var(--surface-2);
    border: 1px solid var(--border);
    border-radius: var(--radius-control);
    font-family: var(--font-mono);
    font-size: var(--text-xs);
  }
  .muted {
    margin: var(--space-2) 0 0;
    color: var(--text-2);
    font-size: var(--text-xs);
  }
  .tools {
    list-style: none;
    margin: 0;
    padding: 0;
    width: 100%;
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(18rem, 1fr));
    gap: 0 var(--space-6);
  }
  .tools li {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
    min-height: 32px;
    font-size: var(--text-sm);
    /* Lines between tools; the row's own bottom line closes the list (no double line). */
    border-top: 1px solid var(--border);
  }
  .tools li.off {
    color: var(--text-2);
  }
  .tags {
    display: flex;
    gap: var(--space-1);
    flex: none;
  }
  .empty {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    margin: 0;
    min-height: 44px;
    border-bottom: 1px solid var(--border);
    color: var(--text-2);
    font-size: var(--text-sm);
  }
  .servers {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .servers li {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    min-height: 44px;
    padding: var(--space-1) 0;
    border-bottom: 1px solid var(--border);
    flex-wrap: wrap;
  }
  .server-text {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
    flex: 1 1 12rem;
  }
  .server-name {
    font-size: var(--text-sm);
    font-weight: var(--weight-medium);
  }
  .server-text code {
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    color: var(--text-2);
    overflow-wrap: anywhere;
  }
  .server-actions {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  .warn {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    color: var(--warning);
    font-size: var(--text-xs);
  }
  .add {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    margin-top: var(--space-4);
  }
  .add h3 {
    margin: 0;
    font-size: var(--text-sm);
    font-weight: var(--weight-semibold);
  }
  .grid {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    gap: var(--space-3);
    align-items: end;
  }
  .transport {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: var(--space-1);
  }
  .label {
    font-size: var(--text-xs);
    font-weight: var(--weight-medium);
    color: var(--text-2);
  }
  .row-end {
    display: flex;
    justify-content: flex-end;
  }
  .confirm {
    margin: 0;
    overflow-wrap: anywhere;
  }
  @media (max-width: 639.98px) {
    .grid {
      grid-template-columns: minmax(0, 1fr);
    }
  }
</style>

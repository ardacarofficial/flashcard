<!--
  Settings → MCP (v1.3): two separate jobs on two tabs.
  - "Connect your assistant": pick the client → the exact setting with THIS profile's
    collection file and the `fc` program filled in (`mcp.setup`), copy, where to paste it,
    how to test it; then what the assistant may do (`mcp.tools.list`) with the read-only switch.
  - "External MCP servers": the app as a client — what it is, when you'd want it, why it is
    safe (approval of the exact command), the registered servers and the add form.
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import type { McpServerDto, McpSetup, McpToolInfo, McpToolList, McpTransport } from '$lib/api/types';
  import { t, td } from '$lib/i18n/index.svelte';
  import type { ScreenCtx } from '../types';
  import {
    CLIENTS,
    claudeDesktopConfigFile,
    fcPlaceholder,
    otherClientFields,
    snippetFor,
    type McpClient,
    type SnippetPaths,
  } from './snippets';
  import {
    Bot,
    Button,
    Chip,
    ClipboardCheck,
    CircleHelp,
    Copy,
    Dialog,
    Icon,
    Input,
    Plus,
    SegmentedControl,
    Server,
    Skeleton,
    Switch,
    Tabs,
    Terminal,
    Textarea,
    TriangleAlert,
    toast,
  } from '$lib/ui';
  import { FormRow, SettingsPage, SettingsSection, isDefault, loadDefaults, resetSetting } from '../core/kit';

  let { ctx }: { ctx: ScreenCtx } = $props();

  const WRITES = 'mcp.server.allow_writes';

  let tab = $state<'connect' | 'servers'>('connect');
  let tools = $state<McpToolList | null>(null);
  let servers = $state<McpServerDto[] | null>(null);
  /** Paths the backend reports; `undefined` while loading. */
  let setup = $state<McpSetup | null | undefined>(undefined);
  let client = $state<McpClient>('claude_desktop');
  /** What the user typed when the backend found no `fc` program. */
  let fcTyped = $state('');

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

  async function loadSetup() {
    try {
      setup = await api.call('mcp.setup', {});
    } catch {
      // The snippet still works with the fallbacks (placeholder path, `fc`).
      setup = null;
    }
  }

  async function setWrites(allowed: boolean) {
    await ctx.guarded(() => api.call('settings.set', { scope: { kind: 'app' }, key: WRITES, value: allowed }));
    await load();
  }

  async function resetWrites() {
    await ctx.guarded(() => resetSetting(WRITES));
    await load();
  }

  async function copy(text: string) {
    try {
      await navigator.clipboard.writeText(text);
      toast.show(t('mcp.connect.copied'), { tone: 'success' });
    } catch {
      toast.show(t('mcp.connect.copy_failed'), { tone: 'warning' });
    }
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
    void loadDefaults();
    void loadSetup();
    void load();
    // Claude (or the CLI) may have changed things from outside; undo/redo changes the list too.
    const unsub = api.on(['data_changed', 'undone', 'redone'], () => void load());
    return () => void unsub.then((u) => u());
  });

  const transports = $derived([
    { value: 'stdio' as McpTransport, label: t('mcp.client.stdio') },
    { value: 'http' as McpTransport, label: t('mcp.client.http') },
  ]);
  const clientOptions = $derived(CLIENTS.map((c) => ({ value: c, label: t(`mcp.connect.clients.${c}`) })));
  const unapproved = $derived(servers?.filter((s) => !s.approved).length ?? 0);
  const tabs = $derived([
    { id: 'connect', label: t('mcp.tabs.connect'), icon: Bot },
    { id: 'servers', label: t('mcp.tabs.servers'), icon: Server, badge: unapproved },
  ]);

  const fcMissing = $derived(setup !== undefined && !setup?.fc_path);
  const paths = $derived<SnippetPaths>({
    db: setup?.db_path ?? null,
    fc: setup?.fc_path ?? fcTyped,
    platform: setup?.platform ?? 'windows',
  });
  const snippet = $derived(snippetFor(client, paths));
  const other = $derived(otherClientFields(paths));
  const steps = $derived((['open', 'paste', 'restart'] as const).map((s) => t(`mcp.connect.${client}.${s}`)));

  const readTools = $derived(tools?.tools.filter((x) => x.access === 'read') ?? []);
  const writeTools = $derived(tools?.tools.filter((x) => x.access === 'write') ?? []);
</script>

{#snippet toolList(list: McpToolInfo[], off: boolean)}
  <ul class="tools" class:off>
    {#each list as tool (tool.name)}
      <li>
        <span>{td(`mcp.tools.${tool.name}`)}</span>
        {#if tool.destructive}<Chip size="sm" tone="warning" label={t('mcp.server.changes_content')} />{/if}
      </li>
    {/each}
  </ul>
{/snippet}

<SettingsPage title={t('mcp.heading')} subtitle={t('mcp.subtitle')}>
  <Tabs {tabs} value={tab} label={t('mcp.tabs.label')} onchange={(id) => (tab = id === 'servers' ? 'servers' : 'connect')} />

  {#if tab === 'connect'}
    <SettingsSection title={t('mcp.connect.setup_heading')} description={t('mcp.connect.intro')} icon={Bot} body="rows">
      <FormRow label={t('mcp.connect.client')} description={t('mcp.connect.client_hint')} stack>
        <SegmentedControl size="sm" label={t('mcp.connect.client')} options={clientOptions} bind:value={client} />
      </FormRow>

      {#if fcMissing}
        <FormRow label={t('mcp.connect.fc_missing.label')} description={t('mcp.connect.fc_missing.description')} stack>
          <Input
            size="sm"
            label={t('mcp.connect.fc_missing.input')}
            icon={Terminal}
            placeholder={fcPlaceholder(paths.platform)}
            spellcheck={false}
            bind:value={fcTyped}
          />
        </FormRow>
      {/if}

      <FormRow label={t('mcp.connect.steps')} description={t('mcp.connect.steps_hint')} stack>
        {#if setup === undefined}
          <Skeleton lines={4} />
        {:else}
          <ol class="steps" data-testid="mcp-steps">
            {#each steps as step, i (i)}
              <li>
                <span class="n" aria-hidden="true">{i + 1}</span>
                <div class="step">
                  <p>{step}</p>
                  {#if i === 0 && client === 'claude_desktop'}
                    <p class="file">{t('mcp.connect.claude_desktop.file')} <code>{claudeDesktopConfigFile(paths.platform)}</code></p>
                  {/if}
                  {#if i === 1}
                    {#if client === 'other'}
                      <dl class="fields">
                        <dt>{t('mcp.connect.other.command')}</dt>
                        <dd><code>{other.command}</code></dd>
                        <dt>{t('mcp.connect.other.args')}</dt>
                        <dd>{#each other.args as a, k (k)}<code>{a}</code>{/each}</dd>
                      </dl>
                    {/if}
                    <div class="snippet">
                      <div class="snippet-head">
                        <span>{t(`mcp.connect.snippet_label.${client}`)}</span>
                        <Button size="sm" variant="secondary" icon={Copy} onclick={() => void copy(snippet)}>{t('mcp.connect.copy')}</Button>
                      </div>
                      <pre data-testid="mcp-snippet"><code>{snippet}</code></pre>
                    </div>
                    {#if setup && !setup.db_path}
                      <p class="note">{t('mcp.connect.db_unknown')}</p>
                    {/if}
                  {/if}
                </div>
              </li>
            {/each}
          </ol>
        {/if}
      </FormRow>

      <FormRow label={t('mcp.connect.test')} description={t('mcp.connect.test_hint')} />
    </SettingsSection>

    <SettingsSection title={t('mcp.server.heading')} description={t('mcp.server.intro')} icon={ClipboardCheck} body="rows">
      {#if !tools}
        <div class="pad"><Skeleton lines={3} /></div>
      {:else}
        <FormRow
          label={t('mcp.server.read_only')}
          description={tools.writes_allowed ? t('mcp.server.read_only_off') : t('mcp.server.read_only_on')}
          example={t('mcp.server.read_only_example')}
          isDefault={isDefault(WRITES, tools.writes_allowed)}
          onReset={() => void resetWrites()}
        >
          <Switch ariaLabel={t('mcp.server.read_only')} checked={!tools.writes_allowed} onchange={(on) => void setWrites(!on)} />
        </FormRow>
        <FormRow label={t('mcp.server.reads')} stack>
          {@render toolList(readTools, false)}
        </FormRow>
        <FormRow label={tools.writes_allowed ? t('mcp.server.writes') : t('mcp.server.writes_off')} stack>
          {@render toolList(writeTools, !tools.writes_allowed)}
        </FormRow>
      {/if}
    </SettingsSection>
  {:else}
    <SettingsSection title={t('mcp.client.heading')} icon={Server}>
      <dl class="about">
        <dt><Icon icon={CircleHelp} size={14} />{t('mcp.client.what_label')}</dt>
        <dd>{t('mcp.client.intro')}</dd>
        <dt><Icon icon={CircleHelp} size={14} />{t('mcp.client.when_label')}</dt>
        <dd>{t('mcp.client.when')}</dd>
        <dt><Icon icon={TriangleAlert} size={14} />{t('mcp.client.safety_label')}</dt>
        <dd>{t('mcp.client.safety')}</dd>
      </dl>
    </SettingsSection>

    <SettingsSection title={t('mcp.client.list')} body="rows">
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
        <p class="hint">{t('mcp.client.add_hint')}</p>
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
  {/if}
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
  .steps {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }
  .steps li {
    display: grid;
    grid-template-columns: 22px minmax(0, 1fr);
    gap: var(--space-2);
    align-items: start;
  }
  .n {
    display: inline-grid;
    place-items: center;
    width: 22px;
    height: 22px;
    border-radius: 50%;
    background: var(--surface-2);
    border: 1px solid var(--border);
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    color: var(--text-2);
  }
  .step {
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }
  .step p {
    margin: 0;
    font-size: var(--text-sm);
    line-height: var(--leading);
    color: var(--text-1);
  }
  .step .file {
    color: var(--text-2);
  }
  code {
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    overflow-wrap: anywhere;
  }
  .file code,
  .fields code {
    padding: 1px 4px;
    background: var(--surface-2);
    border: 1px solid var(--border);
    border-radius: 4px;
  }
  .fields {
    display: grid;
    grid-template-columns: max-content minmax(0, 1fr);
    gap: var(--space-1) var(--space-3);
    margin: 0;
    font-size: var(--text-sm);
  }
  .fields dt {
    color: var(--text-2);
  }
  .fields dd {
    margin: 0;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 2px;
  }
  .snippet {
    border: 1px solid var(--border);
    border-radius: var(--radius-control);
    background: var(--surface-2);
    overflow: hidden;
  }
  .snippet-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
    padding: var(--space-1) var(--space-1) var(--space-1) var(--space-3);
    border-bottom: 1px solid var(--border);
    font-size: var(--text-xs);
    color: var(--text-2);
  }
  pre {
    margin: 0;
    padding: var(--space-2) var(--space-3);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    line-height: var(--leading);
    color: var(--text-1);
  }
  .note {
    color: var(--warning);
  }
  .tools {
    list-style: none;
    margin: 0;
    padding: 0;
    width: 100%;
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(22rem, 1fr));
    gap: var(--space-1) var(--space-6);
  }
  .tools li {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
    min-height: 28px;
    font-size: var(--text-sm);
  }
  .tools li::before {
    content: '';
    flex: none;
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: var(--text-3);
  }
  .tools.off li {
    color: var(--text-3);
    text-decoration: line-through;
  }
  .about {
    display: grid;
    grid-template-columns: max-content minmax(0, 1fr);
    gap: var(--space-3) var(--space-4);
    margin: 0;
    font-size: var(--text-sm);
    line-height: var(--leading);
  }
  .about dt {
    align-self: start;
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    font-weight: var(--weight-medium);
    color: var(--text-1);
  }
  .about dd {
    margin: 0;
    color: var(--text-2);
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
    color: var(--text-2);
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
  .hint {
    margin: calc(-1 * var(--space-2)) 0 0;
    font-size: var(--text-sm);
    color: var(--text-2);
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
    .grid,
    .about {
      grid-template-columns: minmax(0, 1fr);
    }
  }
</style>

<!--
  Settings → Connections → Subtitle (v1.2, docs/06 §5.1): the server address to paste into the
  extension and the three setup steps; the access keys (one per browser: made here, shown once,
  revocable) as dense rows; the deck captures go to and what has arrived. Same frame as the
  other connection pages (MCP, Local API): settings-kit sections, 44 px rows, mono for
  addresses, key prefixes and counts. v1.3: a "how it works" section first (what is captured,
  where it goes, what a card looks like), the deck row marks its default and can put it back.
-->
<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { api, toApiError } from '$lib/api';
  import type { CreatedSubtitleKey, SubtitleKeyDto, SubtitleStatus } from '$lib/api/types';
  import { deckPaths } from '$lib/components/deck-paths';
  import { formatCount, formatDateTime } from '$lib/format';
  import { t } from '$lib/i18n/index.svelte';
  import type { ScreenCtx } from '../types';
  import { Button, Chip, CircleHelp, Copy, Dialog, EmptyState, FolderInput, Icon, Input, KeyRound, Link, Plus, Select, Skeleton, TriangleAlert, toast } from '$lib/ui';
  import { FormRow, SchemaRows, SettingsAdvanced, SettingsPage, SettingsSection, isDefault, loadDefaults } from '../core/kit';
  import type { SubtitleCaptureAttempts } from '$lib/api/types';
  import { formatDuration } from '$lib/format';

  let { ctx }: { ctx: ScreenCtx } = $props();

  let status = $state<SubtitleStatus | null>(null);
  let keys = $state<SubtitleKeyDto[] | null>(null);
  let name = $state('');
  let nameInput = $state<HTMLInputElement | null>(null);
  /** The key just made: shown once, then gone. */
  let created = $state<CreatedSubtitleKey | null>(null);
  let deck = $state<string>('');
  /** The first load failed: an error with a retry instead of a skeleton forever. */
  let loadFailed = $state(false);
  /** The Subtitle module is switched off (its commands refuse). */
  let moduleOff = $state(false);

  /** v1.4 diagnostics: the newest capture attempts (ids, results, times only). */
  let captures = $state<SubtitleCaptureAttempts | null>(null);
  async function loadCaptures() {
    captures = await api.call('subtitle.captures.recent', { limit: 20 }).catch(() => null);
  }

  async function load() {
    try {
      const [s, k] = await Promise.all([api.call('subtitle.status', {}), api.call('subtitle.keys.list', {})]);
      status = s;
      keys = k.keys;
      deck = s.default_deck_id ?? '';
      loadFailed = false;
      moduleOff = false;
    } catch (e) {
      if (toApiError(e).code === 'CONFLICT_MODULE_DISABLED') {
        moduleOff = true;
        return;
      }
      loadFailed = status === null;
      // The shell shows the error as for every other call.
      await ctx.guarded(() => Promise.reject(e));
    }
  }

  async function turnOn() {
    const ok = await ctx.guarded(() => api.call('modules.set_enabled', { id: 'subtitle', enabled: true }));
    if (ok) await load();
  }

  async function copy(text: string) {
    try {
      await navigator.clipboard.writeText(text);
      toast.show(t('subtitle.connect.copied'), { tone: 'success' });
    } catch {
      /* no clipboard (e.g. a test): the text stays selectable */
    }
  }

  async function create() {
    if (!name.trim()) return;
    const ok = await ctx.guarded(async () => {
      created = await api.call('subtitle.keys.create', { name });
    });
    if (ok) {
      name = '';
      await load();
    }
  }

  async function revoke(k: SubtitleKeyDto) {
    await ctx.guarded(() => api.call('subtitle.keys.revoke', { id: k.id }));
    await load();
  }

  async function setDeck(id: string) {
    await ctx.guarded(async () => {
      status = await api.call('subtitle.default_deck.set', { deck_id: id || null });
    });
  }

  /** Step 1 points at the key form: focus it. */
  async function focusName() {
    await tick();
    nameInput?.focus();
  }

  onMount(() => {
    void loadDefaults();
    void load();
    const unsub = api.on(['subtitle_', 'module_toggled', 'data_changed', 'undone', 'redone'], () => void load());
    return () => void unsub.then((u) => u());
  });

  const deckOptions = $derived([
    { value: '', label: t('subtitle.deck.fallback', { name: status?.fallback_deck ?? 'Subtitle' }) },
    ...deckPaths(ctx.decks).map((d) => ({ value: d.id, label: d.path })),
  ]);
  const how = $derived([t('subtitle.how.capture'), t('subtitle.how.sent'), t('subtitle.how.card'), t('subtitle.how.repeat')]);
  /** Where a word goes now (the chosen deck, else the fallback deck). */
  const target = $derived(status ? (status.default_deck_path ?? status.fallback_deck) : '');
  const steps = $derived([t('subtitle.connect.step1'), t('subtitle.connect.step2'), t('subtitle.connect.step3')]);
  const used = (k: SubtitleKeyDto) =>
    k.last_used_at ? t('subtitle.keys.last_used', { time: formatDateTime(k.last_used_at) }) : t('subtitle.keys.never_used');
</script>

<SettingsPage title={t('subtitle.heading')} subtitle={t('subtitle.intro')}>
  {#if moduleOff}
    <SettingsSection>
      <EmptyState
        icon={TriangleAlert}
        title={t('subtitle.off.title')}
        description={t('subtitle.off.body')}
        action={{ label: t('subtitle.off.enable'), onclick: () => void turnOn() }}
      />
    </SettingsSection>
  {:else if !status && loadFailed}
    <SettingsSection>
      <EmptyState size="sm" title={t('subtitle.load_failed')} action={{ label: t('subtitle.retry'), onclick: () => void load() }} />
    </SettingsSection>
  {:else}
    <SettingsSection title={t('subtitle.how.title')} icon={CircleHelp}>
      <ol class="how" data-testid="subtitle-how">
        {#each how as line, i (i)}
          <li><span class="n" aria-hidden="true">{i + 1}</span><span>{line}</span></li>
        {/each}
      </ol>
    </SettingsSection>

    <SettingsSection title={t('subtitle.connect.title')} description={t('subtitle.connect.description')} icon={Link} body="rows">
      {#if !status}
        <div class="pad"><Skeleton lines={2} /></div>
      {:else}
        <FormRow label={t('subtitle.connect.address')} description={t('subtitle.connect.address_hint')}>
          <code class="address" data-testid="subtitle-address">{status.address}</code>
          <Button size="sm" variant="secondary" icon={Copy} onclick={() => void copy(status!.address)}>{t('subtitle.connect.copy')}</Button>
        </FormRow>
        <FormRow label={t('subtitle.connect.steps')} stack>
          <ol class="steps">
            {#each steps as step, i (i)}
              <li>
                <span class="n" aria-hidden="true">{i + 1}</span>
                {#if i === 0}
                  <button type="button" class="step-link" onclick={() => void focusName()}>{step}</button>
                {:else}
                  <span>{step}</span>
                {/if}
              </li>
            {/each}
          </ol>
        </FormRow>
      {/if}
    </SettingsSection>

    <SettingsSection title={t('subtitle.keys.title')} description={t('subtitle.keys.description')} icon={KeyRound} body="rows">
      {#if keys === null}
        <div class="pad"><Skeleton lines={2} /></div>
      {:else if keys.length === 0}
        <p class="empty"><Icon icon={KeyRound} size={16} /><span>{t('subtitle.keys.none')}</span></p>
      {:else}
        <ul class="keys">
          {#each keys as k (k.id)}
            {@const live = k.revoked_at === null}
            <li class:revoked={!live} data-testid="subtitle-key">
              <div class="key-text">
                <span class="key-name">{k.name}</span>
                <span class="key-meta">
                  <code>{k.prefix}…</code>
                  <span aria-hidden="true">·</span>
                  <span>{t('subtitle.keys.created', { time: formatDateTime(k.created_at) })}</span>
                  <span aria-hidden="true">·</span>
                  <span>{live ? used(k) : t('subtitle.keys.revoked_at', { time: formatDateTime(k.revoked_at!) })}</span>
                </span>
              </div>
              <div class="key-actions">
                <Chip size="sm" tone={live ? 'success' : 'neutral'} label={t(live ? 'subtitle.keys.active' : 'subtitle.keys.revoked')} />
                {#if live}
                  <Button size="sm" variant="danger" aria-label={t('subtitle.keys.revoke_named', { name: k.name })} onclick={() => void revoke(k)}>
                    {t('subtitle.keys.revoke')}
                  </Button>
                {/if}
              </div>
            </li>
          {/each}
        </ul>
      {/if}
      <form
        class="add"
        onsubmit={(e) => {
          e.preventDefault();
          void create();
        }}
      >
        <Input
          size="sm"
          label={t('subtitle.keys.name')}
          placeholder={t('subtitle.keys.name_placeholder')}
          maxlength={60}
          bind:value={name}
          bind:element={nameInput}
        />
        <Button size="sm" type="submit" variant="primary" icon={Plus} disabled={!name.trim()}>{t('subtitle.keys.create')}</Button>
      </form>
    </SettingsSection>

    <SettingsSection title={t('subtitle.deck.title')} description={t('subtitle.deck.description')} icon={FolderInput} body="rows">
      {#if !status}
        <div class="pad"><Skeleton lines={2} /></div>
      {:else}
        <FormRow
          label={t('subtitle.deck.label')}
          labelFor="subtitle-deck"
          description={t('subtitle.deck.hint')}
          example={t('subtitle.deck.example', { deck: target })}
          isDefault={isDefault('subtitle.default_deck', status.default_deck_id ?? '')}
          onReset={() => ((deck = ''), void setDeck(''))}
        >
          <Select id="subtitle-deck" label={t('subtitle.deck.label')} hideLabel options={deckOptions} bind:value={deck} onchange={(v) => void setDeck(v)} />
        </FormRow>
        <div data-testid="subtitle-received">
          <FormRow
            label={t('subtitle.deck.received.label')}
            description={status.captures === 0 || status.last_capture_at === null
              ? t('subtitle.deck.received.none')
              : t('subtitle.deck.received.last', { time: formatDateTime(status.last_capture_at) })}
          >
            <span class="count">{formatCount(status.captures)}</span>
          </FormRow>
        </div>
      {/if}
    </SettingsSection>
    <!-- v1.4: what happens with the words that arrive, and what Subtitle may read back. -->
    <SettingsSection title={t('subtitle.words.title')} description={t('subtitle.words.description')} body="rows">
      <SchemaRows keys={['subtitle.same_word', 'subtitle.auto_best_example', 'subtitle.share_known_words']} guarded={ctx.guarded} />
    </SettingsSection>
    <SettingsAdvanced id="subtitle.captures" title={t('subtitle.captures.title')} summary={t('subtitle.captures.summary')}>
      <div class="captures" data-testid="subtitle-captures">
        <Button size="sm" variant="secondary" onclick={() => void loadCaptures()}>{t('subtitle.captures.load')}</Button>
        {#if captures}
          {#if captures.attempts.length === 0}
            <p class="muted">{t('subtitle.captures.none')}</p>
          {:else}
            <table class="cap-table">
              <thead><tr><th>{t('subtitle.captures.when')}</th><th>{t('subtitle.captures.result')}</th><th>{t('subtitle.captures.took')}</th></tr></thead>
              <tbody>
                {#each captures.attempts as a, i (i)}
                  <tr>
                    <td class="num">{formatDateTime(a.at)}</td>
                    <td>{t(`subtitle.captures.results.${a.result}`)}{#if a.reason}<span class="muted"> · {a.reason}</span>{/if}</td>
                    <td class="num">{formatDuration(a.duration_ms)}</td>
                  </tr>
                {/each}
              </tbody>
            </table>
            <p class="muted">{t('subtitle.captures.kept', { count: captures.kept, n: formatCount(captures.kept) })}</p>
          {/if}
        {/if}
      </div>
    </SettingsAdvanced>
  {/if}
</SettingsPage>

{#if created}
  {@const secret = created.secret}
  <Dialog open size="sm" title={t('subtitle.keys.new_title')} description={t('subtitle.keys.new_body')} onclose={() => (created = null)}>
    <div class="secret-box">
      <code class="secret" data-testid="subtitle-secret">{secret}</code>
      <Button size="sm" variant="secondary" icon={Copy} onclick={() => void copy(secret)}>{t('subtitle.connect.copy')}</Button>
    </div>
    <p class="once" role="note"><Icon icon={TriangleAlert} size={14} /><span>{t('subtitle.keys.once')}</span></p>
    {#snippet footer()}
      <Button variant="primary" onclick={() => (created = null)}>{t('subtitle.keys.done')}</Button>
    {/snippet}
  </Dialog>
{/if}

<style>
  .captures {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: var(--space-3);
  }
  .cap-table {
    width: 100%;
    border-collapse: collapse;
    font-size: var(--text-sm);
  }
  .cap-table th {
    text-align: start;
    font-weight: var(--weight-medium);
    color: var(--text-2);
    border-bottom: 1px solid var(--border);
    padding: 4px 8px 4px 0;
  }
  .cap-table td {
    border-bottom: 1px solid var(--border);
    padding: 4px 8px 4px 0;
  }
  .cap-table .num {
    font-family: var(--font-mono);
  }
  .pad {
    padding: var(--space-3) 0;
    border-bottom: 1px solid var(--border);
  }
  code {
    font-family: var(--font-mono);
    font-variant-numeric: tabular-nums;
  }
  .address {
    padding: 3px var(--space-2);
    border: 1px solid var(--border);
    border-radius: var(--radius-control);
    background: var(--surface-2);
    font-size: var(--text-xs);
    color: var(--text-1);
    overflow-wrap: anywhere;
    user-select: all;
  }
  .how {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    max-width: 68ch;
  }
  .how li {
    display: flex;
    align-items: baseline;
    gap: var(--space-2);
    font-size: var(--text-sm);
    line-height: var(--leading);
    color: var(--text-2);
  }
  .steps {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
    width: 100%;
  }
  .steps li {
    display: flex;
    align-items: baseline;
    gap: var(--space-2);
    font-size: var(--text-sm);
    line-height: var(--leading);
    color: var(--text-2);
  }
  .n {
    flex: none;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 18px;
    height: 18px;
    border: 1px solid var(--border-strong);
    border-radius: 50%;
    font-family: var(--font-mono);
    font-size: 10.5px;
    color: var(--text-1);
    transform: translateY(-1px);
  }
  .step-link {
    padding: 0;
    border: none;
    background: none;
    color: inherit;
    font: inherit;
    text-align: start;
    text-decoration: underline;
    text-decoration-color: var(--border-strong);
    text-underline-offset: 3px;
    cursor: pointer;
  }
  .step-link:hover {
    color: var(--text-1);
  }
  .step-link:focus-visible {
    outline: 2px solid var(--focus-ring);
    outline-offset: 2px;
    border-radius: 2px;
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
  .keys {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .keys li {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    min-height: 44px;
    padding: var(--space-1) 0;
    border-bottom: 1px solid var(--border);
  }
  .key-text {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
    flex: 1 1 auto;
  }
  .key-name {
    font-size: var(--text-sm);
    font-weight: var(--weight-medium);
    color: var(--text-1);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .key-meta {
    display: flex;
    flex-wrap: wrap;
    gap: 0 6px;
    font-size: var(--text-xs);
    color: var(--text-2);
  }
  .key-meta code {
    font-size: var(--text-xs);
    color: var(--text-1);
  }
  .revoked .key-name,
  .revoked .key-meta code {
    color: var(--text-2);
  }
  .key-actions {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    flex: none;
  }
  .add {
    display: flex;
    align-items: flex-end;
    gap: var(--space-2);
    padding-top: var(--space-3);
  }
  .add :global(.fc-field) {
    flex: 1 1 auto;
    max-width: 22rem;
  }
  .count {
    font-family: var(--font-mono);
    font-size: var(--text-md);
    font-variant-numeric: tabular-nums;
    color: var(--text-1);
  }
  .secret-box {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  .secret {
    flex: 1 1 auto;
    min-width: 0;
    padding: var(--space-2) var(--space-3);
    border: 1px solid var(--border);
    border-radius: var(--radius-control);
    background: var(--surface-2);
    font-size: var(--text-sm);
    overflow-wrap: anywhere;
    user-select: all;
  }
  .once {
    display: flex;
    align-items: flex-start;
    gap: 6px;
    margin: var(--space-3) 0 0;
    color: var(--warning);
    font-size: var(--text-xs);
    line-height: var(--leading);
  }
  .once :global(svg) {
    flex: none;
    margin-top: 1px;
  }
  @media (max-width: 639.98px) {
    .keys li {
      flex-wrap: wrap;
    }
  }
</style>

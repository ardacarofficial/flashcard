<!--
  Subtitle extension settings (v1.2, docs/06 §5.1): the server address to paste into the
  extension, access keys (made here, shown once, revocable), the deck captures go to, and the
  three setup steps. Kept plain on purpose: the design system pass styles it later.
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import type { CreatedSubtitleKey, SubtitleKeyDto, SubtitleStatus } from '$lib/api/types';
  import { deckPaths } from '$lib/components/deck-paths';
  import { formatDateTime } from '$lib/format';
  import { t } from '$lib/i18n/index.svelte';
  import type { ScreenCtx } from '../types';
  import { Button, Chip, Copy, Dialog, EmptyState, Input, KeyRound, Link, Plus, Select, Skeleton, toast } from '$lib/ui';
  import { FormRow, SettingsPage, SettingsSection } from '../core/kit';

  let { ctx }: { ctx: ScreenCtx } = $props();

  let status = $state<SubtitleStatus | null>(null);
  let keys = $state<SubtitleKeyDto[] | null>(null);
  let name = $state('');
  /** The key just made: shown once, then gone. */
  let created = $state<CreatedSubtitleKey | null>(null);
  let deck = $state<string>('');
  /** The first load failed: an error with a retry instead of a skeleton forever. */
  let loadFailed = $state(false);

  async function load() {
    const ok = await ctx.guarded(async () => {
      const [s, k] = await Promise.all([api.call('subtitle.status', {}), api.call('subtitle.keys.list', {})]);
      status = s;
      keys = k.keys;
      deck = s.default_deck_id ?? '';
    });
    loadFailed = !ok && status === null;
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

  onMount(() => {
    void load();
    const unsub = api.on(['subtitle_', 'data_changed', 'undone', 'redone'], () => void load());
    return () => void unsub.then((u) => u());
  });

  const deckOptions = $derived([
    { value: '', label: t('subtitle.deck.fallback', { name: status?.fallback_deck ?? 'Subtitle' }) },
    ...deckPaths(ctx.decks).map((d) => ({ value: d.id, label: d.path })),
  ]);
  const steps = $derived([t('subtitle.connect.step1'), t('subtitle.connect.step2'), t('subtitle.connect.step3')]);
</script>

<SettingsPage title={t('subtitle.heading')} subtitle={t('subtitle.intro')}>
  <SettingsSection title={t('subtitle.connect.title')} description={t('subtitle.connect.description')} icon={Link} body="rows">
    {#if !status && loadFailed}
      <EmptyState size="sm" title={t('subtitle.load_failed')} action={{ label: t('subtitle.retry'), onclick: () => void load() }} />
    {:else if !status}
      <div class="pad"><Skeleton lines={2} /></div>
    {:else}
      <FormRow label={t('subtitle.connect.address')} description={t('subtitle.connect.address_hint')}>
        <div class="address">
          <code data-testid="subtitle-address">{status.address}</code>
          <Button size="sm" variant="secondary" icon={Copy} onclick={() => void copy(status!.address)}>{t('subtitle.connect.copy')}</Button>
        </div>
      </FormRow>
      <ol class="steps">
        {#each steps as step, i (i)}
          <li><span class="n" aria-hidden="true">{i + 1}</span><span>{step}</span></li>
        {/each}
      </ol>
    {/if}
  </SettingsSection>

  <SettingsSection title={t('subtitle.keys.title')} description={t('subtitle.keys.description')} icon={KeyRound}>
    {#if keys === null}
      {#if !loadFailed}<Skeleton lines={2} />{/if}
    {:else if keys.length === 0}
      <EmptyState size="sm" icon={KeyRound} title={t('subtitle.keys.none')} />
    {:else}
      <ul class="keys">
        {#each keys as k (k.id)}
          <li class:revoked={k.revoked_at !== null}>
            <div class="who">
              <strong>{k.name}</strong>
              <code>{k.prefix}…</code>
              <Chip
                size="sm"
                tone={k.revoked_at === null ? 'success' : 'neutral'}
                label={t(k.revoked_at === null ? 'subtitle.keys.active' : 'subtitle.keys.revoked')}
              />
            </div>
            <p class="muted">
              {t('subtitle.keys.created', { time: formatDateTime(k.created_at) })} ·
              {k.last_used_at ? t('subtitle.keys.last_used', { time: formatDateTime(k.last_used_at) }) : t('subtitle.keys.never_used')}
            </p>
            {#if k.revoked_at === null}
              <Button size="sm" variant="danger" onclick={() => void revoke(k)}>{t('subtitle.keys.revoke')}</Button>
            {/if}
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
      <Input label={t('subtitle.keys.name')} placeholder={t('subtitle.keys.name_placeholder')} bind:value={name} />
      <Button type="submit" variant="primary" icon={Plus} disabled={!name.trim()}>{t('subtitle.keys.create')}</Button>
    </form>
  </SettingsSection>

  <SettingsSection title={t('subtitle.deck.title')} description={t('subtitle.deck.description')} body="rows">
    {#if status}
      <FormRow label={t('subtitle.deck.label')}>
        <Select label={t('subtitle.deck.label')} hideLabel options={deckOptions} bind:value={deck} onchange={(v) => void setDeck(v)} />
      </FormRow>
      <p class="muted pad" data-testid="subtitle-received">
        {status.captures === 0 || status.last_capture_at === null
          ? t('subtitle.deck.received.none')
          : t('subtitle.deck.received.some', { count: status.captures, time: formatDateTime(status.last_capture_at) })}
      </p>
    {/if}
  </SettingsSection>
</SettingsPage>

{#if created}
  {@const secret = created.secret}
  <Dialog open size="sm" title={t('subtitle.keys.new_title')} description={t('subtitle.keys.new_body')} onclose={() => (created = null)}>
    <div class="address">
      <code class="secret" data-testid="subtitle-secret">{secret}</code>
      <Button size="sm" variant="secondary" icon={Copy} onclick={() => void copy(secret)}>{t('subtitle.connect.copy')}</Button>
    </div>
    {#snippet footer()}
      <Button variant="primary" onclick={() => (created = null)}>{t('subtitle.keys.done')}</Button>
    {/snippet}
  </Dialog>
{/if}

<style>
  .pad {
    padding: var(--space-3) var(--space-4);
  }
  .address {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    flex-wrap: wrap;
    min-width: 0;
  }
  code {
    font-family: var(--font-mono, monospace);
    font-size: var(--text-sm);
    overflow-wrap: anywhere;
  }
  .secret {
    user-select: all;
  }
  .steps {
    list-style: none;
    margin: 0;
    padding: var(--space-3) var(--space-4);
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }
  .steps li {
    display: flex;
    gap: var(--space-2);
    align-items: baseline;
  }
  .n {
    flex: none;
    font-variant-numeric: tabular-nums;
    font-weight: var(--weight-medium);
    color: var(--text-2);
  }
  .keys {
    list-style: none;
    margin: 0 0 var(--space-3);
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }
  .keys li {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: var(--space-1);
    padding: var(--space-3);
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
  }
  .keys li.revoked {
    color: var(--text-2);
  }
  .who {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    flex-wrap: wrap;
  }
  .muted {
    margin: 0;
    color: var(--text-2);
    font-size: var(--text-sm);
  }
  .add {
    display: flex;
    align-items: flex-end;
    gap: var(--space-2);
    flex-wrap: wrap;
  }
</style>

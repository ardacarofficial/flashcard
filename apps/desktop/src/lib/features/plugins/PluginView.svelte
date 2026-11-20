<script lang="ts">
  import { onMount } from 'svelte';
  import { api, type ApiError } from '$lib/api';
  import type { UiSession } from '$lib/api/types';
  import { t, td } from '$lib/i18n/index.svelte';
  import PluginNode from './PluginNode.svelte';
  import { uiText } from './texts.svelte';

  /** A plugin view or study mode: opens a session, draws its steps, sends actions. */
  interface Props {
    pluginId: string;
    view: string;
    input?: unknown;
  }
  let { pluginId, view, input = null }: Props = $props();

  let session = $state<UiSession | null>(null);
  let values = $state<Record<string, unknown>>({});
  let error = $state<string | null>(null);
  let busy = $state(false);

  async function step(fn: () => Promise<UiSession>) {
    busy = true;
    error = null;
    try {
      session = await fn();
      values = {};
    } catch (e) {
      error = td((e as ApiError).messageKey, (e as ApiError).details as Record<string, string>);
    } finally {
      busy = false;
    }
  }

  function act(action: string) {
    const s = session;
    if (!s || busy) return;
    void step(() => api.call('plugins.ui.act', { session_id: s.session_id, action, values: $state.snapshot(values) }));
  }

  onMount(() => {
    void step(() => api.call('plugins.ui.open', { plugin_id: pluginId, view, input }));
    return () => {
      const s = session;
      if (s && !s.done) void api.call('plugins.ui.close', { session_id: s.session_id }).catch(() => {});
    };
  });
</script>

<section class="plugin-view" aria-busy={busy}>
  {#if error}<p class="error" role="alert">{error}</p>{/if}
  {#if session}
    {#if session.title}<h2>{uiText(pluginId, session.title)}</h2>{/if}
    {#each session.notices as n, i (i)}<p class="notice" role="status">{uiText(pluginId, n.text)}</p>{/each}
    {#if session.node}
      <PluginNode node={session.node} {pluginId} {values} onaction={act} {busy} />
    {:else if session.done}
      <p class="muted">{t('plugins.view.done')}</p>
    {/if}
  {:else if !error}
    <p class="muted">{t('common.loading')}</p>
  {/if}
</section>

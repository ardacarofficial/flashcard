<script lang="ts">
  import { t } from '$lib/i18n/index.svelte';
  import type { ScreenCtx } from '../types';
  import PluginView from './PluginView.svelte';
  import { parseViewRoute } from './texts.svelte';

  let { ctx }: { ctx: ScreenCtx } = $props();
  const target = $derived(parseViewRoute(ctx.route ?? null));
</script>

{#if target}
  {#key `${ctx.route}|${ctx.selectedDeck}`}
    <PluginView pluginId={target.pluginId} view={target.view} input={{ deck_id: ctx.selectedDeck }} />
  {/key}
{:else}
  <p class="muted">{t('common.loading')}</p>
{/if}

<!--
  A mode page opened without a session (v1.3): the setup panels are gone — what to study is
  chosen in the study launcher, which this page opens on mount (no silent start) and from its
  one button. With the study module off it says so instead.
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import { t } from '$lib/i18n/index.svelte';
  import { Button, Icon, Layers, PageHeader, Play, Skeleton } from '$lib/ui';
  import type { ScreenCtx } from '../types';
  import type { LaunchKind } from './contract';
  import { launcher, openLauncher } from './launcher.svelte';

  interface Props {
    ctx: ScreenCtx;
    kind: LaunchKind;
    title: string;
    /** Open the launcher once on mount (false: the page was reached from a finished session). */
    auto?: boolean;
  }
  let { ctx, kind, title, auto = true }: Props = $props();

  let ready = $state(false);

  const open = () => openLauncher({ deckId: ctx.selectedDeck, kind });

  onMount(() => {
    ready = true;
    // After the screen took a handed-over session (same tick) there is nothing to ask.
    if (auto) queueMicrotask(() => void open());
  });
</script>

<section class="idle-page">
  <PageHeader {title} subtitle={ctx.selectedPath ? undefined : t('study.whole_collection')} />
  {#if !ready}
    <Skeleton shape="card" height="96px" />
  {:else}
    <div class="idle" data-testid="study-idle">
      <span class="idle-icon"><Icon icon={Layers} size={20} /></span>
      <div class="idle-text">
        <h2>{t('study.idle.title')}</h2>
        <p>{launcher.available ? t('study.idle.body') : t('study.idle.off')}</p>
      </div>
      {#if launcher.available}
        <Button variant="primary" icon={Play} data-testid="study-start" onclick={open}>{t('study.idle.open')}</Button>
      {/if}
    </div>
  {/if}
</section>

<style>
  .idle-page {
    display: flex;
    flex-direction: column;
    max-width: 760px;
  }
  .idle {
    display: flex;
    align-items: center;
    gap: var(--space-4);
    flex-wrap: wrap;
    padding: var(--space-4);
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
    background: var(--surface-1);
  }
  .idle-icon {
    display: inline-flex;
    color: var(--accent-text);
  }
  .idle-text {
    flex: 1;
    min-width: 0;
  }
  .idle-text h2 {
    margin: 0;
    font-size: var(--text-lg);
    font-weight: var(--weight-semibold);
  }
  .idle-text p {
    margin: var(--space-1) 0 0;
    color: var(--text-2);
    font-size: var(--text-sm);
  }
</style>

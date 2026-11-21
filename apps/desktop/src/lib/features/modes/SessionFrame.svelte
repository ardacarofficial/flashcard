<!--
  The frame of a running session (v1.2, flows §2): the shell is gone (`ctx.setFocus(true)`),
  so this draws the one slim line on top — menu button (brings the sidebar back), the mode and
  deck, a progress bar, "12 / 40" in mono, the remaining breakdown, extras and the exit button
  (Esc; the screen binds the key) — and centres the session's column under it.
-->
<script lang="ts">
  import type { Snippet } from 'svelte';
  import { t } from '$lib/i18n/index.svelte';
  import { Button, IconButton, PanelLeft, ProgressBar, X, type Tone } from '$lib/ui';

  interface Props {
    /** Mode name ("Cards", "Learn"). */
    title: string;
    /** Deck path or "All decks". */
    subtitle?: string | null;
    /** Accessible name of the progress bar. */
    progressLabel: string;
    value?: number;
    max?: number;
    segments?: { value: number; label: string; tone: Tone }[];
    /** "12 / 40" (mono). */
    count?: string | null;
    countTestid?: string;
    /** Remaining breakdown and other counters (hidden by focus mode's "hide counters"). */
    meta?: Snippet;
    /** Extras on the right (timer, flags, print…). */
    extra?: Snippet;
    exitLabel: string;
    exitText: string;
    exitTestid?: string;
    onexit: () => void;
    /** Max width of the session column (px). */
    width?: number;
    children: Snippet;
  }

  let {
    title,
    subtitle = null,
    progressLabel,
    value = 0,
    max = 1,
    segments,
    count = null,
    countTestid,
    meta,
    extra,
    exitLabel,
    exitText,
    exitTestid,
    onexit,
    width = 720,
    children,
  }: Props = $props();

  /** The sidebar, brought back for this session (the shell reads the class). */
  let navOpen = $state(false);

  $effect(() => {
    const root = document.documentElement;
    root.classList.toggle('fc-session-nav', navOpen);
    return () => root.classList.remove('fc-session-nav');
  });
</script>

<div class="frame">
  <header class="bar">
    <IconButton
      size="sm"
      icon={PanelLeft}
      pressed={navOpen}
      label={navOpen ? t('modes.session.hide_nav') : t('modes.session.show_nav')}
      onclick={() => (navOpen = !navOpen)}
    />
    <p class="where">
      <span class="title">{title}</span>
      {#if subtitle}<span class="sep" aria-hidden="true">/</span><span class="deck">{subtitle}</span>{/if}
    </p>
    <div class="progress" data-focus-hide>
      <ProgressBar label={progressLabel} {value} {max} {segments} />
      {#if count !== null}<span class="count num" data-testid={countTestid}>{count}</span>{/if}
    </div>
    {#if meta}<div class="meta" data-focus-hide>{@render meta()}</div>{/if}
    {#if extra}<div class="extra">{@render extra()}</div>{/if}
    <Button variant="ghost" size="sm" icon={X} kbd="Esc" aria-label={exitLabel} aria-keyshortcuts="Escape" onclick={onexit} data-testid={exitTestid}>
      {exitText}
    </Button>
  </header>
  <div class="body" style:--session-w="{width}px">
    {@render children()}
  </div>
</div>

<style>
  .frame {
    display: flex;
    flex-direction: column;
    min-height: 100%;
  }
  .bar {
    position: sticky;
    top: 0;
    z-index: 2;
    display: flex;
    align-items: center;
    gap: var(--space-4);
    height: var(--topbar-h);
    padding: 0 var(--space-3) 0 var(--space-2);
    border-bottom: 1px solid var(--border);
    background: var(--surface-0);
  }
  .where {
    display: flex;
    align-items: baseline;
    gap: var(--space-2);
    margin: 0;
    min-width: 0;
    white-space: nowrap;
    font-size: var(--text-sm);
  }
  .title {
    font-weight: var(--weight-semibold);
    color: var(--text-1);
  }
  .sep {
    color: var(--text-2);
  }
  .deck {
    color: var(--text-2);
  }
  .progress {
    flex: 1;
    display: flex;
    align-items: center;
    gap: var(--space-3);
    min-width: 120px;
  }
  /* A clearly visible track on the bare window background. */
  .progress :global(.track) {
    background: var(--border-strong);
  }
  .count {
    font-size: var(--text-sm);
    font-weight: var(--weight-medium);
    color: var(--text-1);
    white-space: nowrap;
  }
  .meta,
  .extra {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    white-space: nowrap;
    font-size: var(--text-xs);
    color: var(--text-2);
  }
  .body {
    width: 100%;
    max-width: calc(var(--session-w) + 2 * var(--content-pad-x));
    margin: 0 auto;
    padding: clamp(var(--space-4), 5vh, var(--space-12)) var(--content-pad-x) var(--space-12);
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }
  @media (max-width: 1100px) {
    .deck,
    .sep {
      display: none;
    }
  }
  @media (max-width: 860px) {
    .where {
      display: none;
    }
  }
  @media print {
    .bar {
      display: none;
    }
  }
</style>

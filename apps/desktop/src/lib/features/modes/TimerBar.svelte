<script lang="ts">
  /**
   * Countdown of the shown card in a timed session. The deadline is the backend's; when it
   * passes, `onexpire` runs once (the backend checks the time again).
   */
  import type { TimerDto } from '$lib/api/types';
  import { t } from '$lib/i18n/index.svelte';
  import { Icon, Timer } from '$lib/ui';
  import { secondsLeft } from './modes';

  let { timer, onexpire }: { timer: TimerDto; onexpire: () => void } = $props();

  let now = $state(Date.now());
  let fired: number | null = null;

  $effect(() => {
    const id = setInterval(() => (now = Date.now()), 200);
    return () => clearInterval(id);
  });

  const left = $derived(timer.deadline === null ? timer.seconds : secondsLeft(timer.deadline, now));
  const low = $derived(left <= Math.max(3, Math.ceil(timer.seconds / 5)));

  $effect(() => {
    const d = timer.deadline;
    if (d !== null && now >= d && fired !== d) {
      fired = d;
      onexpire();
    }
  });
</script>

<div class="timer" class:low role="timer" aria-label={t('modes.timer.label')} data-testid="timer">
  <Icon icon={Timer} size={14} />
  <div class="track"><div class="bar" style:width="{(left * 100) / timer.seconds}%"></div></div>
  <span class="left num">{t('modes.timer.left', { s: left })}</span>
</div>

<style>
  .timer {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    width: 100%;
    color: var(--text-2);
    font-size: var(--text-xs);
  }
  .track {
    flex: 1;
    height: 3px;
    border-radius: var(--radius-pill);
    background: var(--surface-2);
    overflow: hidden;
  }
  .bar {
    height: 100%;
    background: var(--accent);
    transition: width 0.2s linear;
  }
  .low {
    color: var(--danger);
  }
  .low .bar {
    background: var(--danger-solid);
  }
  .left {
    min-width: 8ch;
    text-align: right;
  }
  @media (prefers-reduced-motion: reduce) {
    .bar {
      transition: none;
    }
  }
</style>

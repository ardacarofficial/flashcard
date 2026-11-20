<script lang="ts">
  /**
   * Countdown of the shown card in a timed session. The deadline is the backend's; when it
   * passes, `onexpire` runs once (the backend checks the time again).
   */
  import type { TimerDto } from '$lib/api/types';
  import { t } from '$lib/i18n/index.svelte';
  import { secondsLeft } from './modes';

  let { timer, onexpire }: { timer: TimerDto; onexpire: () => void } = $props();

  let now = $state(Date.now());
  let fired: number | null = null;

  $effect(() => {
    const id = setInterval(() => (now = Date.now()), 200);
    return () => clearInterval(id);
  });

  const left = $derived(timer.deadline === null ? timer.seconds : secondsLeft(timer.deadline, now));

  $effect(() => {
    const d = timer.deadline;
    if (d !== null && now >= d && fired !== d) {
      fired = d;
      onexpire();
    }
  });
</script>

<div class="timer" role="timer" aria-label={t('modes.timer.label')} data-testid="timer">
  <div class="bar" style:width="{(left * 100) / timer.seconds}%"></div>
  <span>{t('modes.timer.left', { s: left })}</span>
</div>

<style>
  .timer {
    position: relative;
    border: var(--fc-border-width) solid var(--fc-border);
    border-radius: var(--fc-radius);
    height: 1.4em;
    overflow: hidden;
  }
  .bar {
    position: absolute;
    inset: 0 auto 0 0;
    background: var(--fc-selected);
    transition: width 0.2s linear;
  }
  span {
    position: relative;
    padding: 0 var(--fc-space-2);
    font-size: 0.85em;
  }
</style>

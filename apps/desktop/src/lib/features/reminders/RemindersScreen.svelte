<script lang="ts">
  /**
   * Daily study reminder and the tray (v1.0, docs/02 §19). The settings are ordinary app
   * settings; the backend decides when a reminder is due and the desktop shell shows it as an
   * OS notification.
   */
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import type { ReminderStatus } from '$lib/api/types';
  import { i18n, t, td } from '$lib/i18n/index.svelte';
  import type { ScreenCtx } from '../types';

  let { ctx }: { ctx: ScreenCtx } = $props();

  const DAYS = [0, 1, 2, 3, 4, 5, 6];

  let status = $state<ReminderStatus | null>(null);
  let notice = $state<string | null>(null);

  const pad = (n: number) => n.toString().padStart(2, '0');
  const time = $derived(status ? `${pad(Math.floor(status.minute_of_day / 60))}:${pad(status.minute_of_day % 60)}` : '');
  const next = $derived(
    status?.next_at != null
      ? new Date(status.next_at).toLocaleString(i18n.locale === 'tr' ? 'tr-TR' : 'en-GB', {
          weekday: 'long',
          hour: '2-digit',
          minute: '2-digit',
        })
      : null,
  );

  async function load() {
    await ctx.guarded(async () => {
      status = await api.call('reminders.status', { tz_offset_mins: -new Date().getTimezoneOffset() });
    });
  }

  async function set(key: string, value: unknown) {
    await ctx.guarded(() => api.call('settings.set', { scope: { kind: 'app' }, key, value }));
  }

  async function toggle(field: 'enabled' | 'keep_in_tray', value: boolean) {
    if (!status) return;
    status[field] = value;
    await set(`reminders.${field}`, value);
  }

  async function setTime(value: string) {
    if (!status) return;
    const [h, m] = value.split(':').map(Number);
    if (h === undefined || m === undefined || Number.isNaN(h) || Number.isNaN(m)) return;
    status.minute_of_day = h * 60 + m;
    await set('reminders.minute_of_day', status.minute_of_day);
  }

  async function toggleDay(day: number) {
    if (!status) return;
    status.weekdays ^= 1 << day;
    await set('reminders.weekdays', status.weekdays);
  }

  async function test() {
    notice = null;
    if (await ctx.guarded(() => api.call('reminders.test', {}))) notice = t('reminders.sent');
  }

  onMount(() => void load());
</script>

<section>
  <h1>{t('reminders.heading')}</h1>
  <p class="muted">{t('reminders.intro')}</p>
  {#if status}
    <label class="row">
      <input type="checkbox" checked={status.enabled} onchange={(e) => void toggle('enabled', e.currentTarget.checked)} />
      {t('reminders.enabled')}
    </label>
    <fieldset disabled={!status.enabled}>
      <label class="row">
        {t('reminders.time')}
        <input type="time" value={time} onchange={(e) => void setTime(e.currentTarget.value)} />
      </label>
      <div class="row" role="group" aria-label={t('reminders.days')}>
        <span>{t('reminders.days')}</span>
        {#each DAYS as d (d)}
          <label class="day">
            <input type="checkbox" checked={(status.weekdays & (1 << d)) !== 0} onchange={() => void toggleDay(d)} />
            {td(`reminders.day.${d}`)}
          </label>
        {/each}
      </div>
    </fieldset>
    <label class="row">
      <input type="checkbox" checked={status.keep_in_tray} onchange={(e) => void toggle('keep_in_tray', e.currentTarget.checked)} />
      {t('reminders.keep_in_tray')}
    </label>
    <p>{t('reminders.today', { due: status.counts.due, new: status.counts.new })}</p>
    {#if status.studied_today}<p class="muted">{t('reminders.studied')}</p>{/if}
    <p class="muted">{next ? t('reminders.next', { when: next }) : t('reminders.none')}</p>
    <button onclick={() => void test()}>{t('reminders.test')}</button>
    {#if notice}<p role="status">{notice}</p>{/if}
  {/if}
</section>

<style>
  .row {
    display: flex;
    gap: var(--fc-space-2);
    align-items: center;
    margin: var(--fc-space-2) 0;
  }
  fieldset {
    border: none;
    padding: 0;
    margin: 0;
  }
  .day {
    display: inline-flex;
    gap: var(--fc-space-1);
    align-items: center;
  }
</style>

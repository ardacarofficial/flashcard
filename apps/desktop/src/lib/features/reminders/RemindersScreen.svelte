<!--
  Settings → General → Reminders: the daily study reminder and the tray (v1.0, docs/02 §19).
  The settings are ordinary app settings; the backend decides when a reminder is due and the
  desktop shell shows it as an OS notification.
-->
<script lang="ts">
  import { formatDateTime } from '$lib/format';
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import type { ReminderStatus } from '$lib/api/types';
  import { i18n, t, td } from '$lib/i18n/index.svelte';
  import { Bell, Button, Chip, Clock, Icon, Switch, TimeField } from '$lib/ui';
  import { FormRow, LoadState, SettingsPage, SettingsSection } from '../core/kit';
  import type { ScreenCtx } from '../types';

  let { ctx }: { ctx: ScreenCtx } = $props();

  const DAYS = [0, 1, 2, 3, 4, 5, 6];

  let status = $state<ReminderStatus | null>(null);
  let failed = $state(false);
  let notice = $state<string | null>(null);

  const pad = (n: number) => n.toString().padStart(2, '0');
  const time = $derived(status ? `${pad(Math.floor(status.minute_of_day / 60))}:${pad(status.minute_of_day % 60)}` : '');
  const next = $derived(
    status?.next_at != null
      ? formatDateTime(status.next_at, { weekday: 'long', hour: '2-digit', minute: '2-digit' })
      : null,
  );

  async function load() {
    failed = !(await ctx.guarded(async () => {
      status = await api.call('reminders.status', { tz_offset_mins: -new Date().getTimezoneOffset() });
    }));
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

<SettingsPage title={t('reminders.heading')} subtitle={t('reminders.intro')}>
  {#if !status}
    <SettingsSection>
      <LoadState state={failed ? 'error' : 'loading'} retry={() => void load()} lines={4} />
    </SettingsSection>
  {:else}
    <div class="today" role="status">
      <span class="ic"><Icon icon={Bell} size={16} /></span>
      <p class="strong">{t('reminders.today', { due: status.counts.due, new: status.counts.new })}</p>
      <p class="muted">
        {#if status.studied_today}{t('reminders.studied')}{:else}{next ? t('reminders.next', { when: next }) : t('reminders.none')}{/if}
      </p>
    </div>

    <SettingsSection title={t('reminders.daily')} icon={Clock} body="rows">
      <FormRow label={t('reminders.enabled')} description={t('settings.reminders.enabled.description')}>
        <Switch checked={status.enabled} ariaLabel={t('reminders.enabled')} onchange={(v) => void toggle('enabled', v)} />
      </FormRow>
      <FormRow label={t('reminders.time')} description={t('reminders.time_hint')}>
        <div class="time">
          <TimeField value={time} label={t('reminders.time')} onchange={(v) => void setTime(v)} />
        </div>
      </FormRow>
      <FormRow label={t('reminders.days')} description={t('settings.reminders.weekdays.description')} stack>
        <div class="days" role="group" aria-label={t('reminders.days')}>
          {#each DAYS as d (d)}
            {@const on = (status.weekdays & (1 << d)) !== 0}
            <Chip
              label={td(`reminders.day.${d}`)}
              selected={on}
              tone={on ? 'accent' : 'neutral'}
              onclick={() => void toggleDay(d)}
            />
          {/each}
        </div>
      </FormRow>
      <FormRow label={t('reminders.test_row')} description={t('reminders.test_hint')}>
        {#if notice}<span class="ok" role="status">{notice}</span>{/if}
        <Button size="sm" icon={Bell} onclick={() => void test()}>{t('reminders.test')}</Button>
      </FormRow>
    </SettingsSection>

    <SettingsSection title={t('reminders.tray')} body="rows">
      <FormRow label={t('reminders.keep_in_tray')} description={t('settings.reminders.keep_in_tray.description')}>
        <Switch checked={status.keep_in_tray} ariaLabel={t('reminders.keep_in_tray')} onchange={(v) => void toggle('keep_in_tray', v)} />
      </FormRow>
    </SettingsSection>
  {/if}
</SettingsPage>

<style>
  .today {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: var(--space-1) var(--space-3);
    min-height: 40px;
    padding: var(--space-2) var(--space-4);
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
    background: var(--surface-1);
    font-size: var(--text-sm);
  }
  .ic {
    display: inline-flex;
    flex: none;
    color: var(--accent-text);
  }
  .today p {
    margin: 0;
  }
  .strong {
    font-weight: var(--weight-medium);
    color: var(--text-1);
  }
  .muted {
    color: var(--text-2);
    font-size: var(--text-xs);
  }
  .days {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1);
  }
  .time :global(.fc-field) {
    min-width: 0;
    width: 6rem;
  }
  .ok {
    color: var(--success);
    font-size: var(--text-sm);
  }
</style>

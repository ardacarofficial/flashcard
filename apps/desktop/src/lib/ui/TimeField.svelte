<!--
  TimeField: a time of day (`HH:MM`, 24 h in the value) built from Selects, so it looks the same
  everywhere (no browser time-input chrome). Display follows the UI language: Turkish is always
  24-hour; other languages use their locale's hour cycle (EN: 12-hour with AM/PM).
-->
<script lang="ts">
  import { localeTag } from '$lib/format';
  import { i18n } from '$lib/i18n/index.svelte';
  import Select from './Select.svelte';

  interface Props {
    /** `HH:MM`, 24-hour. */
    value: string;
    label: string;
    disabled?: boolean;
    onchange?: (value: string) => void;
    class?: string;
  }

  let { value = $bindable(), label, disabled = false, onchange, class: klass = '' }: Props = $props();

  const pad = (n: number) => n.toString().padStart(2, '0');
  const tag = $derived(localeTag(i18n.locale));
  const h12 = $derived(i18n.locale !== 'tr' && new Intl.DateTimeFormat(tag, { hour: 'numeric' }).resolvedOptions().hour12 === true);
  const hour = $derived(Number(value.split(':')[0] ?? 0) || 0);
  const minute = $derived(Number(value.split(':')[1] ?? 0) || 0);
  const pm = $derived(hour >= 12);

  const period = (isPm: boolean) =>
    new Intl.DateTimeFormat(tag, { hour: 'numeric', hour12: true }).formatToParts(new Date(2000, 0, 1, isPm ? 13 : 1)).find((p) => p.type === 'dayPeriod')?.value ?? (isPm ? 'PM' : 'AM');

  const hours = $derived(
    h12
      ? Array.from({ length: 12 }, (_, i) => ({ value: i === 0 ? 12 : i, label: String(i === 0 ? 12 : i) }))
      : Array.from({ length: 24 }, (_, i) => ({ value: i, label: pad(i) })),
  );
  const minutes = Array.from({ length: 60 }, (_, i) => ({ value: i, label: pad(i) }));
  const periods = $derived([
    { value: false, label: period(false) },
    { value: true, label: period(true) },
  ]);

  function commit(h24: number, m: number) {
    value = `${pad(h24)}:${pad(m)}`;
    onchange?.(value);
  }
  const shown = $derived(h12 ? hour % 12 || 12 : hour);
  const to24 = (h: number, isPm: boolean) => (h12 ? (h % 12) + (isPm ? 12 : 0) : h);
</script>

<div class={['fc-time', klass]} role="group" aria-label={label}>
  <Select class="part" size="md" {disabled} label={label} hideLabel options={hours} value={shown} onchange={(h) => commit(to24(h, pm), minute)} />
  <span class="sep" aria-hidden="true">:</span>
  <Select class="part" size="md" {disabled} label={label} hideLabel options={minutes} value={minute} onchange={(m) => commit(hour, m)} />
  {#if h12}
    <Select class="part period" size="md" {disabled} label={label} hideLabel options={periods} value={pm} onchange={(p) => commit(to24(shown, p), minute)} />
  {/if}
</div>

<style>
  .fc-time {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
  }
  .fc-time :global(.part) {
    width: 5rem;
  }
  .fc-time :global(.period) {
    width: 5.5rem;
  }
  .sep {
    color: var(--text-2);
    font-weight: var(--weight-semibold);
  }
</style>

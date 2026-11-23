<!--
  The note's sound (v1.4 W4): where the existing sound came from (`media.origins`), or, when
  the sound field is empty, "No sound · Find sound" → `media.audio.fill_preview` for this note
  (the film's own sound, a clip, TTS) → `media.audio.fill`. fc-app `audio_fill` decides; this
  only shows. Nothing shows for a note type without a sound field.
-->
<script lang="ts">
  import { api, type ApiError } from '$lib/api';
  import type { AudioFillItem, MediaOrigin } from '$lib/api/types';
  import { t, td } from '$lib/i18n/index.svelte';
  import { Badge, Button, Search } from '$lib/ui';
  import { followJob, isFinal } from '../io/job';

  interface Props {
    noteId: string;
    /** After a fill: the note changed (the editor re-reads it). */
    onfilled?: () => void;
  }
  let { noteId, onfilled }: Props = $props();

  /** `null` until read; `has` = the note already has sound. */
  let sound = $state<{ has: boolean; field: boolean; origin: MediaOrigin | null } | null>(null);
  let found = $state<AudioFillItem | null>(null);
  let busy = $state(false);
  let message = $state<string | null>(null);
  let error = $state<string | null>(null);

  const ORIGIN: Record<string, MediaOrigin> = { original: 'original', clip: 'clip', tts: 'tts' };

  async function load() {
    const id = noteId;
    try {
      const p = await api.call('media.audio.fill_preview', { scope: { note_ids: [id] } });
      if (id !== noteId) return;
      const item = p.items[0] ?? null;
      const has = p.with_audio > 0;
      let origin: MediaOrigin | null = null;
      if (has) {
        const o = await api.call('media.origins', { hashes: [], note_id: id });
        origin = o.origins[0]?.origin ?? null;
      }
      sound = { has, field: has || item?.reason !== 'no_audio_field', origin };
    } catch {
      sound = null;
    }
  }

  $effect(() => {
    void noteId;
    found = null;
    message = null;
    void load();
  });

  async function find() {
    busy = true;
    error = null;
    message = null;
    try {
      const p = await api.call('media.audio.fill_preview', { scope: { note_ids: [noteId] } });
      found = p.items[0] ?? null;
      if (!found || found.source === 'none') {
        message = found?.reason ? td(`note_extras.audio_reason.${found.reason}`) : t('note_extras.audio_none');
        found = null;
      }
    } catch (e) {
      error = td((e as ApiError).messageKey);
    } finally {
      busy = false;
    }
  }

  async function fill() {
    busy = true;
    error = null;
    try {
      const job = await api.call('media.audio.fill', { scope: { note_ids: [noteId] } });
      const done = isFinal(job) ? job : await followJob(job, () => {});
      if (done.status !== 'succeeded') error = done.error ? td(done.error.message_key) : t('note_extras.audio_failed');
      else {
        message = t('note_extras.audio_done');
        onfilled?.();
      }
    } catch (e) {
      error = td((e as ApiError).messageKey);
    } finally {
      found = null;
      busy = false;
      await load();
    }
  }
</script>

{#if sound?.field}
  <section class="audio" aria-labelledby="audio-title" data-testid="note-audio">
    <h4 id="audio-title">{t('note_extras.audio_title')}</h4>
    <div class="line">
      {#if sound.has}
        <span>{t('note_extras.audio_has')}</span>
        {#if sound.origin}<Badge variant="soft">{t(`note_extras.origin.${sound.origin}`)}</Badge>{/if}
      {:else}
        <span>{t('note_extras.audio_missing')}</span>
        {#if !found}
          <Button size="sm" variant="secondary" icon={Search} loading={busy} disabled={busy} onclick={find}>{t('note_extras.audio_find')}</Button>
        {/if}
      {/if}
    </div>
    {#if found && !sound.has}
      {@const origin = ORIGIN[found.source]}
      <div class="line" data-testid="audio-found">
        <span>{t('note_extras.audio_would')}</span>
        {#if origin}<Badge tone="accent" variant="soft">{t(`note_extras.origin.${origin}`)}</Badge>{/if}
        {#if found.text}<span class="muted">{t('note_extras.audio_text', { text: found.text })}</span>{/if}
        <Button size="sm" variant="primary" loading={busy} disabled={busy} onclick={fill}>{t('note_extras.audio_use')}</Button>
      </div>
    {/if}
    {#if message}<p class="muted" role="status">{message}</p>{/if}
    {#if error}<p class="error" role="alert">{error}</p>{/if}
  </section>
{/if}

<style>
  .audio {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }
  h4 {
    margin: 0;
    font-size: var(--text-xs);
    font-weight: var(--weight-medium);
    color: var(--text-2);
  }
  .line {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
    align-items: center;
    font-size: var(--text-sm);
  }
  .muted {
    margin: 0;
    color: var(--text-2);
    font-size: var(--text-xs);
  }
  .error {
    margin: 0;
    color: var(--danger);
    font-size: var(--text-xs);
  }
</style>

/**
 * F1c (v1.4 W4) Note editor › Ses: "Sesi yok · Ses bul" → `media.audio.fill_preview` → "Bunu
 * ekle" → job `media.audio.fill`, read back from the collection.
 *
 * * "whisper" was captured WITH the film's sound (a real WAV uploaded to
 *   `POST /subtitle/v1/media`, then the capture), and its audio field was later emptied
 *   (`notes.update`): the preview offers the original ("özgün"), applying it puts the same media
 *   back in the note's audio field, and `media.origins` says `original`.
 * * "murmur" was captured without sound: with no TTS provider registered (plugins only; none
 *   in the test profile) the editor says why ("Seslendirme sağlayıcısı yok.", the preview's
 *   reason) and nothing is written. A real TTS fill is NOT covered here.
 */
import { captureSubtitle, expect, noteByWord, openApp, openNoteFromSearch, rpc, t, test, uniq } from '../fixtures';

interface Note {
  id: string;
  fields: Record<string, string>;
  tags: string[];
}
interface Preview {
  notes: number;
  with_audio: number;
  items: { note_id: string; source: string; reason: string | null }[];
}
const preview = (noteId: string) => rpc<Preview>('media.audio.fill_preview', { scope: { note_ids: [noteId] } });
const audioOf = async (noteId: string) => (await rpc<Note>('notes.get', { id: noteId })).fields.audio;

test('F1c audio fill: a missing sound is found (the film’s own), previewed, added and read back', async ({ page }) => {
  const deck = await rpc<{ id: string }>('decks.create', { path: uniq('F1c'), parent_id: null });
  await captureSubtitle(deck.id, [
    { lemma: 'whisper', surface: 'whispered', meaning: 'fısıldamak', sentence: 'She whispered his name.', audio: true },
    { lemma: 'murmur', surface: 'murmured', meaning: 'mırıldanmak', sentence: 'He murmured a reply.' },
  ]);
  const whisper = (await noteByWord(deck.id, 'whisper')) as Note;
  const original = whisper.fields.audio!;
  expect(original).toMatch(/^media:[0-9a-f]{64}$/);
  // The sound field emptied (the user cleared it): the encounter still has its own sound.
  await rpc('notes.update', { id: whisper.id, deck_id: null, fields: { ...whisper.fields, audio: '' }, tags: whisper.tags, attributes: [] });
  expect(await audioOf(whisper.id)).toBe('');
  expect((await preview(whisper.id)).items).toEqual([expect.objectContaining({ note_id: whisper.id, source: 'original' })]);

  await openApp(page);
  let editor = await openNoteFromSearch(page, 'whisper', /whisper/);
  let audio = editor.getByTestId('note-audio');
  await expect(audio).toContainText(t('note_extras.audio_missing'));
  await audio.getByRole('button', { name: t('note_extras.audio_find') }).click();
  const found = audio.getByTestId('audio-found');
  await expect(found).toContainText(t('note_extras.audio_would'));
  await expect(found).toContainText(t('note_extras.origin.original'));
  // Previewing wrote nothing.
  expect(await audioOf(whisper.id)).toBe('');
  await found.getByRole('button', { name: t('note_extras.audio_use') }).click();
  await expect(audio.getByRole('status')).toHaveText(t('note_extras.audio_done'));
  await expect(audio).toContainText(t('note_extras.audio_has'));
  await expect(audio).toContainText(t('note_extras.origin.original'));

  // Saved: the same media is back in the audio field, its origin is the film's.
  await expect.poll(() => audioOf(whisper.id)).toBe(original);
  const origins = await rpc<{ origins: { hash: string; origin: string }[] }>('media.origins', { hashes: [], note_id: whisper.id });
  expect(origins.origins).toEqual([expect.objectContaining({ hash: original.slice('media:'.length), origin: 'original' })]);
  expect((await preview(whisper.id)).with_audio).toBe(1);

  // No sound of its own and no TTS provider: the editor tells why; nothing is written.
  const murmur = (await noteByWord(deck.id, 'murmur')) as Note;
  expect(murmur.fields.audio).toBe('');
  const why = (await preview(murmur.id)).items[0]!;
  expect(why).toEqual(expect.objectContaining({ source: 'none', reason: 'no_tts_provider' }));
  // (Closed with its × button: after "Bunu ekle" the focus drops out of the editor to <body> —
  // the clicked button is gone — so Esc does not reach the dialog.)
  await editor.locator('[data-dialog-close]').first().click();
  await expect(editor).toBeHidden();
  editor = await openNoteFromSearch(page, 'murmur', /murmur/);
  audio = editor.getByTestId('note-audio');
  await expect(audio).toContainText(t('note_extras.audio_missing'));
  await audio.getByRole('button', { name: t('note_extras.audio_find') }).click();
  await expect(audio.getByRole('status')).toHaveText(t('note_extras.audio_reason.no_tts_provider'));
  await expect(audio.getByTestId('audio-found')).toHaveCount(0);
  expect(await audioOf(murmur.id)).toBe('');
});

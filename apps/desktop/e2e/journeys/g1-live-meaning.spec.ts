/**
 * G1 (v1.4) "Bu cümledeki anlam" against a REAL AI provider — opt-in, NOT in the default suite.
 *
 * Skipped unless `FC_AI_LIVE=1` (it spends a little money and needs the network):
 *
 *   FC_AI_LIVE=1 [FC_AI_LIVE_OPENROUTER_KEY=sk-or-v1-…] [FC_AI_LIVE_MODEL=…] pnpm -C apps/desktop e2e -- --grep G1
 *
 * Adds OpenRouter from its preset through the API (the provider screen is covered elsewhere),
 * then, in the UI: Search → a seeded Subtitle note → Enter (note editor) → "Bu cümledeki
 * anlam" → Öner → the preview → Gönder → the model's answer → tick "Kartın anlamına da yaz"
 * → Uygula. Read back: `ai.meaning.get` (applied, the meaning the screen showed, original =
 * the capture's meaning) and `notes.get` (the meaning field replaced). Without a key the
 * proxy of the development container adds one: a dummy key in OpenRouter's format is sent.
 */
import { expect, goToSection, openApp, press, rpc, t, test } from '../fixtures';

const live = process.env.FC_AI_LIVE === '1';
const MODEL = process.env.FC_AI_LIVE_MODEL || 'google/gemini-2.5-flash-lite';

interface Provider {
  id: string;
  status: string;
}

test.skip(!live, 'real AI provider: set FC_AI_LIVE=1');

test('G1 live: "Bu cümledeki anlam" asks the real model, applies, and the note keeps it', async ({ page }) => {
  test.setTimeout(180_000);
  const list = await rpc<{ providers: Provider[]; presets: { id: string; kind: string; name: string; base_url: string }[] }>('ai.providers.list');
  let provider = list.providers[0];
  if (!provider) {
    const preset = list.presets.find((p) => p.id === 'openrouter')!;
    provider = await rpc<Provider>('ai.providers.create', {
      kind: preset.kind,
      name: preset.name,
      base_url: preset.base_url,
      default_model: MODEL,
      is_default: true,
    });
    const key = process.env.FC_AI_LIVE_OPENROUTER_KEY || `sk-or-v1-${'0'.repeat(64)}`;
    provider = await rpc<Provider>('ai.providers.set_key', { provider_id: provider.id, key });
  }
  expect(provider.status).not.toBe('needs_key');

  const word = 'keys';
  const found = await rpc<{ items: { note_id: string }[] }>('search.cards', { query: word, notes_only: true });
  expect(found.items).toHaveLength(1);
  const noteId = found.items[0]!.note_id;

  await openApp(page);
  await goToSection(page, 'shell.section.search');
  await expect(page.getByRole('searchbox').first()).toBeFocused();
  await page.keyboard.type(word);
  await press(page, 'Enter');
  await expect(page.getByTestId('browse-row').first()).toContainText(/key/);
  await press(page, 'ArrowDown', 'Enter');
  const editor = page.getByRole('dialog');
  await expect(editor).toBeVisible();

  const box = editor.getByTestId('ai-meaning');
  await expect(box).toContainText(t('ai.meaning.title'));
  await box.getByRole('button', { name: t('ai.meaning.ask') }).click();
  // The exact request is shown before anything leaves the computer.
  await expect(box.getByRole('heading', { name: t('ai.preview.heading') })).toBeVisible();
  await box.getByRole('button', { name: t('ai.preview.send') }).click();

  const suggestion = box.getByTestId('ai-meaning-suggestion');
  await expect(suggestion).toBeVisible({ timeout: 90_000 });
  const shown = await suggestion.getByRole('textbox').inputValue();
  expect(shown.trim()).not.toBe('');
  await suggestion.getByRole('checkbox', { name: t('ai.meaning.replace_field') }).check();
  await suggestion.getByRole('button', { name: t('ai.meaning.apply') }).click();
  await expect(box.getByTestId('ai-meaning-current')).toContainText(shown.trim());

  const state = await rpc<{ meaning: { status: string; meaning: string; original: string; model: string } | null }>('ai.meaning.get', { note_id: noteId });
  expect(state.meaning?.status).toBe('applied');
  expect(state.meaning?.meaning).toBe(shown.trim());
  expect(state.meaning?.original).toBe('anahtar');
  const note = await rpc<{ fields: Record<string, string> }>('notes.get', { id: noteId });
  expect(note.fields.meaning).toBe(shown.trim());
  console.log(`[G1] ${word}: "${shown.trim()}" (${state.meaning?.model})`);
});

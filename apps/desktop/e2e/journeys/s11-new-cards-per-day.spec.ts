/**
 * S11 "How many new words a day?" (Settings › Deck options): a deck with 15 new cards; a preset
 * with 5 new cards a day, saved and used on that deck → the study launcher (and `study.options`,
 * the queue it shows) offers 5; "Reset to default" + Save → 20 a day again → it offers all 15.
 *
 * Own deck and own preset (other decks keep the defaults: the preset is used only here).
 */
import type { Page } from '@playwright/test';
import { choose, expect, goToSection, makeDeck, openApp, press, rpc, t, test, uniq } from '../fixtures';

interface Scope {
  scope: string;
  count: number;
}

/** `study.options` for the deck: what a session started now would hold, per scope. */
async function counts(deckId: string): Promise<Record<string, number>> {
  const o = await rpc<{ scopes: Scope[] }>('study.options', { deck_id: deckId, include_subdecks: null });
  return Object.fromEntries(o.scopes.map((s) => [s.scope, s.count]));
}

/** Opens the launcher on the deck (Library → its study button) and reads the numbers it shows. */
async function launcherShows(page: Page, deckName: string): Promise<{ due: string; fresh: string; start: string }> {
  await goToSection(page, 'shell.section.library');
  const study = page.locator('#fc-main').getByRole('button', { name: t('library.study', { name: deckName }), exact: true });
  await study.focus();
  await press(page, 'Enter');
  const launcher = page.getByTestId('study-launcher');
  await expect(launcher.getByTestId('launcher-scope-due')).toBeVisible();
  const read = async (id: string) => ((await launcher.getByTestId(id).innerText()).match(/\d+\s*$/)?.[0] ?? '').trim();
  const shown = {
    due: await read('launcher-scope-due'),
    fresh: await read('launcher-scope-new'),
    start: (await page.getByTestId('launcher-start').innerText()).trim(),
  };
  await press(page, 'Escape');
  await expect(launcher).toBeHidden();
  return shown;
}

/**
 * Settings › deck options. The first time Settings opens straight on it (v1.4: Basic › "Daily
 * pace", what the owner would look for); Settings reopens on the last page, so later through
 * the settings navigation.
 */
async function openDeckOptions(page: Page, via: 'guide' | 'nav') {
  await goToSection(page, 'shell.settings');
  const main = page.locator('#fc-main');
  if (via === 'guide') {
    // v1.4: Settings opens on the first Basic page, "Daily pace" — the everyday question itself.
    await expect(main.getByRole('navigation', { name: t('shell.settings_nav') }).getByRole('button', { name: t('shell.page.pace'), exact: true })).toHaveAttribute('aria-current', 'page');
  } else {
    await main.getByRole('navigation', { name: t('shell.settings_nav') }).getByRole('button', { name: t('shell.page.pace'), exact: true }).click();
  }
  await expect(main.getByRole('heading', { level: 1, name: t('presets.heading') })).toBeVisible();
}

test('S11 new cards per day: 5 → launcher offers 5 of 15; reset to default → 15', async ({ page }) => {
  const deckName = uniq('S11');
  const tag = deckName.split(' ')[1];
  const deckId = await makeDeck(
    deckName,
    Array.from({ length: 15 }, (_, i): [string, string] => [`s11 ${tag} word ${i + 1}`, `anlam ${i + 1}`]),
  );
  const defaults = (await rpc<{ defaults: { new_per_day: number } }>('presets.list', {})).defaults;
  expect(defaults.new_per_day).toBe(20);
  expect(await counts(deckId)).toMatchObject({ due: 15, new: 15 });

  await openApp(page);
  // Before: the launcher offers all 15.
  const before = await launcherShows(page, deckName);
  expect(before.due).toBe('15');
  expect(before.fresh).toBe('15');

  // A preset of our own with 5 new cards a day (type its name, Enter).
  await openDeckOptions(page, 'guide');
  const main = page.locator('#fc-main');
  const presetName = uniq('S11 paket');
  const newForm = main.locator('form').filter({ has: page.getByRole('button', { name: t('presets.new'), exact: true }) });
  await newForm.getByRole('textbox').focus();
  await page.keyboard.type(presetName);
  await press(page, 'Enter');
  const editor = main.getByRole('form', { name: presetName });
  await expect(editor).toBeVisible();

  const perDay = editor.getByRole('spinbutton', { name: t('presets.field.new_per_day'), exact: true });
  await expect(perDay).toHaveValue('20');
  await perDay.focus();
  await press(page, 'Control+a');
  await page.keyboard.type('5');
  await expect(perDay).toHaveValue('5');
  await press(page, 'Enter'); // submits the form = Save
  await expect(editor.getByText(t('presets.saved'))).toBeVisible();

  // … used on our deck.
  await choose(page, t('presets.assign_pick'), deckName, main);
  await main.getByRole('button', { name: t('presets.assign'), exact: true }).click();
  await expect.poll(async () => (await rpc<{ preset_id: string | null }>('presets.for_deck', { deck_id: deckId })).preset_id).not.toBeNull();

  // Persisted: the deck studies with 5 new a day; the queue and the launcher follow.
  const eff = await rpc<{ preset_id: string; config: { new_per_day: number } }>('presets.for_deck', { deck_id: deckId });
  expect(eff.config.new_per_day).toBe(5);
  expect(await counts(deckId)).toMatchObject({ due: 5, new: 5, all: 15 });
  const limited = await launcherShows(page, deckName);
  expect(limited.due).toBe('5');
  expect(limited.fresh).toBe('5');
  // "Start · <deck> · 5 cards": the last part of the start button is the planned count.
  expect(limited.start.split('·').pop()?.trim()).toMatch(/^5\D/);

  // Reset to default (the row's own button), Save.
  await openDeckOptions(page, 'nav');
  await main.getByRole('button', { name: presetName }).click();
  await expect(editor).toBeVisible();
  await expect(perDay).toHaveValue('5');
  const row = editor.locator('.fc-form-row').filter({ has: page.getByRole('spinbutton', { name: t('presets.field.new_per_day'), exact: true }) });
  await row.getByRole('button', { name: t('kit.reset'), exact: true }).click();
  await expect(perDay).toHaveValue('20');
  await editor.getByRole('button', { name: t('presets.save'), exact: true }).click();
  await expect(editor.getByText(t('presets.saved'))).toBeVisible();

  await expect.poll(async () => (await rpc<{ config: { new_per_day: number } }>('presets.get', { id: eff.preset_id })).config.new_per_day).toBe(20);
  expect(await counts(deckId)).toMatchObject({ due: 15, new: 15 });
  const restored = await launcherShows(page, deckName);
  expect(restored.due).toBe('15');
  expect(restored.fresh).toBe('15');
});

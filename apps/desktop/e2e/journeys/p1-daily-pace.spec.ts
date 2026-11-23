/**
 * P1 (v1.4) Settings › Basic › "Daily pace": the simple page answers the everyday questions and
 * saves each change at once. A deck with 15 new cards: pick it, "5 · Easy" → reviews 150 →
 * "Strict 95%" → `presets.for_deck` reads back 5 / 150 / 0.95 (a package of its own was made
 * for the deck) and `study.options` offers 5 new; the toast's "Undo" takes the last change back
 * (retention 0.9 again, the rest kept).
 *
 * Own deck (the package is made for it only; other decks keep their options).
 */
import { choose, expect, goToSection, makeDeck, openApp, rpc, t, test, uniq } from '../fixtures';

interface Effective {
  preset_id: string | null;
  config: { new_per_day: number; reviews_per_day: number; desired_retention: number };
}

test('P1 daily pace: 5 new, 150 reviews, strict → saved on the deck, launcher offers 5; undo', async ({ page }) => {
  const deckName = uniq('P1');
  const tag = deckName.split(' ')[1];
  const deckId = await makeDeck(
    deckName,
    Array.from({ length: 15 }, (_, i): [string, string] => [`p1 ${tag} word ${i + 1}`, `anlam ${i + 1}`]),
  );
  const eff = () => rpc<Effective>('presets.for_deck', { deck_id: deckId });
  expect((await eff()).preset_id).toBeNull();

  await openApp(page);
  await goToSection(page, 'shell.settings');
  const main = page.locator('#fc-main');
  const settingsNav = main.getByRole('navigation', { name: t('shell.settings_nav') });
  await settingsNav.getByRole('button', { name: t('shell.page.pace'), exact: true }).click();
  await expect(main.getByRole('heading', { level: 1, name: t('presets.pace.title') })).toBeVisible();
  await choose(page, t('presets.pace.deck'), deckName, main);
  // The page now speaks about our deck: it has no package yet (the first change makes one).
  await expect(main.getByTestId('pace-scope')).toContainText(deckName);

  // 1. A ready-made pace.
  const picks = main.getByRole('radiogroup', { name: t('presets.pace.picks'), exact: true });
  await picks.getByRole('radio', { name: new RegExp(`^5 · ${t('presets.pace.pick.easy')}`) }).click();
  await expect.poll(async () => (await eff()).config.new_per_day).toBe(5);
  expect((await eff()).preset_id).not.toBeNull();
  await expect(picks.getByRole('radio', { name: new RegExp(`^5 · `) })).toHaveAttribute('aria-checked', 'true');

  // 2. The review limit (typed, saved when the field is left).
  const reviews = main.getByRole('spinbutton', { name: t('presets.pace.reviews_label'), exact: true });
  await reviews.fill('150');
  await reviews.press('Tab');
  await expect.poll(async () => (await eff()).config.reviews_per_day).toBe(150);

  // 3. How firmly to remember.
  const firm = main.getByRole('radiogroup', { name: t('presets.pace.retention_label'), exact: true });
  await firm.getByRole('radio', { name: new RegExp(t('presets.pace.retention.strict')) }).click();
  await expect.poll(async () => (await eff()).config.desired_retention).toBe(0.95);

  // Read back: everything is on the deck's own package, and the queue follows it.
  const saved = await eff();
  expect(saved.config).toMatchObject({ new_per_day: 5, reviews_per_day: 150, desired_retention: 0.95 });
  const o = await rpc<{ scopes: { scope: string; count: number }[] }>('study.options', { deck_id: deckId, include_subdecks: null });
  expect(Object.fromEntries(o.scopes.map((s) => [s.scope, s.count]))).toMatchObject({ new: 5 });

  // Undo (the toast of the last save) takes only the last change back.
  await page.getByRole('button', { name: t('presets.pace.undo'), exact: true }).last().click();
  await expect.poll(async () => (await eff()).config.desired_retention).toBe(0.9);
  expect((await eff()).config).toMatchObject({ new_per_day: 5, reviews_per_day: 150 });
  await expect(firm.getByRole('radio', { name: new RegExp(t('presets.pace.retention.balanced')) })).toHaveAttribute('aria-checked', 'true');
});

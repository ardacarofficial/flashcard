/**
 * C2 (closing pass) Planning's "today" is the launcher's today: on a deck with 25 new cards
 * (20 a day) and a preset with 5 reviews a day, Statistics › Planning (deck chosen) says
 * "Bugün ~20 kart" — the count `study.options` gives and the launcher shows — and the
 * forecast read back never plans more than 20 new cards or 5 reviews on any later day (the
 * new cards come back as reviews from day 2 on: the limit carries the rest over).
 */
import { endOpenSessions, expect, goToSection, makeDeck, openApp, press, rpc, studyOptions, t, test, tz, uniq } from '../fixtures';

test('C2 planning today = launcher today; later days keep the new/review limits', async ({ page }) => {
  await endOpenSessions();
  const name = uniq('C2');
  const deckId = await makeDeck(name, Array.from({ length: 25 }, (_, i): [string, string] => [`${name} n${i}`, `yeni ${i}`]));
  const preset = await rpc<{ id: string; name: string; config: Record<string, unknown> }>('presets.create', { name, config: null });
  await rpc('presets.update', { id: preset.id, name, config: { ...preset.config, reviews_per_day: 5 } });
  await rpc('presets.assign', { deck_ids: [deckId], preset_id: preset.id });

  const want = (await studyOptions(deckId)).scopes.find((s) => s.scope === 'due')!.count;
  expect(want).toBe(20);

  await openApp(page);
  await goToSection(page, 'shell.section.library');
  await page.getByTestId('deck-row').filter({ hasText: name }).first().click();
  await page.getByTestId('deck-study').click();
  await expect(page.getByTestId('study-launcher')).toBeVisible();
  await expect(page.getByTestId('launcher-scope-due')).toContainText(String(want));
  await press(page, 'Escape');

  await goToSection(page, 'shell.section.stats');
  await page.getByRole('button', { name: t('stats.ahead.plan') }).click();
  await page.getByTestId('planning-pace').getByRole('combobox', { name: t('planning.pace.deck'), exact: true }).click();
  await page.getByRole('option', { name, exact: true }).click();
  const today = page.getByText(t('planning.pace.today', { cards: want, min: 'MIN' }).split('MIN')[0]!);
  await expect(today).toBeVisible();

  const f = await rpc<{ days: { cards: number; reviews: number; new_cards: number }[] }>('planning.forecast', {
    deck_id: deckId,
    new_per_day: null,
    exam_date: null,
    days: 14,
    tz_offset_mins: tz,
  });
  expect(f.days[0]!.cards).toBe(want);
  for (const d of f.days) {
    expect(d.new_cards).toBeLessThanOrEqual(20 + 1e-9);
    expect(d.reviews).toBeLessThanOrEqual(5 + 1e-9);
  }
});

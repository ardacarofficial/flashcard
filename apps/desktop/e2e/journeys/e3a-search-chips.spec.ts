/**
 * E3a Search filter chips (v1.4): Search → chip "Nereden" → "Altyazıdan" writes the predicate
 * `nereden:altyazı` into the query box (the user sees the language) and runs it: the rows are
 * exactly what `search.cards` (the method the screen calls) returns for that query — the 10
 * seeded Subtitle captures and nothing from "İngilizce". × on the chip deletes the token.
 */
import { expect, goToSection, openApp, rpc, seed, t, test } from '../fixtures';

interface Page {
  total: number;
  items: { sort_field: string; deck_path: string; note_id: string }[];
}

const search = (query: string) =>
  rpc<Page>('search.cards', { query, sort: 'sort_field', descending: false, notes_only: false, offset: 0, limit: 100 });

test('E3a search chips: Nereden → Altyazıdan writes nereden:altyazı; rows = search.cards read back', async ({ page }) => {
  const s = seed();
  await openApp(page);
  await goToSection(page, 'shell.section.search');
  const box = page.getByRole('searchbox').first();
  await expect(box).toBeFocused();
  const rows = page.getByTestId('browse-row');
  const summary = page.getByTestId('browse-summary');
  const all = await search('');
  await expect(summary).toHaveText(t('browse.results', { count: all.total }));

  // Pick the chip value: the token is written into the box and run.
  const chips = page.getByTestId('filter-chips');
  await chips.getByRole('button', { name: t('browse.chips.from'), exact: true }).click();
  await page.getByRole('menuitem', { name: t('browse.chips.from_subtitle'), exact: true }).click();
  await expect(box).toHaveValue('nereden:altyazı');
  await expect(chips.getByRole('button', { name: t('browse.chips.active', { name: t('browse.chips.from'), value: t('browse.chips.from_subtitle') }) })).toBeVisible();

  // Read back through the API: same query, same rows, same count.
  const found = await search('nereden:altyazı');
  expect(found.total).toBeLessThanOrEqual(100);
  await expect(summary).toHaveText(t('browse.results', { count: found.total }));
  // The table is virtual: its row count is on the grid; the rendered rows are the first ones.
  await expect(page.getByRole('grid')).toHaveAttribute('aria-rowcount', String(found.total + 1));
  const shown = await rows.allTextContents();
  expect(shown.length).toBeGreaterThan(0);
  expect(shown).toEqual(found.items.slice(0, shown.length).map((r) => r.sort_field || t('common.none')));
  // Every seeded capture is there, nothing typed by hand in "İngilizce".
  const subtitleNotes = new Set(found.items.filter((r) => r.deck_path === 'Subtitle').map((r) => r.note_id));
  expect(subtitleNotes.size).toBeGreaterThanOrEqual(s.subtitle.length);
  expect(found.items.some((r) => r.deck_path === 'İngilizce')).toBe(false);

  // × on the active chip deletes the token: back to the whole collection.
  await chips.getByRole('button', { name: t('browse.chips.remove', { name: t('browse.chips.from') }), exact: true }).click();
  await expect(box).toHaveValue('');
  await expect(summary).toHaveText(t('browse.results', { count: all.total }));
});

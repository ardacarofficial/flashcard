/**
 * O3 (owner review) The deck page's Learn with an unfinished Test of the same deck: the
 * launcher lists the Test under "Continue", but the focus (and so Enter) is NOT on it — Enter
 * starts Learn, as asked; the Test stays open to continue later.
 */
import { endOpenSessions, expect, focused, goToSection, makeDeck, openApp, press, rpc, studyOptions, test, tz, uniq } from '../fixtures';

test('O3 deck page → Learn with a paused Test: Enter starts Learn, the Test stays open', async ({ page }) => {
  await endOpenSessions();
  const name = uniq('O3');
  const deckId = await makeDeck(
    name,
    Array.from({ length: 6 }, (_, i): [string, string] => [`${name} w${i}`, `anlam ${i}`]),
  );
  const paper = await rpc<{ id: string }>('study.start', { kind: 'test', deck_id: deckId, scope: 'all', tz_offset_mins: tz });

  await openApp(page);
  await goToSection(page, 'shell.section.library');
  await page.getByTestId('deck-row').filter({ hasText: name }).first().click();
  await page.getByTestId('deck-mode-learn').click();
  const launcher = page.getByTestId('study-launcher');
  await expect(launcher).toBeVisible();
  await expect(launcher.getByTestId('launcher-resume')).toHaveCount(1);
  await expect(launcher.getByTestId('launcher-resume')).toHaveAttribute('data-kind', 'test');
  await expect(focused(page)).not.toHaveAttribute('data-testid', 'launcher-resume');
  await expect(page.getByTestId('launcher-start')).toBeEnabled();
  await press(page, 'Enter');
  await expect(launcher).toBeHidden();

  await expect.poll(async () => (await studyOptions(deckId)).resumable.map((r) => r.session.kind).sort()).toEqual(['learn', 'test']);
  expect((await studyOptions(deckId)).resumable.find((r) => r.session.kind === 'test')!.session.id).toBe(paper.id);
});

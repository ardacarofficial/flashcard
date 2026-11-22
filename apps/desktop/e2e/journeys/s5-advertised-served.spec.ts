/**
 * S5 Advertised = served (the 21 new cards / limit 20 case): after 20 studied, the launcher
 * and Home both show 0 for today's cards of that deck, the launcher says why (today's limit),
 * nothing that would serve zero can be started, and a refused (empty) start leaves an
 * unfinished session alone.
 */
import { endOpenSessions, expect, goToSection, makeDeck, openApp, rpc, RpcError, sessionNow, studyOptions, t, test, tz, uniq, type SessionRow } from '../fixtures';

test('S5 21 new / limit 20: launcher and Home show 0 with the reason; empty start refused, open session kept', async ({ page }) => {
  await endOpenSessions();
  const name = uniq('S5');
  const deckId = await makeDeck(name, Array.from({ length: 21 }, (_, i): [string, string] => [`${name} w${i}`, `anlam ${i}`]));
  let s = await rpc<SessionRow>('study.start', { kind: 'normal', deck_id: deckId, tz_offset_mins: tz });
  for (let i = 0; i < 20; i++) {
    expect(s.current).not.toBeNull();
    s = await rpc<SessionRow>('study.answer', { session_id: s.id, card_id: s.current!.card_id, rating: 'easy', duration_ms: 1000 });
  }
  await rpc('study.end', { session_id: s.id }).catch(() => undefined);

  // The core: today's queue is empty because of the daily limit.
  const due = (await studyOptions(deckId)).scopes.find((o) => o.scope === 'due')!;
  expect(due).toMatchObject({ count: 0, empty_reason: 'daily_limit_reached' });

  // An unfinished session elsewhere (another deck) …
  const other = await makeDeck(uniq('S5 other'), [[`${name} x`, 'y'], [`${name} z`, 'w']]);
  const open = await rpc<SessionRow>('study.start', { kind: 'normal', deck_id: other, tz_offset_mins: tz });
  await rpc('study.answer', { session_id: open.id, card_id: open.current!.card_id, rating: 'good', duration_ms: 1000 });
  // … survives a start that has nothing to serve.
  const refused = await rpc('study.start', { kind: 'normal', deck_id: deckId, scope: 'due', tz_offset_mins: tz }).then(
    () => null,
    (e: unknown) => e,
  );
  expect(refused).toBeInstanceOf(RpcError);
  expect((refused as RpcError).api.code).toBe('CONFLICT_NOTHING_TO_STUDY');
  expect((refused as RpcError).api.details).toMatchObject({ reason: 'daily_limit_reached' });
  expect((await sessionNow(open.id)).ended_at).toBeNull();
  expect((await sessionNow(open.id)).answered).toBe(1);

  // Home: the deck advertises 0 for today (its row, if listed, has no number above 0).
  await openApp(page);
  const homeRow = page.getByRole('listitem').filter({ has: page.getByTestId('home-deck').filter({ hasText: name }) });
  if (await homeRow.count()) {
    const numbers = ((await homeRow.first().innerText()).replace(name, '').match(/\d+/g) ?? []).map(Number);
    expect(numbers.every((n) => n === 0), `Home row of ${name}: ${numbers}`).toBe(true);
  }

  // The launcher on the deck: today's cards 0, disabled, with the reason; Start never serves zero.
  await goToSection(page, 'shell.section.library');
  await page.getByTestId('deck-row').filter({ hasText: name }).first().click();
  await page.getByTestId('deck-study').click();
  const launcher = page.getByTestId('study-launcher');
  await expect(launcher).toBeVisible();
  const dueRow = page.getByTestId('launcher-scope-due');
  await expect(dueRow).toContainText(t('study.launcher.empty.daily_limit'));
  await expect(dueRow.getByRole('radio')).toBeDisabled();
  await expect(page.getByTestId('launcher-scope-new').getByRole('radio')).toBeDisabled();
  // What is preselected can be started and serves what it says.
  const opts = await studyOptions(deckId);
  for (const o of opts.scopes) {
    const radio = page.getByTestId(`launcher-scope-${o.scope}`).getByRole('radio');
    if (o.count === 0) await expect(radio).toBeDisabled();
    else await expect(radio).toBeEnabled();
  }
  const checked = page.locator('[data-testid^="launcher-scope-"] [aria-checked="true"]');
  await expect(checked).toHaveCount(1);
  const scope = (await checked.locator('xpath=ancestor::*[@data-scope][1]').getAttribute('data-scope'))!;
  expect(opts.scopes.find((o) => o.scope === scope)!.count).toBeGreaterThan(0);
  await expect(page.getByTestId('launcher-start')).toBeEnabled();
  // Nothing was started by looking.
  expect((await sessionNow(open.id)).ended_at).toBeNull();
});

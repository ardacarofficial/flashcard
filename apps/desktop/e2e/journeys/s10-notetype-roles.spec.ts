/**
 * S10 Note type editor (Settings › Note types): change what a field contains ("Bu alan ne
 * içeriyor?", `FieldDef.role`) and what a card asks for (`CardTemplate.answer_field`), save,
 * reload the page → both are persisted (`notetypes.get`) and the editor shows them again.
 *
 * Own note type (a copy of the built-in Vocabulary type made through the API) — built-ins and
 * the seeded Subtitle type are left alone.
 */
import type { Page } from '@playwright/test';
import { expect, goToSection, openApp, press, rpc, t, test, uniq } from '../fixtures';

/** The built-in "Vocabulary" note type: word (term), meaning, pronunciation, example (example) … */
const VOCABULARY_NOTETYPE_ID = '01900000-0000-7000-8000-00000000000a';

interface NoteType {
  id: string;
  name: string;
  fields: { key: string; name: string; role?: string | null }[];
  templates: { key: string; name: string; answer_field?: string | null }[];
}

/** Settings › Note types, then the type named `name` in the list. */
async function openType(page: Page, name: string) {
  await goToSection(page, 'shell.settings');
  const main = page.locator('#fc-main');
  await main.getByRole('navigation', { name: t('shell.settings_nav') }).getByRole('button', { name: t('nav.notetypes'), exact: true }).click();
  const entry = main.getByRole('button', { name: new RegExp(`^${name}`) });
  await entry.focus();
  await press(page, 'Enter');
  await expect(main.getByRole('heading', { level: 2, name, exact: true })).toBeVisible();
}

/**
 * Keyboard on a `$lib/ui` Select: focus, Enter opens, type the option's first word (type-ahead;
 * a space would pick), Enter picks.
 */
async function pick(page: Page, combobox: string, option: string) {
  const box = page.locator('#fc-main').getByRole('combobox', { name: combobox, exact: true });
  await box.focus();
  await press(page, 'Enter');
  await expect(page.getByRole('listbox')).toBeVisible();
  await page.keyboard.type((option.match(/^\p{L}+/u)?.[0] ?? option).toLocaleLowerCase('tr'));
  await press(page, 'Enter');
  await expect(page.getByRole('listbox')).toBeHidden();
  await expect(box).toContainText(option);
}

test('S10 note type editor: a field role and a card answer field survive save + reload', async ({ page }) => {
  const created = await rpc<NoteType>('notetypes.create', { name: uniq('S10 tür'), copy_from: VOCABULARY_NOTETYPE_ID });
  const { name } = created;
  const pron = created.fields.find((f) => f.key === 'pronunciation');
  const word = created.fields.find((f) => f.role === 'term');
  const recognition = created.templates.find((x) => x.answer_field === 'meaning');
  // Preconditions: the copy has an unclassified field and a card asking for the meaning.
  expect(pron?.role ?? null).toBeNull();
  expect(word).toBeTruthy();
  expect(recognition).toBeTruthy();

  await openApp(page);
  await openType(page, name);
  const roleBox = t('notetype.role.pick', { name: pron!.name });
  const asksBox = t('notetype.asks.pick', { name: recognition!.name });
  await expect(page.locator('#fc-main').getByRole('combobox', { name: roleBox, exact: true })).toContainText(t('notetype.role.none'));

  // "What does the Pronunciation field contain?" → a note; "What does Recognition ask?" → the word.
  await pick(page, roleBox, t('notetype.role.note'));
  await pick(page, asksBox, word!.name);

  const save = page.locator('#fc-main').getByRole('button', { name: t('notetype.save'), exact: true });
  await expect(save).toBeEnabled();
  await save.focus();
  await press(page, 'Enter');
  await expect(save).toBeDisabled(); // nothing left unsaved

  // Persisted.
  await expect.poll(async () => (await rpc<NoteType>('notetypes.get', { id: created.id })).fields.find((f) => f.key === 'pronunciation')?.role).toBe('note');
  const stored = await rpc<NoteType>('notetypes.get', { id: created.id });
  expect(stored.templates.find((x) => x.key === recognition!.key)?.answer_field).toBe(word!.key);
  // Nothing else moved: the other roles and the other card are as they were.
  for (const f of created.fields.filter((f) => f.key !== 'pronunciation')) {
    expect(stored.fields.find((g) => g.key === f.key)?.role ?? null, f.key).toBe(f.role ?? null);
  }
  for (const x of created.templates.filter((x) => x.key !== recognition!.key)) {
    expect(stored.templates.find((y) => y.key === x.key)?.answer_field ?? null, x.key).toBe(x.answer_field ?? null);
  }

  // Reload: the editor shows what is stored.
  await page.reload();
  await expect(page.locator('[data-testid="top-bar"]')).toBeVisible();
  await openType(page, name);
  await expect(page.locator('#fc-main').getByRole('combobox', { name: roleBox, exact: true })).toContainText(t('notetype.role.note'));
  await expect(page.locator('#fc-main').getByRole('combobox', { name: asksBox, exact: true })).toContainText(word!.name);
});

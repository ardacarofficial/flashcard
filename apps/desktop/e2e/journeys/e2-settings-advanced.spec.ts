/**
 * E2 (v1.4) Settings in two states + the new backend settings in their places:
 * - the sidebar shows the basic places only; Library is not there;
 * - Settings shows "Basic"; the one "Advanced" switch is remembered by the core
 *   (`app.show_advanced` read back) and brings Library back to the sidebar;
 * - search finds an Advanced page while the switch is off and marks it "In Advanced";
 * - Basic › Subtitle: "share known words with Subtitle" (default off) → on, read back;
 * - Basic › Backup: kept automatic copies 7 → 3, read back; the status block is there.
 */
import { expect, goToSection, openApp, rpc, sidebarButton, t, test } from '../fixtures';

const setting = async (key: string) =>
  (await rpc<{ values: { key: string; value: unknown }[] }>('settings.get', { scope: { kind: 'app' }, keys: [key] })).values.find((v) => v.key === key)?.value ?? null;

test('E2 Settings: Basic first, one remembered Advanced switch, v1.4 settings saved', async ({ page }) => {
  await rpc('settings.set', { scope: { kind: 'app' }, key: 'app.show_advanced', value: false });
  await openApp(page);
  await expect(sidebarButton(page, 'shell.section.stats')).toBeVisible();
  await expect(page.getByRole('navigation', { name: t('shell.nav_label') }).getByRole('button', { name: t('shell.section.library') })).toHaveCount(0);

  await goToSection(page, 'shell.settings');
  const main = page.locator('#fc-main');
  const sub = main.getByRole('navigation', { name: t('shell.settings_nav') });
  await expect(sub.getByRole('heading', { level: 2 })).toHaveText([t('shell.group.basic')]);

  // Search finds an Advanced page with the switch off, marked as such; opening it is one-off.
  await main.getByRole('searchbox', { name: t('kit.search.label') }).fill('MCP');
  const hit = sub.getByTestId('subnav-item').first();
  await expect(hit.getByTestId('advanced-tag')).toHaveText(t('shell.advanced.tag'));
  await hit.click();
  await expect(main.getByTestId('advanced-one-off')).toBeVisible();
  expect(await setting('app.show_advanced')).toBe(false);
  await main.getByRole('searchbox', { name: t('kit.search.label') }).fill('');

  // Basic › Subtitle: share known words (off by default) → on, persisted.
  await sub.getByRole('button', { name: t('shell.page.subtitle'), exact: true }).click();
  expect(await setting('subtitle.share_known_words')).toBe(false);
  await main.locator('[data-setting="subtitle.share_known_words"]').getByRole('switch').click();
  await expect.poll(() => setting('subtitle.share_known_words')).toBe(true);

  // Basic › Backup: status block + kept copies 7 → 3, persisted.
  await sub.getByRole('button', { name: t('shell.page.backup'), exact: true }).click();
  await expect(main.getByTestId('backup-status')).toBeVisible();
  const keep = main.locator('[data-setting="io.auto_backup_keep"] input');
  await keep.fill('3');
  await keep.press('Enter');
  await keep.blur();
  await expect.poll(() => setting('io.auto_backup_keep')).toBe(3);

  // The one switch: on → remembered by the core, Library back in the sidebar.
  await main.getByTestId('advanced-switch').getByRole('switch').click();
  await expect.poll(() => setting('app.show_advanced')).toBe(true);
  await expect(sidebarButton(page, 'shell.section.library')).toBeVisible();
  await expect(sub.getByRole('heading', { level: 2, name: t('shell.group.connections') })).toBeVisible();

  // Remembered across a restart of the window.
  await openApp(page);
  await expect(sidebarButton(page, 'shell.section.library')).toBeVisible();
  await rpc('settings.set', { scope: { kind: 'app' }, key: 'app.show_advanced', value: false });
  await rpc('settings.set', { scope: { kind: 'app' }, key: 'subtitle.share_known_words', value: false });
});

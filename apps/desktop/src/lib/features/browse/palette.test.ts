import { describe, expect, it } from 'vitest';
import { METHODS, type MethodName } from '$lib/api/generated/methods';
import { createMockTransport } from '$lib/api/mock/transport';
import { createApiClient } from '$lib/api';
import type { PaletteEntry, ShortcutDto } from '$lib/api/types';
import { translateDynamic } from '$lib/i18n/core';
import { bindingMap, comboOf, filterItems, paletteItems } from './palette';

const entry = (method: string, over: Partial<PaletteEntry> = {}): PaletteEntry => ({
  method,
  kind: 'command',
  module: 'core',
  label_key: `commands.${method.replace(/^v1\./, '')}`,
  direct: false,
  enabled: true,
  shortcut: null,
  ...over,
});

describe('command palette', () => {
  it('lists every public method the backend returns (v0.4 acceptance, UI side)', async () => {
    const api = createApiClient(createMockTransport());
    const { entries } = await api.call('palette.list', {});
    const items = paletteItems(entries, [], (k) => translateDynamic('en', k));
    const methods = Object.keys(METHODS) as MethodName[];
    expect(items.filter((i) => i.kind === 'method').map((i) => i.id).sort()).toEqual(
      methods.map((m) => `v1.${m}`).sort(),
    );
  });

  it('every public method has a TR and EN palette label', () => {
    for (const m of Object.keys(METHODS)) {
      for (const lang of ['tr', 'en'] as const) {
        expect(translateDynamic(lang, `commands.${m}`), `${lang} commands.${m}`).not.toBe(`commands.${m}`);
      }
    }
  });

  it('puts screens first, hides disabled modules, falls back to the method name', () => {
    const items = paletteItems(
      [entry('v1.decks.create'), entry('v1.x.y', { label_key: 'commands.x.y' }), entry('v1.off.z', { enabled: false })],
      [{ id: 'browse', label: 'Browse' }],
      (k) => (k === 'commands.decks.create' ? 'New deck' : k),
    );
    expect(items.map((i) => i.label)).toEqual(['Browse', 'New deck', 'v1.x.y']);
  });

  it('filters accent- and case-insensitively on every word', () => {
    const items = paletteItems(
      [entry('v1.decks.create'), entry('v1.bulk.set_flag')],
      [],
      (k) => ({ 'commands.decks.create': 'Yeni deste', 'commands.bulk.set_flag': 'Kartları bayrakla' })[k] ?? k,
    );
    expect(filterItems(items, 'YENİ').map((i) => i.id)).toEqual(['v1.decks.create']);
    expect(filterItems(items, 'kart bayrak').map((i) => i.id)).toEqual(['v1.bulk.set_flag']);
    expect(filterItems(items, '  ')).toHaveLength(2);
  });
});

describe('shortcuts', () => {
  const ev = (key: string, code: string, mods: Partial<Record<'ctrlKey' | 'altKey' | 'shiftKey' | 'metaKey', boolean>> = {}) => ({
    key,
    code,
    ctrlKey: false,
    altKey: false,
    shiftKey: false,
    metaKey: false,
    ...mods,
  });

  it('maps key events to the backend canonical form', () => {
    expect(comboOf(ev('k', 'KeyK', { ctrlKey: true }))).toBe('Ctrl+K');
    expect(comboOf(ev('Z', 'KeyZ', { ctrlKey: true, shiftKey: true }))).toBe('Ctrl+Shift+Z');
    expect(comboOf(ev(' ', 'Space'))).toBe('Space');
    expect(comboOf(ev('!', 'Digit1', { shiftKey: true }))).toBe('Shift+1');
    expect(comboOf(ev('Delete', 'Delete'))).toBe('Delete');
    expect(comboOf(ev('Control', 'ControlLeft', { ctrlKey: true }))).toBeNull();
  });

  it('binds global actions everywhere and screen actions only on their screen', () => {
    const items: ShortcutDto[] = [
      { action: 'ui.palette', label_key: '', context: 'global', combo: 'Ctrl+K', default_combo: 'Ctrl+K' },
      { action: 'browse.info', label_key: '', context: 'browse', combo: 'Ctrl+I', default_combo: 'Ctrl+I' },
      { action: 'x', label_key: '', context: 'global', combo: null, default_combo: null },
    ];
    expect([...bindingMap(items, []).keys()]).toEqual(['Ctrl+K']);
    expect(bindingMap(items, ['browse']).get('Ctrl+I')).toEqual(['browse.info']);
  });
});

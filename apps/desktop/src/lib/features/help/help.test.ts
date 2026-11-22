import { describe, expect, it } from 'vitest';
import { createMockTransport } from '$lib/api/mock/transport';
import { createApiClient } from '$lib/api';
import type { ShortcutDto, ShortcutList } from '$lib/api/types';
import { boundCount, CONTEXT_ORDER, helpGroups } from './help';

const row = (action: string, context: string, combo: string | null, label_key = `shortcuts.${action}`): ShortcutDto => ({
  action,
  label_key,
  context: context as ShortcutDto['context'],
  combo,
  default_combo: combo,
});

const LABELS: Record<string, string> = {
  'shortcuts.ui.palette': 'Open command palette',
  'shortcuts.ui.undo': 'Undo',
  'shortcuts.study.reveal': 'Show answer',
  'shortcuts.study.good': 'Good',
  'shortcuts.browse.delete': 'Delete selection',
  'shortcuts.study.ilerle': 'İleri git',
};
/** As the sheet resolves labels: the translation, else the action id. */
const label = (key: string, action: string) => LABELS[key] ?? action;

const list = (items: ShortcutDto[]): ShortcutList => ({ items, conflicts: [] });

const flat = (groups: ReturnType<typeof helpGroups>) => groups.map((g) => [g.context, g.rows.map((r) => `${r.label}=${r.combo}`)]);

describe('shortcuts help sheet', () => {
  it('shows the live combination: a rebound shortcut appears with its new key', () => {
    const before = list([row('study.reveal', 'study', 'Space')]);
    const after = list([{ ...row('study.reveal', 'study', 'K'), default_combo: 'Space' }]);
    expect(flat(helpGroups(before, label, ''))).toEqual([['study', ['Show answer=Space']]]);
    const groups = helpGroups(after, label, '');
    expect(flat(groups)).toEqual([['study', ['Show answer=K']]]);
    expect(JSON.stringify(groups)).not.toContain('Space');
  });

  it('hides unbound rows (the method table) and drops contexts left empty', () => {
    const groups = helpGroups(
      list([
        row('ui.undo', 'global', 'Ctrl+Z'),
        row('v1.decks.create', 'global', null, 'commands.decks.create'),
        row('v1.notes.delete', 'global', null, 'commands.notes.delete'),
        row('browse.delete', 'browse', null),
      ]),
      label,
      '',
    );
    expect(flat(groups)).toEqual([['global', ['Undo=Ctrl+Z']]]);
    expect(boundCount(list([row('a', 'global', null), row('b', 'global', 'B')]))).toBe(1);
  });

  it('orders everywhere, studying, lists, editor, then unknown contexts in first-seen order', () => {
    const groups = helpGroups(
      list([
        row('reader.next', 'reader', 'J'),
        row('browse.delete', 'browse', 'Delete'),
        row('editor.bold', 'editor', 'Ctrl+B'),
        row('study.reveal', 'study', 'Space'),
        row('graph.zoom', 'graph', 'Z'),
        row('ui.undo', 'global', 'Ctrl+Z'),
        row('study.good', 'study', '3'),
      ]),
      label,
      '',
    );
    expect(groups.map((g) => g.context)).toEqual(['global', 'study', 'browse', 'editor', 'reader', 'graph']);
    expect(CONTEXT_ORDER.slice(0, 3)).toEqual(['global', 'study', 'browse']);
    // Rows keep the backend's order within a context; an untranslated label falls back to the id.
    expect(groups[1]!.rows.map((r) => r.action)).toEqual(['study.reveal', 'study.good']);
    expect(groups[4]!.rows[0]!.label).toBe('reader.next');
  });

  it('searches label, action and combination, case- and accent-insensitively', () => {
    const l = list([
      row('ui.palette', 'global', 'Ctrl+K'),
      row('ui.undo', 'global', 'Ctrl+Z'),
      row('study.reveal', 'study', 'Space'),
      row('study.good', 'study', '3'),
      row('study.ilerle', 'study', 'L'),
      row('browse.delete', 'browse', 'Delete'),
    ]);
    const actions = (q: string) => helpGroups(l, label, q).flatMap((g) => g.rows.map((r) => r.action));
    expect(actions('ANSWER')).toEqual(['study.reveal']);
    expect(actions('ctrl+k')).toEqual(['ui.palette']);
    expect(actions('space')).toEqual(['study.reveal']);
    expect(actions('browse.')).toEqual(['browse.delete']);
    expect(actions('ileri')).toEqual(['study.ilerle']);
    expect(actions('undo ctrl')).toEqual(['ui.undo']);
    expect(actions('nothing-like-this')).toEqual([]);
    expect(actions('  ')).toHaveLength(6);
  });

  it('reads the table from shortcuts.list: a change made there shows up on the next read', async () => {
    // Generic over the table's content (its definitions are not this feature's).
    const api = createApiClient(createMockTransport());
    const first = await api.call('shortcuts.list', {});
    const groups = helpGroups(first, label, '');
    const shown = groups.flatMap((g) => g.rows);
    expect(shown).toHaveLength(boundCount(first));
    const known = groups.map((g) => CONTEXT_ORDER.indexOf(g.context)).filter((i) => i >= 0);
    expect(known).toEqual([...known].sort((a, b) => a - b));
    const target = shown[0]!;
    const combo = target.combo === 'Alt+Shift+F9' ? 'Alt+Shift+F8' : 'Alt+Shift+F9';
    await api.call('shortcuts.set', { action: target.action, combo });
    const again = helpGroups(await api.call('shortcuts.list', {}), label, '').flatMap((g) => g.rows);
    expect(again.find((r) => r.action === target.action)?.combo).toBe(combo);
  });
});

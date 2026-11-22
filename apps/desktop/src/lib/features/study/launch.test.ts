import { describe, expect, it } from 'vitest';
import type { StudyScopeOption } from './contract';
import { emptyPrefs, emptyReasonKey, isCustom, minutesUntil, parsePrefs, pickKind, pickScope, plannedCount, startRequest, type LaunchChoice } from './launch';

const scope = (s: StudyScopeOption['scope'], count: number): StudyScopeOption => ({
  scope: s,
  count,
  mode: s === 'due' || s === 'new' ? 'scheduled' : 'trial',
  empty_reason: count ? null : 'nothing_due',
  next_available_at: null,
});
const scopes = (due: number, fresh: number, studied: number, struggled: number, all: number) => [
  scope('due', due),
  scope('new', fresh),
  scope('studied_today', studied),
  scope('struggled_today', struggled),
  scope('all', all),
];
const choice = (over: Partial<LaunchChoice> = {}): LaunchChoice => ({
  kind: 'normal',
  scope: 'due',
  deckId: 'd1',
  limit: null,
  practice: false,
  query: '',
  savedQueryId: '',
  attribute: null,
  modes: null,
  testKinds: null,
  testQuestions: 20,
  ...over,
});

describe('launcher preselection', () => {
  const modes = { modes: ['normal', 'learn', 'test', 'match'] as const };

  it('a mode button wins, then the deck’s last mode, then Cards', () => {
    const prefs = { ...emptyPrefs(), modeByDeck: { d1: 'learn' as const } };
    expect(pickKind({ modes: [...modes.modes] }, 'test', prefs, 'd1')).toBe('test');
    expect(pickKind({ modes: [...modes.modes] }, undefined, prefs, 'd1')).toBe('learn');
    expect(pickKind({ modes: [...modes.modes] }, undefined, prefs, 'd2')).toBe('normal');
    // A mode whose module is off is never preselected.
    expect(pickKind({ modes: ['normal', 'match'] }, 'test', prefs, 'd1')).toBe('normal');
    expect(pickKind({ modes: [] }, undefined, prefs, null)).toBeNull();
  });

  it('the asked scope, else the mode’s last one, else its default — the first that has cards', () => {
    const prefs = { ...emptyPrefs(), scopeByMode: { normal: 'all' as const } };
    expect(pickScope(scopes(5, 2, 3, 1, 9), 'normal', 'struggled_today', prefs)).toBe('struggled_today');
    expect(pickScope(scopes(5, 2, 3, 1, 9), 'normal', undefined, prefs)).toBe('all');
    expect(pickScope(scopes(5, 2, 3, 1, 9), 'normal', undefined, emptyPrefs())).toBe('due');
    expect(pickScope(scopes(5, 2, 3, 1, 9), 'test', undefined, emptyPrefs())).toBe('all');
    // Nothing due: the first scope that has cards.
    expect(pickScope(scopes(0, 0, 3, 0, 9), 'normal', undefined, emptyPrefs())).toBe('studied_today');
    // Nothing at all: the wanted one, shown disabled with its reason.
    expect(pickScope(scopes(0, 0, 0, 0, 0), 'normal', 'struggled_today', emptyPrefs())).toBe('struggled_today');
  });

  it('remembered prefs survive junk', () => {
    expect(parsePrefs('not json')).toEqual(emptyPrefs());
    expect(parsePrefs(JSON.stringify({ modeByDeck: { a: 'test', b: 'nope' }, scopeByMode: { learn: 'all', x: 'due', test: 'bad' } }))).toEqual({
      modeByDeck: { a: 'test' },
      scopeByMode: { learn: 'all' },
    });
  });

  it('empty reasons and minutes', () => {
    expect(emptyReasonKey('new', 'daily_limit_reached')).toBe('study.launcher.empty.new_limit');
    expect(emptyReasonKey('due', 'daily_limit_reached')).toBe('study.launcher.empty.daily_limit');
    expect(emptyReasonKey('studied_today', 'none_studied_today')).toBe('study.launcher.empty.none_studied_today');
    expect(minutesUntil(1_000 + 13.2 * 60_000, 1_000)).toBe(14);
    expect(minutesUntil(500, 1_000)).toBe(1);
  });
});

describe('launcher start request', () => {
  const due = scope('due', 31);

  it('Cards on today’s queue: a scheduled normal session with the scope', () => {
    expect(startRequest(choice(), due, 180)).toEqual({ kind: 'normal', mode: 'scheduled', deck_id: 'd1', filter: null, scope: 'due', tz_offset_mins: 180, modes: null });
  });

  it('a practice scope is trial; a count becomes the filter limit', () => {
    const r = startRequest(choice({ scope: 'struggled_today', limit: 10 }), scope('struggled_today', 4), 0);
    expect(r).toMatchObject({ kind: 'normal', mode: 'trial', scope: 'struggled_today', filter: { attributes: [], limit: 10, query: null, saved_query_id: null } });
  });

  it('the practice switch makes today’s cards trial (Cards and Learn only)', () => {
    expect(startRequest(choice({ practice: true }), due, 0).mode).toBe('trial');
    expect(startRequest(choice({ kind: 'learn', practice: true, scope: 'new' }), scope('new', 3), 0).mode).toBe('trial');
    expect(startRequest(choice({ kind: 'match', practice: true, scope: 'all' }), scope('all', 3), 0).mode).toBe('trial');
  });

  it('Test: the count is the number of questions, types pass through', () => {
    const r = startRequest(choice({ kind: 'test', scope: 'all', limit: 10, testKinds: ['written'] }), scope('all', 40), 0);
    expect(r).toMatchObject({ kind: 'test', scope: 'all', filter: null, modes: { test: { questions: 10, kinds: ['written'] } } });
    expect(startRequest(choice({ kind: 'test', scope: 'all', testKinds: [] }), scope('all', 40), 0).modes?.test).toEqual({ questions: 20, kinds: null });
  });

  it('a query / classification makes a custom session without a scope', () => {
    const c = choice({ query: 'level:A2' });
    expect(isCustom(c)).toBe(true);
    expect(startRequest(c, due, 0)).toMatchObject({ kind: 'custom', scope: null, filter: { query: 'level:A2', due_only: null } });
    const a = choice({ attribute: { attrId: 'lvl', value: 'A1' } });
    expect(startRequest(a, due, 0)).toMatchObject({ kind: 'custom', filter: { attributes: [{ attr_id: 'lvl', values: ['A1'] }], due_only: true } });
    expect(startRequest(choice({ kind: 'learn', query: 'tag:x', scope: 'new' }), scope('new', 2), 0)).toMatchObject({ kind: 'learn', scope: null });
  });

  it('the Start button’s count', () => {
    expect(plannedCount(choice(), due)).toBe(31);
    expect(plannedCount(choice({ limit: 10 }), due)).toBe(10);
    expect(plannedCount(choice({ limit: 50 }), due)).toBe(31);
    expect(plannedCount(choice({ kind: 'test' }), scope('all', 40))).toBe(20);
    expect(plannedCount(choice({ kind: 'test', limit: 10 }), scope('all', 40))).toBe(10);
    expect(plannedCount(choice({ query: 'x' }), due)).toBeNull();
  });
});

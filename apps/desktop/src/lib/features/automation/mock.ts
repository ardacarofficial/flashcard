/**
 * Dev-only mock of the v0.9 `automation` module: keeps rules in memory and answers previews
 * from a small fixed card table (lapses, stability, flags, tags, a few Subtitle cards), so a
 * recipe shows a believable "N cards right now". Only the simple query shapes the screen
 * writes are understood (`lapses>=4`, `tag:"zor" stability>=30`, `flag:1`, `notetype:"…"`,
 * `deck:"…"`); any other term matches every other card. Validation, the query language,
 * loop safety and the real dry run live in fc-app.
 */
import { err, menu, module, type MockEnv, type MockFeature } from '$lib/api/mock/env';
import type { ActionInfo, PreviewCard, RuleAction, RuleDto, RunDto } from '$lib/api/types';

const ACTIONS = [
  ['bulk.add_tags', 'note_ids'],
  ['bulk.remove_tags', 'note_ids'],
  ['bulk.set_suspended', 'card_ids'],
  ['bulk.set_buried', 'card_ids'],
  ['bulk.set_flag', 'card_ids'],
  ['bulk.move_cards', 'card_ids'],
  ['bulk.set_attribute', 'card_ids'],
  ['bulk.reset_progress', 'card_ids'],
  ['inbox.accept', 'items'],
] as const;

interface FakeCard {
  id: string;
  text: string;
  deck: string;
  notetype: string;
  lapses: number;
  stability: number;
  flag: number;
  tags: string[];
  suspended: boolean;
}

const WORDS = [
  'to give up', 'to carry out', 'to come across', 'ubiquitous', 'reluctant', 'borrow', 'a piece of cake',
  'resilient', 'thorough', 'to put off', 'eager', 'to figure out', 'ambiguous', 'mitigate', 'scarce',
  'to break down', 'meticulous', 'to look forward to', 'whereas', 'nevertheless', 'to run out of',
  'conscientious', 'to cope with', 'inevitable', 'to take over', 'feasible', 'to bring up', 'subtle',
];
const CAPTURED = [
  'gonna', 'whatsoever', 'to get away with', 'sketchy', 'to bail on', 'heads-up', 'to freak out', 'legit',
  'to hang out', 'to show up', 'ridiculous', 'to wrap up', 'awkward', 'to mess up', 'for real', 'to chill',
  'no way', 'to hit it off', 'to screw up', 'bummer',
];

function fakeCards(): FakeCard[] {
  const out: FakeCard[] = [];
  WORDS.forEach((text, i) => {
    const lapses = (i * 7) % 11;
    out.push({
      id: `fake-card-${i}`,
      text,
      deck: i % 3 === 0 ? 'İngilizce::Fiiller' : 'İngilizce::Kelime',
      notetype: 'Klasik',
      lapses,
      stability: (i * 37) % 160,
      flag: i % 9 === 4 ? 1 : i % 13 === 6 ? 3 : 0,
      tags: lapses >= 6 ? ['zor'] : [],
      suspended: false,
    });
  });
  CAPTURED.forEach((text, i) => {
    out.push({
      id: `fake-sub-${i}`,
      text,
      deck: 'Subtitle',
      notetype: i % 2 === 0 ? 'Altyazı kartı' : 'Subtitle card',
      lapses: (i * 5) % 7,
      stability: (i * 23) % 60,
      flag: i === 3 ? 1 : 0,
      tags: i < 6 ? ['subtitle'] : [],
      suspended: false,
    });
  });
  return out;
}

const TERM = /(-?)([\p{L}_]+)(:|>=|<=|>|<|=)("([^"]*)"|\S+)|(\S+)/gu;
const fold = (s: string) => s.toLocaleLowerCase('tr');

/** The mock's reading of a query (see the header): a predicate over the fake cards. */
function matcher(query: string): (c: FakeCard, i: number) => boolean {
  const tests: ((c: FakeCard, i: number) => boolean)[] = [];
  for (const m of query.matchAll(TERM)) {
    const [, neg, key, op, raw, quoted, word] = m;
    let test: (c: FakeCard, i: number) => boolean;
    if (word !== undefined) {
      test = (c) => fold(c.text).includes(fold(word));
    } else {
      const v = quoted ?? raw ?? '';
      const num = Number(v);
      const cmp = (x: number) =>
        op === '>=' ? x >= num : op === '<=' ? x <= num : op === '>' ? x > num : op === '<' ? x < num : x === num;
      switch (fold(key ?? '')) {
        case 'lapses':
          test = (c) => cmp(c.lapses);
          break;
        case 'stability':
          test = (c) => cmp(c.stability);
          break;
        case 'flag':
          test = (c) => c.flag === num;
          break;
        case 'tag':
        case 'etiket':
          test = (c) => c.tags.some((t) => fold(t) === fold(v));
          break;
        case 'notetype':
        case 'note':
          // Both built-in Subtitle names count as the Subtitle note type here.
          test = (c) => fold(c.notetype) === fold(v) || (c.notetype !== 'Klasik' && /altyaz|subtitle/i.test(v));
          break;
        case 'deck':
        case 'deste':
          test = (c) => fold(c.deck) === fold(v) || fold(c.deck).startsWith(`${fold(v)}::`) || fold(c.deck).endsWith(`::${fold(v)}`);
          break;
        case 'is':
          test = (c) => (v === 'suspended' ? c.suspended : true);
          break;
        default:
          test = (_c, i) => i % 2 === 0;
      }
    }
    tests.push(neg ? (c, i) => !test(c, i) : test);
  }
  return (c, i) => tests.every((t) => t(c, i));
}

export default {
  modules: [module('automation', [menu('automation.screen', 'nav.automation', '/automation', 85)])],
  handlers: ({ now, id, emit, decks }: MockEnv) => {
    const rules: RuleDto[] = [];
    const runs: RunDto[] = [];
    const cards = fakeCards();
    let seq = 0;
    const find = (rid: string) => {
      const r = rules.find((x) => x.id === rid);
      if (!r) throw err('NOT_FOUND_AUTOMATION_RULE', 'errors.not_found.automation_rule', rid);
      return r;
    };
    const check = (name: string, actions: RuleAction[]) => {
      if (!name.trim() || actions.length === 0)
        throw err('VALIDATION_AUTOMATION_RULE', 'errors.validation.automation_rule', 'invalid rule');
      for (const a of actions) {
        const p = a.params as Record<string, unknown>;
        if (a.method === 'bulk.move_cards' && !p.deck_id)
          throw err('VALIDATION_AUTOMATION_RULE', 'errors.validation.automation_rule', 'deck_id is required');
      }
    };
    const deckPath = (deckId: unknown) => decks.find((d) => d.id === deckId)?.path ?? String(deckId);
    /** Whether `a` would change card `c`, and the change itself (applied on real runs). */
    const effect = (a: RuleAction, c: FakeCard): (() => void) | null => {
      const p = a.params as Record<string, unknown>;
      const tags = (p.tags as string[] | undefined) ?? [];
      switch (a.method) {
        case 'bulk.add_tags':
          return tags.some((t) => !c.tags.includes(t)) ? () => (c.tags = [...new Set([...c.tags, ...tags])]) : null;
        case 'bulk.remove_tags':
          return tags.some((t) => c.tags.includes(t)) ? () => (c.tags = c.tags.filter((t) => !tags.includes(t))) : null;
        case 'bulk.set_suspended':
          return c.suspended !== p.suspended ? () => (c.suspended = Boolean(p.suspended)) : null;
        case 'bulk.set_flag': {
          const on = p.on !== false;
          const flag = Number(p.flag);
          return (c.flag === flag) !== on ? () => (c.flag = on ? flag : 0) : null;
        }
        case 'bulk.move_cards': {
          const to = deckPath(p.deck_id);
          return c.deck !== to ? () => (c.deck = to) : null;
        }
        case 'bulk.reset_progress':
          return c.lapses > 0 || c.stability > 0 ? () => ((c.lapses = 0), (c.stability = 0)) : null;
        default:
          return () => {};
      }
    };
    const evaluate = (condition: string, actions: RuleAction[]) => {
      const test = matcher(condition);
      const matched = cards.filter((c, i) => test(c, i));
      const changes = matched.map((c) => ({ c, fx: actions.map((a) => effect(a, c)).filter((f) => f !== null) }));
      return { matched, changes };
    };
    const changed = () => emit({ type: 'automation_changed' });
    const create = (name: string, enabled: boolean, trigger: RuleDto['trigger'], condition: string, actions: RuleAction[]) => {
      const t = now();
      const r: RuleDto = {
        id: id('rule'),
        name: name.trim(),
        enabled,
        position: rules.length,
        trigger,
        condition,
        actions,
        last_run_at: null,
        created_at: t,
        updated_at: t,
      };
      rules.push(r);
      return r;
    };
    const log = (r: RuleDto, trigger: string, matched: number, at = now(), status = matched ? 'applied' : 'no_match') => {
      const run: RunDto = { seq: ++seq, rule_id: r.id, at, trigger, depth: 0, matched, status, batch_id: null, error: null };
      runs.unshift(run);
      return run;
    };

    // `pnpm dev:mock` / screenshots: a collection that already has two rules and some history.
    if (import.meta.env.MODE === 'mock') {
      const day = 86_400_000;
      const forgotten = create('Sürekli unuttuğum kartları etiketle', true, { kind: 'card_answered', ratings: ['again'] }, 'lapses>=4', [
        { method: 'bulk.add_tags', params: { tags: ['zor'] } },
      ]);
      const sub = create('Yeni Subtitle kartlarına etiket ekle', false, { kind: 'note_added' }, 'notetype:"Altyazı kartı"', [
        { method: 'bulk.add_tags', params: { tags: ['subtitle'] } },
      ]);
      const t0 = now();
      log(sub, 'note_added', 1, t0 - 2 * day - 3_600_000);
      const failed = log(forgotten, 'card_answered', 4, t0 - day - 7_200_000, 'failed');
      failed.error = {
        code: 'VALIDATION_TAG_NAME',
        message_key: 'errors.validation.tag_name',
        message: 'invalid tag',
        details: {},
        retryable: false,
      };
      log(forgotten, 'card_answered', 3, t0 - day + 3_000_000);
      log(forgotten, 'manual', 0, t0 - 600_000);
      log(forgotten, 'card_answered', 1, t0 - 120_000);
    }

    return {
      'automation.rules.list': () => ({ rules: structuredClone(rules) }),
      'automation.actions': () => ({
        actions: ACTIONS.map(
          ([method, selection]): ActionInfo => ({ method, selection, label_key: `automation.action.${method}` }),
        ),
      }),
      'automation.rules.create': ({ name, enabled, trigger, condition, actions }) => {
        check(name, actions);
        const r = create(name, enabled ?? true, trigger, condition, actions);
        changed();
        return r;
      },
      'automation.rules.update': (req) => {
        const r = find(req.id);
        if (req.name != null) r.name = req.name;
        if (req.enabled != null) r.enabled = req.enabled;
        if (req.trigger != null) r.trigger = req.trigger;
        if (req.condition != null) r.condition = req.condition;
        if (req.actions != null) r.actions = req.actions;
        check(r.name, r.actions);
        r.updated_at = now();
        changed();
        return r;
      },
      'automation.rules.delete': ({ id: rid }) => {
        rules.splice(rules.indexOf(find(rid)), 1);
        changed();
        return {};
      },
      'automation.rules.reorder': ({ ids }) => {
        const sorted = ids.map(find);
        rules.splice(0, rules.length, ...sorted);
        rules.forEach((r, i) => (r.position = i));
        changed();
        return { rules: structuredClone(rules) };
      },
      'automation.rules.run': ({ id: rid }) => {
        const r = find(rid);
        const { matched, changes } = evaluate(r.condition, r.actions);
        for (const { fx } of changes) for (const f of fx) f();
        r.last_run_at = now();
        const run = log(r, 'manual', matched.length);
        changed();
        return { runs: [run] };
      },
      // Dry run: nothing in `cards` changes.
      'automation.preview': ({ rule_id, draft }) => {
        const rule = rule_id ? find(rule_id) : null;
        const d = rule ?? draft;
        if (!d) throw err('VALIDATION_AUTOMATION_RULE', 'errors.validation.automation_rule', 'preview takes rule_id or draft');
        check('preview', d.actions);
        if (d.trigger.kind === 'inbox_added') return { matched: 2, changed: 2, truncated: false, cards: [], actions: [] };
        const { matched, changes } = evaluate(d.condition, d.actions);
        const rows: PreviewCard[] = changes
          .map(({ c, fx }) => ({ card_id: c.id, note_id: `note-${c.id}`, text: c.text, deck: c.deck, changes: fx.length > 0 }))
          .sort((a, b) => Number(b.changes) - Number(a.changes));
        return {
          matched: matched.length,
          changed: rows.filter((r) => r.changes).length,
          truncated: false,
          cards: rows.slice(0, 100),
          actions: d.actions.map((a) => ({ method: a.method, response: {} })),
        };
      },
      'automation.runs.list': ({ limit }) => ({ runs: runs.slice(0, limit ?? 50) }),
    };
  },
} satisfies MockFeature;

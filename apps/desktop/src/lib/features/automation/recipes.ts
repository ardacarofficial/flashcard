/**
 * v1.3 "rules as sentences": ready-made rules (recipes) and the plain-language reading of a
 * rule. Presentation only. A recipe is just a pre-filled draft of the existing trigger /
 * query / actions (what a rule is, what it may do and what it matches stays fc-app's); the
 * "which cards" part recognises the few exact query shapes the recipes write (`lapses>=4`,
 * `tag:"zor" stability>=30`…) so they can be read and edited as words — any other query is
 * shown as typed.
 */
import type { NoteTypeDto, Rating, RuleAction, RuleTrigger } from '$lib/api/types';
import { localeTag } from '$lib/format';
import { t, td } from '$lib/i18n/index.svelte';
import { Archive, CirclePause, Flag, Languages, Tag, TrendingUp, type IconNode } from '$lib/ui';
import { emptyAction, fromTrigger, toAction, toTrigger, type ActionForm, type TriggerForm } from './actions';

// ------------------------------------------------------------------ "which cards"

export type CondKind = 'all' | 'lapses' | 'stability' | 'tag_stability' | 'tag' | 'notetype' | 'deck' | 'flag' | 'query';

/** The "which cards" part as words: a recognised query shape and its values. */
export interface CondForm {
  kind: CondKind;
  /** lapses / stability threshold. */
  n: number;
  tag: string;
  /** Note type name or deck path. */
  name: string;
  flag: number;
  /** The text as typed (kind `query`). */
  query: string;
}

/** Kinds the "which cards" picker offers, in order (`query` last: the free-form fallback). */
export const COND_KINDS: CondKind[] = ['all', 'lapses', 'stability', 'tag_stability', 'tag', 'notetype', 'deck', 'flag', 'query'];

const quote = (v: string) => `"${v.replaceAll('"', '')}"`;

const SHAPES: { kind: CondKind; re: RegExp; read: (m: RegExpExecArray) => Partial<CondForm> }[] = [
  { kind: 'lapses', re: /^lapses>=(\d+)$/, read: (m) => ({ n: Number(m[1]) }) },
  { kind: 'stability', re: /^stability>=(\d+)$/, read: (m) => ({ n: Number(m[1]) }) },
  { kind: 'tag_stability', re: /^tag:"([^"]+)" stability>=(\d+)$/, read: (m) => ({ tag: m[1], n: Number(m[2]) }) },
  { kind: 'tag', re: /^tag:"([^"]+)"$/, read: (m) => ({ tag: m[1] }) },
  { kind: 'notetype', re: /^notetype:"([^"]+)"$/, read: (m) => ({ name: m[1] }) },
  { kind: 'deck', re: /^deck:"([^"]+)"$/, read: (m) => ({ name: m[1] }) },
  { kind: 'flag', re: /^flag:([1-7])$/, read: (m) => ({ flag: Number(m[1]) }) },
];

const BLANK: CondForm = { kind: 'all', n: 4, tag: '', name: '', flag: 1, query: '' };

/** Reads a stored condition. Exact shapes only, so `build(parse(q)) === q` always holds. */
export function parseCondition(q: string): CondForm {
  if (q === '') return { ...BLANK, kind: 'all' };
  for (const s of SHAPES) {
    const m = s.re.exec(q);
    if (m) return { ...BLANK, query: q, kind: s.kind, ...s.read(m) };
  }
  return { ...BLANK, kind: 'query', query: q };
}

/** The query text of a form. */
export function buildCondition(f: CondForm): string {
  switch (f.kind) {
    case 'all':
      return '';
    case 'lapses':
      return `lapses>=${f.n}`;
    case 'stability':
      return `stability>=${f.n}`;
    case 'tag_stability':
      return `tag:${quote(f.tag)} stability>=${f.n}`;
    case 'tag':
      return `tag:${quote(f.tag)}`;
    case 'notetype':
      return `notetype:${quote(f.name)}`;
    case 'deck':
      return `deck:${quote(f.name)}`;
    case 'flag':
      return `flag:${f.flag}`;
    case 'query':
      return f.query;
  }
}

/** Switching the picker to `kind`: sensible values, keeping what fits. */
export function switchKind(f: CondForm, kind: CondKind, defaults: { tag: string; notetype: string; deck: string }): CondForm {
  const n = kind === f.kind ? f.n : kind === 'lapses' ? 4 : kind === 'stability' ? 90 : kind === 'tag_stability' ? 30 : f.n;
  const name = kind === 'notetype' ? (f.kind === 'notetype' ? f.name : defaults.notetype) : kind === 'deck' ? (f.kind === 'deck' ? f.name : defaults.deck) : f.name;
  return { ...f, kind, n, name, tag: f.tag || defaults.tag, query: kind === 'query' ? buildCondition(f) : f.query };
}

// ------------------------------------------------------------------ the rule as a draft

/** The editor's state of a rule (what `automation.rules.create` takes, as form values). */
export interface RuleForm {
  name: string;
  enabled: boolean;
  trigger: TriggerForm;
  condition: string;
  actions: ActionForm[];
}

export function draftOf(f: RuleForm): { trigger: RuleTrigger; condition: string; actions: RuleAction[] } {
  return { trigger: toTrigger(f.trigger), condition: f.condition, actions: f.actions.map(toAction) };
}

/** What still has to be filled in before the rule can be previewed or saved (i18n keys). */
export function missing(f: RuleForm): ('missing_name' | 'missing_deck' | 'missing_tags' | 'missing_attr')[] {
  const out = new Set<'missing_name' | 'missing_deck' | 'missing_tags' | 'missing_attr'>();
  for (const a of f.actions) {
    if (a.method === 'bulk.move_cards' && !a.deckId) out.add('missing_deck');
    if ((a.method === 'bulk.add_tags' || a.method === 'bulk.remove_tags') && !a.tags.split(',').some((s) => s.trim()))
      out.add('missing_tags');
    if (a.method === 'bulk.set_attribute' && !a.attrId) out.add('missing_attr');
  }
  if (!f.name.trim()) out.add('missing_name');
  return [...out];
}

// ------------------------------------------------------------------ recipes

export type RecipeId = 'tag_forgotten' | 'suspend_leeches' | 'archive_learned' | 'tag_subtitle' | 'suspend_flagged' | 'untag_learned';

export interface Recipe {
  id: RecipeId;
  icon: IconNode;
  name: () => string;
  description: () => string;
  /** The pre-filled rule. */
  form: (env: RecipeEnv) => RuleForm;
}

/** What recipes fill in from the collection. */
export interface RecipeEnv {
  /** Name of the built-in Subtitle note type (`builtin: "subtitle"`). */
  subtitleType: string;
  /** A deck that looks like an archive (`Arşiv` / `Archive`), else `''` (the user picks). */
  archiveDeckId: string;
}

const tags = (method: string, list: string) => ({ ...emptyAction(method), tags: list });
const suspend = () => ({ ...emptyAction('bulk.set_suspended'), on: true });
const form = (name: string, trigger: RuleTrigger, condition: string, actions: ActionForm[]): RuleForm => ({
  name,
  enabled: true,
  trigger: fromTrigger(trigger),
  condition,
  actions,
});

const DIFFICULT = () => t('automation.recipes.tag_forgotten.tag');

export const RECIPES: Recipe[] = [
  {
    id: 'tag_forgotten',
    icon: Tag,
    name: () => t('automation.recipes.tag_forgotten.name'),
    description: () => t('automation.recipes.tag_forgotten.description'),
    form: () =>
      form(t('automation.recipes.tag_forgotten.name'), { kind: 'card_answered', ratings: ['again'] }, 'lapses>=4', [
        tags('bulk.add_tags', DIFFICULT()),
      ]),
  },
  {
    id: 'suspend_leeches',
    icon: CirclePause,
    name: () => t('automation.recipes.suspend_leeches.name'),
    description: () => t('automation.recipes.suspend_leeches.description'),
    form: () =>
      form(t('automation.recipes.suspend_leeches.name'), { kind: 'card_answered', ratings: ['again'] }, 'lapses>=8', [suspend()]),
  },
  {
    id: 'archive_learned',
    icon: Archive,
    name: () => t('automation.recipes.archive_learned.name'),
    description: () => t('automation.recipes.archive_learned.description'),
    form: (env) =>
      form(t('automation.recipes.archive_learned.name'), { kind: 'card_answered', ratings: [] }, 'stability>=90', [
        { ...emptyAction('bulk.move_cards'), deckId: env.archiveDeckId },
      ]),
  },
  {
    id: 'tag_subtitle',
    icon: Languages,
    name: () => t('automation.recipes.tag_subtitle.name'),
    description: () => t('automation.recipes.tag_subtitle.description'),
    form: (env) =>
      form(t('automation.recipes.tag_subtitle.name'), { kind: 'note_added' }, `notetype:${quote(env.subtitleType)}`, [
        tags('bulk.add_tags', t('automation.recipes.tag_subtitle.tag')),
      ]),
  },
  {
    id: 'suspend_flagged',
    icon: Flag,
    name: () => t('automation.recipes.suspend_flagged.name'),
    description: () => t('automation.recipes.suspend_flagged.description'),
    form: () => form(t('automation.recipes.suspend_flagged.name'), { kind: 'card_flagged', flags: [1] }, 'flag:1', [suspend()]),
  },
  {
    id: 'untag_learned',
    icon: TrendingUp,
    name: () => t('automation.recipes.untag_learned.name'),
    description: () => t('automation.recipes.untag_learned.description'),
    form: () =>
      form(
        t('automation.recipes.untag_learned.name'),
        { kind: 'schedule', every_mins: 1440 },
        `tag:${quote(DIFFICULT())} stability>=30`,
        [tags('bulk.remove_tags', DIFFICULT())],
      ),
  },
];

/** Recipe defaults from the collection. */
export function recipeEnv(notetypes: NoteTypeDto[], decks: { id: string; path: string }[]): RecipeEnv {
  const subtitle = notetypes.find((n) => n.builtin === 'subtitle');
  const leaf = (p: string) => p.split('::').pop()?.toLocaleLowerCase('tr') ?? '';
  const archive = decks.find((d) => ['arşiv', 'arsiv', 'archive'].includes(leaf(d.path)));
  return { subtitleType: subtitle?.name ?? t('automation.recipes.subtitle_type'), archiveDeckId: archive?.id ?? '' };
}

// ------------------------------------------------------------------ the rule as one sentence

export interface SentenceEnv {
  decks: { id: string; path: string }[];
  attrs: { id: string; name: string; options: { id: string; label: string }[] }[];
  subtitleType: string;
}

const lower = (s: string) => s.toLocaleLowerCase(localeTag());
const flagName = (f: number) => lower(td(`browse.flags.${f}`));
const quoted = (s: string) => `“${s}”`;
const join = (parts: string[], sep: string) => parts.join(sep);

/** "When you answer a card with “Again”" */
export function whenText(tr: TriggerForm): string {
  switch (tr.kind) {
    case 'card_answered':
      return tr.ratings.length
        ? t('automation.sentence.when.card_answered_with', {
            ratings: join(
              tr.ratings.map((r: Rating) => quoted(t(`automation.rating.${r}`))),
              t('automation.sentence.or'),
            ),
          })
        : t('automation.sentence.when.card_answered');
    case 'card_flagged':
      return tr.flags.length
        ? t('automation.sentence.when.card_flagged_with', { flags: join(tr.flags.map(flagName), t('automation.sentence.or')) })
        : t('automation.sentence.when.card_flagged');
    case 'schedule': {
      const m = Number(tr.everyMins) || 0;
      if (m > 0 && m % 1440 === 0) return t('automation.sentence.when.schedule_days', { n: m / 1440 });
      if (m > 0 && m % 60 === 0) return t('automation.sentence.when.schedule_hours', { n: m / 60 });
      return t('automation.sentence.when.schedule_minutes', { n: m });
    }
    default:
      return td(`automation.sentence.when.${tr.kind}`);
  }
}

/** "cards you have forgotten 4+ times" */
export function cardsText(condition: string, tr: TriggerForm, env: SentenceEnv): string {
  if (tr.kind === 'inbox_added')
    return condition ? t('automation.sentence.cards.inbox_query', { query: condition }) : t('automation.sentence.cards.inbox_all');
  const f = parseCondition(condition);
  switch (f.kind) {
    case 'all':
      return tr.kind === 'schedule' ? t('automation.sentence.cards.all') : t('automation.sentence.cards.event');
    case 'lapses':
      return t('automation.sentence.cards.lapses', { n: f.n });
    case 'stability':
      return t('automation.sentence.cards.stability', { n: f.n });
    case 'tag_stability':
      return t('automation.sentence.cards.tag_stability', { tag: f.tag, n: f.n });
    case 'tag':
      return t('automation.sentence.cards.tag', { tag: f.tag });
    case 'notetype':
      return f.name === env.subtitleType ? t('automation.sentence.cards.subtitle') : t('automation.sentence.cards.notetype', { name: f.name });
    case 'deck':
      return t('automation.sentence.cards.deck', { deck: f.name });
    case 'flag':
      return t('automation.sentence.cards.flag', { flag: flagName(f.flag) });
    case 'query':
      return t('automation.sentence.cards.query', { query: f.query });
  }
}

/** "add the tag “difficult”" */
export function actionText(a: ActionForm, env: SentenceEnv): string {
  const p = toAction(a).params as Record<string, unknown>;
  const deck = (id: string) => env.decks.find((d) => d.id === id)?.path ?? '';
  switch (a.method) {
    case 'bulk.add_tags':
    case 'bulk.remove_tags': {
      const list = (p.tags as string[]) ?? [];
      const add = a.method === 'bulk.add_tags';
      if (list.length === 0) return add ? t('automation.sentence.action.no_tags') : t('automation.sentence.action.no_tags_remove');
      const args = { n: list.length, tags: join(list.map(quoted), t('automation.sentence.list')) };
      return add ? t('automation.sentence.action.add_tags', args) : t('automation.sentence.action.remove_tags', args);
    }
    case 'bulk.set_suspended':
      return a.on ? t('automation.sentence.action.suspend') : t('automation.sentence.action.unsuspend');
    case 'bulk.set_buried':
      return a.on ? t('automation.sentence.action.bury') : t('automation.sentence.action.unbury');
    case 'bulk.set_flag':
      return a.on
        ? t('automation.sentence.action.flag_on', { flag: flagName(Number(a.flag)) })
        : t('automation.sentence.action.flag_off', { flag: flagName(Number(a.flag)) });
    case 'bulk.move_cards':
      return a.deckId ? t('automation.sentence.action.move', { deck: deck(a.deckId) }) : t('automation.sentence.action.move_none');
    case 'bulk.set_attribute': {
      const attr = env.attrs.find((x) => x.id === a.attrId);
      if (!attr) return t('automation.sentence.action.attribute_none');
      const opt = attr.options.find((o) => o.id === a.optionId);
      return opt
        ? t('automation.sentence.action.set_attribute', { attr: attr.name, value: quoted(opt.label) })
        : t('automation.sentence.action.clear_attribute', { attr: attr.name });
    }
    case 'bulk.reset_progress':
      return t('automation.sentence.action.reset_progress');
    case 'inbox.accept':
      return a.deckId ? t('automation.sentence.action.accept_deck', { deck: deck(a.deckId) }) : t('automation.sentence.action.accept');
    default:
      return lower(td(`automation.action.${a.method}`));
  }
}

export function actionsText(actions: ActionForm[], env: SentenceEnv): string {
  return join(
    actions.map((a) => actionText(a, env)),
    t('automation.sentence.and'),
  );
}

/** The whole rule in one sentence (rules list). */
export function sentence(f: Pick<RuleForm, 'trigger' | 'condition' | 'actions'>, env: SentenceEnv): string {
  return t('automation.sentence.pattern', {
    when: whenText(f.trigger),
    cards: cardsText(f.condition, f.trigger, env),
    action: actionsText(f.actions, env),
  });
}

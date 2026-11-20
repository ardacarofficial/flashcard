/**
 * Editor state of one rule action ↔ the `RuleAction` sent to Rust. Only shapes the params the
 * user typed; which actions exist, what they may touch and whether the params are valid is
 * decided by fc-app (`automation.actions`, `automation.rules.create`).
 */
import type { AttributeValue, Rating, RuleAction, RuleTrigger } from '$lib/api/types';

export interface ActionForm {
  method: string;
  tags: string;
  on: boolean;
  flag: number;
  deckId: string;
  attrId: string;
  optionId: string;
}

export function emptyAction(method: string): ActionForm {
  return { method, tags: '', on: true, flag: 1, deckId: '', attrId: '', optionId: '' };
}

/** The params of an action (without the selection, which the rule fills in). */
export function toAction(a: ActionForm): RuleAction {
  const tags = a.tags
    .split(',')
    .map((s) => s.trim())
    .filter(Boolean);
  switch (a.method) {
    case 'bulk.add_tags':
    case 'bulk.remove_tags':
      return { method: a.method, params: { tags } };
    case 'bulk.set_suspended':
      return { method: a.method, params: { suspended: a.on } };
    case 'bulk.set_buried':
      return { method: a.method, params: { buried: a.on } };
    case 'bulk.set_flag':
      return { method: a.method, params: { flag: Number(a.flag), on: a.on } };
    case 'bulk.move_cards':
      return { method: a.method, params: { deck_id: a.deckId } };
    case 'inbox.accept':
      return { method: a.method, params: a.deckId ? { deck_id: a.deckId } : {} };
    case 'bulk.set_attribute': {
      const value: AttributeValue | null = a.optionId ? { type: 'choice', value: a.optionId } : null;
      return { method: a.method, params: { attr_id: a.attrId, value } };
    }
    default:
      return { method: a.method, params: {} };
  }
}

/** Back from a stored action (editing a rule). */
export function fromAction(r: RuleAction): ActionForm {
  const p = r.params as Record<string, unknown>;
  const f = emptyAction(r.method);
  if (Array.isArray(p.tags)) f.tags = (p.tags as string[]).join(', ');
  if (typeof p.suspended === 'boolean') f.on = p.suspended;
  if (typeof p.buried === 'boolean') f.on = p.buried;
  if (typeof p.on === 'boolean') f.on = p.on;
  if (typeof p.flag === 'number') f.flag = p.flag;
  if (typeof p.deck_id === 'string') f.deckId = p.deck_id;
  if (typeof p.attr_id === 'string') f.attrId = p.attr_id;
  const v = p.value as AttributeValue | null | undefined;
  if (v && v.type === 'choice') f.optionId = v.value;
  return f;
}

export type TriggerKind = RuleTrigger['kind'];

export const TRIGGER_KINDS: TriggerKind[] = [
  'note_added',
  'note_updated',
  'card_answered',
  'card_flagged',
  'classification_changed',
  'imported',
  'inbox_added',
  'schedule',
];

/** Inbox rules act on inbox items (selection `items`), every other rule on cards or notes. */
export function fitsTrigger(selection: string, kind: TriggerKind): boolean {
  return (selection === 'items') === (kind === 'inbox_added');
}

export interface TriggerForm {
  kind: TriggerKind;
  ratings: Rating[];
  flags: number[];
  everyMins: number;
}

export function toTrigger(f: TriggerForm): RuleTrigger {
  switch (f.kind) {
    case 'card_answered':
      return { kind: 'card_answered', ratings: f.ratings };
    case 'card_flagged':
      return { kind: 'card_flagged', flags: f.flags };
    case 'schedule':
      return { kind: 'schedule', every_mins: Number(f.everyMins) };
    default:
      return { kind: f.kind } as RuleTrigger;
  }
}

export function fromTrigger(t: RuleTrigger): TriggerForm {
  return {
    kind: t.kind,
    ratings: t.kind === 'card_answered' ? [...t.ratings] : [],
    flags: t.kind === 'card_flagged' ? [...t.flags] : [],
    everyMins: t.kind === 'schedule' ? t.every_mins : 60,
  };
}

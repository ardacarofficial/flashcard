/**
 * Goals as sentences with blanks (v1.3): "Her gün {n} kart çalış", "{date} tarihine kadar {deck}
 * destesini bitir". Presentation only — the templates are i18n strings; this splits them into
 * text and slots (word order differs per language) and maps a template to the `goals.create`
 * fields. Progress and pace are computed in Rust.
 */
import type { GoalDto, GoalKind } from '$lib/api/types';

export type Slot = 'n' | 'date' | 'deck';
export type Part = { text: string } | { slot: Slot };

/** `"Her gün {n} kart çalış"` → text / slot parts in order (unknown `{x}` stay text). */
export function sentenceParts(template: string): Part[] {
  const out: Part[] = [];
  const re = /\{(n|date|deck)\}/g;
  let last = 0;
  for (let m = re.exec(template); m; m = re.exec(template)) {
    if (m.index > last) out.push({ text: template.slice(last, m.index) });
    out.push({ slot: m[1] as Slot });
    last = m.index + m[0].length;
  }
  if (last < template.length) out.push({ text: template.slice(last) });
  return out;
}

/** The goal templates offered when creating a goal, in this order. */
export const TEMPLATES = ['daily_cards', 'finish_deck', 'cards_learned', 'study_minutes', 'streak', 'deck_completion'] as const;
export type Template = (typeof TEMPLATES)[number];

/** Kind stored for a template ("finish a deck" = learn its unstarted cards by a date). */
export function kindOf(t: Template): GoalKind {
  return t === 'finish_deck' ? 'cards_learned' : t;
}

/** Whether the template sentence has a deck blank. */
export const hasDeck = (t: Template) => t === 'finish_deck' || t === 'deck_completion';

/** The query of a deck scope, as the query language writes it. */
export function deckQuery(path: string): string {
  return `deck:"${path.replace(/"/g, '')}"`;
}

/** i18n key of the sentence that describes a stored goal. */
export function sentenceKey(g: Pick<GoalDto, 'kind' | 'deadline'>): string {
  return `goals.s.${g.kind}${g.deadline ? '_by' : ''}`;
}

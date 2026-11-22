/**
 * Starting points of "New note type" (v1.3). Each is either a copy of a built-in note type
 * (`notetypes.create` with `copy_from`, found by its `builtin` marker) or a small definition
 * sent as the request's `definition`. This is request data only: the backend validates,
 * assigns keys and stores the note type.
 */
import type { CardTemplate, CreateNoteType, FieldDef, NoteTypeDto, NoteTypeInput } from '$lib/api/types';
import type { MessageKey } from '$lib/i18n/index.svelte';

export type StarterKey = 'word' | 'word_example' | 'reverse' | 'cloze' | 'subtitle' | 'blank';

/** In the order the dialog shows them. */
export const STARTERS: StarterKey[] = ['word', 'word_example', 'reverse', 'cloze', 'subtitle', 'blank'];

/** Built-in note type (its `builtin` marker) a starting point copies. */
const COPIES: Partial<Record<StarterKey, string>> = {
  reverse: 'classic_reverse',
  cloze: 'cloze',
  subtitle: 'subtitle',
  blank: 'classic',
};

type Translate = (key: MessageKey) => string;

const ANSWER = '{{FrontSide}}\n<hr class="fc-answer">\n';

function field(key: string, name: string, required: boolean, unique = false): FieldDef {
  return { key, name, kind: 'rich_text', required: required || unique, unique, searchable: true, default_value: null };
}

function recognition(tr: Translate, back: string): CardTemplate {
  return { key: 'recognition', name: tr('notetype.start.template.recognition'), front: '{{word}}', back: ANSWER + back, card_type: 'classic' };
}

/** The definition a starting point sends, or `null` when it copies a built-in. */
export function starterDefinition(key: StarterKey, name: string, tr: Translate): NoteTypeInput | null {
  const word = field('word', tr('notetype.start.field.word'), true, true);
  const meaning = field('meaning', tr('notetype.start.field.meaning'), true);
  switch (key) {
    case 'word':
      return { name, kind: 'standard', fields: [word, meaning], templates: [recognition(tr, '{{meaning}}')], css: '', sort_field: 'word' };
    case 'word_example':
      return {
        name,
        kind: 'standard',
        fields: [word, meaning, field('example', tr('notetype.start.field.example'), false)],
        templates: [recognition(tr, '{{meaning}}{{#example}}<div class="fc-example">{{example}}</div>{{/example}}')],
        css: '',
        sort_field: 'word',
      };
    default:
      return null;
  }
}

/** The built-in note type a starting point copies, if it copies one and it exists. */
export function starterSource(key: StarterKey, types: NoteTypeDto[]): NoteTypeDto | null {
  const marker = COPIES[key];
  return marker ? (types.find((n) => n.builtin === marker) ?? null) : null;
}

/** Whether the starting point can be used (a copied built-in may have been deleted). */
export function starterAvailable(key: StarterKey, types: NoteTypeDto[]): boolean {
  return !COPIES[key] || starterSource(key, types) !== null;
}

/** The `notetypes.create` request for a starting point named `name`, or `null` when unavailable. */
export function starterRequest(key: StarterKey, name: string, types: NoteTypeDto[], tr: Translate): CreateNoteType | null {
  const trimmed = name.trim();
  const definition = starterDefinition(key, trimmed, tr);
  if (definition) return { name: trimmed, copy_from: null, definition, id: null };
  const src = starterSource(key, types);
  return src ? { name: trimmed, copy_from: src.id, definition: null, id: null } : null;
}

/**
 * Where a field key is used in the templates: `{{key}}`, `{{#key}}…{{/key}}`, `{{type:key}}`,
 * … (display only: tells the user why removing it needs a template edit first; the backend
 * validates templates on save).
 */
export function templatesUsing(key: string, templates: CardTemplate[]): CardTemplate[] {
  if (!key) return [];
  const esc = key.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  const re = new RegExp(`\\{\\{\\s*[#^/]?\\s*(?:[\\w-]+:)*${esc}\\s*\\}\\}`);
  return templates.filter((t) => re.test(t.front) || re.test(t.back));
}

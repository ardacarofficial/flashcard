/**
 * Dev-only mock of the v0.3 card methods (`pnpm dev:mock`). Deliberately naive: no template
 * language, no sanitizing — the real rendering lives in Rust (fc-core::content).
 */
import { err, menu, module, type MockEnv, type MockFeature } from '$lib/api/mock/env';
import type { CardMetaDto, FieldRole, FlagDto, MediaDto, NoteTypeDto, NoteTypeInput, RenderedSides } from '$lib/api/types';
import { CLASSIC_NOTETYPE_ID } from '$lib/api/client';

const esc = (s: string) => s.replace(/[&<>"]/g, (c) => `&#${c.charCodeAt(0)};`);

function classic(): NoteTypeDto {
  return {
    id: CLASSIC_NOTETYPE_ID,
    name: 'Klasik',
    kind: 'standard',
    fields: [
      { key: 'front', name: 'Ön', kind: 'rich_text', required: true, unique: true, searchable: true, default_value: null, role: 'term' },
      { key: 'back', name: 'Arka', kind: 'rich_text', required: false, unique: false, searchable: true, default_value: null, role: 'meaning' },
    ],
    templates: [{ key: 'forward', name: '1', front: '{{front}}', back: '{{FrontSide}}<hr>{{back}}', card_type: 'classic', answer_field: 'back' }],
    css: '',
    sort_field: 'front',
    version: 1,
    builtin: 'classic',
    updated_at: 0,
  };
}

/** Well-known id of a built-in note type (`fc_core::content::builtin`, `id(n)`). */
const builtinId = (n: number) => `01900000-0000-7000-8000-${n.toString(16).padStart(12, '0')}`;
type MockField = [key: string, name: string, kind: NoteTypeDto['fields'][number]['kind'], required?: boolean, unique?: boolean, role?: FieldRole];
const ANSWER = '{{FrontSide}}\n<hr class="fc-answer">\n';
/** The labelled example part of the built-in vocabulary backs (fc_core `example_part!`). */
const examplePart = (field: string, translation?: string) =>
  `{{#${field}}}<div class="fc-part fc-part-example"><span class="fc-part-label" data-fc-part="example"></span><div class="fc-example-text">{{${field}}}</div>` +
  (translation ? `{{#${translation}}}<div class="fc-example-translation">{{${translation}}}</div>{{/${translation}}}` : '') +
  `</div>{{/${field}}}`;
/** fc_core `PARTS_CSS`: the built-in vocabulary types ship it as their note type CSS. */
const PARTS_CSS = `.fc-part { margin-top: 0.75em; }
.fc-part-meaning, .fc-part-answer { font-size: 1.3em; font-weight: 600; line-height: 1.35; }
.fc-part-label { display: block; margin-bottom: 0.15em; font-size: 0.7em; font-weight: 600; letter-spacing: 0.06em; text-transform: uppercase; opacity: 0.65; }
.fc-part-label:empty { display: none; }
.fc-part-example { padding-left: 0.75em; border-left: 2px solid rgba(128, 128, 128, 0.45); }
.fc-example-translation { margin-top: 0.25em; opacity: 0.7; }
.fc-part-source { font-size: 0.8em; opacity: 0.7; }
.fc-part-source .fc-part-label { display: inline; margin-right: 0.4em; }
`;
const SOURCE_PART = '{{#source}}<div class="fc-part fc-part-source"><span class="fc-part-label" data-fc-part="source"></span>{{source}}</div>{{/source}}';

/** A few more built-ins (TR names, as the app seeds them) so the note type screen has choices. */
function builtins(): NoteTypeDto[] {
  const make = (
    n: number,
    builtin: string,
    name: string,
    kind: NoteTypeDto['kind'],
    fields: MockField[],
    templates: NoteTypeDto['templates'],
    css = '',
  ): NoteTypeDto => ({
    id: builtinId(n),
    name,
    kind,
    fields: fields.map(([key, name, kind, required = false, unique = false, role]) => ({
      key,
      name,
      kind,
      required: required || unique,
      unique,
      searchable: kind === 'text' || kind === 'rich_text',
      default_value: null,
      ...(role ? { role } : {}),
    })),
    templates,
    css,
    sort_field: fields[0]![0],
    version: 1,
    builtin,
    updated_at: 0,
  });
  return [
    make(2, 'classic_reverse', 'Klasik + ters', 'standard', [['front', 'Ön', 'rich_text', true, true, 'term'], ['back', 'Arka', 'rich_text', true, false, 'meaning']], [
      { key: 'forward', name: 'İleri', front: '{{front}}', back: `${ANSWER}{{back}}`, card_type: 'classic', answer_field: 'back' },
      { key: 'reverse', name: 'Ters', front: '{{back}}', back: `${ANSWER}{{front}}`, card_type: 'classic', answer_field: 'front' },
    ]),
    make(5, 'cloze', 'Boşluk doldurma', 'cloze', [['text', 'Metin', 'rich_text', true, true], ['extra', 'Ek bilgi', 'rich_text', false, false, 'note']], [
      { key: 'cloze', name: 'Cloze', front: '{{cloze:text}}', back: '{{cloze:text}}{{#extra}}\n<hr class="fc-answer">\n{{extra}}{{/extra}}', card_type: 'cloze' },
    ]),
    make(
      10,
      'vocabulary',
      'Kelime kartı',
      'standard',
      [
        ['word', 'Kelime', 'rich_text', true, true, 'term'],
        ['meaning', 'Anlam', 'rich_text', true, false, 'meaning'],
        ['pronunciation', 'Telaffuz', 'text'],
        ['example', 'Örnek cümle', 'rich_text', false, false, 'example'],
        ['audio', 'Ses', 'audio'],
        ['image', 'Resim', 'image'],
      ],
      [
        {
          key: 'recognition',
          name: 'Tanıma',
          front: '{{word}}',
          back: `${ANSWER}<div class="fc-part fc-part-meaning">{{meaning}}</div>${examplePart('example')}`,
          card_type: 'classic',
          answer_field: 'meaning',
        },
        {
          key: 'production',
          name: 'Üretme',
          front: '{{meaning}}',
          back: `${ANSWER}<div class="fc-part fc-part-answer">{{word}}</div>${examplePart('example')}`,
          card_type: 'classic',
          answer_field: 'word',
        },
      ],
      PARTS_CSS,
    ),
    make(
      12,
      'subtitle',
      'Altyazı kartı',
      'standard',
      [
        ['word', 'Kelime', 'rich_text', true, false, 'term'],
        ['surface', 'Geçtiği hâli', 'text', false, false, 'surface'],
        ['pos', 'Sözcük türü', 'text'],
        ['meaning', 'Anlam', 'rich_text', false, false, 'meaning'],
        ['sentence', 'Cümle', 'rich_text', false, false, 'example'],
        ['translation', 'Çeviri', 'rich_text', false, false, 'example_translation'],
        ['image', 'Ekran görüntüsü', 'image'],
        ['audio', 'Ses', 'audio'],
        ['source', 'Kaynak', 'rich_text', false, false, 'source'],
      ],
      [
        {
          key: 'recognition',
          name: 'Tanıma',
          front: '<div class="fc-word">{{word}}</div>{{#sentence}}<div class="fc-example">{{sentence}}</div>{{/sentence}}',
          back:
            '<div class="fc-word">{{word}}</div>\n<hr class="fc-answer">\n<div class="fc-part fc-part-meaning">{{meaning}}</div>{{#pos}}<div class="fc-pron">{{pos}}</div>{{/pos}}' +
            examplePart('sentence', 'translation') +
            SOURCE_PART,
          card_type: 'classic',
          answer_field: 'meaning',
        },
      ],
      PARTS_CSS,
    ),
  ];
}

/**
 * A definition with keys for new fields / templates, as the backend stores it. Roles and
 * answer fields are kept (an answer field whose field is gone is cleared, as in Rust).
 */
function keyed(def: NoteTypeInput): Pick<NoteTypeDto, 'fields' | 'templates'> {
  const fields = def.fields.map((f, i) => ({ ...f, key: f.key || `field_${i + 1}` }));
  const keys = new Set(fields.map((f) => f.key));
  const templates = def.templates.map(({ answer_field, ...t }, i) => ({
    ...t,
    key: t.key || `template_${i + 1}`,
    ...(answer_field && keys.has(answer_field) ? { answer_field } : {}),
  }));
  return { fields, templates };
}

/** Just enough of the template language for a believable preview (the real one is fc-core). */
function renderSide(src: string, fields: Record<string, string>, front = ''): string {
  // Sections may nest (`{{#sentence}}…{{#translation}}…{{/translation}}…{{/sentence}}`).
  let out = src.replace(/\{\{FrontSide\}\}/g, front);
  for (let prev = ''; prev !== out; ) {
    prev = out;
    out = out.replace(/\{\{#(\w+)\}\}([\s\S]*?)\{\{\/\1\}\}/g, (_m, k: string, inner: string) => (fields[k]?.trim() ? inner : ''));
  }
  return out
    .replace(/\{\{(?:[\w-]+:)*(\w+)\}\}/g, (_m, k: string) => esc(fields[k] ?? '').replace(/\{\{c\d+::([^}]*)\}\}/g, '<b>[…]</b>'));
}

export default {
  modules: [
    module('cardtypes', [menu('cardtypes.notetypes', 'nav.notetypes', '/notetypes', 60)]),
    module('media', []),
    module('links', []),
    module('flags', []),
    module('card_notes', []),
    module('math', []),
    module('code_highlight', []),
  ],
  handlers: ({ notes, emit, cardFlags }: MockEnv) => {
    const types: NoteTypeDto[] = [classic(), ...builtins()];
    const media = new Map<string, { dto: MediaDto; data: string }>();
    const flags: FlagDto[] = ['red', 'orange', 'green', 'blue', 'pink', 'turquoise', 'purple'].map((color, i) => ({
      flag: i + 1,
      name: null,
      color,
    }));
    const meta = new Map<string, CardMetaDto>();
    const requireType = (id: string) => {
      const nt = types.find((x) => x.id === id);
      if (!nt) throw err('NOT_FOUND_NOTE_TYPE', 'errors.not_found.note_type', id);
      return nt;
    };
    const requireNote = (id: string) => {
      const n = notes.find((x) => x.id === id);
      if (!n) throw err('NOT_FOUND_NOTE', 'errors.not_found.note', id);
      return n;
    };
    const metaOf = (c: { id: string; deck_id: string; template_key: string }): CardMetaDto =>
      meta.get(c.id) ?? {
        id: c.id,
        template_key: c.template_key,
        template_name: c.template_key,
        card_type: 'classic',
        deck_id: c.deck_id,
        flags: cardFlags.get(c.id) ?? 0,
        user_note: null,
        empty: false,
      };
    const side = (fields: Record<string, string>): RenderedSides => {
      const vals = Object.values(fields).filter((v) => v.trim());
      return {
        template_key: 'forward',
        template_name: '1',
        card_type: 'classic',
        question: esc(vals[0] ?? ''),
        answer: vals.map(esc).join('<hr>'),
        css: '',
        interaction: { kind: 'self_grade' },
        tts: [],
      };
    };
    return {
      'notetypes.list': () => ({ notetypes: types }),
      'notetypes.get': ({ id }) => requireType(id),
      'notetypes.usage': ({ id }) => {
        const of = notes.filter((n) => n.notetype_id === id);
        const cards_by_template: Record<string, number> = {};
        for (const c of of.flatMap((n) => n.cards)) cards_by_template[c.template_key] = (cards_by_template[c.template_key] ?? 0) + 1;
        return { notes: of.length, cards_by_template };
      },
      'notetypes.create': ({ name, copy_from, definition }) => {
        const src = copy_from ? requireType(copy_from) : null;
        const base: NoteTypeDto = src ? structuredClone(src) : definition ? { ...classic(), ...structuredClone(definition) } : classic();
        const nt: NoteTypeDto = {
          ...base,
          ...keyed(base),
          id: `nt-${types.length + 1}`,
          name: name ?? (src ? `${src.name} (2)` : 'Note type'),
          version: 1,
          builtin: null,
        };
        types.push(nt);
        emit({ type: 'note_type_changed', data: { notetype_id: nt.id } });
        return nt;
      },
      'notetypes.update': ({ id, definition }) => {
        const nt = requireType(id);
        Object.assign(nt, keyed(definition), { version: nt.version + 1 });
        emit({ type: 'note_type_changed', data: { notetype_id: id } });
        return nt;
      },
      'notetypes.delete': ({ id }) => {
        const nt = requireType(id);
        if (id === CLASSIC_NOTETYPE_ID) throw err('CONFLICT_NOTE_TYPE_REQUIRED', 'errors.conflict.note_type_required', id);
        if (notes.some((n) => n.notetype_id === id)) throw err('CONFLICT_NOTE_TYPE_IN_USE', 'errors.conflict.note_type_in_use', id);
        types.splice(types.indexOf(nt), 1);
        emit({ type: 'note_type_changed', data: { notetype_id: id } });
        return nt;
      },
      'cardtypes.list': () => ({
        card_types: ['classic', 'cloze', 'image_occlusion', 'multiple_choice', 'typing'].map((id) => ({
          id,
          name_key: `cardtypes.engine.${id}`,
        })),
      }),
      'cards.preview': ({ fields, definition }) => {
        if (!definition) return { cards: Object.values(fields).some((v) => v.trim()) ? [side(fields)] : [] };
        const cards = definition.templates.map((t): RenderedSides => {
          const question = renderSide(t.front, fields);
          return { ...side(fields), template_key: t.key, template_name: t.name, card_type: t.card_type, question, answer: renderSide(t.back, fields, question), css: definition.css };
        });
        return { cards: cards.filter((c) => c.question.trim()) };
      },
      'cards.meta': ({ note_id }) => ({ items: requireNote(note_id).cards.map(metaOf) }),
      'cards.set_flag': ({ ids, flag, on }) => {
        const items = ids.map((id) => {
          const card = notes.flatMap((n) => n.cards).find((c) => c.id === id);
          if (!card) throw err('NOT_FOUND_CARD', 'errors.not_found.card', id);
          const m = metaOf(card);
          m.flags = on ? m.flags | (1 << (flag - 1)) : m.flags & ~(1 << (flag - 1));
          meta.set(id, m);
          cardFlags.set(id, m.flags);
          return { id, flags: m.flags };
        });
        emit({ type: 'cards_updated', data: { ids } });
        return { items };
      },
      'cards.set_user_note': ({ id, text }) => {
        const card = notes.flatMap((n) => n.cards).find((c) => c.id === id);
        if (!card) throw err('NOT_FOUND_CARD', 'errors.not_found.card', id);
        const m = { ...metaOf(card), user_note: text?.trim() || null };
        meta.set(id, m);
        emit({ type: 'cards_updated', data: { ids: [id] } });
        return m;
      },
      'flags.list': () => ({ flags }),
      'flags.update': ({ flag, name, color }) => {
        const f = flags[flag - 1];
        if (!f) throw err('VALIDATION_FLAG', 'errors.validation.flag', String(flag));
        f.name = name?.trim() || null;
        if (color) f.color = color;
        emit({ type: 'flags_changed' });
        return f;
      },
      'links.suggest': ({ prefix }) => ({
        items: notes
          .filter((n) => n.sort_field.toLowerCase().startsWith(prefix.toLowerCase()))
          .map((n) => ({ id: n.id, title: n.sort_field })),
      }),
      'links.backlinks': () => ({ items: [] }),
      'media.add': ({ name, data_base64 }) => {
        const hash = Array.from({ length: 64 }, (_, i) => ((data_base64.charCodeAt(i % data_base64.length) + i) % 16).toString(16)).join('');
        const existing = media.get(hash);
        if (existing) return { ...existing.dto, created: false };
        const dto: MediaDto = {
          hash,
          reference: `media:${hash}`,
          mime: 'image/png',
          kind: 'image',
          size: data_base64.length,
          original_name: name ?? null,
          ref_count: 0,
          created: true,
        };
        media.set(hash, { dto, data: data_base64 });
        return dto;
      },
      'media.read': ({ hash }) => {
        const m = media.get(hash);
        if (!m) throw err('NOT_FOUND_MEDIA', 'errors.not_found.media', hash);
        return { mime: m.dto.mime, data_base64: m.data };
      },
      'notes.get': ({ id }) => requireNote(id),
      'notes.update': ({ id, fields }) => {
        const n = requireNote(id);
        n.fields = fields;
        n.sort_field = fields['front'] ?? n.sort_field;
        emit({ type: 'note_updated', data: { note_id: id } });
        return n;
      },
    };
  },
} satisfies MockFeature;

/**
 * Dev-only mock of the v0.3 card methods (`pnpm dev:mock`). Deliberately naive: no template
 * language, no sanitizing — the real rendering lives in Rust (fc-core::content).
 */
import { err, menu, module, type MockEnv, type MockFeature } from '$lib/api/mock/env';
import type { CardMetaDto, FlagDto, MediaDto, NoteTypeDto, RenderedSides } from '$lib/api/types';
import { CLASSIC_NOTETYPE_ID } from '$lib/api/client';

const esc = (s: string) => s.replace(/[&<>"]/g, (c) => `&#${c.charCodeAt(0)};`);

function classic(): NoteTypeDto {
  return {
    id: CLASSIC_NOTETYPE_ID,
    name: 'Klasik',
    kind: 'standard',
    fields: [
      { key: 'front', name: 'Ön', kind: 'rich_text', required: true, unique: true, searchable: true, default_value: null },
      { key: 'back', name: 'Arka', kind: 'rich_text', required: false, unique: false, searchable: true, default_value: null },
    ],
    templates: [{ key: 'forward', name: '1', front: '{{front}}', back: '{{FrontSide}}<hr>{{back}}', card_type: 'classic' }],
    css: '',
    sort_field: 'front',
    version: 1,
    builtin: 'classic',
    updated_at: 0,
  };
}

/** Well-known id of a built-in note type (`fc_core::content::builtin`, `id(n)`). */
const builtinId = (n: number) => `01900000-0000-7000-8000-${n.toString(16).padStart(12, '0')}`;
type MockField = [key: string, name: string, kind: NoteTypeDto['fields'][number]['kind'], required?: boolean, unique?: boolean];
const ANSWER = '{{FrontSide}}\n<hr class="fc-answer">\n';

/** A few more built-ins (TR names, as the app seeds them) so the note type screen has choices. */
function builtins(): NoteTypeDto[] {
  const make = (
    n: number,
    builtin: string,
    name: string,
    kind: NoteTypeDto['kind'],
    fields: MockField[],
    templates: NoteTypeDto['templates'],
  ): NoteTypeDto => ({
    id: builtinId(n),
    name,
    kind,
    fields: fields.map(([key, name, kind, required = false, unique = false]) => ({
      key,
      name,
      kind,
      required: required || unique,
      unique,
      searchable: kind === 'text' || kind === 'rich_text',
      default_value: null,
    })),
    templates,
    css: '',
    sort_field: fields[0]![0],
    version: 1,
    builtin,
    updated_at: 0,
  });
  return [
    make(2, 'classic_reverse', 'Klasik + ters', 'standard', [['front', 'Ön', 'rich_text', true, true], ['back', 'Arka', 'rich_text', true]], [
      { key: 'forward', name: 'İleri', front: '{{front}}', back: `${ANSWER}{{back}}`, card_type: 'classic' },
      { key: 'reverse', name: 'Ters', front: '{{back}}', back: `${ANSWER}{{front}}`, card_type: 'classic' },
    ]),
    make(5, 'cloze', 'Boşluk doldurma', 'cloze', [['text', 'Metin', 'rich_text', true, true], ['extra', 'Ek bilgi', 'rich_text']], [
      { key: 'cloze', name: 'Cloze', front: '{{cloze:text}}', back: '{{cloze:text}}{{#extra}}\n<hr class="fc-answer">\n{{extra}}{{/extra}}', card_type: 'cloze' },
    ]),
    make(
      10,
      'vocabulary',
      'Kelime kartı',
      'standard',
      [
        ['word', 'Kelime', 'rich_text', true, true],
        ['meaning', 'Anlam', 'rich_text', true],
        ['pronunciation', 'Telaffuz', 'text'],
        ['example', 'Örnek cümle', 'rich_text'],
        ['audio', 'Ses', 'audio'],
        ['image', 'Resim', 'image'],
      ],
      [
        { key: 'recognition', name: 'Tanıma', front: '{{word}}', back: `${ANSWER}{{meaning}}{{#example}}<div class="fc-example">{{example}}</div>{{/example}}`, card_type: 'classic' },
        { key: 'production', name: 'Üretme', front: '{{meaning}}', back: `${ANSWER}{{word}}{{#example}}<div class="fc-example">{{example}}</div>{{/example}}`, card_type: 'classic' },
      ],
    ),
    make(
      12,
      'subtitle',
      'Altyazı kartı',
      'standard',
      [
        ['word', 'Kelime', 'rich_text', true],
        ['surface', 'Geçtiği hâli', 'text'],
        ['pos', 'Sözcük türü', 'text'],
        ['meaning', 'Anlam', 'rich_text'],
        ['sentence', 'Cümle', 'rich_text'],
        ['translation', 'Çeviri', 'rich_text'],
        ['image', 'Ekran görüntüsü', 'image'],
        ['audio', 'Ses', 'audio'],
        ['source', 'Kaynak', 'rich_text'],
      ],
      [
        {
          key: 'recognition',
          name: 'Tanıma',
          front: '<div class="fc-word">{{word}}</div>{{#sentence}}<div class="fc-example">{{sentence}}</div>{{/sentence}}',
          back: `${ANSWER}{{meaning}}{{#translation}}<div class="fc-example">{{translation}}</div>{{/translation}}{{#source}}<div class="fc-source">{{source}}</div>{{/source}}`,
          card_type: 'classic',
        },
      ],
    ),
  ];
}

/** Just enough of the template language for a believable preview (the real one is fc-core). */
function renderSide(src: string, fields: Record<string, string>, front = ''): string {
  return src
    .replace(/\{\{FrontSide\}\}/g, front)
    .replace(/\{\{#(\w+)\}\}([\s\S]*?)\{\{\/\1\}\}/g, (_m, k: string, inner: string) => (fields[k]?.trim() ? inner : ''))
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
          id: `nt-${types.length + 1}`,
          name: name ?? (src ? `${src.name} (2)` : 'Note type'),
          fields: base.fields.map((f, i) => ({ ...f, key: f.key || `field_${i + 1}` })),
          version: 1,
          builtin: null,
        };
        types.push(nt);
        emit({ type: 'note_type_changed', data: { notetype_id: nt.id } });
        return nt;
      },
      'notetypes.update': ({ id, definition }) => {
        const nt = requireType(id);
        Object.assign(nt, definition, {
          fields: definition.fields.map((f, i) => ({ ...f, key: f.key || `field_${i + 1}` })),
          version: nt.version + 1,
        });
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
          return { ...side(fields), template_key: t.key, template_name: t.name, card_type: t.card_type, question, answer: renderSide(t.back, fields, question) };
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

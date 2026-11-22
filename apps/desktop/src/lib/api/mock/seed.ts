/**
 * Demo fixtures of the dev mock: several decks with Seviye/Sinav classifications, due
 * counts, notes of mixed maturity. Exists so `pnpm screens` (visual QA) and `pnpm dev:mock`
 * look like a real collection. Dev-only; behaviour lives in fc-app.
 */
import type { AttributeDefDto, DeckCounts, DeckDto, NoteDto } from '../types';
import type { MockEnv } from './env';

type Env = Pick<MockEnv, 'now' | 'id' | 'decks' | 'attrs' | 'notes' | 'addDeck'>;

const CEFR = ['A1', 'A2', 'B1', 'B2', 'C1', 'C2'];
const EXAMS = ['YKS', 'TOEFL', 'IELTS', 'KPSS', 'YDS'];

/** Counts with the v1.3 `today_*` twins (the mock's today numbers are the plain ones). */
export const deckCounts = (c: Pick<DeckCounts, 'new' | 'learning' | 'due' | 'total'>): DeckCounts => ({
  ...c,
  today_due: c.due,
  today_learning: c.learning,
  today_new: c.new,
  today_total: c.new + c.learning + c.due,
});

/** Per deck-name counts shown in the tree (leaf numbers; parents show their own too). */
export const DEMO_COUNTS: Record<string, DeckCounts> = {
  İngilizce: deckCounts({ new: 12, learning: 4, due: 31, total: 420 }),
  Fiiller: deckCounts({ new: 5, learning: 2, due: 14, total: 160 }),
  Kelime: deckCounts({ new: 7, learning: 2, due: 17, total: 210 }),
  Deyimler: deckCounts({ new: 0, learning: 0, due: 0, total: 50 }),
  YKS: deckCounts({ new: 20, learning: 6, due: 48, total: 640 }),
  Matematik: deckCounts({ new: 8, learning: 3, due: 20, total: 260 }),
  Biyoloji: deckCounts({ new: 6, learning: 2, due: 18, total: 220 }),
  Tarih: deckCounts({ new: 6, learning: 1, due: 10, total: 160 }),
  Almanca: deckCounts({ new: 10, learning: 0, due: 0, total: 35 }),
  'Tıbbi Terminoloji': deckCounts({ new: 0, learning: 1, due: 9, total: 180 }),
};

const NOTES: Array<[deck: string, front: string, back: string, level?: string, exam?: string]> = [
  ['Fiiller', 'to give up', 'vazgeçmek', 'B1', 'TOEFL'],
  ['Fiiller', 'to carry out', 'yürütmek, yerine getirmek', 'B2', 'IELTS'],
  ['Fiiller', 'to come across', 'rastlamak', 'B2'],
  ['Kelime', 'ubiquitous', 'her yerde bulunan', 'C1', 'TOEFL'],
  ['Kelime', 'reluctant', 'isteksiz', 'B2'],
  ['Kelime', 'borrow', 'ödünç almak', 'A2'],
  ['Deyimler', 'a piece of cake', 'çocuk oyuncağı', 'B1'],
  ['Matematik', 'sin²x + cos²x = ?', '1', undefined, 'YKS'],
  ['Matematik', 'Bir üçgenin iç açıları toplamı', '180°', undefined, 'YKS'],
  ['Biyoloji', 'Hücrenin enerji santrali', 'Mitokondri', undefined, 'YKS'],
  ['Tarih', 'Malazgirt Savaşı hangi yıl?', '1071', undefined, 'YKS'],
  ['Almanca', 'der Schmetterling', 'kelebek', 'A1'],
  ['Tıbbi Terminoloji', 'Dispne', 'Nefes darlığı'],
];

export function seedDemo(env: Env, englishId: string): void {
  const { now, id, decks, attrs, notes, addDeck } = env;
  const byName = (n: string): DeckDto => decks.find((d) => d.name === n)!;
  const add = (name: string, parent: DeckDto | null, color: string | null) => {
    const d = decks.find((x) => x.name === name && x.parent_id === (parent?.id ?? null)) ?? addDeck(name, parent);
    d.color = color;
    return d;
  };
  const en = decks.find((d) => d.id === englishId)!;
  en.color = '#3b82f6';
  add('Kelime', en, null);
  add('Deyimler', en, null);
  const yks = byName('YKS');
  yks.color = '#f59e0b';
  for (const n of ['Matematik', 'Biyoloji', 'Tarih']) add(n, yks, null);
  add('Almanca', null, '#10b981');
  add('Tıbbi Terminoloji', null, '#ef4444');

  const level = attrs.find((a) => a.key === 'level')!;
  level.scope_deck_id = null; // classification applies to every deck
  const exam: AttributeDefDto = {
    ...level,
    id: id('attr'),
    key: 'exam',
    name: 'Sınav',
    kind: 'enum',
    options: EXAMS.map((label, i) => ({ id: `opt-${label}`, label, color: null, order: i })),
    template_id: null,
    position: 1,
  };
  attrs.push(exam);

  for (const [deck, front, back, lvl, ex] of NOTES) {
    const d = byName(deck);
    const t = now();
    const n: NoteDto = {
      id: id('note'),
      notetype_id: '01900000-0000-7000-8000-000000000001',
      home_deck_id: d.id,
      fields: { front, back },
      tags: [],
      attributes: [
        ...(lvl ? [{ attr_id: level.id, value: { type: 'choice' as const, value: `opt-${lvl}` } }] : []),
        ...(ex ? [{ attr_id: exam.id, value: { type: 'choice' as const, value: `opt-${ex}` } }] : []),
      ],
      cards: [{ id: id('card'), deck_id: d.id, template_key: 'forward', state: 'new', due: null, suspended: false }],
      sort_field: front,
      created_at: t,
      updated_at: t,
    };
    notes.push(n);
  }
}

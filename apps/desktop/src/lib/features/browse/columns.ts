/** Search screen: which columns exist and how a row's state is labelled (display only). */
import type { CardRow, CardSortKey } from '$lib/api/types';
import { t, td, type MessageKey } from '$lib/i18n/index.svelte';
import type { Tone } from '$lib/ui/types';

export type ColumnId = 'sort_field' | 'deck' | 'state' | 'due' | 'lapses' | 'reps' | 'tags' | 'created';

export interface ColumnDef {
  id: ColumnId;
  label: MessageKey;
  /** Sort key when the backend can sort by it. */
  sort: CardSortKey | null;
  width: string;
  /** Always shown (the row's name). */
  fixed?: boolean;
}

export const COLUMNS: ColumnDef[] = [
  { id: 'sort_field', label: 'browse.col.front', sort: 'sort_field', width: 'minmax(180px, 3fr)', fixed: true },
  { id: 'deck', label: 'browse.col.deck', sort: null, width: 'minmax(120px, 2fr)' },
  { id: 'state', label: 'browse.col.state', sort: null, width: '120px' },
  { id: 'due', label: 'browse.col.due', sort: 'due', width: '110px' },
  { id: 'lapses', label: 'browse.col.lapses', sort: 'lapses', width: '80px' },
  { id: 'reps', label: 'browse.col.reps', sort: 'reps', width: '80px' },
  { id: 'tags', label: 'browse.col.tags', sort: null, width: 'minmax(100px, 1fr)' },
  { id: 'created', label: 'browse.col.created', sort: 'created', width: '110px' },
];

export const DEFAULT_COLUMNS: ColumnId[] = ['sort_field', 'deck', 'state', 'due', 'tags'];

const STORE = 'fc.browse.columns';

/** The viewer's column choice (a per-device convenience; falls back to the defaults). */
export function loadColumns(): ColumnId[] {
  try {
    const raw = JSON.parse(localStorage.getItem(STORE) ?? 'null') as unknown;
    if (Array.isArray(raw)) {
      const ids = raw.filter((x): x is ColumnId => COLUMNS.some((c) => c.id === x));
      if (ids.includes('sort_field')) return ids;
    }
  } catch {
    /* ignore */
  }
  return [...DEFAULT_COLUMNS];
}

export function saveColumns(ids: ColumnId[]): void {
  try {
    localStorage.setItem(STORE, JSON.stringify(ids));
  } catch {
    /* ignore */
  }
}

export function stateLabel(r: Pick<CardRow, 'suspended' | 'buried' | 'state'>): string {
  if (r.suspended) return t('browse.state.suspended');
  if (r.buried) return t('browse.state.buried');
  return td(`browse.state.${r.state}`);
}

/** Same colours as Home and the deck page: new grey, learning orange, review green. */
export function stateTone(r: Pick<CardRow, 'suspended' | 'buried' | 'state'>): Tone {
  if (r.suspended) return 'danger';
  if (r.buried) return 'neutral';
  switch (r.state) {
    case 'learning':
    case 'relearning':
      return 'warning';
    case 'review':
      return 'success';
    default:
      return 'neutral';
  }
}

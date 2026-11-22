/**
 * "Getting started" guide state (v1.3): which of its four steps the user has done and whether
 * the checklist is showing. UI state only (`localStorage`, per device): the backend keeps
 * what matters (`onboarding.status` — was the first-run setup done or skipped).
 *
 * Kept free of components so anything (the help sheet, the starters page) can import
 * {@link openGuide} without pulling the guide's UI into its chunk.
 */
export const GUIDE_STEPS = ['add', 'study', 'where', 'extension'] as const;
export type GuideStep = (typeof GUIDE_STEPS)[number];

/** `panel`: the checklist; `pill`: hidden to a small button; `closed`: finished or skipped. */
export type GuideView = 'panel' | 'pill' | 'closed';

interface Saved {
  view: GuideView;
  done: GuideStep[];
  /** The deck the setup created (step 2 studies it). */
  deck: string | null;
}

const KEY = 'fc.guide';

function read(): Saved {
  const empty: Saved = { view: 'closed', done: [], deck: null };
  try {
    const raw = localStorage.getItem(KEY);
    if (!raw) return empty;
    const v = JSON.parse(raw) as Partial<Saved>;
    return {
      view: v.view === 'panel' || v.view === 'pill' ? v.view : 'closed',
      done: Array.isArray(v.done) ? GUIDE_STEPS.filter((s) => v.done!.includes(s)) : [],
      deck: typeof v.deck === 'string' ? v.deck : null,
    };
  } catch {
    return empty;
  }
}

const state = $state<Saved & { focus: number }>({ ...read(), focus: 0 });

function save(): void {
  try {
    localStorage.setItem(KEY, JSON.stringify({ view: state.view, done: state.done, deck: state.deck }));
  } catch {
    /* storage unavailable: the guide just forgets */
  }
}

/** Reactive view of the guide (read it in components and effects). */
export const guide = {
  get view(): GuideView {
    return state.view;
  },
  get done(): readonly GuideStep[] {
    return state.done;
  },
  get deck(): string | null {
    return state.deck;
  },
  /** Bumped by {@link openGuide}: the checklist takes the keyboard focus (the user asked for it). */
  get focus(): number {
    return state.focus;
  },
};

/** Opens the checklist again — from Help or the starters page, also after it was finished. */
export function openGuide(): void {
  state.view = 'panel';
  state.focus += 1;
  save();
}

/**
 * Shows the checklist without taking focus (right after the first-run setup). `added`: the
 * setup put cards in (a starter deck) — step 1 is done; an empty deck ("my own words") leaves
 * it open (it ticks itself once the collection has cards) and the checklist starts folded,
 * so it never covers the add page the user is about to type on.
 */
export function startGuide(deck: string | null, added = true): void {
  state.view = added ? 'panel' : 'pill';
  state.deck = deck;
  if (added && !state.done.includes('add')) state.done = [...state.done, 'add'];
  save();
}

export function setGuideView(view: GuideView): void {
  state.view = view;
  save();
}

export function markStep(step: GuideStep): void {
  if (state.done.includes(step)) return;
  state.done = GUIDE_STEPS.filter((s) => s === step || state.done.includes(s));
  save();
}

/** Tests: back to a device that never saw the guide. */
export function resetGuide(): void {
  state.view = 'closed';
  state.done = [];
  state.deck = null;
  state.focus = 0;
  try {
    localStorage.removeItem(KEY);
  } catch {
    /* ignore */
  }
}

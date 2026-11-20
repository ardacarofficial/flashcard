/**
 * Form drafts (v1.0 crash recovery): a form keeps what the user typed under a key in the
 * backend (`maintenance.drafts.*`) so a crash or an unclean shutdown loses nothing; the form
 * restores it when it opens again and discards it once saved. Best effort: a failing call
 * never disturbs the form.
 */
import { api } from '$lib/api';

/** How long typing must pause before the draft is written. */
export const DRAFT_DELAY_MS = 800;

export interface DraftKeeper<T> {
  /** The saved draft, if any. */
  restore(): Promise<T | null>;
  /** Saves `payload` after a pause in typing (`null` or empty = nothing worth keeping: discard). */
  save(payload: T | null): void;
  /** Drops the draft (the form was saved or cleared). */
  discard(): Promise<void>;
}

export function draftKeeper<T>(key: string, delay = DRAFT_DELAY_MS): DraftKeeper<T> {
  let timer: ReturnType<typeof setTimeout> | null = null;
  const cancel = () => {
    if (timer) clearTimeout(timer);
    timer = null;
  };
  return {
    async restore() {
      try {
        const r = await api.call('maintenance.drafts.get', { key });
        return (r.draft?.payload as T | undefined) ?? null;
      } catch {
        return null;
      }
    },
    save(payload) {
      cancel();
      timer = setTimeout(() => {
        timer = null;
        const call =
          payload === null
            ? api.call('maintenance.drafts.discard', { key })
            : api.call('maintenance.drafts.save', { key, payload });
        call.catch(() => {});
      }, delay);
    },
    async discard() {
      cancel();
      try {
        await api.call('maintenance.drafts.discard', { key });
      } catch {
        // Nothing to do.
      }
    },
  };
}

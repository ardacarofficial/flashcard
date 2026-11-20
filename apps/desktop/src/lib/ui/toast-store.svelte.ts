/**
 * Toasts: short, non-blocking messages ("Saved", "Card deleted · Undo"). Any code calls
 * `toast.show(t('…'))`; one `<ToastHost />` (mounted by the app shell) draws them.
 * Errors that need a decision belong in a Dialog, not a toast.
 */
import type { Tone } from './types';

export interface ToastAction {
  label: string;
  run: () => void;
}

export interface ToastItem {
  id: number;
  message: string;
  tone: Exclude<Tone, 'accent'>;
  action?: ToastAction;
  /** Milliseconds before it disappears (0 = stays until dismissed). */
  duration: number;
}

let seq = 0;
const items = $state<ToastItem[]>([]);
const timers = new Map<number, ReturnType<typeof setTimeout>>();

export const toast = {
  get items(): readonly ToastItem[] {
    return items;
  },
  /** Shows a toast; returns its id. Default 5 s (8 s with an action). */
  show(message: string, opts: { tone?: ToastItem['tone']; action?: ToastAction; duration?: number } = {}): number {
    const id = ++seq;
    const duration = opts.duration ?? (opts.action ? 8000 : 5000);
    const item: ToastItem = { id, message, tone: opts.tone ?? 'neutral', duration };
    if (opts.action) item.action = opts.action;
    items.push(item);
    // At most 3 at once: the oldest goes.
    while (items.length > 3) toast.dismiss(items[0]!.id);
    if (duration > 0) timers.set(id, setTimeout(() => toast.dismiss(id), duration));
    return id;
  },
  dismiss(id: number): void {
    clearTimeout(timers.get(id));
    timers.delete(id);
    const i = items.findIndex((t) => t.id === id);
    if (i >= 0) items.splice(i, 1);
  },
  clear(): void {
    for (const t of [...items]) toast.dismiss(t.id);
  },
};

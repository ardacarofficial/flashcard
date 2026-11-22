/** Remembered open/closed state of a settings "Advanced" disclosure (per page id). */
const key = (id: string) => `fc.settings.advanced.${id}`;

export function loadOpen(id: string): boolean {
  try {
    return localStorage.getItem(key(id)) === '1';
  } catch {
    return false;
  }
}

export function saveOpen(id: string, open: boolean): void {
  try {
    if (open) localStorage.setItem(key(id), '1');
    else localStorage.removeItem(key(id));
  } catch {
    /* storage unavailable: the state just is not remembered */
  }
}

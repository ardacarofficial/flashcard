/**
 * The app's ONE "Advanced" switch (v1.4, prototype `ayarlar.html`): off by default, it hides
 * the rarely needed settings pages and sidebar places; nothing is deleted and search still
 * finds every page. Its value is the core setting `app.show_advanced` (remembered by the
 * backend, read back by the journeys), so screens of every feature read `advanced.on`.
 *
 * Presentation only: no rule of the core depends on it.
 */
import { api } from '$lib/api';

export const SHOW_ADVANCED = 'app.show_advanced';

class AdvancedState {
  on = $state(false);
  loaded = $state(false);

  /** Reads the remembered value (keeps `false` when the backend is unreachable). */
  async load(): Promise<void> {
    try {
      const r = await api.call('settings.get', { scope: { kind: 'app' }, keys: [SHOW_ADVANCED] });
      this.on = r.values.find((v) => v.key === SHOW_ADVANCED)?.value === true;
    } catch {
      /* stays off */
    } finally {
      this.loaded = true;
    }
  }

  /** Flips the switch at once and saves it; a failed save puts it back. */
  async set(on: boolean): Promise<boolean> {
    const before = this.on;
    this.on = on;
    try {
      await api.call('settings.set', { scope: { kind: 'app' }, key: SHOW_ADVANCED, value: on });
      return true;
    } catch {
      this.on = before;
      return false;
    }
  }
}

export const advanced = new AdvancedState();

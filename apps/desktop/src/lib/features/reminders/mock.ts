/**
 * Dev-only mock of the `reminders` module. Minimal: when a reminder is due is decided by
 * fc-core (`ux::reminder`); the desktop shell turns `reminder_due` into a notification.
 */
import { menu, module, type MockEnv, type MockFeature } from '$lib/api/mock/env';

export default {
  modules: [module('reminders', [menu('reminders.screen', 'nav.reminders', '/reminders', 95)])],
  handlers: ({ emit }: MockEnv) => {
    const counts = { due: 12, new: 20 };
    return {
      'reminders.status': () => ({
        enabled: false,
        minute_of_day: 19 * 60,
        weekdays: 0b111_1111,
        keep_in_tray: false,
        counts,
        studied_today: false,
        next_at: null,
      }),
      'reminders.test': () => {
        emit({ type: 'reminder_due', data: { counts, test: true } });
        return counts;
      },
    };
  },
} satisfies MockFeature;

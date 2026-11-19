import { module, settingsPage, type MockFeature } from '$lib/api/mock/env';

export default {
  modules: [module('core', [settingsPage('core.general', 'settings.pages.general', '/settings/general', 0)])],
  handlers: () => ({}),
} satisfies MockFeature;

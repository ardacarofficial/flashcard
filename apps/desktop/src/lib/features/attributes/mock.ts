import { err, menu, module, settingsPage, type MockEnv, type MockFeature } from '$lib/api/mock/env';

export default {
  modules: [module('attributes', [settingsPage('attributes.manage', 'settings.pages.attributes', '/settings/attributes', 20)])],
  handlers: ({ attrs, ancestors }: MockEnv) => ({
  'attributes.effective': ({ deck_id, target }) => {
    const chain = deck_id ? ancestors(deck_id) : [];
    return {
      attributes: attrs
        .filter((a) => a.target === target)
        .filter((a) => a.scope_deck_id === null || chain.includes(a.scope_deck_id))
        .map((def) => ({
          def,
          inherited: def.scope_deck_id !== deck_id,
          suggested_value: null,
        })),
    };
  },
  }),
} satisfies MockFeature;

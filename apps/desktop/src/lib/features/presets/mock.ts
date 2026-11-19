/**
 * Dev-only mock of the presets module (see `api/mock/transport.ts`). No validation and no
 * inheritance rules beyond the obvious: the real ones live in fc-app `modules/presets.rs`.
 */
import { menu, module, err, type MockEnv, type MockFeature } from '$lib/api/mock/env';
import type { PresetConfig, PresetDto } from '$lib/api/types';

const DEFAULTS: PresetConfig = {
  new_per_day: 20,
  reviews_per_day: 200,
  learning_steps_mins: [1, 10],
  relearning_steps_mins: [10],
  desired_retention: 0.9,
  fsrs_weights: [],
  bury_siblings: false,
  new_order: 'added',
  review_order: 'due',
};

export default {
  modules: [module('presets', [menu('presets.editor', 'nav.presets', '/presets', 15)])],
  handlers: ({ now, id, emit, decks }: MockEnv) => {
    const presets: PresetDto[] = [];
    const withDecks = (p: PresetDto): PresetDto => ({
      ...p,
      deck_ids: decks.filter((d) => d.preset_id === p.id).map((d) => d.id),
    });
    const find = (pid: string) => {
      const p = presets.find((x) => x.id === pid);
      if (!p) throw err('NOT_FOUND_PRESET', 'errors.not_found.preset', pid);
      return p;
    };
    const add = (name: string, config: PresetConfig) => {
      const t = now();
      const p: PresetDto = { id: id('preset'), name: name.trim(), config, deck_ids: [], created_at: t, updated_at: t };
      presets.push(p);
      emit({ type: 'presets_changed' });
      return withDecks(p);
    };
    return {
      'presets.list': () => ({
        items: [...presets].sort((a, b) => a.name.localeCompare(b.name, 'tr')).map(withDecks),
        defaults: DEFAULTS,
      }),
      'presets.get': ({ id: pid }) => withDecks(find(pid)),
      'presets.for_deck': ({ deck_id }) => {
        const p = presets.find((x) => x.id === decks.find((d) => d.id === deck_id)?.preset_id);
        return { deck_id, preset_id: p?.id ?? null, assigned_on: p ? deck_id : null, config: p?.config ?? DEFAULTS };
      },
      'presets.create': ({ name, config }) => add(name, config ?? DEFAULTS),
      'presets.clone': ({ id: pid, name }) => add(name, structuredClone(find(pid).config)),
      'presets.update': ({ id: pid, name, config }) => {
        const p = find(pid);
        Object.assign(p, { name: name.trim(), config, updated_at: now() });
        emit({ type: 'presets_changed' });
        return withDecks(p);
      },
      'presets.delete': ({ id: pid }) => {
        const p = withDecks(find(pid));
        presets.splice(presets.indexOf(find(pid)), 1);
        for (const d of decks) if (d.preset_id === pid) d.preset_id = null;
        emit({ type: 'presets_changed' });
        return p;
      },
      'presets.assign': ({ deck_ids, preset_id }) => {
        let changed = 0;
        for (const d of decks) {
          if (deck_ids.includes(d.id) && d.preset_id !== preset_id) {
            d.preset_id = preset_id;
            changed++;
          }
        }
        emit({ type: 'presets_changed' });
        return { changed };
      },
    };
  },
} satisfies MockFeature;

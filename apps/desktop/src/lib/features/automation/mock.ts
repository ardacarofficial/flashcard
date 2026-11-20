/**
 * Dev-only mock of the v0.9 `automation` module: keeps rules in memory and answers previews
 * with fixed numbers. Validation, matching, loop safety and the real dry run live in fc-app.
 */
import { err, menu, module, type MockEnv, type MockFeature } from '$lib/api/mock/env';
import type { ActionInfo, RuleDto, RunDto } from '$lib/api/types';

const ACTIONS = [
  ['bulk.add_tags', 'note_ids'],
  ['bulk.remove_tags', 'note_ids'],
  ['bulk.set_suspended', 'card_ids'],
  ['bulk.set_buried', 'card_ids'],
  ['bulk.set_flag', 'card_ids'],
  ['bulk.move_cards', 'card_ids'],
  ['bulk.set_attribute', 'card_ids'],
  ['bulk.reset_progress', 'card_ids'],
] as const;

export default {
  modules: [module('automation', [menu('automation.screen', 'nav.automation', '/automation', 85)])],
  handlers: ({ now, id, emit }: MockEnv) => {
    const rules: RuleDto[] = [];
    const runs: RunDto[] = [];
    let seq = 0;
    const find = (rid: string) => {
      const r = rules.find((x) => x.id === rid);
      if (!r) throw err('NOT_FOUND_AUTOMATION_RULE', 'errors.not_found.automation_rule', rid);
      return r;
    };
    const check = (name: string, actions: unknown[]) => {
      if (!name.trim() || actions.length === 0)
        throw err('VALIDATION_AUTOMATION_RULE', 'errors.validation.automation_rule', 'invalid rule');
    };
    const changed = () => emit({ type: 'automation_changed' });
    return {
      'automation.rules.list': () => ({ rules: structuredClone(rules) }),
      'automation.actions': () => ({
        actions: ACTIONS.map(
          ([method, selection]): ActionInfo => ({ method, selection, label_key: `automation.action.${method}` }),
        ),
      }),
      'automation.rules.create': ({ name, enabled, trigger, condition, actions }) => {
        check(name, actions);
        const t = now();
        const r: RuleDto = {
          id: id('rule'),
          name: name.trim(),
          enabled: enabled ?? true,
          position: rules.length,
          trigger,
          condition,
          actions,
          last_run_at: null,
          created_at: t,
          updated_at: t,
        };
        rules.push(r);
        changed();
        return r;
      },
      'automation.rules.update': (req) => {
        const r = find(req.id);
        if (req.name != null) r.name = req.name;
        if (req.enabled != null) r.enabled = req.enabled;
        if (req.trigger != null) r.trigger = req.trigger;
        if (req.condition != null) r.condition = req.condition;
        if (req.actions != null) r.actions = req.actions;
        check(r.name, r.actions);
        r.updated_at = now();
        changed();
        return r;
      },
      'automation.rules.delete': ({ id: rid }) => {
        rules.splice(rules.indexOf(find(rid)), 1);
        changed();
        return {};
      },
      'automation.rules.reorder': ({ ids }) => {
        const sorted = ids.map(find);
        rules.splice(0, rules.length, ...sorted);
        rules.forEach((r, i) => (r.position = i));
        changed();
        return { rules: structuredClone(rules) };
      },
      'automation.rules.run': ({ id: rid }) => {
        const r = find(rid);
        const run: RunDto = {
          seq: ++seq,
          rule_id: r.id,
          at: now(),
          trigger: 'manual',
          depth: 0,
          matched: 3,
          status: 'applied',
          batch_id: null,
          error: null,
        };
        runs.unshift(run);
        changed();
        return { runs: [run] };
      },
      'automation.preview': () => ({
        matched: 2,
        changed: 1,
        truncated: false,
        cards: [
          { card_id: 'card-a', note_id: 'note-a', text: 'to be', deck: 'İngilizce', changes: true },
          { card_id: 'card-b', note_id: 'note-b', text: 'to go', deck: 'İngilizce', changes: false },
        ],
        actions: [],
      }),
      'automation.runs.list': ({ limit }) => ({ runs: runs.slice(0, limit ?? 50) }),
    };
  },
} satisfies MockFeature;

/**
 * Dev-only stand-in for the ai module (the real providers, prompts, preview hashing, jobs and
 * keychain live in fc-app/fc-ai): one canned answer per feature, jobs that finish at once.
 * Keys are kept only as "set / not set" — even the mock never stores them.
 */
import { err, menu, module, type MockEnv, type MockFeature } from '$lib/api/mock/env';
import type {
  AiFeature,
  AiJobResult,
  AiOutput,
  AiPreview,
  AiPriceDto,
  AiProviderDto,
  AiProviderKind,
  AiRouteDto,
  AiSourceText,
  AiTask,
  AiUsageTotals,
  JobDto,
  JsonValue,
  NoteDto,
} from '$lib/api/types';

const FEATURES: AiFeature[] = [
  'generate_cards',
  'fill_fields',
  'explain',
  'hint',
  'check_answer',
  'improve',
  'rewrite_leech',
  'distractors',
];

const KINDS = [
  {
    kind: 'anthropic' as AiProviderKind,
    default_base_url: 'https://api.anthropic.com',
    default_model: 'claude-sonnet-5-5',
    suggested_models: ['claude-opus-5-5', 'claude-sonnet-5-5', 'claude-haiku-5-5'],
    needs_key: true,
    local: false,
  },
  { kind: 'openai' as AiProviderKind, default_base_url: 'https://api.openai.com/v1', default_model: '', suggested_models: [], needs_key: true, local: false },
  { kind: 'openai_compatible' as AiProviderKind, default_base_url: 'http://localhost:1234/v1', default_model: '', suggested_models: [], needs_key: false, local: false },
  { kind: 'ollama' as AiProviderKind, default_base_url: 'http://localhost:11434', default_model: '', suggested_models: [], needs_key: false, local: true },
];

const NAMES: Record<AiProviderKind, string> = {
  anthropic: 'Claude',
  openai: 'OpenAI',
  openai_compatible: 'OpenAI-compatible',
  ollama: 'Ollama',
};

const zero = (): AiUsageTotals => ({ requests: 0, input_tokens: 0, output_tokens: 0, cost_usd: 0 });

function output(task: AiTask): AiOutput {
  switch (task.feature) {
    case 'generate_cards':
      return {
        feature: 'generate_cards',
        deck_id: task.deck_id,
        notetype_id: task.notetype_id,
        notes: Array.from({ length: Math.min(task.count, 2) }, (_, i) => ({
          fields: { front: `term ${i + 1}`, back: `meaning ${i + 1}` },
        })),
        attributes: task.attributes,
      };
    case 'fill_fields':
      return { feature: 'fill_fields', fields: { back: 'a filled value' } };
    case 'explain':
      return { feature: 'explain', text: 'An explanation.' };
    case 'hint':
      return { feature: 'hint', text: 'A hint.' };
    case 'check_answer':
      return { feature: 'check_answer', verdict: 'correct', feedback: 'Same meaning.' };
    case 'improve':
      return { feature: 'improve', note_id: task.note_id, changes: [], reason: 'Already clear.' };
    case 'rewrite_leech':
      return { feature: 'rewrite_leech', note_id: task.note_id, changes: [], mnemonic: 'A mnemonic.' };
    case 'distractors':
      return { feature: 'distractors', note_id: task.note_id, items: ['wrong 1', 'wrong 2'], field: null };
  }
}

export default {
  modules: [module('ai', [menu('ai.main', 'nav.ai', '/ai', 40)])],
  handlers: ({ now, id, emit, notes, requireDeck }: MockEnv) => {
    const providers: AiProviderDto[] = [];
    const routes = new Map<AiFeature, AiRouteDto>();
    let prices: AiPriceDto[] = [
      { model: 'claude-haiku-5-5', input_per_mtok: 1, output_per_mtok: 5 },
      { model: 'claude-opus-5-5', input_per_mtok: 5, output_per_mtok: 25 },
      { model: 'claude-sonnet-5-5', input_per_mtok: 3, output_per_mtok: 15 },
    ];
    const month = zero();
    const changed = () => emit({ type: 'ai_config_changed' });
    const find = (pid: string) => {
      const p = providers.find((x) => x.id === pid);
      if (!p) throw err('NOT_FOUND_AI_PROVIDER', 'errors.not_found.ai_provider', pid);
      return p;
    };
    const resolve = (pid: string | null, model: string | null) => {
      const p = pid ? find(pid) : (providers.find((x) => x.is_default) ?? providers[0]);
      if (!p) throw err('CONFLICT_AI_NO_PROVIDER', 'errors.conflict.ai_no_provider', 'no provider');
      const m = model ?? p.default_model;
      if (!m) throw err('CONFLICT_AI_NO_MODEL', 'errors.conflict.ai_no_model', p.id);
      return { p, m };
    };
    const hash = (task: AiTask, pid: string, model: string) => `${pid}:${model}:${JSON.stringify(task).length}`;
    const preview = (task: AiTask, pid: string | null, model: string | null): AiPreview => {
      const { p, m } = resolve(pid, model);
      const text = task.feature === 'generate_cards' ? task.source : JSON.stringify(task);
      const price = prices.find((x) => x.model === m);
      const tokens = Math.ceil(text.length / 4) + 100;
      return {
        feature: task.feature,
        provider: p,
        model: m,
        system: 'You are a careful assistant inside a flashcard app.',
        messages: [{ role: 'user', text, images: [] }],
        max_tokens: 1024,
        structured: task.feature !== 'explain' && task.feature !== 'hint',
        estimated_input_tokens: tokens,
        estimated_cost_usd: price ? (tokens * price.input_per_mtok + 1024 * price.output_per_mtok) / 1e6 : null,
        items: 1,
        preview_hash: hash(task, p.id, m),
        missing_key: p.needs_key && !p.has_key,
        budget: { month_spent_usd: month.cost_usd, limit_usd: null, warning: false, exceeded: false },
      };
    };
    return {
      'ai.providers.list': () => ({ providers, kinds: KINDS, keychain: true, keychain_session_only: false }),
      'ai.providers.create': (req) => {
        const info = KINDS.find((k) => k.kind === req.kind)!;
        const t = now();
        const p: AiProviderDto = {
          id: id('aiprov'),
          kind: req.kind,
          name: req.name ?? NAMES[req.kind],
          base_url: req.base_url ?? info.default_base_url,
          default_model: req.default_model ?? info.default_model,
          models: info.suggested_models,
          is_default: (req.is_default ?? false) || !providers.some((x) => x.is_default),
          has_key: false,
          needs_key: info.needs_key,
          local: info.local,
          created_at: t,
          updated_at: t,
        };
        if (p.is_default) for (const x of providers) x.is_default = false;
        providers.push(p);
        changed();
        return p;
      },
      'ai.providers.update': (req) => {
        const p = find(req.id);
        if (req.is_default) for (const x of providers) x.is_default = false;
        // As in fc-app: a key never follows the provider to a new address unconfirmed.
        if (req.base_url.replace(/\/+$/, '') !== p.base_url && req.keep_key !== true) p.has_key = false;
        Object.assign(p, {
          name: req.name,
          base_url: req.base_url,
          default_model: req.default_model,
          models: req.models,
          is_default: p.is_default || req.is_default,
          updated_at: now(),
        });
        changed();
        return p;
      },
      'ai.providers.delete': (req) => {
        const p = find(req.id);
        providers.splice(providers.indexOf(p), 1);
        if (p.is_default && providers[0]) providers[0].is_default = true;
        changed();
        return p;
      },
      'ai.providers.set_key': (req) => {
        const p = find(req.provider_id);
        if (req.key !== null && (!req.key.trim() || /\s/.test(req.key.trim()))) {
          throw err('VALIDATION_AI_KEY', 'errors.validation.ai_key', 'bad key');
        }
        p.has_key = req.key !== null;
        changed();
        return p;
      },
      'ai.providers.refresh_models': (req) => {
        const p = find(req.id);
        const t = now();
        return {
          id: id('job'),
          kind: 'ai.models',
          status: 'succeeded',
          progress: 1,
          message: null,
          error: null,
          result: { models: p.models } as JsonValue,
          created_at: t,
          updated_at: t,
        } satisfies JobDto;
      },
      'ai.routes.list': () => ({
        routes: FEATURES.map((f) => routes.get(f) ?? { feature: f, provider_id: null, model: null }),
      }),
      'ai.routes.set': (req) => {
        routes.set(req.feature, req);
        changed();
        return req;
      },
      'ai.prices.list': () => ({ prices }),
      'ai.prices.set': (req) => {
        if (!req.model.trim() || req.input_per_mtok < 0 || req.output_per_mtok < 0) {
          throw err('VALIDATION_AI_PRICE', 'errors.validation.ai_price', req.model);
        }
        prices = [...prices.filter((p) => p.model !== req.model), req].sort((a, b) => a.model.localeCompare(b.model));
        changed();
        return req;
      },
      'ai.prices.delete': (req) => {
        prices = prices.filter((p) => p.model !== req.model);
        changed();
        return {};
      },
      'ai.usage.summary': () => ({
        month_start: 0,
        month,
        week: month,
        by_feature: [],
        by_model: [],
        budget: { month_spent_usd: month.cost_usd, limit_usd: null, warning: false, exceeded: false },
      }),
      'ai.preview': (req) => preview(req.task, req.provider_id ?? null, req.model ?? null),
      'ai.run': (req) => {
        const p = preview(req.task, req.provider_id ?? null, req.model ?? null);
        if (p.preview_hash !== req.preview_hash) {
          throw err('CONFLICT_AI_PREVIEW_STALE', 'errors.conflict.ai_preview_stale', 'changed');
        }
        if (p.missing_key) throw err('PROVIDER_AI_UNAUTHORIZED', 'errors.provider.ai_unauthorized', 'no key');
        month.requests += 1;
        month.input_tokens += p.estimated_input_tokens;
        const result: AiJobResult = {
          output: output(req.task),
          provider_id: p.provider.id,
          model: p.model,
          input_tokens: p.estimated_input_tokens,
          output_tokens: 50,
          cost_usd: p.estimated_cost_usd,
          attempts: 1,
          truncated: false,
        };
        const t = now();
        const jobId = id('job');
        // Streamed features: the pieces arrive (as events) before the result.
        const out = result.output;
        if (out.feature === 'explain' || out.feature === 'hint') {
          const words = out.text.split(' ');
          for (let i = 1; i <= words.length; i++) {
            emit({ type: 'ai_partial', data: { job_id: jobId, text: words.slice(0, i).join(' ') } });
          }
        }
        return {
          id: jobId,
          kind: 'ai.request',
          status: 'succeeded',
          progress: 1,
          message: null,
          error: null,
          result: result as unknown as JsonValue,
          created_at: t,
          updated_at: t,
        };
      },
      'ai.sources.extract': (req) => {
        const t = now();
        const src = req.source;
        const text: AiSourceText =
          src.kind === 'pdf'
            ? {
                title: src.path,
                text: `Text of pages ${src.from_page ?? 1}–${src.to_page ?? 3} of ${src.path}.`,
                page_count: 3,
                from_page: src.from_page ?? 1,
                to_page: src.to_page ?? 3,
                truncated: false,
              }
            : { title: src.url, text: `Readable text of ${src.url}.`, page_count: null, from_page: null, to_page: null, truncated: false };
        return {
          id: id('job'),
          kind: 'ai.source',
          status: 'succeeded',
          progress: 1,
          message: null,
          error: null,
          result: text as unknown as JsonValue,
          created_at: t,
          updated_at: t,
        };
      },
      'ai.notes.add': (req) => {
        requireDeck(req.deck_id);
        const ids: string[] = [];
        let skipped = 0;
        for (const d of req.notes) {
          const front = d.fields.front ?? '';
          if (!front.trim() || notes.some((n) => n.fields.front === front)) {
            skipped += 1;
            continue;
          }
          const t = now();
          const n: NoteDto = {
            id: id('note'),
            notetype_id: req.notetype_id,
            home_deck_id: req.deck_id,
            fields: d.fields,
            tags: req.tags,
            attributes: req.attributes,
            cards: [{ id: id('card'), deck_id: req.deck_id, template_key: 'forward', state: 'new', due: null, suspended: false }],
            sort_field: front,
            created_at: t,
            updated_at: t,
          };
          notes.push(n);
          ids.push(n.id);
          emit({ type: 'note_created', data: { note_id: n.id, deck_id: req.deck_id } });
        }
        return { note_ids: ids, skipped };
      },
      'ai.notes.update': (req) => {
        const n = notes.find((x) => x.id === req.note_id);
        if (!n) throw err('NOT_FOUND_NOTE', 'errors.not_found.note', req.note_id);
        n.fields = { ...n.fields, ...req.fields };
        n.updated_at = now();
        emit({ type: 'note_updated', data: { note_id: n.id } });
        return n;
      },
    };
  },
} satisfies MockFeature;

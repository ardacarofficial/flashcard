import { formatCurrency } from '$lib/format';
import { api } from '$lib/api';
import { td } from '$lib/i18n/index.svelte';
import type {
  AiConnectionTest,
  AiJobResult,
  AiModelList,
  AiPreview,
  AiProviderId,
  AiSourceInput,
  AiSourceText,
  AiTask,
  JobDto,
} from '$lib/api/types';
import { followJob, isFinal, resultOf } from '../io/job';

/** Which provider/model a request uses (`null` = the feature's route, else the default). */
export interface Choice {
  provider_id: AiProviderId | null;
  model: string | null;
}

export const DEFAULT_CHOICE: Choice = { provider_id: null, model: null };

/** Builds the exact request `task` would send. Nothing leaves the app. */
export function previewTask(task: AiTask, choice: Choice = DEFAULT_CHOICE): Promise<AiPreview> {
  return api.call('ai.preview', { task, ...choice });
}

/**
 * Sends what `preview` showed (the backend refuses if anything changed since) and waits for
 * the job. `onUpdate` sees progress snapshots; `onPartial` sees the answer so far while it
 * streams in (explanations and hints).
 */
export async function sendPreviewed(
  task: AiTask,
  preview: AiPreview,
  choice: Choice = DEFAULT_CHOICE,
  onUpdate: (job: JobDto) => void = () => {},
  onPartial: (text: string) => void = () => {},
): Promise<JobDto> {
  // Listen before the job exists: its first pieces may arrive before `ai.run` returns.
  let jobId: string | null = null;
  const early = new Map<string, string>();
  const off = await api.on('ai_partial', (env) => {
    if (env.event.type !== 'ai_partial') return;
    const { job_id, text } = env.event.data;
    if (jobId === null) early.set(job_id, text);
    else if (job_id === jobId) onPartial(text);
  });
  try {
    const job = await api.call('ai.run', { task, ...choice, preview_hash: preview.preview_hash });
    jobId = job.id;
    const seen = early.get(job.id);
    if (seen !== undefined) onPartial(seen);
    onUpdate(job);
    return isFinal(job) ? job : await followJob(job, onUpdate);
  } finally {
    off();
  }
}

/** Extracts card-source text from a PDF page range or a web page (nothing goes to an AI). */
export async function extractSource(
  source: AiSourceInput,
  onUpdate: (job: JobDto) => void = () => {},
): Promise<{ job: JobDto; text: AiSourceText | null }> {
  const started = await api.call('ai.sources.extract', { source });
  onUpdate(started);
  const job = isFinal(started) ? started : await followJob(started, onUpdate);
  return { job, text: resultOf<AiSourceText>(job) };
}

/** The typed result of a finished `ai.request` job. */
export function aiResult(job: JobDto): AiJobResult | null {
  return job.status === 'succeeded' && job.result !== null ? (job.result as unknown as AiJobResult) : null;
}

/** `$0.0123` style; `null` = unknown. */
export function usd(v: number | null | undefined): string | null {
  if (v === null || v === undefined) return null;
  return formatCurrency(v, 'USD');
}

/** Fetches a provider's model list (an `ai.models` job); the list is also cached on the provider. */
export async function fetchModels(id: AiProviderId): Promise<{ job: JobDto; list: AiModelList | null }> {
  const started = await api.call('ai.providers.refresh_models', { id });
  const job = isFinal(started) ? started : await followJob(started, () => {});
  return { job, list: resultOf<AiModelList>(job) };
}

/** Tests a provider's connection (an `ai.test` job: one tiny request, explained). */
export async function testConnection(id: AiProviderId): Promise<{ job: JobDto; test: AiConnectionTest | null }> {
  const started = await api.call('ai.providers.test', { id });
  const job = isFinal(started) ? started : await followJob(started, () => {});
  return { job, test: resultOf<AiConnectionTest>(job) };
}

/** Models matching `query` (case-insensitive, every word), the exact match first. */
export function filterModels(models: readonly string[], query: string): string[] {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  const hits = models.filter((m) => words.every((w) => m.toLowerCase().includes(w)));
  const exact = query.trim();
  return exact && hits.includes(exact) ? [exact, ...hits.filter((m) => m !== exact)] : hits;
}

/** A failed job's error in words: its message plus the provider's own reason, if any. */
export function jobErrorText(job: JobDto, fallback: string): string {
  const e = job.error;
  if (!e) return fallback;
  const reason = typeof e.details?.reason === 'string' ? e.details.reason : null;
  const text = td(e.message_key);
  return reason && reason !== text ? `${text} (${reason})` : text;
}

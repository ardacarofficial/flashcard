import { api } from '$lib/api';
import type { AiJobResult, AiPreview, AiProviderId, AiTask, JobDto } from '$lib/api/types';
import { followJob, isFinal } from '../io/job';

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
 * the job. `onUpdate` sees progress snapshots.
 */
export async function sendPreviewed(
  task: AiTask,
  preview: AiPreview,
  choice: Choice = DEFAULT_CHOICE,
  onUpdate: (job: JobDto) => void = () => {},
): Promise<JobDto> {
  const job = await api.call('ai.run', { task, ...choice, preview_hash: preview.preview_hash });
  onUpdate(job);
  return isFinal(job) ? job : followJob(job, onUpdate);
}

/** The typed result of a finished `ai.request` job. */
export function aiResult(job: JobDto): AiJobResult | null {
  return job.status === 'succeeded' && job.result !== null ? (job.result as unknown as AiJobResult) : null;
}

/** `$0.0123` style; `null` = unknown. */
export function usd(v: number | null | undefined): string | null {
  if (v === null || v === undefined) return null;
  if (v === 0) return '$0';
  return v < 0.01 ? `$${v.toFixed(4)}` : `$${v.toFixed(2)}`;
}

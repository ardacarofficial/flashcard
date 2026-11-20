import { api } from '$lib/api';
import type { JobDto, JobStatus } from '$lib/api/types';

const FINAL: readonly JobStatus[] = ['succeeded', 'failed', 'cancelled', 'interrupted'];

export function isFinal(job: JobDto): boolean {
  return FINAL.includes(job.status);
}

/**
 * Follows a background job: calls `onUpdate` with every `job_progress` snapshot and resolves
 * with the finished job. (Reads the job once after subscribing, in case it already ended.)
 */
export function followJob(job: JobDto, onUpdate: (job: JobDto) => void): Promise<JobDto> {
  return new Promise((resolve, reject) => {
    let done = false;
    let unsub: (() => void) | null = null;
    const update = (j: JobDto) => {
      if (done) return;
      onUpdate(j);
      if (isFinal(j)) {
        done = true;
        unsub?.();
        resolve(j);
      }
    };
    api
      .on('job_progress', (env) => {
        if (env.event.type === 'job_progress' && env.event.data.job.id === job.id) {
          update(env.event.data.job);
        }
      })
      .then((u) => {
        unsub = u;
        if (done) u();
        return api.call('jobs.get', { id: job.id });
      })
      .then(update)
      .catch(reject);
  });
}

/** The typed result of a finished job. */
export function resultOf<T>(job: JobDto): T | null {
  return job.status === 'succeeded' && job.result !== null ? (job.result as unknown as T) : null;
}

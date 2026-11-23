/**
 * Dev-only mock of the v1.0 `maintenance` and `updates` modules: a fixed status, one crash
 * report, an update to "download". Crash detection, redaction, the integrity check and the
 * signature check live in fc-app and the desktop shell.
 */
import { err, menu, mockScenario, module, type MockEnv, type MockFeature } from '$lib/api/mock/env';
import type { BackupStatus, DraftDto, IntegrityDto, UpdateStatus } from '$lib/api/types';

export default {
  modules: [
    { ...module('maintenance', [menu('maintenance.screen', 'nav.app_maintenance', '/app-maintenance', 95)]), can_disable: false },
    module('updates', []),
  ],
  handlers: ({ now, emit }: MockEnv) => {
    let crashes = [
      {
        file: 'crash-20261120-101112-123.txt',
        version: '1.0.0',
        time: '2026-11-20T10:11:12.123Z',
        thread: 'main',
        location: 'crates/fc-app/src/x.rs:1:1',
        message: 'example panic',
      },
    ];
    let integrity: IntegrityDto | null = null;
    const drafts = new Map<string, DraftDto>();
    const update: UpdateStatus = {
      current_version: '1.0.0',
      supported: true,
      check_on_start: false,
      check_due: false,
      last_checked_at: null,
      available: mockScenario() === 'update' ? { version: '1.1.1', notes: 'Fixes and small improvements.', date: null } : null,
      phase: 'idle',
      install_job: null,
      last_error: null,
    };
    const status = () => ({
      version: '1.0.0',
      data_dir: '/mock/data',
      log_dir: '/mock/data/logs',
      logs: [
        { name: 'flashcard.log', bytes: 12_345 },
        { name: 'flashcard.log.1', bytes: 1_048_576 },
      ],
      crash_reports: structuredClone(crashes),
      previous_run: { started_at: 1, version: '1.0.0', clean: true },
      integrity,
    });
    const check = (quick: boolean): IntegrityDto => {
      integrity = { ok: true, quick, problems: [], checked_at: now(), duration_ms: 12 };
      return integrity;
    };
    const noOffer = () => ({ reason: null, previous_run: status().previous_run, crash: null, integrity: null, session: null, drafts: [] });
    return {
      // v1.4: Home's "son yedek" line (E4). The io feature's mock may override it (same shape).
      'io.backup.status': (): BackupStatus => ({
        schedule: 'off',
        keep: 7,
        folder: '/home/demo/.flashcard/backups/auto',
        last_verified: { path: '/home/demo/yedek.fcbackup', created_at: now() - 12 * 86_400_000, verified_at: now() - 12 * 86_400_000, bytes: 2_400_000, complete: true, automatic: false },
        automatic: [],
        next_due: null,
        last_failure: null,
        stale: 'old',
      }),
      'maintenance.status': () => status(),
      'maintenance.recovery.get': () => noOffer(),
      'maintenance.recovery.dismiss': () => noOffer(),
      'maintenance.integrity.check': ({ quick }) => check(quick ?? false),
      'maintenance.logs.export': ({ path }) => {
        if (!path.toLowerCase().endsWith('.zip'))
          throw err('VALIDATION_INVALID_PARAMS', 'errors.validation.invalid_params', 'path must end in .zip');
        return { path, files: 3 + crashes.length, bytes: 4096, redactions: 2 };
      },
      'maintenance.crash_reports.clear': () => {
        const removed = crashes.length;
        crashes = [];
        return { removed };
      },
      'maintenance.drafts.save': ({ key, payload }) => {
        const t = now();
        const d: DraftDto = { key, payload, created_at: drafts.get(key)?.created_at ?? t, updated_at: t };
        drafts.set(key, d);
        return d;
      },
      'maintenance.drafts.get': ({ key }) => ({ draft: drafts.get(key) ?? null }),
      'maintenance.drafts.list': () => ({ items: [...drafts.values()].reverse() }),
      'maintenance.drafts.discard': ({ key }) => {
        const draft = drafts.get(key) ?? null;
        drafts.delete(key);
        return { draft };
      },
      'updates.status': () => structuredClone(update),
      'updates.check': () => {
        update.last_checked_at = now();
        update.available = { version: '1.0.1', notes: 'Fixes', date: null };
        return structuredClone(update);
      },
      'updates.install': () => {
        if (!update.available)
          throw err('CONFLICT_UPDATE_NOT_AVAILABLE', 'errors.conflict.update_not_available', 'no update');
        const t = now();
        const job = {
          id: 'job-update',
          kind: 'updates.install',
          status: 'succeeded' as const,
          progress: 1,
          message: null,
          error: null,
          result: null,
          created_at: t,
          updated_at: t,
        };
        update.phase = 'ready';
        emit({ type: 'job_progress', data: { job } });
        return job;
      },
      'updates.apply': () => {
        if (update.phase !== 'ready')
          throw err('CONFLICT_UPDATE_NOT_READY', 'errors.conflict.update_not_ready', 'not downloaded');
        return structuredClone(update);
      },
    };
  },
} satisfies MockFeature;

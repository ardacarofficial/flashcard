import { afterEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { i18n } from '$lib/i18n/index.svelte';
import { api } from '$lib/api';
import type { RecoveryDto } from '$lib/api/types';
import type { ScreenCtx } from '../types';
import MaintenanceScreen from './MaintenanceScreen.svelte';
import RecoveryHost from './RecoveryHost.svelte';
import { bundleName, formatSize } from './format';

i18n.locale = 'en';

const ctx: ScreenCtx = {
  decks: [],
  selectedDeck: null,
  selectedPath: null,
  selectDeck: () => {},
  guarded: async (fn) => {
    try {
      await fn();
      return true;
    } catch {
      return false;
    }
  },
};

afterEach(() => vi.restoreAllMocks());

describe('maintenance screen (dev mock)', () => {
  it('shows logs and crash reports, exports logs, checks the database and clears reports', async () => {
    render(MaintenanceScreen, { ctx });
    expect(await screen.findByText('Version 1.0.0')).toBeTruthy();
    expect(screen.getByText('2 files, 1.0 MB')).toBeTruthy();
    expect(screen.getByText(/example panic/)).toBeTruthy();

    vi.spyOn(window, 'prompt').mockReturnValue('/tmp/bug.zip');
    await fireEvent.click(screen.getByRole('button', { name: 'Export logs…' }));
    expect(await screen.findByText('Saved 4 files to /tmp/bug.zip. 2 secrets removed.')).toBeTruthy();

    await fireEvent.click(screen.getByRole('button', { name: 'Check the database' }));
    expect(await screen.findByText('No problems found (12 ms).')).toBeTruthy();

    await fireEvent.click(screen.getByRole('button', { name: 'Delete crash reports' }));
    expect(await screen.findByText('No crash reports.')).toBeTruthy();
  });

  it('checks, downloads and installs an update only on clicks', async () => {
    const calls: string[] = [];
    const real = api.call.bind(api);
    vi.spyOn(api, 'call').mockImplementation(((m: string, p: unknown) => {
      calls.push(m);
      return real(m as never, p as never);
    }) as typeof api.call);
    render(MaintenanceScreen, { ctx });
    await fireEvent.click(await screen.findByRole('button', { name: 'Check for updates' }));
    expect(await screen.findByText('Version 1.0.1 is available.')).toBeTruthy();
    expect(calls).not.toContain('updates.install');
    await fireEvent.click(screen.getByRole('button', { name: 'Download' }));
    await fireEvent.click(await screen.findByRole('button', { name: 'Install and restart' }));
    await waitFor(() => expect(calls).toContain('updates.apply'));
  });
});

describe('<RecoveryHost>', () => {
  it('draws nothing after a clean start', async () => {
    render(RecoveryHost, { open: () => {} });
    await new Promise((r) => setTimeout(r, 10));
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('offers to resume the session after a crash and dismisses the offer', async () => {
    const offer: RecoveryDto = {
      reason: 'crash',
      previous_run: { started_at: 1, version: '1.0.0', clean: false },
      crash: { file: 'c.txt', version: '1.0.0', time: 't', thread: 'main', location: null, message: 'boom' },
      integrity: { ok: true, quick: true, problems: [], checked_at: 2, duration_ms: 3 },
      session: { id: 's1', kind: 'normal', started_at: 1 },
      drafts: [{ key: 'notes.add', payload: {}, created_at: 1, updated_at: 1 }],
    };
    const calls: string[] = [];
    const real = api.call.bind(api);
    vi.spyOn(api, 'call').mockImplementation(((m: string, p: unknown) => {
      calls.push(m);
      if (m === 'maintenance.recovery.get') return Promise.resolve(structuredClone(offer));
      return real(m as never, p as never);
    }) as typeof api.call);
    const opened: string[] = [];
    render(RecoveryHost, { open: (r: string) => opened.push(r) });
    expect(await screen.findByText('Flashcard closed unexpectedly')).toBeTruthy();
    expect(screen.getByText('Database checked: no problems.')).toBeTruthy();
    expect(screen.getByText('1 unsaved form, restored when you open them again')).toBeTruthy();
    expect(screen.getByText('Crash report: boom')).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Resume studying' }));
    await waitFor(() => expect(opened).toEqual(['/study']));
    expect(calls).toContain('maintenance.recovery.dismiss');
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
  });
});

describe('format', () => {
  it('sizes and bundle names', () => {
    expect(formatSize(500)).toBe('1 KB');
    expect(formatSize(3 * 1024 * 1024)).toBe('3.0 MB');
    expect(bundleName('1.0.0', new Date('2026-11-20T10:00:00Z'))).toBe('flashcard-logs-1.0.0-2026-11-20.zip');
  });
});

describe('form drafts', () => {
  it('keep what was typed, restore it and drop it once saved', async () => {
    const { draftKeeper } = await import('./drafts');
    const d = draftKeeper<{ fields: Record<string, string> }>('notes.add', 0);
    expect(await d.restore()).toBeNull();
    d.save({ fields: { front: 'yarım' } });
    await waitFor(async () => expect(await d.restore()).toEqual({ fields: { front: 'yarım' } }));
    await d.discard();
    expect(await d.restore()).toBeNull();
  });
});

import { err, menu, module, type MockEnv, type MockFeature } from '$lib/api/mock/env';
import type { JobDto, JsonValue, NoteDto, TextOptionsDto } from '$lib/api/types';

/**
 * Dev-only stand-in for the io module (the real parsing, mapping and jobs live in fc-app):
 * comma/colon/tab rows, jobs that finish immediately.
 */
export default {
  modules: [module('io', [menu('io.main', 'nav.io', '/io', 60)])],
  handlers: ({ now, id, emit, notes, requireDeck }: MockEnv) => {
    const jobs = new Map<string, JobDto>();
    const finished = (kind: string, result: unknown): JobDto => {
      const t = now();
      const job: JobDto = {
        id: id('job'),
        kind,
        status: 'succeeded',
        progress: 1,
        message: null,
        error: null,
        result: result as JsonValue,
        created_at: t,
        updated_at: t,
      };
      jobs.set(job.id, job);
      return job;
    };
    const options: TextOptionsDto = {
      field_separator: ',',
      record_separator: '\n',
      quotes: true,
      max_fields: null,
    };
    const rows = (text: string) =>
      text
        .split('\n')
        .filter((l) => l.trim())
        .map((l) => l.split(/\t|,|:/).map((s) => s.trim()));
    const textOf = (s: { kind: 'text'; text: string } | { kind: 'file'; path: string }) =>
      s.kind === 'text' ? s.text : '';
    return {
      'io.import.preview': (req) => {
        const all = rows(textOf(req.source));
        return {
          format: 'text',
          text_options: options,
          confidence: 0.9,
          columns: 2,
          rows: all.slice(0, req.limit ?? 20),
          total_rows: all.length,
          suggested_columns: [
            { kind: 'field', key: 'front' },
            { kind: 'field', key: 'back' },
          ],
          package: null,
        };
      },
      'io.import.start': (req) => {
        const deck = requireDeck(req.deck_id ?? '');
        const added = rows(textOf(req.source));
        for (const [front = '', back = ''] of added) {
          const t = now();
          const n: NoteDto = {
            id: id('note'),
            notetype_id: req.notetype_id ?? 'classic',
            home_deck_id: deck.id,
            fields: { front, back },
            tags: [],
            attributes: [],
            cards: [],
            sort_field: front,
            created_at: t,
            updated_at: t,
          };
          notes.push(n);
          emit({ type: 'note_created', data: { note_id: n.id, deck_id: deck.id } });
        }
        return finished('io.import', {
          added_notes: added.length,
          updated_notes: 0,
          skipped_duplicates: 0,
          added_cards: added.length,
          added_decks: 0,
          added_notetypes: 0,
          added_attributes: 0,
          added_options: 0,
          added_media: 0,
          added_reviews: 0,
          issues: [],
        });
      },
      'io.export.start': (req) =>
        finished('io.export', { path: req.path, bytes: 0, notes: notes.length, cards: 0, media: 0 }),
      'io.backup.create': (req) =>
        finished('io.backup', { path: req.path, bytes: 0, notes: notes.length, cards: 0, media: 0 }),
      'io.backup.inspect': () => ({
        created_at: now(),
        counts: { notes: notes.length },
        media: 0,
        compatible: true,
      }),
      'io.backup.restore': (req) =>
        finished('io.restore', { safety_backup_path: `${req.path}.pre`, counts: {}, media: 0 }),
      'io.print.render': (req) => ({ html: `<p>${req.deck_id}</p>`, cards: notes.length }),
      'jobs.get': ({ id: jobId }) => {
        const job = jobs.get(jobId);
        if (!job) throw err('NOT_FOUND_JOB', 'errors.not_found.job', jobId);
        return job;
      },
      'jobs.cancel': ({ id: jobId }) => {
        const job = jobs.get(jobId);
        if (!job) throw err('NOT_FOUND_JOB', 'errors.not_found.job', jobId);
        return job;
      },
    };
  },
} satisfies MockFeature;

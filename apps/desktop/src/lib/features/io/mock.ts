import { err, menu, module, type MockEnv, type MockFeature } from '$lib/api/mock/env';
import type { ColumnTarget, JobDto, JsonValue, NoteDto, TextOptionsDto } from '$lib/api/types';

/**
 * Dev-only stand-in for the io module (the real parsing, mapping and jobs live in fc-app):
 * comma/colon/tab rows, header-aware column suggestion, an import that honours the mapping, the
 * header row and the duplicate rule (undoable), jobs that finish immediately.
 */
export default {
  modules: [module('io', [menu('io.main', 'nav.io', '/io', 60)])],
  handlers: ({ now, id, emit, notes, requireDeck, undoLog }: MockEnv) => {
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
    /** A rough stand-in for fc-interop's `detect`: the first separator every line holds. */
    const detect = (text: string): TextOptionsDto => {
      const lines = text.split('\n').filter((l) => l.trim());
      const field = ['\t', ' - ', '::', ':', ',', ';', '  '].find((sep) => lines.length > 0 && lines.every((l) => l.includes(sep))) ?? ',';
      return { field_separator: field, record_separator: '\n', quotes: true, max_fields: null };
    };
    const rows = (text: string, opts: TextOptionsDto = detect(text)) =>
      text
        .split(opts.record_separator)
        .filter((l) => l.trim())
        .map((l) => (opts.field_separator === '  ' ? l.trim().split(/\s{2,}/) : l.split(opts.field_separator)).map((s) => s.trim()));
    const textOf = (s: { kind: 'text'; text: string } | { kind: 'file'; path: string }) =>
      s.kind === 'text' ? s.text : '';
    /** Field keys of the mock's note types (the real ones come from the note type). */
    const fieldsOf = (nt: string | null | undefined) =>
      nt === '01900000-0000-7000-8000-00000000000b' ? ['word', 'meaning', 'pronunciation', 'example'] : ['front', 'back'];
    /** Like fc-app's `suggest`: a header naming fields maps them, else fields in order. */
    const suggest = (keys: string[], header: string[] | undefined, columns: number): ColumnTarget[] => {
      const named = (header ?? []).map((c) => keys.find((k) => k === c.trim().toLowerCase()));
      const byOrder = Array.from({ length: columns }, (_, i): ColumnTarget => (keys[i] ? { kind: 'field', key: keys[i]! } : { kind: 'ignore' }));
      if (!named.some(Boolean)) return byOrder;
      return Array.from({ length: columns }, (_, i): ColumnTarget => (named[i] ? { kind: 'field', key: named[i]! } : { kind: 'ignore' }));
    };
    return {
      'io.import.preview': (req) => {
        const text = textOf(req.source);
        const options = req.text_options ?? detect(text);
        const all = rows(text, options);
        const columns = Math.max(0, ...all.map((r) => r.length));
        return {
          format: 'text',
          text_options: options,
          confidence: req.text_options ? null : 0.9,
          columns,
          rows: all.slice(0, req.limit ?? 20),
          total_rows: all.length,
          suggested_columns: suggest(fieldsOf(req.notetype_id), all[0], columns),
          package: null,
        };
      },
      'io.import.start': (req) => {
        const deck = requireDeck(req.deck_id ?? '');
        const keys = fieldsOf(req.notetype_id);
        const all = rows(textOf(req.source), req.text_options ?? undefined);
        const body = req.skip_first_row ? all.slice(1) : all;
        const columns = req.columns?.length ? req.columns : suggest(keys, undefined, Math.max(0, ...all.map((r) => r.length)));
        const notetype = req.notetype_id ?? 'classic';
        const added: string[] = [];
        let skipped = 0;
        let updated = 0;
        const issues: { row: number; message: { key: string; params: Record<string, string> } }[] = [];
        body.forEach((cells, i) => {
          const fields: Record<string, string> = {};
          const tags: string[] = [];
          columns.forEach((c, j) => {
            if (c.kind === 'field' && cells[j]) fields[c.key] = cells[j]!;
            if (c.kind === 'tags' && cells[j]) tags.push(...cells[j]!.split(/[\s,]+/).filter(Boolean));
          });
          const first = fields[keys[0]!]?.trim() ?? '';
          if (!first) {
            issues.push({ row: i + 1 + (req.skip_first_row ? 1 : 0), message: { key: 'io.issue.invalid_fields', params: {} } });
            return;
          }
          const same = notes.find((n) => n.notetype_id === notetype && (n.fields[keys[0]!] ?? '').trim().toLowerCase() === first.toLowerCase());
          if (same && req.duplicates !== 'add') {
            if (req.duplicates === 'update') {
              same.fields = { ...same.fields, ...fields };
              updated += 1;
            } else skipped += 1;
            return;
          }
          const t = now();
          const n: NoteDto = {
            id: id('note'),
            notetype_id: notetype,
            home_deck_id: deck.id,
            fields,
            tags,
            attributes: [],
            cards: [{ id: id('card'), deck_id: deck.id, template_key: 'forward', state: 'new', due: null, suspended: false }],
            sort_field: first,
            created_at: t,
            updated_at: t,
          };
          notes.push(n);
          added.push(n.id);
          emit({ type: 'note_created', data: { note_id: n.id, deck_id: deck.id } });
        });
        undoLog.push({
          summary: 'add.summary.undone',
          revert: () => {
            for (const noteId of added) {
              const i = notes.findIndex((n) => n.id === noteId);
              if (i >= 0) notes.splice(i, 1);
            }
            emit({ type: 'notes_deleted', data: { ids: added } });
          },
        });
        return finished('io.import', {
          added_notes: added.length,
          updated_notes: updated,
          skipped_duplicates: skipped,
          added_cards: added.length,
          added_decks: 0,
          added_notetypes: 0,
          added_attributes: 0,
          added_options: 0,
          added_media: 0,
          added_reviews: 0,
          issues,
        });
      },
      'io.export.start': (req) =>
        finished('io.export', { path: req.path, bytes: 0, notes: notes.length, cards: 0, media: 0 }),
      'io.backup.create': (req) =>
        finished('io.backup', { path: req.path, bytes: 0, notes: notes.length, cards: 0, media: 0, mirror: null }),
      'io.backup.inspect': () => ({
        created_at: now(),
        counts: { notes: notes.length },
        media: 0,
        compatible: true,
        complete: true,
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

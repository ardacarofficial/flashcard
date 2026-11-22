// Shared pieces of the Subtitle contract checks (docs/06 §5.1): the `fc` binary, a real
// `fc api serve` that can be killed and restarted, the extension's own modules, and a small
// HTTP proxy between the extension's client and the server that can break one exchange on
// purpose (the network drops, the response is lost, the server process dies mid-request).
import { execFileSync, spawn } from 'node:child_process';
import { existsSync } from 'node:fs';
import { createServer as createHttpServer, request as httpRequest } from 'node:http';
import { register } from 'node:module';
import { createServer } from 'node:net';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const localFc = resolve(dirname(fileURLToPath(import.meta.url)), '../../target/debug', process.platform === 'win32' ? 'fc.exe' : 'fc');
export const FC = process.env.FC_BIN ?? (existsSync(localFc) ? localFc : 'fc');
export const REPO = process.env.SUBTITLE_REPO ?? '/home/user/subtitle';

/** The extension's `api.ts` and `outbox.ts`, unchanged (siblings resolved by `resolve-ts.mjs`). */
export async function loadExtension() {
  register('./resolve-ts.mjs', import.meta.url);
  const dir = join(REPO, 'src/flashcards');
  const api = await import(pathToFileURL(join(dir, 'api.ts')).href);
  const outbox = await import(pathToFileURL(join(dir, 'outbox.ts')).href);
  return { ...api, ...outbox };
}

/** `fc --db <db> --json <args…>` → parsed JSON (null for no output). */
export const fcCli = (db) => (...args) => {
  const out = execFileSync(FC, ['--db', db, '--json', ...args], { encoding: 'utf8' });
  return out.trim() === '' ? null : JSON.parse(out);
};
/** `fc call <method> <params>` on `db`. */
export const fcCall = (db) => {
  const fc = fcCli(db);
  return (method, params = {}) => fc('call', method, JSON.stringify(params));
};

export const freePort = () =>
  new Promise((done) => {
    const s = createServer();
    s.listen(0, '127.0.0.1', () => {
      const { port } = s.address();
      s.close(() => done(port));
    });
  });

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

/** A real `fc --db <db> api serve` process; `kill()` is SIGKILL (no clean shutdown). */
export class FcServer {
  constructor(db) {
    this.db = db;
    this.proc = null;
    this.port = 0;
  }

  async start() {
    this.port = await freePort();
    this.proc = spawn(FC, ['--db', this.db, 'api', 'serve', '--port', String(this.port)], { stdio: ['ignore', 'ignore', 'inherit'] });
    const exited = new Promise((r) => this.proc.once('exit', r));
    for (let i = 0; ; i++) {
      try {
        // Any answer (even 401) means it listens.
        await fetch(`http://127.0.0.1:${this.port}/subtitle/v1/me`);
        return this;
      } catch (e) {
        if (i > 150) throw e;
        if (await Promise.race([exited.then(() => true), sleep(100).then(() => false)])) throw new Error('fc api serve exited');
      }
    }
  }

  kill() {
    if (!this.proc) return Promise.resolve();
    const p = this.proc;
    this.proc = null;
    if (p.exitCode !== null || p.signalCode !== null) return Promise.resolve();
    return new Promise((r) => {
      p.once('exit', r);
      p.kill('SIGKILL');
    });
  }
}

const parse = (buf) => {
  try {
    return JSON.parse(buf.toString('utf8'));
  } catch {
    return null;
  }
};

/**
 * An HTTP proxy in front of the server: the extension talks to `url`, the proxy forwards each
 * request to `upstream` (a port; changeable, so a restarted server keeps the address the
 * extension knows). A rule armed with `once(match, action)` breaks the next matching exchange:
 *
 * * `drop`: the request reaches the server, then both connections are cut before it answers
 *   (the network went away while the server worked);
 * * `lose`: the server answers in full (it has committed), the client's socket is reset
 *   instead of getting the answer;
 * * `{ kill, delayMs }`: the request reaches the server, `kill()` is called after `delayMs`
 *   (the server process dies mid-request), the client's socket is reset; an answer that came
 *   before the kill is never delivered.
 *
 * `exchanges` lists every request with the server's answer as the proxy saw it (also the
 * answers the client never got), so a check can compare what happened with what was told.
 */
export class Proxy {
  constructor() {
    this.upstream = 0;
    this.rules = [];
    this.exchanges = [];
    this.server = createHttpServer((req, res) => this.handle(req, res));
  }

  async listen() {
    const port = await freePort();
    await new Promise((r) => this.server.listen(port, '127.0.0.1', r));
    this.port = port;
    return this;
  }

  url(prefix = '/subtitle') {
    return `http://127.0.0.1:${this.port}${prefix}`;
  }

  /** Arms a rule; resolves with the broken exchange (rejects when nothing matched in time). */
  once(match, action, timeoutMs = 60_000) {
    return new Promise((fired, fail) => {
      const timer = setTimeout(() => fail(new Error(`proxy rule ${JSON.stringify(action?.kill ? 'kill' : action)} never fired`)), timeoutMs);
      this.rules.push({
        match,
        action,
        fired: (exchange) => {
          clearTimeout(timer);
          fired(exchange);
        },
      });
    });
  }

  close() {
    this.server.closeAllConnections?.();
    return new Promise((r) => this.server.close(r));
  }

  handle(req, res) {
    const chunks = [];
    req.on('data', (c) => chunks.push(c));
    req.on('end', () => {
      const body = Buffer.concat(chunks);
      const at = this.rules.findIndex((r) => r.match(req.method, req.url));
      const rule = at >= 0 ? this.rules.splice(at, 1)[0] : null;
      const action = rule?.action ?? null;
      const exchange = { method: req.method, path: req.url, action: action?.kill ? 'kill' : action, status: null, body: null, delivered: false };
      this.exchanges.push(exchange);
      const headers = { ...req.headers, host: `127.0.0.1:${this.upstream}`, connection: 'close' };
      const cut = () => req.socket.resetAndDestroy?.() ?? req.socket.destroy();
      let told = false;
      let killed = null;
      const fire = () => {
        if (rule && !told) {
          told = true;
          rule.fired(exchange);
        }
      };
      const up = httpRequest({ host: '127.0.0.1', port: this.upstream, method: req.method, path: req.url, headers }, (ur) => {
        const parts = [];
        ur.on('data', (c) => parts.push(c));
        ur.on('error', () => cut());
        ur.on('end', () => {
          const raw = Buffer.concat(parts);
          exchange.status = ur.statusCode;
          exchange.body = parse(raw);
          if (action?.kill) {
            // The process dies before its answer gets out (even when it had finished).
            (killed ?? Promise.resolve()).then(() => {
              cut();
              fire();
            });
            return;
          }
          if (action === 'lose' || action === 'drop') {
            // Committed, but the client never hears it.
            cut();
            fire();
            return;
          }
          const out = { ...ur.headers };
          delete out['transfer-encoding'];
          delete out.connection;
          out['content-length'] = String(raw.length);
          res.writeHead(ur.statusCode, out);
          res.end(raw);
          exchange.delivered = true;
        });
      });
      up.on('error', () => {
        cut();
        fire();
      });
      up.end(body, () => {
        if (action === 'drop') {
          // The bytes have left; the line goes dead both ways before any answer.
          setImmediate(() => {
            up.destroy();
            cut();
            fire();
          });
        } else if (action?.kill) {
          killed = new Promise((done) => setTimeout(() => Promise.resolve(action.kill()).then(done), action.delayMs ?? 0));
        }
      });
    });
  }
}

/** `chrome.storage.local` as the outbox uses it, in memory. */
export class MemoryStorage {
  constructor(items = {}) {
    this.items = new Map(Object.entries(items));
  }
  async get(keys) {
    return Object.fromEntries(keys.filter((k) => this.items.has(k)).map((k) => [k, structuredClone(this.items.get(k))]));
  }
  async set(items) {
    for (const [k, v] of Object.entries(items)) this.items.set(k, structuredClone(v));
  }
  async remove(keys) {
    for (const k of keys) this.items.delete(k);
  }
}

/** A well-formed capture UUIDv7 numbered `n` (same prefix as the contract's example). */
export const captureId = (n) => `0192812a-6f3c-7a41-9c2e-${n.toString(16).padStart(12, '0')}`;

/** A distinct JPEG frame / WAV clip (content-addressed media: different bytes per capture). */
export const jpegOf = (n) => Buffer.concat([Buffer.from([0xff, 0xd8, 0xff, 0xe0, 0x00, 0x10]), Buffer.from(`JFIF\0frame ${n}`)]).toString('base64');
export const wavOf = (n) => Buffer.concat([Buffer.from('RIFF'), Buffer.from([36, 0, 0, 0]), Buffer.from('WAVEfmt '), Buffer.from(`clip ${n} `.padEnd(24, '.'))]).toString('base64');

/** A capture as the extension builds it (`capture.ts`), word `lemma` in `sentence`. */
export function makeCapture(n, lemma, sentence, extra = {}) {
  const at = sentence.indexOf(lemma);
  return {
    schema: 1,
    capture_id: captureId(n),
    kind: 'word',
    lang: { target: 'en', native: 'tr' },
    term: { surface: lemma, lemma, pos: 'noun' },
    definition: { text: `anlam ${n}`, source: 'dict' },
    context: { sentence, translation: `çeviri ${n}`, term_offset: at >= 0 ? [at, at + lemma.length] : null },
    source: { type: 'video', url: `https://www.youtube.com/watch?v=dQw4w9WgXcQ&t=${n}s`, title: 'Contract check', site: 'youtube', video_id: 'dQw4w9WgXcQ', t_start_ms: n * 1000, t_end_ms: n * 1000 + 1500 },
    media: {},
    deck_id: null,
    tags: ['subtitle'],
    created_at: new Date(Date.UTC(2026, 10, 1, 0, 0, n % 60)).toISOString(),
    client: { name: 'subtitle-extension', ext_version: '3.1.3' },
    ...extra,
  };
}

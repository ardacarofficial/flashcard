#!/usr/bin/env node
/**
 * The data journeys J4–J7 of the Windows run (./data-journeys.mjs) on ANY machine, against the
 * real Rust core: `fc dev serve` (the dev bridge: the same dispatch as the desktop IPC) plus
 * `fc api serve` (the same local HTTP server the app runs for the Subtitle extension) on a
 * temporary collection. No UI, no WebDriver: it checks the requests the journeys send and the
 * read-backs they make, so the Windows run cannot fail on a wrong method name or payload.
 *
 *   cargo build -p fc-cli
 *   node apps/desktop/e2e-windows/local-check.mjs [--fc target/debug/fc] [--only J4,J6]
 *
 * "Close the app" here stops both processes; "start on a damaged collection" runs the CLI's
 * `fc rescue --from backup` (the same `rescue()` the app's native dialog calls) and starts them
 * again — the native dialog itself only exists in the installed app (run.mjs drives it).
 */
import { execFileSync, spawn } from 'node:child_process';
import { existsSync, mkdtempSync, rmSync } from 'node:fs';
import { createServer } from 'node:net';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { j4Capture, j5Reopen, j6BackupRestore, j7Rescue, until } from './data-journeys.mjs';

const here = dirname(fileURLToPath(import.meta.url));
const arg = (name) => {
  const i = process.argv.indexOf(`--${name}`);
  return i > 0 ? process.argv[i + 1] : undefined;
};
const FC = resolve(arg('fc') ?? process.env.FC_BIN ?? join(here, '../../../target/debug', process.platform === 'win32' ? 'fc.exe' : 'fc'));
if (!existsSync(FC)) {
  console.error(`fc not found at ${FC}: cargo build -p fc-cli (or --fc / FC_BIN)`);
  process.exit(2);
}
const only = arg('only')?.split(',');
const dir = mkdtempSync(join(tmpdir(), 'fc-win-journeys-'));
const db = join(dir, 'profile', 'collection.db');

const freePort = () =>
  new Promise((done) => {
    const s = createServer();
    s.listen(0, '127.0.0.1', () => {
      const { port } = s.address();
      s.close(() => done(port));
    });
  });

let bridge = null;
let api = null;
let apiPort = 0;

async function startApp() {
  // The dev bridge (prints {"url","token"} on its first line).
  const proc = spawn(FC, ['--db', db, 'dev', 'serve'], { stdio: ['ignore', 'pipe', 'inherit'] });
  const first = await new Promise((ok, fail) => {
    let buf = '';
    proc.stdout.on('data', (d) => {
      buf += d;
      const nl = buf.indexOf('\n');
      if (nl >= 0) ok(JSON.parse(buf.slice(0, nl)));
    });
    proc.once('exit', (c) => fail(new Error(`fc dev serve exited (${c})`)));
  });
  bridge = { proc, ...first };
  if (!apiPort) {
    apiPort = await freePort();
    // What the app reads for the Subtitle address (`subtitle.status.address`).
    await env.rpc('settings.set', { scope: { kind: 'app' }, key: 'local_api.port', value: apiPort });
  }
  api = spawn(FC, ['--db', db, 'api', 'serve', '--port', String(apiPort)], { stdio: ['ignore', 'ignore', 'inherit'] });
  await until('fc api serve', async () => (await fetch(`http://127.0.0.1:${apiPort}/subtitle/v1/me`).catch(() => null)) !== null, 30_000, 100);
}

async function stop(p) {
  if (!p || p.exitCode !== null || p.signalCode !== null) return;
  await new Promise((r) => {
    p.once('exit', r);
    p.kill('SIGTERM');
  });
}

async function closeApp() {
  await stop(bridge?.proc);
  await stop(api);
  bridge = null;
  api = null;
}

const env = {
  tag: Date.now().toString(36),
  out: dir,
  ui: null,
  log: (...a) => console.log('   ', ...a),
  async rpc(method, params = {}) {
    const res = await fetch(`${bridge.url}/rpc`, {
      method: 'POST',
      headers: { authorization: `Bearer ${bridge.token}`, 'content-type': 'application/json' },
      body: JSON.stringify({ method, params }),
    });
    const reply = await res.json().catch(() => null);
    if (reply?.ok === true) return reply.result;
    throw new Error(`${method}: ${JSON.stringify(reply?.error ?? res.status)}`);
  },
  fc: (args) => execFileSync(FC, args, { encoding: 'utf8' }),
  closeApp,
  startApp,
  async startDamaged() {
    let report;
    try {
      report = JSON.parse(env.fc(['--db', db, 'rescue', '--from', 'backup']));
    } catch (e) {
      throw new Error(`fc rescue --from backup failed: ${e.stderr ?? e.message}`);
    }
    await startApp();
    return { rescued_by: 'fc rescue --from backup (local check: no native dialog here)', rescue: { mode: report.mode, backup: report.backup, kept_in: report.kept_in } };
  },
};

const journeys = [
  ['J4', 'capture over the local API → card + encounter', j4Capture],
  ['J5', 'reopen: card, review and encounter survive', j5Reopen],
  ['J6', 'backup → change → restore', j6BackupRestore],
  ['J7', 'rescue a truncated collection from the verified backup', j7Rescue],
];

let failed = 0;
try {
  await startApp();
  await env.rpc('onboarding.skip', {});
  for (const [id, name, fn] of journeys) {
    if (only && !only.includes(id)) continue;
    const facts = {};
    const t0 = Date.now();
    try {
      if (!bridge) await startApp();
      await fn(env, facts);
      console.log(`PASS ${id} ${name} (${((Date.now() - t0) / 1000).toFixed(1)}s)`);
    } catch (e) {
      failed++;
      console.log(`FAIL ${id} ${name}\n     ${e.message}`);
    }
    console.log(`     facts: ${JSON.stringify(facts)}`);
  }
} finally {
  await closeApp();
  if (!process.env.FC_KEEP) rmSync(dir, { recursive: true, force: true });
  else console.log(`kept ${dir}`);
}
process.exit(failed ? 1 : 0);

#!/usr/bin/env node
/**
 * `pnpm e2e` — journeys against the REAL Rust core (docs/07 §7, v1.3).
 *
 *   1. builds `fc` (`cargo build -p fc-cli`, a debug build: it has `fc dev serve`), or uses
 *      `FC_BIN`;
 *   2. makes a temporary profile and starts the dev bridge on it (`fc dev serve`, random port
 *      and token);
 *   3. seeds it through real commands (`e2e/seed.mjs`: decks, notes, Subtitle captures);
 *   4. starts vite in `--mode bridge` on a random port;
 *   5. runs Playwright (headless Chromium) on `e2e/journeys/*.spec.ts`;
 *   6. stops everything and deletes the profile (`FC_E2E_KEEP=1` keeps it).
 *
 * Extra arguments go to `playwright test` (`pnpm e2e -- --grep J3`, `pnpm e2e -- --headed`).
 * Chromium: `PLAYWRIGHT_CHROMIUM_EXECUTABLE`, else Playwright's own (`playwright install chromium`).
 */
import { spawn, spawnSync } from 'node:child_process';
import { existsSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { startBridge, stop, waitForHttp } from './lib/procs.mjs';
import { seed } from './seed.mjs';

const appDir = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const repo = resolve(appDir, '..', '..');
const exe = process.platform === 'win32' ? 'fc.exe' : 'fc';

function locateFc() {
  if (process.env.FC_BIN) {
    if (!existsSync(process.env.FC_BIN)) throw new Error(`FC_BIN=${process.env.FC_BIN} does not exist`);
    return process.env.FC_BIN;
  }
  console.log('[e2e] cargo build -p fc-cli');
  const r = spawnSync('cargo', ['build', '-p', 'fc-cli'], { cwd: repo, stdio: 'inherit' });
  if (r.status !== 0) throw new Error('cargo build -p fc-cli failed');
  const target = process.env.CARGO_TARGET_DIR ? resolve(repo, process.env.CARGO_TARGET_DIR) : join(repo, 'target');
  return join(target, 'debug', exe);
}

const randomPort = () => 20000 + Math.floor(Math.random() * 20000);

async function main() {
  const fc = locateFc();
  const profile = mkdtempSync(join(tmpdir(), 'fc-e2e-'));
  const db = join(profile, 'collection.db');
  const procs = [];
  let code = 1;
  try {
    const bridge = await startBridge(fc, db);
    procs.push(bridge.child);
    console.log(`[e2e] bridge ${bridge.url} · profile ${profile}`);
    const seeded = await seed({ fc, db, url: bridge.url, token: bridge.token });
    const seedFile = join(profile, 'seed.json');
    writeFileSync(seedFile, JSON.stringify(seeded, null, 2));

    // vite in bridge mode (dev-only transport); a random port per run, never 1420.
    let vite;
    let viteUrl;
    for (let attempt = 0; attempt < 5 && !vite; attempt++) {
      const port = randomPort();
      const child = spawn(
        process.execPath,
        [join(appDir, 'node_modules', 'vite', 'bin', 'vite.js'), '--mode', 'bridge', '--host', '127.0.0.1', '--port', String(port), '--strictPort'],
        {
          cwd: appDir,
          env: { ...process.env, VITE_FC_BRIDGE_URL: bridge.url, VITE_FC_BRIDGE_TOKEN: bridge.token },
          stdio: ['ignore', 'pipe', 'pipe'],
          detached: process.platform !== 'win32',
        },
      );
      let failed = false;
      child.on('exit', () => (failed = true));
      child.stderr.on('data', (d) => process.stderr.write(`[vite] ${d}`));
      const url = `http://127.0.0.1:${port}`;
      try {
        await waitForHttp(`${url}/`, 60_000, () => failed);
        vite = child;
        viteUrl = url;
      } catch {
        stop(child);
      }
    }
    if (!vite) throw new Error('vite did not start');
    procs.push(vite);
    console.log(`[e2e] vite ${viteUrl}`);

    const args = process.argv.slice(2).filter((a) => a !== '--');
    const pw = spawnSync(
      process.execPath,
      [join(appDir, 'node_modules', '@playwright', 'test', 'cli.js'), 'test', '-c', join(appDir, 'e2e', 'playwright.config.ts'), ...args],
      {
        cwd: appDir,
        stdio: 'inherit',
        env: {
          ...process.env,
          FC_BIN: fc,
          FC_E2E_DB: db,
          FC_E2E_PROFILE: profile,
          FC_E2E_SEED: seedFile,
          FC_E2E_URL: viteUrl,
          FC_BRIDGE_URL: bridge.url,
          FC_BRIDGE_TOKEN: bridge.token,
        },
      },
    );
    code = pw.status ?? 1;
  } catch (e) {
    console.error(`[e2e] ${e instanceof Error ? e.stack : e}`);
    code = 1;
  } finally {
    for (const p of procs.reverse()) stop(p);
    if (process.env.FC_E2E_KEEP === '1') console.log(`[e2e] kept profile ${profile}`);
    else rmSync(profile, { recursive: true, force: true, maxRetries: 5, retryDelay: 200 });
  }
  process.exit(code);
}

await main();

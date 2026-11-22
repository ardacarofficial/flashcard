/** Process helpers of the journey harness: the dev bridge, readiness waits, teardown. */
import { spawn, spawnSync } from 'node:child_process';
import { createInterface } from 'node:readline';

/**
 * Starts `fc --db <db> dev serve --port 0` and resolves with `{child, url, token}` from the
 * JSON line it prints on stdout. The collection is created and migrated if missing.
 */
export function startBridge(fc, db, { timeoutMs = 60_000 } = {}) {
  return new Promise((resolvePromise, reject) => {
    const env = { ...process.env };
    delete env.FC_DB;
    delete env.FC_BRIDGE_TOKEN;
    const child = spawn(fc, ['--db', db, 'dev', 'serve', '--port', '0'], { stdio: ['ignore', 'pipe', 'pipe'], env });
    let stderr = '';
    child.stderr.on('data', (d) => (stderr += d));
    const timer = setTimeout(() => {
      stop(child);
      reject(new Error(`fc dev serve did not start in ${timeoutMs} ms\n${stderr}`));
    }, timeoutMs);
    child.on('exit', (code) => {
      clearTimeout(timer);
      reject(new Error(`fc dev serve exited (${code})\n${stderr}`));
    });
    const lines = createInterface({ input: child.stdout });
    lines.once('line', (line) => {
      clearTimeout(timer);
      try {
        const { url, token } = JSON.parse(line);
        resolvePromise({ child, url, token });
      } catch (e) {
        stop(child);
        reject(new Error(`unexpected first line from fc dev serve: ${line} (${e})`));
      }
    });
  });
}

/** Polls `url` until it answers (any status < 500) or `timeoutMs` passes / `gaveUp()` says so. */
export async function waitForHttp(url, timeoutMs, gaveUp = () => false) {
  const until = Date.now() + timeoutMs;
  while (Date.now() < until) {
    if (gaveUp()) throw new Error(`${url}: process exited`);
    try {
      const r = await fetch(url);
      if (r.status < 500) return;
    } catch {
      /* not up yet */
    }
    await new Promise((r) => setTimeout(r, 250));
  }
  throw new Error(`${url} did not answer within ${timeoutMs} ms`);
}

/** Stops a child and its own children (vite's esbuild). */
export function stop(child) {
  if (!child || child.exitCode !== null) return;
  try {
    if (process.platform === 'win32') {
      spawnSync('taskkill', ['/pid', String(child.pid), '/T', '/F'], { stdio: 'ignore' });
    } else {
      try {
        process.kill(-child.pid, 'SIGTERM');
      } catch {
        child.kill('SIGTERM');
      }
    }
  } catch {
    /* already gone */
  }
}

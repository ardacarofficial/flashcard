// Stages the release `fc` command-line program for bundling (v1.3): Settings › MCP gives
// assistants the path of `fc` next to the app's executable (`mcp.setup`), so the installer
// must ship it. Builds `cargo build --release -p fc-cli --locked` (no `dev-bridge` feature: the
// development bridge is never in a release `fc`) and copies it to
// `src-tauri/binaries/fc-<host triple>[.exe]`, the name Tauri's `bundle.externalBin`
// (`src-tauri/tauri.cli.conf.json`) expects; the bundle installs it as `fc[.exe]` beside the app.
//
//   node scripts/stage-cli.mjs            # build + stage
//   node scripts/stage-cli.mjs --no-build # stage an already built target/release/fc
import { execFileSync } from 'node:child_process';
import { copyFileSync, existsSync, mkdirSync, statSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const desktop = resolve(here, '..');
const root = resolve(desktop, '..', '..');

const host = /^host: (.+)$/m.exec(execFileSync('rustc', ['-vV'], { encoding: 'utf8' }))?.[1]?.trim();
if (!host) throw new Error('stage-cli: no host triple in `rustc -vV`');
const ext = host.includes('windows') ? '.exe' : '';

if (!process.argv.includes('--no-build')) {
  console.log('[stage-cli] cargo build --release -p fc-cli --locked');
  execFileSync('cargo', ['build', '--release', '-p', 'fc-cli', '--locked'], { cwd: root, stdio: 'inherit' });
}
const target = process.env.CARGO_TARGET_DIR ? resolve(process.env.CARGO_TARGET_DIR) : join(root, 'target');
const built = join(target, 'release', `fc${ext}`);
if (!existsSync(built)) throw new Error(`stage-cli: ${built} does not exist`);
const out = join(desktop, 'src-tauri', 'binaries', `fc-${host}${ext}`);
mkdirSync(dirname(out), { recursive: true });
copyFileSync(built, out);
console.log(`[stage-cli] ${built} → ${out} (${(statSync(out).size / 1e6).toFixed(1)} MB)`);

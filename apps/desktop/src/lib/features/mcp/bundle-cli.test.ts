import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';
import { join, resolve } from 'node:path';

// v1.3: Settings › MCP points assistants at the `fc` program next to the app (`mcp.setup`), so
// every Windows installer must ship it. The bundle itself is built on Windows CI only; this
// keeps the pieces that make it happen from drifting apart.
const root = resolve(__dirname, '../../../../../..');
const read = (p: string) => readFileSync(join(root, p), 'utf8');

describe('the installers ship the fc CLI', () => {
  it('the CLI overlay bundles binaries/fc, which stage-cli writes (release build, no dev bridge)', () => {
    const conf = JSON.parse(read('apps/desktop/src-tauri/tauri.cli.conf.json')) as { bundle: { externalBin: string[] } };
    expect(conf.bundle.externalBin).toEqual(['binaries/fc']);
    const stage = read('apps/desktop/scripts/stage-cli.mjs');
    expect(stage).toContain("'build', '--release', '-p', 'fc-cli', '--locked'");
    expect(stage).not.toContain('--features');
    expect(stage).toContain("join(desktop, 'src-tauri', 'binaries', `fc-${host}${ext}`)");
    // The default config stays without it: `tauri dev` needs no staged binary.
    expect(read('apps/desktop/src-tauri/tauri.conf.json')).not.toContain('externalBin');
  });

  it.each(['.github/workflows/windows-build.yml', '.github/workflows/release.yml'])('%s stages fc, bundles it and fails when fc.exe is missing', (wf) => {
    const y = read(wf);
    expect(y).toContain('node scripts/stage-cli.mjs');
    expect(y).toContain('--config src-tauri/tauri.cli.conf.json');
    expect(y).toContain('fc.exe is missing from');
    expect(y.indexOf('node scripts/stage-cli.mjs')).toBeLessThan(y.indexOf('--config src-tauri/tauri.cli.conf.json'));
  });
});

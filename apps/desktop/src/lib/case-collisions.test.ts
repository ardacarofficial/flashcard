import { describe, expect, it } from 'vitest';
import { mkdtempSync, mkdirSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
// @ts-expect-error plain .mjs script shared with CI
import { scan } from '../../../../.github/scripts/check-case-collisions.mjs';

describe('case-insensitive filesystem safety (Windows/macOS)', () => {
  it('the real sources have no colliding paths or ambiguous imports', () => {
    const root = resolve(__dirname, '../../../..');
    const problems: string[] = scan([
      join(root, 'apps/desktop/src'),
      join(root, 'apps/browser-extension/src'),
    ]);
    expect(problems).toEqual([]);
  });

  it('detects the Toast.svelte vs toast.svelte.ts trap and plain case collisions', () => {
    const d = mkdtempSync(join(tmpdir(), 'case-'));
    writeFileSync(join(d, 'Toast.svelte'), '<script>import { toast } from "./toast.svelte";</script>');
    writeFileSync(join(d, 'toast.svelte.ts'), 'export const toast = 1;');
    mkdirSync(join(d, 'sub'));
    writeFileSync(join(d, 'sub', 'A.ts'), '');
    writeFileSync(join(d, 'sub', 'a.ts'), '');
    const problems: string[] = scan([d]);
    expect(problems.some((p) => p.startsWith('ambiguous import'))).toBe(true);
    expect(problems.some((p) => p.startsWith('case collision'))).toBe(true);
  });
});

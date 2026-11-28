import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import { legalHoldTargetGuardPrefix } from '../../../modules/legal/server/src/adapters/legal-read-tail-r24-sources';

const srcRoot = resolve(__dirname, '../../../modules/legal/server/src');
const preDedupePrefix =
  'SELECT 1 FROM legal_hold_targets target JOIN legal_holds hold ON hold.id=target.hold_id WHERE ';

describe('legal hold-target guard read prefix family (r24)', () => {
  it('emits byte-identical SQL for the pre-dedupe literals', () => {
    expect(legalHoldTargetGuardPrefix).toBe(preDedupePrefix);
  });

  it('keeps the fragment read-only (no writes or row locks)', () => {
    expect(legalHoldTargetGuardPrefix).toContain('SELECT 1 FROM legal_hold_targets target');
    expect(legalHoldTargetGuardPrefix).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
  });

  it('keeps one definition and no inline guard-prefix literals in the three callers', async () => {
    const [shared, preservation, mutation, service] = await Promise.all([
      readFile(new URL('../../../modules/legal/server/src/adapters/legal-read-tail-r24-sources.ts', import.meta.url), 'utf8'),
      readFile(new URL('../../../modules/legal/server/src/adapters/runtime-preservation-authority.ts', import.meta.url), 'utf8'),
      readFile(new URL('../../../modules/legal/server/src/adapters/runtime-mutation-authority.ts', import.meta.url), 'utf8'),
      readFile(new URL('../../../modules/legal/server/src/adapters/legal/service.ts', import.meta.url), 'utf8'),
    ]);
    expect(shared.match(/export const legalHoldTargetGuardPrefix/g) ?? []).toHaveLength(1);
    for (const [name, text, token] of [
      ['preservation', preservation, '"+legalHoldTargetGuardPrefix+"'],
      ['mutation', mutation, '${legalHoldTargetGuardPrefix}'],
      ['service', service, '"+legalHoldTargetGuardPrefix+'],
    ] as const) {
      expect(text, name).toMatch(/import\s*\{[^}]*legalHoldTargetGuardPrefix/);
      expect(text, name).toContain(token);
      expect(text, name).not.toContain(preDedupePrefix);
    }
  });

  it('retirement:ts-statements stays strictly below the recorded 261 r23 baseline', async () => {
    let total = 0;
    const walk = async (dir: string) => {
      for (const entry of await readdir(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) await walk(p);
        else if (entry.name.endsWith('.ts')) total += ((await readFile(p, 'utf8')).match(/SELECT/g) ?? []).length;
      }
    };
    await walk(srcRoot);
    expect(total).toBeLessThan(261);
    expect(total).toBeLessThanOrEqual(259);
  });
});

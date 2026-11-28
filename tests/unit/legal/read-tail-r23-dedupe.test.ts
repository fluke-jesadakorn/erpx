import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import { legalHoldActiveSql } from '../../../modules/legal/server/src/adapters/legal-read-tail-r23-sources';

const srcRoot = resolve(__dirname, '../../../modules/legal/server/src');
const pointRead = (inequality: string) =>
  `SELECT 1 FROM legal_hold_targets t JOIN legal_holds h ON h.id=t.hold_id WHERE t.item_id=? AND h.state${inequality}'released'`;

describe('legal hold-active point read family (r23)', () => {
  it('emits byte-identical SQL for both pre-dedupe literals', () => {
    expect(legalHoldActiveSql('!=')).toBe(pointRead('!='));
    expect(legalHoldActiveSql('<>')).toBe(pointRead('<>'));
  });

  it('keeps the fragment read-only (no writes or row locks)', () => {
    const sql = legalHoldActiveSql('!=');
    expect(sql).toContain('SELECT 1 FROM legal_hold_targets');
    expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
  });

  it('keeps one generator definition and no inline hold-active literals in callers', async () => {
    const [shared, worker, retention] = await Promise.all([
      readFile(new URL('../../../modules/legal/server/src/adapters/legal-read-tail-r23-sources.ts', import.meta.url), 'utf8'),
      readFile(new URL('../../../modules/legal/server/src/adapters/legal/destruction-worker.ts', import.meta.url), 'utf8'),
      readFile(new URL('../../../modules/legal/server/src/adapters/retention-execution.ts', import.meta.url), 'utf8'),
    ]);
    expect(shared.match(/export const legalHoldActiveSql/g) ?? []).toHaveLength(1);
    expect(worker).not.toContain('SELECT 1 FROM legal_hold_targets');
    expect(worker).toContain("legalHoldActiveSql('!=')");
    expect(retention).not.toContain('SELECT 1 FROM legal_hold_targets');
    expect(retention).toContain("legalHoldActiveSql('<>')");
  });

  it('retirement:ts-statements stays strictly below the recorded 262 r22 baseline', async () => {
    let total = 0;
    const walk = async (dir: string) => {
      for (const entry of await readdir(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) await walk(p);
        else if (entry.name.endsWith('.ts')) total += ((await readFile(p, 'utf8')).match(/SELECT/g) ?? []).length;
      }
    };
    await walk(srcRoot);
    expect(total).toBeLessThan(262);
    expect(total).toBeLessThanOrEqual(261);
  });
});

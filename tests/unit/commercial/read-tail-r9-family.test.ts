import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { holdingSnapshotReadSql } from '../../../modules/commercial/server/src/adapters/commercial-read-tail-r9-sources';

const srcRoot = resolve(__dirname, '../../../modules/commercial/server/src');
const adapters = (name: string) => readFileSync(
  new URL(`../../../modules/commercial/server/src/adapters/${name}`, import.meta.url), 'utf8',
);

// Pre-dedupe literals copied verbatim from HEAD (whitespace-normalised); byte-identity oracles only.
const SNAPSHOT_HEADER_LOCK = 'SELECT refreshed_at FROM billing_usage_snapshots WHERE holding_id=? AND month=? FOR UPDATE';
const SNAPSHOT_MONTHS = 'SELECT month FROM billing_usage_snapshots WHERE holding_id=?';

describe('Commercial read-tail r9 parameterized Holding-scoped usage-snapshot family', () => {
  test('holdingSnapshotReadSql reproduces every original byte-identically', () => {
    expect(holdingSnapshotReadSql('refreshed_at', ' AND month=? FOR UPDATE')).toBe(SNAPSHOT_HEADER_LOCK);
    expect(holdingSnapshotReadSql('month', '')).toBe(SNAPSHOT_MONTHS);
  });

  test('both former sites compose the builder and keep their own prepare/bind', () => {
    const pj = adapters('projection.ts');
    expect(pj).toContain('holdingSnapshotReadSql');
    expect(pj).not.toContain(SNAPSHOT_HEADER_LOCK);
    expect(pj).not.toContain(SNAPSHOT_MONTHS);
    expect(pj.match(/holdingSnapshotReadSql\(/g)).toHaveLength(2); // 2 call sites
    expect(pj).toContain('.bind(binding.id, month).first<{refreshed_at:number}>()');
    expect(pj).toContain('.bind(binding.id).all<{ month: string }>()');
  });

  test('r1/r3 are retired; r4/r5/r6/r7/r8 helpers and advisory/row locks stay untouched', () => {
    expect(() => adapters('commercial-read-tail-r1-sources.ts')).toThrow();
    expect(() => adapters('commercial-read-tail-r3-sources.ts')).toThrow();
    expect(adapters('commercial-read-tail-r4-sources.ts')).toContain('purchaseFactAgg');
    expect(adapters('commercial-read-tail-r5-sources.ts')).toContain('receiptProbe');
    expect(adapters('commercial-read-tail-r6-sources.ts')).toContain('snapshotByMonthSql');
    expect(adapters('commercial-read-tail-r7-sources.ts')).toContain('holdingMonthReadSql');
    expect(adapters('commercial-read-tail-r8-sources.ts')).toContain('companyScopedStarSql');
    expect(adapters('package-grants.ts')).toContain('SELECT pg_advisory_xact_lock(hashtextextended(?,0))');
    expect(adapters('purchase-command-boundary.ts')).toContain('SELECT pg_advisory_xact_lock(hashtextextended(?,0))');
    expect(adapters('purchase-command-boundary.ts')).toContain('SELECT company_id,package_id FROM company_package_grants ORDER BY company_id,package_id FOR SHARE');
  });

  test('the single scoped head owns the literal and the r9 module delegates', () => {
    const shared = adapters('commercial-read-tail-r9-sources.ts');
    expect(shared.match(/SELECT /g) ?? []).toHaveLength(0);
    expect(shared).toContain("scopedReadHead(columns, 'billing_usage_snapshots', 'holding', ' ', rest)");
    const head = adapters('commercial-read-tail-r14-sources.ts');
    expect(head).toContain("holding: 'holding_id=?',");
  });

  test('retirement:ts-statements is strictly lower than BEFORE=29', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBe(11);
    expect(total).toBeLessThan(29);
  });
});

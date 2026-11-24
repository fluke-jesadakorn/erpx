import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { holdingMonthReadSql } from '../../../packages/server-commercial/src/adapters/commercial-read-tail-r7-sources';

const srcRoot = resolve(__dirname, '../../../packages/server-commercial/src');
const adapters = (name: string) => readFileSync(
  new URL(`../../../packages/server-commercial/src/adapters/${name}`, import.meta.url), 'utf8',
);

// Pre-dedupe literals copied verbatim from HEAD; byte-identity oracles only.
const SEAT_SNAPSHOT = 'SELECT snapshot_id,seat_coverage,seat_source_count FROM billing_usage_snapshots WHERE holding_id=? AND month=?';
const SEAT_SUMMARY = 'SELECT intervals_json::text intervals_json,snapshot_id FROM billing_seat_usage WHERE holding_id=? AND month=?';

const readOnly = (sql: string) => {
  expect(sql.toUpperCase()).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
};

describe('Commercial read-tail r7 byte-identical / parameterized family', () => {
  test('holding+month builder reproduces both originals byte-identically', () => {
    expect(holdingMonthReadSql('snapshot_id,seat_coverage,seat_source_count', 'billing_usage_snapshots')).toBe(SEAT_SNAPSHOT);
    expect(holdingMonthReadSql('intervals_json::text intervals_json,snapshot_id', 'billing_seat_usage')).toBe(SEAT_SUMMARY);
    [SEAT_SNAPSHOT, SEAT_SUMMARY].forEach(readOnly);
  });

  test('every former site composes the builder and keeps its own prepare/bind', () => {
    const seats = adapters('seats.ts');
    expect(seats).toContain('holdingMonthReadSql');
    expect(seats).not.toContain(SEAT_SNAPSHOT);
    expect(seats).not.toContain(SEAT_SUMMARY);
    expect(seats.match(/holdingMonthReadSql\(/g)).toHaveLength(2); // 2 call sites
    expect(seats.match(/\.bind\(holdingId,month\)/g)).toHaveLength(2);
  });

  test('r3/r4/r5/r6 helpers and advisory-lock statements stay untouched', () => {
    expect(() => adapters('commercial-read-tail-r1-sources.ts')).toThrow();
    expect(adapters('commercial-read-tail-r3-sources.ts')).toContain('seatHistorySql');
    expect(adapters('commercial-read-tail-r4-sources.ts')).toContain('purchaseFactAgg');
    expect(adapters('commercial-read-tail-r4-sources.ts')).toContain('coverageProbe');
    expect(adapters('commercial-read-tail-r5-sources.ts')).toContain('receiptProbe');
    expect(adapters('commercial-read-tail-r6-sources.ts')).toContain('snapshotByMonthSql');
    expect(adapters('package-grants.ts')).toContain('SELECT pg_advisory_xact_lock(hashtextextended(?,0))');
    expect(adapters('purchase-command-boundary.ts')).toContain('SELECT pg_advisory_xact_lock(hashtextextended(?,0))');
    expect(adapters('purchase-command-boundary.ts')).toContain('SELECT company_id,package_id FROM company_package_grants ORDER BY company_id,package_id FOR SHARE');
  });

  test('the single scoped head owns the literal and the r7 module delegates', () => {
    const shared = adapters('commercial-read-tail-r7-sources.ts');
    expect(shared.match(/SELECT /g) ?? []).toHaveLength(0);
    expect(shared).toContain("scopedReadHead(columns, table, 'holding-month')");
    const head = adapters('commercial-read-tail-r14-sources.ts');
    expect(head).toContain("'holding-month': 'holding_id=? AND month=?',");
  });

  test('retirement:ts-statements stays strictly below the r7 BEFORE=32 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBe(16);
    expect(total).toBeLessThan(32);
  });
});

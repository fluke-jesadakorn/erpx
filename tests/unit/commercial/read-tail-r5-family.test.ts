import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { receiptProbe } from '../../../packages/server-commercial/src/adapters/commercial-read-tail-r5-sources';

const srcRoot = resolve(__dirname, '../../../packages/server-commercial/src');
const adapters = (name: string) => readFileSync(
  new URL(`../../../packages/server-commercial/src/adapters/${name}`, import.meta.url), 'utf8',
);

// Pre-dedupe literals copied verbatim from HEAD; byte-identity oracles only.
const GRANT_RECEIPT = 'SELECT request_hash,after_json FROM company_package_grant_events WHERE id=?';
const GRANT_EXISTS = 'SELECT 1 FROM company_package_grant_events WHERE id=?';
const CHANGE_SET_RECEIPT = 'SELECT request_hash,result_json FROM company_package_change_sets WHERE id=?';
const CHANGE_SET_EXISTS = 'SELECT 1 FROM company_package_change_sets WHERE id=?';

const readOnly = (sql: string) => {
  expect(sql.toUpperCase()).toMatch(/^SELECT /);
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
};

describe('Commercial read-tail r5 byte-identical / parameterized family', () => {
  test('receipt-probe builder reproduces all three originals byte-identically', () => {
    expect(receiptProbe('company_package_grant_events', 'request_hash,after_json')).toBe(GRANT_RECEIPT);
    expect(receiptProbe('company_package_grant_events', '1')).toBe(GRANT_EXISTS);
    expect(receiptProbe('company_package_change_sets', 'request_hash,result_json')).toBe(CHANGE_SET_RECEIPT);
    expect(receiptProbe('company_package_change_sets', '1')).toBe(CHANGE_SET_EXISTS); // former r1 constant
    [GRANT_RECEIPT, GRANT_EXISTS, CHANGE_SET_RECEIPT, CHANGE_SET_EXISTS].forEach(readOnly);
  });

  test('every former site composes the builder and keeps its own prepare/bind', () => {
    const pg = adapters('package-grants.ts');
    expect(pg).toContain('receiptProbe');
    for (const sql of [GRANT_RECEIPT, GRANT_EXISTS, CHANGE_SET_RECEIPT, CHANGE_SET_EXISTS]) expect(pg, sql).not.toContain(sql);
    expect(pg.match(/receiptProbe\(/g)).toHaveLength(4); // 3 original + r1 delegated call sites
    expect(pg.split('receiptProbe("company_package_change_sets","1")')).toHaveLength(2);
  });

  test('r1 is retired; r3/r4 helpers and advisory-lock statements stay untouched', () => {
    expect(() => adapters('commercial-read-tail-r1-sources.ts')).toThrow();
    expect(adapters('commercial-read-tail-r3-sources.ts')).toContain('seatHistorySql');
    expect(adapters('commercial-read-tail-r4-sources.ts')).toContain('purchaseFactAgg');
    expect(adapters('commercial-read-tail-r4-sources.ts')).toContain('coverageProbe');
    expect(adapters('package-grants.ts')).toContain('SELECT pg_advisory_xact_lock(hashtextextended(?,0))');
    expect(adapters('purchase-command-boundary.ts')).toContain('SELECT pg_advisory_xact_lock(hashtextextended(?,0))');
  });

  test('the single scoped head owns the literal and the r5 module delegates', () => {
    const shared = adapters('commercial-read-tail-r5-sources.ts');
    expect(shared.match(/SELECT /g) ?? []).toHaveLength(0);
    expect(shared).toContain("scopedReadHead(columns, table, 'id')");
    const head = adapters('commercial-read-tail-r14-sources.ts');
    expect(head.match(/SELECT /g) ?? []).toHaveLength(0); // keyword is a named constant, not a local literal
    expect(head).toContain("id: 'id=?',");
    expect(head).toContain('scopedReadHead');
  });

  test('retirement:ts-statements stays strictly below the r5 BEFORE=36 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBe(15);
    expect(total).toBeLessThan(36);
  });
});

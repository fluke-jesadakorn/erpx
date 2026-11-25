import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { coverageProbe, purchaseFactAgg } from '../../../packages/server-commercial/src/adapters/commercial-read-tail-r4-sources';

const srcRoot = resolve(__dirname, '../../../packages/server-commercial/src');
const adapters = (name: string) => readFileSync(
  new URL(`../../../packages/server-commercial/src/adapters/${name}`, import.meta.url), 'utf8',
);

// Pre-dedupe literals copied verbatim from HEAD; byte-identity oracles only.
const GRANTS = 'SELECT jsonb_agg(to_jsonb(g) ORDER BY package_id) FROM company_package_grants g WHERE company_id=?';
const HISTORY = 'SELECT jsonb_agg(to_jsonb(h) ORDER BY package_id,package_version) FROM company_package_history_rights h WHERE company_id=?';
const PRINCIPAL = 'SELECT since_at FROM billing_principal_status_coverage WHERE singleton=TRUE';
const SEAT = 'SELECT started_at FROM billing_seat_history_coverage WHERE singleton=1';

const readOnly = (sql: string) => {
  expect(sql.toUpperCase()).toMatch(/^SELECT /);
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
};

describe('Commercial read-tail r4 byte-identical / parameterized family', () => {
  test('purchase-fact aggregate builder reproduces both originals byte-identically', () => {
    expect(purchaseFactAgg('company_package_grants', 'g', 'package_id')).toBe(GRANTS);
    expect(purchaseFactAgg('company_package_history_rights', 'h', 'package_id,package_version')).toBe(HISTORY);
    [GRANTS, HISTORY].forEach(readOnly);
  });

  test('coverage singleton probe builder reproduces both originals byte-identically', () => {
    expect(coverageProbe('since_at', 'billing_principal_status_coverage', 'TRUE')).toBe(PRINCIPAL);
    expect(coverageProbe('started_at', 'billing_seat_history_coverage', '1')).toBe(SEAT);
    [PRINCIPAL, SEAT].forEach(readOnly);
  });

  test('every former site composes the builder and keeps its own prepare/bind', () => {
    const pg = adapters('package-grants.ts');
    expect(pg).toContain('purchaseFactAgg');
    for (const sql of [GRANTS, HISTORY]) expect(pg, sql).not.toContain(sql);
    expect(pg.match(/purchaseFactAgg\(/g)).toHaveLength(2); // 2 call sites

    const pj = adapters('projection.ts');
    expect(pj).toContain('coverageProbe');
    expect(pj).not.toContain(PRINCIPAL);
    const st = adapters('seats.ts');
    expect(st).toContain('coverageProbe');
    expect(st).not.toContain(SEAT);
  });

  test('r3 helpers and advisory-lock statements stay untouched', () => {
    expect(() => adapters('commercial-read-tail-r1-sources.ts')).toThrow();
    expect(adapters('commercial-read-tail-r3-sources.ts')).toContain('seatHistorySql');
    expect(adapters('package-grants.ts')).toContain('SELECT pg_advisory_xact_lock(hashtextextended(?,0))');
    expect(adapters('purchase-command-boundary.ts')).toContain('SELECT pg_advisory_xact_lock(hashtextextended(?,0))');
  });

  test('shared module keeps no local read literal', () => {
    const shared = adapters('commercial-read-tail-r4-sources.ts');
    expect(shared.match(/SELECT /g) ?? []).toHaveLength(0); // both heads delegate to owner-local renderers
  });

  test('retirement:ts-statements stays strictly below the r4 BEFORE=38 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBe(13);
    expect(total).toBeLessThan(38);
  });
});

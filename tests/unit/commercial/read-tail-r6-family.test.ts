import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { purchaseObservationAgg, snapshotByMonthSql } from '../../../packages/server-commercial/src/adapters/commercial-read-tail-r6-sources';

const srcRoot = resolve(__dirname, '../../../packages/server-commercial/src');
const adapters = (name: string) => readFileSync(
  new URL(`../../../packages/server-commercial/src/adapters/${name}`, import.meta.url), 'utf8',
);

// Pre-dedupe literals copied verbatim from HEAD; byte-identity oracles only.
const SNAPSHOT_FULL = `SELECT holding_id,ai_coverage,storage_coverage,seat_coverage,seat_source_count,unreconciled_originals,
      active_seat_count,active_seat_observed_at
      FROM billing_usage_snapshots WHERE month=?`;
const SNAPSHOT_SEAT = `SELECT holding_id,active_seat_count,active_seat_observed_at
      FROM billing_usage_snapshots WHERE month=?`;
const GRANTS_AGG = "COALESCE((SELECT jsonb_agg(jsonb_build_object('companyId',g.company_id,'packageId',g.package_id,'packageVersion',g.package_version,'state',g.state,'startsAt',g.starts_at,'endsAt',g.ends_at,'version',g.version)) FROM company_package_grants g WHERE g.company_id=purchase_scope.company_id),'[]'::jsonb)";
const HISTORY_AGG = "COALESCE((SELECT jsonb_agg(jsonb_build_object('companyId',h.company_id,'packageId',h.package_id,'packageVersion',h.package_version,'sourceGrantVersion',h.source_grant_version,'sourceEventId',h.source_event_id,'retainedAt',h.retained_at)) FROM company_package_history_rights h WHERE h.company_id=COMPANY),'[]'::jsonb)";

const readOnly = (sql: string) => {
  expect(sql.toUpperCase()).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
};

describe('Commercial read-tail r6 byte-identical / parameterized family', () => {
  test('snapshot-by-month builder reproduces both originals byte-identically', () => {
    expect(snapshotByMonthSql(`holding_id,ai_coverage,storage_coverage,seat_coverage,seat_source_count,unreconciled_originals,
      active_seat_count,active_seat_observed_at`)).toBe(SNAPSHOT_FULL);
    expect(snapshotByMonthSql('holding_id,active_seat_count,active_seat_observed_at')).toBe(SNAPSHOT_SEAT);
    [SNAPSHOT_FULL, SNAPSHOT_SEAT].forEach(readOnly);
  });

  test('purchase-observation aggregate builder reproduces both originals byte-identically', () => {
    expect(purchaseObservationAgg('purchase_scope.company_id', 'company_package_grants', 'g',
      "'companyId',g.company_id,'packageId',g.package_id,'packageVersion',g.package_version,'state',g.state,'startsAt',g.starts_at,'endsAt',g.ends_at,'version',g.version")).toBe(GRANTS_AGG);
    expect(purchaseObservationAgg('COMPANY', 'company_package_history_rights', 'h',
      "'companyId',h.company_id,'packageId',h.package_id,'packageVersion',h.package_version,'sourceGrantVersion',h.source_grant_version,'sourceEventId',h.source_event_id,'retainedAt',h.retained_at")).toBe(HISTORY_AGG);
    [GRANTS_AGG, HISTORY_AGG].forEach(readOnly);
  });

  test('every former site composes the builder and keeps its own prepare/bind', () => {
    const pj = adapters('projection.ts');
    expect(pj).toContain('snapshotByMonthSql');
    expect(pj).not.toContain('SELECT holding_id,ai_coverage');
    expect(pj).not.toContain('SELECT holding_id,active_seat_count,active_seat_observed_at\n');
    expect(pj.match(/snapshotByMonthSql\(/g)).toHaveLength(2); // 2 call sites

    const pcb = adapters('purchase-command-boundary.ts');
    expect(pcb).toContain('purchaseObservationAgg');
    expect(pcb).not.toContain("COALESCE((SELECT jsonb_agg(jsonb_build_object('companyId',g.company_id");
    expect(pcb.match(/purchaseObservationAgg\(/g)).toHaveLength(2); // 2 call sites
  });

  test('r3/r4/r5 helpers and advisory-lock statements stay untouched', () => {
    expect(() => adapters('commercial-read-tail-r1-sources.ts')).toThrow();
    expect(adapters('commercial-read-tail-r3-sources.ts')).toContain('seatHistorySql');
    expect(adapters('commercial-read-tail-r4-sources.ts')).toContain('purchaseFactAgg');
    expect(adapters('commercial-read-tail-r4-sources.ts')).toContain('coverageProbe');
    expect(adapters('commercial-read-tail-r5-sources.ts')).toContain('receiptProbe');
    expect(adapters('package-grants.ts')).toContain('SELECT pg_advisory_xact_lock(hashtextextended(?,0))');
    expect(adapters('purchase-command-boundary.ts')).toContain('SELECT pg_advisory_xact_lock(hashtextextended(?,0))');
    expect(adapters('purchase-command-boundary.ts')).toContain('SELECT company_id,package_id FROM company_package_grants ORDER BY company_id,package_id FOR SHARE');
  });

  test('shared module declares each remaining read literal exactly once', () => {
    const shared = adapters('commercial-read-tail-r6-sources.ts');
    // The snapshot-by-month literal now lives in the r13 month-scoped builder;
    // r6 delegates to it and keeps only its purchase-observation literal.
    expect(shared.match(/SELECT /g)).toHaveLength(1);
  });

  test('retirement:ts-statements stays strictly below the r6 BEFORE=34 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBe(14);
    expect(total).toBeLessThan(34);
  });
});

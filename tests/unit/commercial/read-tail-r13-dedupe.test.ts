import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { monthScopedRead } from '../../../packages/server-commercial/src/adapters/commercial-read-tail-r13-sources';

const srcRoot = resolve(__dirname, '../../../packages/server-commercial/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');

// Pre-dedupe oracles copied verbatim from the r12 baseline (BEFORE=23). The
// two usage-snapshot header reads (r6) and the billing-usage-meter source read
// share `SELECT <columns> FROM <table> WHERE month=?` and are parameterized
// byte-identically by one owner-local builder. Binds stay in each caller.
const SNAPSHOT_FULL = `SELECT holding_id,ai_coverage,storage_coverage,seat_coverage,seat_source_count,unreconciled_originals,
      active_seat_count,active_seat_observed_at
      FROM billing_usage_snapshots WHERE month=?`;
const SNAPSHOT_SEAT = `SELECT holding_id,active_seat_count,active_seat_observed_at
      FROM billing_usage_snapshots WHERE month=?`;
const METER_COLUMNS = `holding_id,month,source_key,company_id,company_name,branch_id,branch_name,
      meter_kind,funding_source,pricing_status,quantity,input_tokens,output_tokens,
      average_daily_bytes,active_at_period_end,request_status,gateway_status,provider,model,
      units_json::text units_json,cost_status,verified_cost_satang,cost_evidence_key,
      cost_reconciled_at,active_from,active_until,byte_milliseconds::text byte_milliseconds`;
const METER_SQL = `SELECT ${METER_COLUMNS}
      FROM billing_usage_meters WHERE month=?`;

const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
};

describe('Commercial read-tail r13 parameterized month-scoped read family', () => {
  test('builder reproduces all three pre-dedupe month reads byte-for-byte', () => {
    expect(monthScopedRead(`holding_id,ai_coverage,storage_coverage,seat_coverage,seat_source_count,unreconciled_originals,
      active_seat_count,active_seat_observed_at`, 'billing_usage_snapshots')).toBe(SNAPSHOT_FULL);
    expect(monthScopedRead('holding_id,active_seat_count,active_seat_observed_at', 'billing_usage_snapshots')).toBe(SNAPSHOT_SEAT);
    expect(monthScopedRead(METER_COLUMNS, 'billing_usage_meters')).toBe(METER_SQL);
    [SNAPSHOT_FULL, SNAPSHOT_SEAT, METER_SQL].forEach(readOnly);
  });

  test('one shared scoped head; r13 and r6 delegate and projection composes it', () => {
    const shared = src('commercial-read-tail-r13-sources.ts');
    expect(shared.match(/SELECT /g) ?? []).toHaveLength(0);
    expect(shared).toContain("scopedReadHead(columns, table, 'month', '\\n      ')");
    expect(shared).toContain('export function monthScopedRead');
    const head = src('commercial-read-tail-r14-sources.ts');
    expect(head).toContain("month: 'month=?',");
    expect(shared).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);

    const r6 = src('commercial-read-tail-r6-sources.ts');
    expect(r6).toContain('monthScopedRead(selectList, "billing_usage_snapshots")');
    expect(r6.match(/SELECT /g)).toHaveLength(1); // only purchaseObservationAgg remains local

    const pj = src('projection.ts');
    // Lane 37 wave 2 moved the three control-DB facts reads to the native owner.
    expect((pj.match(/monthScopedRead\(/g) ?? [])).toHaveLength(0);
    expect(pj).not.toContain('monthScopedRead(`holding_id,month,source_key');
    expect(pj).toContain('readNativeProjectionFacts');
    expect(pj.match(/SELECT /g) ?? []).toHaveLength(0); // principal-status event base is native now
  });

  test('r1-r12 helpers, source locks and native statement kinds stay untouched', () => {
    for (const [file, symbol] of [
      ['commercial-read-tail-r3-sources.ts', 'seatHistorySql'],
      ['commercial-read-tail-r4-sources.ts', 'purchaseFactAgg'],
      ['commercial-read-tail-r5-sources.ts', 'receiptProbe'],
      ['commercial-read-tail-r6-sources.ts', 'purchaseObservationAgg'],
      ['commercial-read-tail-r7-sources.ts', 'holdingMonthReadSql'],
      ['commercial-read-tail-r8-sources.ts', 'companyScopedStarSql'],
      ['commercial-read-tail-r9-sources.ts', 'holdingSnapshotReadSql'],
      ['commercial-read-tail-r10-sources.ts', 'packageSourceProbe'],
      ['commercial-read-tail-r11-sources.ts', 'packageSourceProjection'],
      ['commercial-read-tail-r12-sources.ts', 'purchaseScopeRead'],
    ] as const) expect(src(file)).toContain(symbol);

    const pg = src('package-grants.ts');
    expect(pg).toContain('SELECT pg_advisory_xact_lock(hashtextextended(?,0))');
    expect(pg).toContain('SELECT version FROM company_package_revisions WHERE company_id=? FOR UPDATE');
    const pcb = src('purchase-command-boundary.ts');
    expect(pcb).toContain('SELECT pg_advisory_xact_lock(hashtextextended(?,0))');
    expect(pcb).toContain("SELECT pg_advisory_xact_lock(hashtextextended('commercial.package:company:'||company_id,0))");
    expect(pcb).toContain('FOR SHARE');
    expect(pcb).toContain("nativeOwnerStatement('purchase.assertion'");
    expect(src('native-owner-read.ts')).toContain("startsWith('SELECT')");
  });

  test('retirement:ts-statements is strictly lower than BEFORE=23', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(23);
    expect(total).toBe(12);
  });
});

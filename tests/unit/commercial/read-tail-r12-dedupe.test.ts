import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { purchaseScopeRead } from '../../../packages/server-commercial/src/adapters/commercial-read-tail-r12-sources';
import { purchaseObservationAgg } from '../../../packages/server-commercial/src/adapters/commercial-read-tail-r6-sources';

const srcRoot = resolve(__dirname, '../../../packages/server-commercial/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');

// Pre-dedupe oracles copied verbatim from HEAD (r11 baseline, BEFORE=24). The
// two purchase-scope reads in purchase-command-boundary.ts are parameterized
// byte-identically by one owner-local builder; the CTE prefix, binds, aggregates
// and lock/native clauses stay in the caller.
const predicate = 'purchase_scope.company_id IS NOT NULL';
const GUARD_PROJ = `CASE WHEN ${predicate} THEN 1 ELSE 0 END ok`;
const GUARD_SQL = `SELECT CASE WHEN ${predicate} THEN 1 ELSE 0 END ok FROM purchase_scope`;

const grants = purchaseObservationAgg("purchase_scope.company_id", "company_package_grants", "g", "'companyId',g.company_id,'packageId',g.package_id,'packageVersion',g.package_version,'state',g.state,'startsAt',g.starts_at,'endsAt',g.ends_at,'version',g.version");
const history = purchaseObservationAgg("purchase_scope.company_id", "company_package_history_rights", "h", "'companyId',h.company_id,'packageId',h.package_id,'packageVersion',h.package_version,'sourceGrantVersion',h.source_grant_version,'sourceEventId',h.source_event_id,'retainedAt',h.retained_at");
const OBS_PROJ = `true commercial_purchase_observation,purchase_scope.company_id,?::text capability,?::text purpose,${predicate} allowed,purchase_scope.now observed_at,
    ${grants} grants,
    ${history} history`;
const OBS_SQL = `SELECT ${OBS_PROJ} FROM purchase_scope`;

const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
};

describe('Commercial read-tail r12 parameterized purchase-scope read family', () => {
  test('builder reproduces both pre-dedupe purchase-scope reads byte-for-byte', () => {
    expect(purchaseScopeRead(GUARD_PROJ)).toBe(GUARD_SQL);
    expect(purchaseScopeRead(OBS_PROJ)).toBe(OBS_SQL);
    readOnly(GUARD_SQL);
    readOnly(OBS_SQL);
  });

  test('one shared r27 head and two delegating call sites', () => {
    const shared = src('commercial-read-tail-r12-sources.ts');
    expect(shared.match(/SELECT /g) ?? []).toHaveLength(0); // head delegates to the r27 renderer
    expect(shared).toContain('import { packageSourceSelect } from "./commercial-read-tail-r27-sources"');
    expect(shared).toContain('packageSourceSelect(projection, "purchase_scope", "")');
    expect(src('commercial-read-tail-r27-sources.ts').match(/SELECT /g)).toHaveLength(1);
    expect(shared).toContain('export function purchaseScopeRead');
    expect(shared).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);

    const consumer = src('purchase-command-boundary.ts');
    expect(consumer.match(/purchaseScopeRead\(/g)).toHaveLength(2); // 2 call sites
    expect(consumer.match(/SELECT /g)).toHaveLength(6); // clock CTE + 3 lock statements (one with two tokens), no local guard/observation SELECT
    expect(consumer).not.toContain('SELECT CASE WHEN');
    expect(consumer).not.toContain('SELECT true commercial_purchase_observation');
    expect(consumer).toContain('clock + purchaseScopeRead(`CASE WHEN');
    expect(consumer).toContain('clock + purchaseScopeRead(`true commercial_purchase_observation');
  });

  test('r1-r11 helpers, source locks and native statement kinds stay untouched', () => {
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
    ] as const) expect(src(file)).toContain(symbol);

    const pg = src('package-grants.ts');
    expect(pg).toContain('SELECT pg_advisory_xact_lock(hashtextextended(?,0))');
    expect(pg).toContain('SELECT version FROM company_package_revisions WHERE company_id=? FOR UPDATE');
    const pcb = src('purchase-command-boundary.ts');
    expect(pcb).toContain('SELECT pg_advisory_xact_lock(hashtextextended(?,0))');
    expect(pcb).toContain("SELECT pg_advisory_xact_lock(hashtextextended('commercial.package:company:'||company_id,0))");
    expect(pcb).toContain('FOR SHARE');
    expect(pcb).toContain("nativeOwnerStatement('purchase.assertion'");
  });

  test('retirement:ts-statements is strictly lower than BEFORE=24', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(24);
    expect(total).toBe(13);
  });
});

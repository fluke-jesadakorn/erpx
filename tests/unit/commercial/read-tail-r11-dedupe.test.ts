import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { packageSourceProjection } from '../../../packages/server-commercial/src/adapters/commercial-read-tail-r11-sources';

const srcRoot = resolve(__dirname, '../../../packages/server-commercial/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');

// Pre-dedupe oracles copied verbatim from HEAD (r10 baseline, BEFORE=25). The
// two package-source projection SELECTs in the history `sources` UNION are
// parameterized byte-identically by one owner-local builder; binds, source
// identifiers and lock/native clauses stay in the callers.
const company = 'scope.company_id';
const now = 'scope.now';
const GRANTS = `SELECT package_id,package_version FROM company_package_grants WHERE company_id=${company} AND starts_at<=${now} AND state<>'inactive'`;
const HISTORY = `SELECT package_id,package_version FROM company_package_history_rights WHERE company_id=${company} AND retained_at<=${now}
        AND retained_at>=0 AND retained_at<=9007199254740991 AND source_grant_version>0 AND source_event_id<>''`;

const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
};

describe('Commercial read-tail r11 parameterized package-source projection family', () => {
  test('builder reproduces both pre-dedupe source SELECTs byte-for-byte', () => {
    expect(packageSourceProjection('company_package_grants', company, `starts_at<=${now} AND state<>'inactive'`)).toBe(GRANTS);
    expect(packageSourceProjection('company_package_history_rights', company, `retained_at<=${now}
        AND retained_at>=0 AND retained_at<=9007199254740991 AND source_grant_version>0 AND source_event_id<>''`)).toBe(HISTORY);
    const SOURCES = `${GRANTS}\n      UNION ${HISTORY}`;
    const rebuilt = `${packageSourceProjection('company_package_grants', company, `starts_at<=${now} AND state<>'inactive'`)}\n      UNION ${packageSourceProjection('company_package_history_rights', company, `retained_at<=${now}\n        AND retained_at>=0 AND retained_at<=9007199254740991 AND source_grant_version>0 AND source_event_id<>''`)}`;
    expect(rebuilt).toBe(SOURCES);
    readOnly(GRANTS);
    readOnly(HISTORY);
  });

  test('one shared r27 head and two delegating call sites', () => {
    const shared = src('commercial-read-tail-r11-sources.ts');
    expect(shared.match(/SELECT /g) ?? []).toHaveLength(0); // head delegates to the r27 renderer
    expect(shared).toContain('import { packageSourceSelect } from "./commercial-read-tail-r27-sources"');
    expect(shared).toContain('packageSourceSelect("package_id,package_version", source, ` WHERE company_id=${company} AND ${predicate}`)');
    expect(src('commercial-read-tail-r27-sources.ts').match(/SELECT /g)).toHaveLength(1);
    expect(shared).toContain('export function packageSourceProjection');
    expect(shared).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);

    const consumer = src('purchased-capability-sql.ts');
    expect(consumer.match(/packageSourceProjection\(/g)).toHaveLength(2); // 2 call sites
    expect(consumer).not.toContain('SELECT package_id,package_version FROM');
    expect(consumer).toContain('packageSourceProjection("company_package_grants"');
    expect(consumer).toContain('packageSourceProjection("company_package_history_rights"');
    expect(consumer).toContain('UNION ${packageSourceProjection(');
    // The r10 EXISTS-probe dedupe stays intact and composed from both sources.
    expect(consumer.match(/packageSourceProbe\(/g)).toHaveLength(4);
  });

  test('r1-r10 helpers, source locks and native statement kinds stay untouched', () => {
    expect(() => src('commercial-read-tail-r1-sources.ts')).toThrow();
    expect(src('commercial-read-tail-r3-sources.ts')).toContain('seatHistorySql');
    expect(src('commercial-read-tail-r4-sources.ts')).toContain('purchaseFactAgg');
    expect(src('commercial-read-tail-r5-sources.ts')).toContain('receiptProbe');
    expect(src('commercial-read-tail-r6-sources.ts')).toContain('purchaseObservationAgg');
    expect(src('commercial-read-tail-r7-sources.ts')).toContain('holdingMonthReadSql');
    expect(src('commercial-read-tail-r8-sources.ts')).toContain('companyScopedStarSql');
    expect(src('commercial-read-tail-r9-sources.ts')).toContain('holdingSnapshotReadSql');
    expect(src('commercial-read-tail-r10-sources.ts')).toContain('packageSourceProbe');

    const pg = src('package-grants.ts');
    expect(pg).toContain('SELECT pg_advisory_xact_lock(hashtextextended(?,0))');
    expect(pg).toContain('SELECT version FROM company_package_revisions WHERE company_id=? FOR UPDATE');
    const pcb = src('purchase-command-boundary.ts');
    expect(pcb).toContain('SELECT pg_advisory_xact_lock(hashtextextended(?,0))');
    expect(pcb).toContain("SELECT pg_advisory_xact_lock(hashtextextended('commercial.package:company:'||company_id,0))");
    expect(pcb).toContain('FOR SHARE');
    expect(pcb).toContain("nativeOwnerStatement('purchase.assertion'");
  });

  test('retirement:ts-statements is strictly lower than BEFORE=25', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(25);
    expect(total).toBe(12);
  });
});

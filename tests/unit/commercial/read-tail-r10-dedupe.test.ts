import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { packageSourceProbe } from '../../../modules/commercial/server/src/adapters/commercial-read-tail-r10-sources';

const srcRoot = resolve(__dirname, '../../../modules/commercial/server/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');

// Pre-dedupe oracles copied verbatim from HEAD (r9 baseline, BEFORE=28). The
// four EXISTS package-source probes are parameterized byte-identically by one
// owner-local builder; binds, aliases and lock clauses stay in the callers.
const literal = (v: string) => `'${v.replaceAll("'", "''")}'`;
const S = {
  company: 'scope.company_id',
  sources: `SELECT package_id,package_version FROM company_package_grants WHERE company_id=scope.company_id AND starts_at<=scope.now AND state<>'inactive'
      UNION SELECT package_id,package_version FROM company_package_history_rights WHERE company_id=scope.company_id AND retained_at<=scope.now
        AND retained_at>=0 AND retained_at<=9007199254740991 AND source_grant_version>0 AND source_event_id<>''`,
  id: 'support.tickets',
  version: 3,
  versions: [1, 2, 3],
  available: `scope.now>=0 AND state='active'`,
};
const oracle = {
  historyPurchase: (s: typeof S) => `EXISTS(SELECT 1 FROM (${s.sources}) purchase WHERE purchase.package_id=${literal(s.id)} AND purchase.package_version=${s.version})`,
  historyParent: (s: typeof S) => ` AND EXISTS(SELECT 1 FROM (${s.sources}) parent WHERE parent.package_id=${literal(s.id)} AND parent.package_version IN (${s.versions.join(",")}))`,
  currentPurchase: (s: typeof S) => `EXISTS(SELECT 1 FROM company_package_grants purchase WHERE purchase.company_id=${s.company} AND purchase.package_id=${literal(s.id)} AND purchase.package_version=${s.version} AND ${s.available})`,
  currentParent: (s: typeof S) => ` AND EXISTS(SELECT 1 FROM company_package_grants parent WHERE parent.company_id=${s.company} AND parent.package_id=${literal(s.id)} AND parent.package_version IN (${s.versions.join(",")}) AND ${s.available})`,
};

const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
};

describe('Commercial read-tail r10 parameterized EXISTS package-source family', () => {
  test('builder reproduces all four pre-dedupe probe SELECTs byte-for-byte', () => {
    const produced = {
      historyPurchase: packageSourceProbe(`(${S.sources})`, 'purchase', `purchase.package_id=${literal(S.id)} AND purchase.package_version`, `=${S.version}`, ''),
      historyParent: ` AND ${packageSourceProbe(`(${S.sources})`, 'parent', `parent.package_id=${literal(S.id)} AND parent.package_version`, ` IN (${S.versions.join(",")})`, '')}`,
      currentPurchase: packageSourceProbe('company_package_grants', 'purchase', `purchase.company_id=${S.company} AND purchase.package_id=${literal(S.id)} AND purchase.package_version`, `=${S.version}`, ` AND ${S.available}`),
      currentParent: ` AND ${packageSourceProbe('company_package_grants', 'parent', `parent.company_id=${S.company} AND parent.package_id=${literal(S.id)} AND parent.package_version`, ` IN (${S.versions.join(",")})`, ` AND ${S.available}`)}`,
    };
    for (const key of Object.keys(oracle) as (keyof typeof oracle)[]) {
      expect(produced[key]).toBe(oracle[key](S));
      readOnly(oracle[key](S));
    }
  });

  test('one shared literal and four composing call sites', () => {
    const shared = src('commercial-read-tail-r10-sources.ts');
    expect(shared.match(/SELECT /g)).toHaveLength(1);
    expect(shared).toContain('export function packageSourceProbe');
    expect(shared).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);

    const consumer = src('purchased-capability-sql.ts');
    expect(consumer).toContain('packageSourceProbe');
    expect(consumer.match(/packageSourceProbe\(/g)).toHaveLength(4); // 4 call sites
    expect(consumer).not.toContain('EXISTS(SELECT 1 FROM');
    expect(consumer.match(/EXISTS\(SELECT 1 FROM/g) ?? []).toHaveLength(0);
  });

  test('r1/r3 retired; r4-r9 helpers, source locks and native statement kinds stay untouched', () => {
    expect(() => src('commercial-read-tail-r1-sources.ts')).toThrow();
    expect(() => src('commercial-read-tail-r3-sources.ts')).toThrow();
    expect(src('commercial-read-tail-r4-sources.ts')).toContain('purchaseFactAgg');
    expect(src('commercial-read-tail-r5-sources.ts')).toContain('receiptProbe');
    expect(src('commercial-read-tail-r6-sources.ts')).toContain('purchaseObservationAgg');
    expect(src('commercial-read-tail-r7-sources.ts')).toContain('holdingMonthReadSql');
    expect(src('commercial-read-tail-r8-sources.ts')).toContain('companyScopedStarSql');
    expect(src('commercial-read-tail-r9-sources.ts')).toContain('holdingSnapshotReadSql');

    const pg = src('package-grants.ts');
    expect(pg).toContain('SELECT pg_advisory_xact_lock(hashtextextended(?,0))');
    expect(pg).toContain('SELECT version FROM company_package_revisions WHERE company_id=? FOR UPDATE');
    const pcb = src('purchase-command-boundary.ts');
    expect(pcb).toContain('SELECT pg_advisory_xact_lock(hashtextextended(?,0))');
    expect(pcb).toContain("SELECT pg_advisory_xact_lock(hashtextextended('commercial.package:company:'||company_id,0))");
    expect(pcb).toContain('FOR SHARE');
    expect(pcb).toContain("nativeOwnerStatement('purchase.assertion'");
  });

  test('retirement:ts-statements is strictly lower than BEFORE=28', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(28);
    expect(total).toBe(11);
  });
});

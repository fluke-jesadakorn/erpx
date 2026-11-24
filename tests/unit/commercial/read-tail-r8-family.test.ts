import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { companyScopedStarSql } from '../../../packages/server-commercial/src/adapters/commercial-read-tail-r8-sources';

const srcRoot = resolve(__dirname, '../../../packages/server-commercial/src');
const adapters = (name: string) => readFileSync(
  new URL(`../../../packages/server-commercial/src/adapters/${name}`, import.meta.url), 'utf8',
);

// Pre-dedupe literals copied verbatim from HEAD; byte-identity oracles only.
const HISTORY_RIGHTS = 'SELECT * FROM company_package_history_rights WHERE company_id=? ORDER BY package_id,package_version';
const CURRENT_GRANT = 'SELECT * FROM company_package_grants WHERE company_id=? AND package_id=?';
const grantsOrder = (lock: string) => `SELECT * FROM company_package_grants WHERE company_id=? ORDER BY package_id${lock}`;

describe('Commercial read-tail r8 parameterized Company-scoped star-read family', () => {
  test('companyScopedStarSql reproduces every original byte-identically', () => {
    expect(companyScopedStarSql('company_package_history_rights', ' ORDER BY package_id,package_version')).toBe(HISTORY_RIGHTS);
    expect(companyScopedStarSql('company_package_grants', ' AND package_id=?')).toBe(CURRENT_GRANT);
    expect(companyScopedStarSql('company_package_grants', ' ORDER BY package_id')).toBe(grantsOrder(''));
    expect(companyScopedStarSql('company_package_grants', ' ORDER BY package_id FOR SHARE')).toBe(grantsOrder(' FOR SHARE'));
  });

  test('former sites compose the builder and keep their own prepare/bind', () => {
    const pg = adapters('package-grants.ts');
    expect(pg).toContain('companyScopedStarSql');
    expect(pg).not.toContain(HISTORY_RIGHTS);
    expect(pg).not.toContain(CURRENT_GRANT);
    expect(pg).not.toContain('SELECT * FROM company_package_grants WHERE company_id=? ORDER BY package_id');
    expect(pg.match(/companyScopedStarSql\(/g)).toHaveLength(3); // 3 call sites
    expect(pg).toContain('.bind(companyId, packageId).first<Row>()');
    expect(pg).toContain('ORDER BY package_id,package_version');
  });

  test('r1/r3/r4/r5/r6/r7 helpers and advisory/row locks stay untouched', () => {
    expect(adapters('commercial-read-tail-r1-sources.ts')).toContain('commercialChangeSetReceiptSql');
    expect(adapters('commercial-read-tail-r3-sources.ts')).toContain('seatHistorySql');
    expect(adapters('commercial-read-tail-r4-sources.ts')).toContain('purchaseFactAgg');
    expect(adapters('commercial-read-tail-r4-sources.ts')).toContain('coverageProbe');
    expect(adapters('commercial-read-tail-r5-sources.ts')).toContain('receiptProbe');
    expect(adapters('commercial-read-tail-r6-sources.ts')).toContain('snapshotByMonthSql');
    expect(adapters('commercial-read-tail-r7-sources.ts')).toContain('holdingMonthReadSql');
    const pg = adapters('package-grants.ts');
    expect(pg).toContain('SELECT pg_advisory_xact_lock(hashtextextended(?,0))');
    expect(pg).toContain('SELECT version FROM company_package_revisions WHERE company_id=? FOR UPDATE');
    expect(pg).toContain('forShare ? " FOR SHARE" : ""');
    const pcb = adapters('purchase-command-boundary.ts');
    expect(pcb).toContain('SELECT pg_advisory_xact_lock(hashtextextended(?,0))');
    expect(pcb).toContain('SELECT company_id,package_id FROM company_package_grants ORDER BY company_id,package_id FOR SHARE');
  });

  test('shared module declares the read literal exactly once', () => {
    const shared = adapters('commercial-read-tail-r8-sources.ts');
    expect(shared.match(/SELECT /g)).toHaveLength(1);
  });

  test('retirement:ts-statements stays strictly below the r8 BEFORE=31 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBe(23);
    expect(total).toBeLessThan(31);
  });
});

import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import { legalPurchaseClockExistsSql } from '../../../packages/server-legal/src/adapters/legal-read-tail-r29-sources';
import { legalPurchaseClockCteSql } from '../../../packages/server-legal/src/adapters/legal-read-tail-r21-sources';

const srcRoot = resolve(__dirname, '../../../packages/server-legal/src');
const src = (name: string) => readFile(new URL(`../../../packages/server-legal/src/adapters/${name}`, import.meta.url), 'utf8');

describe('legal read-tail r29 parameterized family dedupe', () => {
  it('renders byte-identical purchase-clock EXISTS wrappers for all three callers', () => {
    expect(legalPurchaseClockExistsSql('legal_purchase_clock', 'P')).toBe(
      `EXISTS(WITH legal_purchase_clock AS MATERIALIZED ${legalPurchaseClockCteSql} SELECT 1 FROM legal_purchase_clock WHERE P)`);
    expect(legalPurchaseClockExistsSql('legal_portal_purchase_clock', 'P')).toBe(
      `EXISTS(WITH legal_portal_purchase_clock AS MATERIALIZED ${legalPurchaseClockCteSql} SELECT 1 FROM legal_portal_purchase_clock WHERE P)`);
    expect(legalPurchaseClockExistsSql('legal_queued_clock', 'P', '')).toBe(
      `EXISTS(WITH legal_queued_clock AS MATERIALIZED${legalPurchaseClockCteSql} SELECT 1 FROM legal_queued_clock WHERE P)`);
  });

  it('keeps the fragment read-only (no writes or row locks)', () => {
    const all = ['legal_purchase_clock', 'legal_portal_purchase_clock', 'legal_queued_clock']
      .map(c => legalPurchaseClockExistsSql(c, 'P')).join(' ');
    expect(all).toContain('SELECT 1 FROM');
    expect(all).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
  });

  it('keeps one definition and no inline wrapper literals in the three callers', async () => {
    const [shared, purchases, portalPurchases, queuedPurchases] = await Promise.all([
      src('legal-read-tail-r29-sources.ts'), src('purchases.ts'),
      src('portal-purchases.ts'), src('queued-purchases.ts'),
    ]);
    expect(shared.match(/export const legalPurchaseClockExistsSql/g) ?? []).toHaveLength(1);
    expect(purchases).toContain("legalPurchaseClockExistsSql('legal_purchase_clock',predicate)");
    expect(portalPurchases).toContain("legalPurchaseClockExistsSql('legal_portal_purchase_clock',");
    expect(queuedPurchases).toContain("legalPurchaseClockExistsSql('legal_queued_clock',");
    for (const file of [purchases, portalPurchases, queuedPurchases]) {
      expect(file).not.toMatch(/EXISTS\(WITH legal_\w+ AS MATERIALIZED/);
      expect(file).not.toContain('legalPurchaseClockCteSql');
    }
  });

  it('leaves corpus/citation, locks and the r9/r16-r28 families untouched', async () => {
    const names = [
      'legal-read-tail-r9-sources.ts', 'legal-read-tail-r15-sources.ts', 'legal-read-tail-r16-sources.ts',
      'legal-read-tail-r18-sources.ts', 'legal-read-tail-r19-sources.ts', 'legal-read-tail-r20-sources.ts',
      'legal-read-tail-r21-sources.ts', 'legal-read-tail-r22-sources.ts', 'legal-read-tail-r23-sources.ts',
      'legal-read-tail-r24-sources.ts', 'legal-read-tail-r25-sources.ts', 'legal-read-tail-r26-sources.ts',
      'legal-read-tail-r27-sources.ts', 'legal-read-tail-r28-sources.ts', 'legal/law-corpus.ts', 'legal/citations.ts',
      'legal/access.ts', 'legal/destruction-worker.ts',
    ];
    const texts = await Promise.all(names.map(src));
    const symbols = [
      'legalDocumentsItemPairSql', 'legalLaterApprovalGuardSql', 'legalItemSelectSql',
      'legalRetentionPolicySelectSql', 'legalRateCountPrefix', 'legalDisposedDocumentPrefix',
      'legalPurchaseClockCteSql', 'legalApprovedCycleSql', 'legalHoldActiveSql',
      'legalHoldTargetGuardPrefix', 'legalJsonbAggByItemSql', 'legalVersionExistsSql',
      'legalByIdAndItemSql', 'legalByIdAndCompanySql',
    ];
    for (const symbol of symbols) {
      expect(texts.slice(0, 14).join('\n')).toContain(`export const ${symbol}`);
    }
    expect(texts[14]).toContain('legal_law_source_checks');
    expect(texts[15]).toContain('export function legalCitation');
    expect(texts[16]).toContain('legalCompanyAdvisoryLockSql');
    expect(texts[17]).toContain('FOR UPDATE OF x,i');
  });

  it('retirement:ts-statements stays strictly below the recorded BEFORE=245', async () => {
    let total = 0;
    const walk = async (dir: string) => {
      for (const entry of await readdir(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) await walk(p);
        else if (entry.name.endsWith('.ts')) total += ((await readFile(p, 'utf8')).match(/SELECT/g) ?? []).length;
      }
    };
    await walk(srcRoot);
    expect(total).toBeLessThan(245);
    expect(total).toBeLessThanOrEqual(244);
  });
});

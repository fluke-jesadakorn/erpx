import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import { legalByIdAndCompanySql } from '../../../packages/server-legal/src/adapters/legal-read-tail-r28-sources';

const srcRoot = resolve(__dirname, '../../../packages/server-legal/src');
const src = (name: string) => readFile(new URL(`../../../packages/server-legal/src/adapters/${name}`, import.meta.url), 'utf8');
const byIdAndCompany = (table: string) => `SELECT * FROM ${table} WHERE id=? AND company_id=?`;

const tables = ['legal_intake_forms', 'legal_trust_material'];

describe('legal read-tail r28 parameterized family dedupe', () => {
  it('renders byte-identical company-scoped point-read SQL for each legal fact table', () => {
    for (const table of tables) {
      expect(legalByIdAndCompanySql(table)).toBe(byIdAndCompany(table));
    }
  });

  it('keeps the fragment read-only (no writes or row locks)', () => {
    const all = tables.map(legalByIdAndCompanySql).join(' ');
    expect(all).toMatch(/SELECT \* FROM legal_/);
    expect(all).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
  });

  it('keeps one definition and no inline family literals in the two callers', async () => {
    const [shared, governance, trust] = await Promise.all([
      src('legal-read-tail-r28-sources.ts'),
      src('legal/governance.ts'),
      src('legal/trust.ts'),
    ]);
    expect(shared.match(/export const legalByIdAndCompanySql/g) ?? []).toHaveLength(1);
    expect(governance).toContain('legalByIdAndCompanySql("legal_intake_forms")');
    expect(trust).toContain('legalByIdAndCompanySql("legal_trust_material")');
    for (const file of [governance, trust]) {
      expect(file).not.toMatch(/SELECT \* FROM legal_\w+ WHERE id=\? AND company_id=\?/);
    }
  });

  it('leaves corpus/citation, locks and the r9/r15-r27 families untouched', async () => {
    const names = [
      'legal-read-tail-r9-sources.ts', 'legal-read-tail-r15-sources.ts', 'legal-read-tail-r16-sources.ts',
      'legal-read-tail-r18-sources.ts', 'legal-read-tail-r19-sources.ts', 'legal-read-tail-r20-sources.ts',
      'legal-read-tail-r21-sources.ts', 'legal-read-tail-r22-sources.ts', 'legal-read-tail-r23-sources.ts',
      'legal-read-tail-r24-sources.ts', 'legal-read-tail-r25-sources.ts', 'legal-read-tail-r26-sources.ts',
      'legal-read-tail-r27-sources.ts', 'legal/law-corpus.ts', 'legal/citations.ts', 'legal/access.ts',
    ];
    const texts = await Promise.all(names.map(src));
    const symbols = [
      'legalDocumentsItemPairSql', 'legalLaterApprovalGuardSql', 'legalItemSelectSql',
      'legalRetentionPolicySelectSql', 'legalRateCountPrefix', 'legalDisposedDocumentPrefix',
      'legalPurchaseClockCteSql', 'legalApprovedCycleSql', 'legalHoldActiveSql',
      'legalHoldTargetGuardPrefix', 'legalJsonbAggByItemSql', 'legalVersionExistsSql',
      'legalByIdAndItemSql',
    ];
    for (const symbol of symbols) {
      expect(texts.slice(0, 13).join('\n')).toContain(`export const ${symbol}`);
    }
    expect(texts[13]).toContain('legal_law_source_checks');
    expect(texts[14]).toContain('export function legalCitation');
    expect(texts[15]).toContain('legalCompanyAdvisoryLockSql');
  });

  it('retirement:ts-statements stays strictly below the recorded 246 r27 baseline', async () => {
    let total = 0;
    const walk = async (dir: string) => {
      for (const entry of await readdir(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) await walk(p);
        else if (entry.name.endsWith('.ts')) total += ((await readFile(p, 'utf8')).match(/SELECT/g) ?? []).length;
      }
    };
    await walk(srcRoot);
    expect(total).toBeLessThan(246);
    expect(total).toBeLessThanOrEqual(245);
  });
});

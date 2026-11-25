import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import { legalOrderedHistorySql } from '../../../packages/server-legal/src/adapters/legal-read-tail-r30-sources';

const srcRoot = resolve(__dirname, '../../../packages/server-legal/src');
const src = (name: string) => readFile(new URL(`../../../packages/server-legal/src/adapters/${name}`, import.meta.url), 'utf8');
const ordered = (table: string, key: string) => `SELECT * FROM ${table} WHERE ${key}=? ORDER BY created_at,id`;

const members: [string, string][] = [
  ['legal_executed_reviews', 'executed_copy_id'],
  ['legal_feature_history', 'item_id'],
];

describe('legal read-tail r30 parameterized family dedupe', () => {
  it('renders byte-identical ordered-history list SQL for both legal fact tables', () => {
    for (const [table, key] of members) {
      expect(legalOrderedHistorySql(table, key)).toBe(ordered(table, key));
    }
  });

  it('keeps the fragment read-only (no writes or row locks)', () => {
    const all = members.map(([t, k]) => legalOrderedHistorySql(t, k)).join(' ');
    expect(all).toMatch(/SELECT \* FROM legal_\w+ WHERE \w+=\? ORDER BY created_at,id/);
    expect(all).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
  });

  it('keeps one definition and no inline family literals in the two callers', async () => {
    const [shared, executedCopies, operations] = await Promise.all([
      src('legal-read-tail-r30-sources.ts'),
      src('legal/executed-copies.ts'),
      src('legal/operations.ts'),
    ]);
    expect(shared.match(/export const legalOrderedHistorySql/g) ?? []).toHaveLength(1);
    // Lane 26 moved the executed-copy ordered-history read to the native owner;
    // the TypeScript caller is now a strict transport only.
    expect(executedCopies).toContain("from '../native-lane-26/client'");
    expect(executedCopies).not.toContain('legalOrderedHistorySql');
    expect(operations).toContain('legalOrderedHistorySql("legal_feature_history","item_id")');
    for (const file of [executedCopies, operations]) {
      expect(file).not.toMatch(/SELECT \* FROM legal_\w+ WHERE \w+=\? ORDER BY created_at,id"/);
    }
  });

  it('leaves corpus/citation, locks and the r9/r15-r29 families untouched', async () => {
    const names = [
      'legal-read-tail-r9-sources.ts', 'legal-read-tail-r15-sources.ts', 'legal-read-tail-r16-sources.ts',
      'legal-read-tail-r18-sources.ts', 'legal-read-tail-r19-sources.ts', 'legal-read-tail-r20-sources.ts',
      'legal-read-tail-r21-sources.ts', 'legal-read-tail-r22-sources.ts', 'legal-read-tail-r23-sources.ts',
      'legal-read-tail-r24-sources.ts', 'legal-read-tail-r25-sources.ts', 'legal-read-tail-r26-sources.ts',
      'legal-read-tail-r27-sources.ts', 'legal-read-tail-r28-sources.ts', 'legal-read-tail-r29-sources.ts',
      'legal/law-corpus.ts', 'legal/citations.ts', 'legal/access.ts', 'legal/destruction-worker.ts',
    ];
    const texts = await Promise.all(names.map(src));
    const symbols = [
      'legalDocumentsItemPairSql', 'legalLaterApprovalGuardSql', 'legalItemSelectSql',
      'legalRetentionPolicySelectSql', 'legalRateCountPrefix', 'legalDisposedDocumentPrefix',
      'legalPurchaseClockCteSql', 'legalPurchaseClockExistsSql', 'legalApprovedCycleSql',
      'legalHoldActiveSql', 'legalHoldTargetGuardPrefix', 'legalJsonbAggByItemSql', 'legalVersionExistsSql',
      'legalByIdAndItemSql', 'legalByIdAndCompanySql', 'legalMemberEditExistsSql', 'legalEventsByItemSql',
    ];
    for (const symbol of symbols) {
      expect(texts.slice(0, 15).join('\n')).toContain(`export const ${symbol}`);
    }
    expect(texts[15]).toContain("nativeLegalAuxiliaryRead<LawCorpusView>(a,'laws',ruleId)");
    expect(texts[16]).toContain('export function legalCitation');
    expect(texts[17]).toContain('legalCompanyAdvisoryLockSql');
    expect(texts[18]).toContain('FOR UPDATE OF x,i');
  });

  it('retirement:ts-statements stays strictly below the recorded BEFORE=243', async () => {
    let total = 0;
    const walk = async (dir: string) => {
      for (const entry of await readdir(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) await walk(p);
        else if (entry.name.endsWith('.ts')) total += ((await readFile(p, 'utf8')).match(/SELECT/g) ?? []).length;
      }
    };
    await walk(srcRoot);
    expect(total).toBeLessThan(243);
    expect(total).toBeLessThanOrEqual(242);
  });
});

import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import { legalPointByKeySql } from '../../../packages/server-legal/src/adapters/legal-read-tail-r36-sources';

const srcRoot = resolve(__dirname, '../../../packages/server-legal/src');
const src = (name: string) => readFile(new URL(`../../../packages/server-legal/src/adapters/${name}`, import.meta.url), 'utf8');
const point = (columns: string, table: string, key: string) => `SELECT ${columns} FROM ${table} WHERE ${key}=?`;

const members: [string, string, 'document_id' | 'id'][] = [
  ['1', 'legal_executed_copies', 'document_id'],
  ['kind', 'legal_items', 'id'],
];

describe('legal read-tail r36 arbitrary-key point-read family', () => {
  it('renders byte-identical SQL for every member', () => {
    for (const [columns, table, key] of members) {
      expect(legalPointByKeySql(columns, table, key)).toBe(point(columns, table, key));
    }
  });

  it('keeps the fragment read-only (no writes or row locks)', () => {
    const all = members.map(([c, t, k]) => legalPointByKeySql(c, t, k)).join(' ');
    expect(all).toMatch(/SELECT [\w,*]+ FROM legal_\w+ WHERE (?:document_id|id)=\?/);
    expect(all).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
  });

  it('keeps one definition and removes every inline family literal from the callers', async () => {
    const [shared, approval, recordPurchases] = await Promise.all([
      src('legal-read-tail-r36-sources.ts'),
      src('legal/approval.ts'),
      src('record-purchases.ts'),
    ]);
    expect(shared.match(/export const legalPointByKeySql/g) ?? []).toHaveLength(1);
    expect(approval).toContain('legalPointByKeySql("1","legal_executed_copies","document_id")');
    expect(recordPurchases).toContain('legalPointByKeySql("kind","legal_items","id")');
    expect(approval).not.toContain('"SELECT 1 FROM legal_executed_copies WHERE document_id=?"');
    expect(recordPurchases).not.toContain("'SELECT kind FROM legal_items WHERE id=?'");
  });

  it('leaves the corpus/citation/lock-bearing reads intact', async () => {
    const [corpus, citations, access, destructionWorker] = await Promise.all([
      src('legal/law-corpus.ts'), src('legal/citations.ts'), src('legal/access.ts'), src('legal/destruction-worker.ts'),
    ]);
    expect(corpus).toContain('legal_law_source_checks');
    expect(citations).toContain('export function legalCitation');
    expect(access).toContain('legalCompanyAdvisoryLockSql');
    expect(destructionWorker).toContain('FOR UPDATE OF x,i');
  });

  it('leaves r32 legalPointByIdSql and the r9/r15/r16/r18-r35 families untouched', async () => {
    const names = [
      'legal-read-tail-r9-sources.ts',
      'legal-read-tail-r15-sources.ts',
      'legal-read-tail-r16-sources.ts',
      'legal-read-tail-r18-sources.ts',
      'legal-read-tail-r19-sources.ts',
      'legal-read-tail-r20-sources.ts',
      'legal-read-tail-r21-sources.ts',
      'legal-read-tail-r22-sources.ts',
      'legal-read-tail-r23-sources.ts',
      'legal-read-tail-r24-sources.ts',
      'legal-read-tail-r25-sources.ts',
      'legal-read-tail-r26-sources.ts',
      'legal-read-tail-r27-sources.ts',
      'legal-read-tail-r28-sources.ts',
      'legal-read-tail-r29-sources.ts',
      'legal-read-tail-r30-sources.ts',
      'legal-read-tail-r31-sources.ts',
      'legal-read-tail-r32-sources.ts',
      'legal-read-tail-r33-sources.ts',
      'legal-read-tail-r34-sources.ts',
      'legal-read-tail-r35-sources.ts',
    ];
    const texts = await Promise.all(names.map(src));
    const symbols = [
      'legalSettingsEnabledSql', 'legalManagerAvailableSql', 'legalDestructionActiveSql',
      'legalDocumentsItemPairSql', 'legalDocumentByIdSql', 'legalMembersAggSql',
      'legalFilesAdvisoryLockSql', 'legalCompanyAdvisoryLockSql',
      'legalLaterApprovalGuardSql',
      'legalItemSelectSql',
      'legalRetentionPolicySelectSql', 'legalActorScopeProjectionSql', 'legalEventsByItemSql',
      'legalRateCountPrefix',
      'legalDisposedDocumentPrefix',
      'legalPurchaseClockCteSql',
      'legalApprovedCycleSql',
      'legalHoldActiveSql',
      'legalHoldTargetGuardPrefix',
      'legalMemberEditExistsSql', 'legalJsonbAggByItemSql',
      'legalVersionExistsSql',
      'legalByIdAndItemSql',
      'legalByIdAndCompanySql',
      'legalPurchaseClockExistsSql',
      'legalOrderedHistorySql',
      'legalRecentItemListSql',
      'legalPointByIdSql',
      'legalStatePointSql',
      'legalPointByIdAndKeySql',
      'legalItemOrderedSql',
    ];
    for (const symbol of symbols) {
      expect(texts.join('\n')).toContain(`export const ${symbol}`);
    }
  });

  it('retirement:ts-statements stays strictly below the recorded BEFORE=231', async () => {
    let total = 0;
    const walk = async (dir: string) => {
      for (const entry of await readdir(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) await walk(p);
        else if (entry.name.endsWith('.ts')) total += ((await readFile(p, 'utf8')).match(/SELECT /g) ?? []).length;
      }
    };
    await walk(srcRoot);
    expect(total).toBeLessThan(231);
    expect(total).toBeLessThanOrEqual(226);
  });
});

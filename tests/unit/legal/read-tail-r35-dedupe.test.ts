import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import { legalItemOrderedSql } from '../../../modules/legal/server/src/adapters/legal-read-tail-r35-sources';

const srcRoot = resolve(__dirname, '../../../modules/legal/server/src');
const src = (name: string) => readFile(new URL(`../../../modules/legal/server/src/adapters/${name}`, import.meta.url), 'utf8');
const ordered = (columns: string, table: string, order: string, limit?: number) =>
  `SELECT ${columns} FROM ${table} WHERE item_id=? ORDER BY ${order}${limit === undefined ? '' : ` LIMIT ${limit}`}`;

const members: [string, string, string, number?][] = [
  ['id,version,name,checksum,bytes,media_type,locked,created_at', 'legal_documents', 'version'],
  ['id,state,manifest,requested_by,approved_by,approved_at,completed_at', 'legal_destructions', 'created_at'],
  ['*', 'legal_documents', 'version DESC', 100],
];

describe('legal read-tail r35 item-scoped ordered-list family', () => {
  it('renders byte-identical SQL for every member', () => {
    for (const [columns, table, order, limit] of members) {
      expect(legalItemOrderedSql(columns, table, order, limit)).toBe(ordered(columns, table, order, limit));
    }
  });

  it('keeps the fragment read-only (no writes or row locks)', () => {
    const all = members.map(([c, t, o, l]) => legalItemOrderedSql(c, t, o, l)).join(' ');
    expect(all).toMatch(/SELECT [\w,*]+ FROM legal_\w+ WHERE item_id=\? ORDER BY [\w ]+/);
    expect(all).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
  });

  it('keeps one definition and removes every inline family literal from the callers', async () => {
    const [shared, operations, service] = await Promise.all([
      src('legal-read-tail-r35-sources.ts'),
      src('legal/operations.ts'),
      src('legal/service.ts'),
    ]);
    expect(shared.match(/export const legalItemOrderedSql/g) ?? []).toHaveLength(1);
    expect(operations).toContain('legalItemOrderedSql("id,version,name,checksum,bytes,media_type,locked,created_at","legal_documents","version")');
    expect(operations).toContain('legalItemOrderedSql("id,state,manifest,requested_by,approved_by,approved_at,completed_at","legal_destructions","created_at")');
    expect(service).toContain('legalItemOrderedSql("*","legal_documents","version DESC",100)');
    for (const [c, t, o, l] of members) {
      const inline = `SELECT ${c} FROM ${t} WHERE item_id=? ORDER BY ${o}${l === undefined ? '' : ` LIMIT ${l}`}`;
      expect(operations).not.toContain(inline);
      expect(service).not.toContain(inline);
    }
  });

  it('leaves the corpus/citation/lock-bearing reads intact', async () => {
    const [corpus, citations, access, destructionWorker] = await Promise.all([
      src('legal/law-corpus.ts'), src('legal/citations.ts'), src('legal/access.ts'), src('legal/destruction-worker.ts'),
    ]);
    expect(corpus).toContain("nativeLegalAuxiliaryRead<LawCorpusView>(a,'laws',ruleId)");
    expect(citations).toContain('export function legalCitation');
    expect(access).toContain('legalCompanyAdvisoryLockSql');
    expect(destructionWorker).toContain('FOR UPDATE OF x,i');
  });

  it('leaves the r9/r15/r16/r18-r34 families untouched', async () => {
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
    ];
    for (const symbol of symbols) {
      expect(texts.slice(0, 19).join('\n')).toContain(`export const ${symbol}`);
    }
    expect(texts[19]).toContain('export const legalPointByIdAndKeySql');
  });

  it('retirement:ts-statements stays strictly below the recorded BEFORE=233', async () => {
    let total = 0;
    const walk = async (dir: string) => {
      for (const entry of await readdir(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) await walk(p);
        else if (entry.name.endsWith('.ts')) total += ((await readFile(p, 'utf8')).match(/SELECT /g) ?? []).length;
      }
    };
    await walk(srcRoot);
    expect(total).toBeLessThan(233);
    expect(total).toBeLessThanOrEqual(231);
  });
});

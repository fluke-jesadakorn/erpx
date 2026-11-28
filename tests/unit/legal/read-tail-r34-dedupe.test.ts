import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import { legalPointByIdAndKeySql } from '../../../modules/legal/server/src/adapters/legal-read-tail-r34-sources';

const srcRoot = resolve(__dirname, '../../../modules/legal/server/src');
const src = (name: string) => readFile(new URL(`../../../modules/legal/server/src/adapters/${name}`, import.meta.url), 'utf8');
const point = (columns: string, table: string, key2: string) =>
  `SELECT ${columns} FROM ${table} WHERE id=? AND ${key2}=?`;

const members: [string, string, 'user_id' | 'company_id' | 'item_id'][] = [
  ['1', 'legal_portal_receipts', 'user_id'],
  ['request_hash,result_json', 'legal_portal_receipts', 'user_id'],
  ['id,item_id,document_id,state,result_json,error,attempts', 'legal_processing_jobs', 'company_id'],
  ['owner_id', 'legal_counsel_entries', 'item_id'],
];

describe('legal read-tail r34 id+key point-read family', () => {
  it('renders byte-identical SQL for every member', () => {
    for (const [columns, table, key2] of members) {
      expect(legalPointByIdAndKeySql(columns, table, key2)).toBe(point(columns, table, key2));
    }
  });

  it('keeps the fragment read-only (no writes or row locks)', () => {
    const all = members.map(([c, t, k]) => legalPointByIdAndKeySql(c, t, k)).join(' ');
    expect(all).toMatch(/SELECT [\w,*]+ FROM legal_\w+ WHERE id=\? AND \w+=\?/);
    expect(all).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
  });

  it('keeps one definition and removes every inline family literal from the callers', async () => {
    const [shared, purchases, portal, processing, features] = await Promise.all([
      src('legal-read-tail-r34-sources.ts'),
      src('portal-purchases.ts'),
      src('legal/portal.ts'),
      src('legal/processing.ts'),
      src('legal/features.ts'),
    ]);
    expect(shared.match(/export const legalPointByIdAndKeySql/g) ?? []).toHaveLength(1);
    expect(purchases).toContain('legalPointByIdAndKeySql("1","legal_portal_receipts","user_id")');
    expect(portal).toContain('legalPointByIdAndKeySql("request_hash,result_json","legal_portal_receipts","user_id")');
    expect(processing).toContain('legalPointByIdAndKeySql("id,item_id,document_id,state,result_json,error,attempts","legal_processing_jobs","company_id")');
    expect(features).toContain('legalPointByIdAndKeySql("owner_id",spec.table,"item_id")');
    for (const [c, t, k] of members) {
      const inline = `SELECT ${c} FROM ${t} WHERE id=? AND ${k}=?`;
      expect(purchases).not.toContain(inline);
      expect(portal).not.toContain(inline);
      expect(processing).not.toContain(inline);
      expect(features).not.toContain(inline);
    }
  });

  it('leaves the portal LOCK TABLE lock intact', async () => {
    const purchases = await src('portal-purchases.ts');
    expect(purchases).toContain('LOCK TABLE legal_settings,legal_destructions,legal_comments,legal_envelopes,legal_signers,legal_counsel_entries IN SHARE ROW EXCLUSIVE MODE');
  });

  it('leaves corpus/citation/locks and the r9/r15/r16/r18-r33 families untouched', async () => {
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
      'legal/law-corpus.ts', 'legal/citations.ts', 'legal/access.ts', 'legal/destruction-worker.ts',
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
    expect(texts[19]).toContain("nativeLegalAuxiliaryRead<LawCorpusView>(a,'laws',ruleId)");
    expect(texts[20]).toContain('export function legalCitation');
    expect(texts[21]).toContain('legalCompanyAdvisoryLockSql');
    expect(texts[22]).toContain('FOR UPDATE OF x,i');
  });

  it('retirement:ts-statements stays strictly below the recorded BEFORE=236', async () => {
    let total = 0;
    const walk = async (dir: string) => {
      for (const entry of await readdir(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) await walk(p);
        else if (entry.name.endsWith('.ts')) total += ((await readFile(p, 'utf8')).match(/SELECT /g) ?? []).length;
      }
    };
    await walk(srcRoot);
    expect(total).toBeLessThan(236);
    expect(total).toBeLessThanOrEqual(231);
  });
});

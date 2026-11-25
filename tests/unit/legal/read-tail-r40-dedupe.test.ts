import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import { legalExistsPointGuardHeadSql } from '../../../packages/server-legal/src/adapters/legal-read-tail-r40-sources';

const srcRoot = resolve(__dirname, '../../../packages/server-legal/src');
const src = (name: string) => readFile(new URL(`../../../packages/server-legal/src/adapters/${name}`, import.meta.url), 'utf8');

const members: [string, string, string, string][] = [
  ['legal_approvals', 'item_id', "cycle=? AND document_id=? AND document_hash=? AND decision='approved') AND ",
    "EXISTS(SELECT 1 FROM legal_approvals WHERE item_id=? AND cycle=? AND document_id=? AND document_hash=? AND decision='approved') AND "],
  ['legal_documents', 'id', 'item_id=?) AND NOT EXISTS(${legalDisposedDocumentPrefix}=?)',
    'EXISTS(SELECT 1 FROM legal_documents WHERE id=? AND item_id=?) AND NOT EXISTS(${legalDisposedDocumentPrefix}=?)'],
  ['legal_intake_forms', 'id', "version=? AND state='draft')",
    "EXISTS(SELECT 1 FROM legal_intake_forms WHERE id=? AND version=? AND state='draft')"],
  ['legal_envelopes', 'id', "state='sent' AND expires_at>?)",
    "EXISTS(SELECT 1 FROM legal_envelopes WHERE id=? AND state='sent' AND expires_at>?)"],
];

describe('legal read-tail r40 EXISTS point-guard head family', () => {
  it('renders byte-identical SQL for every member', () => {
    for (const [table, key, tail, oracle] of members) {
      expect(legalExistsPointGuardHeadSql(table, key) + tail).toBe(oracle);
    }
  });

  it('keeps the head read-only (no writes or row locks)', () => {
    const all = members.map(([t, k]) => legalExistsPointGuardHeadSql(t, k)).join(' ');
    expect(all).toMatch(/EXISTS\(SELECT 1 FROM legal_\w+ WHERE \w+=\? AND /);
    expect(all).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
  });

  it('keeps one definition and removes every inline family head from the callers', async () => {
    const [shared, documents, governance, signing] = await Promise.all([
      src('legal-read-tail-r40-sources.ts'),
      src('legal/documents.ts'), src('legal/governance.ts'), src('legal/signing.ts'),
    ]);
    expect(shared.match(/export const legalExistsPointGuardHeadSql/g) ?? []).toHaveLength(1);
    expect(documents).toContain("legalExistsPointGuardHeadSql('legal_approvals','item_id')");
    expect(documents).toContain("legalExistsPointGuardHeadSql('legal_documents','id')");
    expect(governance).toContain("legalExistsPointGuardHeadSql('legal_intake_forms','id')");
    expect(signing).toContain("legalExistsPointGuardHeadSql('legal_envelopes','id')");
    expect(documents).not.toContain("EXISTS(SELECT 1 FROM legal_approvals WHERE item_id=? AND cycle=?");
    expect(documents).not.toContain('EXISTS(SELECT 1 FROM legal_documents WHERE id=? AND item_id=?)');
    expect(governance).not.toContain("EXISTS(SELECT 1 FROM legal_intake_forms WHERE id=? AND version=?");
    expect(signing).not.toContain("EXISTS(SELECT 1 FROM legal_envelopes WHERE id=? AND state='sent'");
  });

  it('leaves the corpus/citation/lock-bearing reads intact', async () => {
    const [corpus, citations, r9, r24, destructionWorker] = await Promise.all([
      src('legal/law-corpus.ts'), src('legal/citations.ts'),
      src('legal-read-tail-r9-sources.ts'), src('legal-read-tail-r24-sources.ts'),
      src('legal/destruction-worker.ts'),
    ]);
    expect(corpus).toContain("nativeLegalAuxiliaryRead<LawCorpusView>(a,'laws',ruleId)");
    expect(citations).toContain('export function legalCitation');
    expect(r9).toContain('legalFilesAdvisoryLockSql');
    expect(r9).toContain('legalCompanyAdvisoryLockSql');
    expect(r24).toContain('legalHoldTargetGuardPrefix');
    expect(destructionWorker).toContain('FOR UPDATE OF x,i');
  });

  it('leaves r9/r15/r16/r18-r39 helpers untouched', async () => {
    const names = [
      'legal-read-tail-r9-sources.ts', 'legal-read-tail-r15-sources.ts', 'legal-read-tail-r16-sources.ts',
      'legal-read-tail-r18-sources.ts', 'legal-read-tail-r19-sources.ts', 'legal-read-tail-r20-sources.ts',
      'legal-read-tail-r21-sources.ts', 'legal-read-tail-r22-sources.ts', 'legal-read-tail-r23-sources.ts',
      'legal-read-tail-r24-sources.ts', 'legal-read-tail-r25-sources.ts', 'legal-read-tail-r26-sources.ts',
      'legal-read-tail-r27-sources.ts', 'legal-read-tail-r28-sources.ts', 'legal-read-tail-r29-sources.ts',
      'legal-read-tail-r30-sources.ts', 'legal-read-tail-r31-sources.ts', 'legal-read-tail-r32-sources.ts',
      'legal-read-tail-r33-sources.ts', 'legal-read-tail-r34-sources.ts', 'legal-read-tail-r35-sources.ts',
      'legal-read-tail-r36-sources.ts', 'legal-read-tail-r38-sources.ts', 'legal-read-tail-r39-sources.ts',
    ];
    const texts = await Promise.all(names.map(src));
    const symbols = [
      'legalSettingsEnabledSql', 'legalManagerAvailableSql', 'legalDestructionActiveSql',
      'legalDocumentsItemPairSql', 'legalDocumentByIdSql', 'legalMembersAggSql',
      'legalFilesAdvisoryLockSql', 'legalCompanyAdvisoryLockSql',
      'legalLaterApprovalGuardSql', 'legalItemSelectSql',
      'legalRetentionPolicySelectSql', 'legalActorScopeProjectionSql', 'legalEventsByItemSql',
      'legalRateCountPrefix', 'legalDisposedDocumentPrefix', 'legalPurchaseClockCteSql',
      'legalApprovedCycleSql', 'legalHoldActiveSql', 'legalHoldTargetGuardPrefix',
      'legalMemberEditExistsSql', 'legalJsonbAggByItemSql', 'legalVersionExistsSql',
      'legalByIdAndItemSql', 'legalByIdAndCompanySql', 'legalPurchaseClockExistsSql',
      'legalOrderedHistorySql', 'legalRecentItemListSql', 'legalPointByIdSql',
      'legalStatePointSql', 'legalPointByIdAndKeySql', 'legalItemOrderedSql', 'legalPointByKeySql',
      'legalReleasedItemGuardSql', 'legalMemberEditPointSql', 'legalFactDataSql', 'legalMetricCountPrefix',
      'legalDisposedGuardSql', 'legalHoldOwnTargetSql',
    ];
    for (const symbol of symbols) expect(texts.join('\n')).toContain(`export const ${symbol}`);
  });

  it('retirement:ts-statements stays strictly below the recorded BEFORE=224', async () => {
    let total = 0;
    const walk = async (dir: string) => {
      for (const entry of await readdir(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) await walk(p);
        else if (entry.name.endsWith('.ts')) total += ((await readFile(p, 'utf8')).match(/SELECT /g) ?? []).length;
      }
    };
    await walk(srcRoot);
    expect(total).toBeLessThan(224);
  });
});

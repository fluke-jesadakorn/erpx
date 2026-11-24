import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import {
  legalDisposedGuardSql,
  legalHoldOwnTargetSql,
} from '../../../packages/server-legal/src/adapters/legal-read-tail-r39-sources';

const srcRoot = resolve(__dirname, '../../../packages/server-legal/src');
const src = (name: string) => readFile(new URL(`../../../packages/server-legal/src/adapters/${name}`, import.meta.url), 'utf8');

describe('legal read-tail r39 fragment families', () => {
  it('renders byte-identical SQL for every member of each family', () => {
    expect(legalDisposedGuardSql('disposed', 'legal_documents.id'))
      .toBe('NOT EXISTS(SELECT 1 FROM legal_disposals disposed WHERE disposed.document_id=legal_documents.id)');
    expect(legalDisposedGuardSql('x', 'd.id'))
      .toBe('NOT EXISTS(SELECT 1 FROM legal_disposals x WHERE x.document_id=d.id)');
    expect(legalHoldOwnTargetSql('h'))
      .toBe('EXISTS(SELECT 1 FROM legal_hold_targets own WHERE own.hold_id=h.id AND own.item_id=?)');
    expect(legalHoldOwnTargetSql('hold'))
      .toBe('EXISTS(SELECT 1 FROM legal_hold_targets own WHERE own.hold_id=hold.id AND own.item_id=?)');
  });

  it('keeps every fragment read-only (no writes or row locks)', () => {
    const all = [
      legalDisposedGuardSql('disposed', 'legal_documents.id'),
      legalDisposedGuardSql('x', 'd.id'),
      legalHoldOwnTargetSql('h'),
      legalHoldOwnTargetSql('hold'),
    ].join(' ');
    expect(all).toMatch(/SELECT /);
    expect(all).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
  });

  it('keeps one definition and removes every inline family literal from the callers', async () => {
    const [shared, destruction, queued, service] = await Promise.all([
      src('legal-read-tail-r39-sources.ts'),
      src('legal/destruction-sources.ts'),
      src('queued-purchases.ts'),
      src('legal/service.ts'),
    ]);
    for (const symbol of ['legalDisposedGuardSql', 'legalHoldOwnTargetSql']) {
      expect(shared.match(new RegExp(`export const ${symbol}`, 'g')) ?? []).toHaveLength(1);
    }
    expect(destruction).toContain("legalDisposedGuardSql('disposed','legal_documents.id')");
    expect(queued).toContain("legalDisposedGuardSql('x','d.id')");
    expect(service).toContain("legalHoldOwnTargetSql('h')");
    expect(service).toContain("legalHoldOwnTargetSql('hold')");
    expect(destruction).not.toContain('NOT EXISTS(SELECT 1 FROM legal_disposals disposed');
    expect(queued).not.toContain('NOT EXISTS(SELECT 1 FROM legal_disposals x');
    expect(service).not.toContain('own.hold_id=h.id AND own.item_id=?');
    expect(service).not.toContain('own.hold_id=hold.id AND own.item_id=?');
  });

  it('leaves the corpus/citation/lock-bearing reads intact', async () => {
    const [corpus, citations, r9, r24, destructionWorker] = await Promise.all([
      src('legal/law-corpus.ts'),
      src('legal/citations.ts'),
      src('legal-read-tail-r9-sources.ts'),
      src('legal-read-tail-r24-sources.ts'),
      src('legal/destruction-worker.ts'),
    ]);
    expect(corpus).toContain('legal_law_source_checks');
    expect(corpus).toContain('NOT EXISTS(SELECT 1 FROM legal_disposals x WHERE x.document_id=c.document_id)');
    expect(citations).toContain('export function legalCitation');
    expect(r9).toContain('legalFilesAdvisoryLockSql');
    expect(r9).toContain('legalCompanyAdvisoryLockSql');
    expect(r24).toContain('legalHoldTargetGuardPrefix');
    expect(destructionWorker).toContain('FOR UPDATE OF x,i');
  });

  it('leaves r9/r15/r16/r18-r38 helpers untouched', async () => {
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
      'legal-read-tail-r36-sources.ts',
      'legal-read-tail-r38-sources.ts',
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
      'legalPointByKeySql',
      'legalReleasedItemGuardSql', 'legalMemberEditPointSql', 'legalFactDataSql', 'legalMetricCountPrefix',
    ];
    for (const symbol of symbols) {
      expect(texts.join('\n')).toContain(`export const ${symbol}`);
    }
  });

  it('retirement:ts-statements stays strictly below the recorded BEFORE=226', async () => {
    let total = 0;
    const walk = async (dir: string) => {
      for (const entry of await readdir(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) await walk(p);
        else if (entry.name.endsWith('.ts')) total += ((await readFile(p, 'utf8')).match(/SELECT /g) ?? []).length;
      }
    };
    await walk(srcRoot);
    expect(total).toBeLessThan(226);
  });
});

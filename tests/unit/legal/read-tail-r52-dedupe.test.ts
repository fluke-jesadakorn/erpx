import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import { legalDestructionGuardSql } from '../../../packages/server-legal/src/adapters/legal-read-tail-r52-sources';
import { legalPointByKeySql } from '../../../packages/server-legal/src/adapters/legal-read-tail-r36-sources';
import { legalItemOrderedSql } from '../../../packages/server-legal/src/adapters/legal-read-tail-r35-sources';
import { legalPointByIdSql } from '../../../packages/server-legal/src/adapters/legal-read-tail-r32-sources';
import { legalByIdAndCompanySql } from '../../../packages/server-legal/src/adapters/legal-read-tail-r28-sources';
import {
  legalDocumentByIdSql,
  legalDestructionActiveSql,
} from '../../../packages/server-legal/src/adapters/legal-read-tail-r9-sources';
import {
  legalRetentionPolicySelectSql,
  legalEventsByItemSql,
} from '../../../packages/server-legal/src/adapters/legal-read-tail-r18-sources';

const srcRoot = resolve(__dirname, '../../../packages/server-legal/src');
const src = (name: string) => readFile(new URL(`../../../packages/server-legal/src/adapters/${name}`, import.meta.url), 'utf8');

/** Independent pre-r52 oracles: the exact inline SQL captured before delegation. */
const accessesSql = 'SELECT * FROM legal_access_events WHERE item_id=? ORDER BY created_at,id LIMIT 10001';
const sharesSql = 'SELECT id,email,document_id,user_id,expires_at,revoked_at FROM legal_shares WHERE item_id=? ORDER BY created_at DESC LIMIT 100';
const eventsSql = 'SELECT * FROM legal_events WHERE item_id=? ORDER BY created_at';
const documentByIdSql = 'SELECT * FROM legal_documents WHERE id=?';
const retentionPolicySql = 'SELECT * FROM legal_retention_policies WHERE id=? AND company_id=?';
const membersSql = 'SELECT user_id,access FROM legal_members WHERE item_id=?';
const destroyingSql = "SELECT 1 FROM legal_destructions WHERE item_id=? AND state='executing'";
const activeSql = "SELECT 1 FROM legal_destructions WHERE item_id=? AND state IN ('executing','completed')";

describe('legal read-tail r52 ordered, point and destruction guard delegation', () => {
  it('renders byte-identical SQL for every delegated head', () => {
    expect(legalItemOrderedSql('*', 'legal_access_events', 'created_at,id', 10001)).toBe(accessesSql);
    expect(legalItemOrderedSql('id,email,document_id,user_id,expires_at,revoked_at', 'legal_shares', 'created_at DESC', 100)).toBe(sharesSql);
    expect(legalEventsByItemSql).toBe(eventsSql);
    expect(legalPointByIdSql('*', 'legal_documents')).toBe(documentByIdSql);
    expect(legalDocumentByIdSql).toBe(documentByIdSql);
    expect(legalByIdAndCompanySql('legal_retention_policies')).toBe(retentionPolicySql);
    expect(legalRetentionPolicySelectSql).toBe(retentionPolicySql);
    expect(legalPointByKeySql('user_id,access', 'legal_members', 'item_id')).toBe(membersSql);
    expect(legalDestructionGuardSql('executing')).toBe(destroyingSql);
    expect(legalDestructionGuardSql('active')).toBe(activeSql);
    expect(legalDestructionActiveSql).toBe(activeSql);
  });

  it('keeps legalEventsByItemSql bytes so both appending callers stay exact', () => {
    expect(`${legalEventsByItemSql},id`).toBe('SELECT * FROM legal_events WHERE item_id=? ORDER BY created_at,id');
    expect(`${legalEventsByItemSql} DESC,id DESC LIMIT 100`)
      .toBe('SELECT * FROM legal_events WHERE item_id=? ORDER BY created_at DESC,id DESC LIMIT 100');
    expect(eventsSql).toBe('SELECT * FROM legal_events WHERE item_id=? ORDER BY created_at');
  });

  it('adds no binds and introduces no write or lock text', () => {
    const all = [
      accessesSql, sharesSql, eventsSql, documentByIdSql, retentionPolicySql,
      membersSql, destroyingSql, activeSql,
    ].join(' ');
    expect((all.match(/\?/g) ?? []).length).toBe(9);
    expect(all).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    for (const sql of [destroyingSql, activeSql]) expect((sql.match(/\?/g) ?? []).length).toBe(1);
    expect((retentionPolicySql.match(/\?/g) ?? []).length).toBe(2);
  });

  it('introduces no SELECT token and one zero-token helper in the new source', async () => {
    const r52 = await src('legal-read-tail-r52-sources.ts');
    expect(r52).not.toContain('SELECT ');
    expect(r52.match(/export const legalDestructionGuardSql/g) ?? []).toHaveLength(1);
    expect(r52).toContain("from './legal-read-tail-r36-sources'");
    expect(r52).toContain("legalPointByKeySql('1', 'legal_destructions', 'item_id')");
  });

  it('removes every inline head from the five callers', async () => {
    const [shared, operations, service, r9, r18, documents] = await Promise.all([
      src('legal-read-tail-r52-sources.ts'), src('legal/operations.ts'), src('legal/service.ts'),
      src('legal-read-tail-r9-sources.ts'), src('legal-read-tail-r18-sources.ts'), src('legal/documents.ts'),
    ]);
    expect(shared.match(/export const legalDestructionGuardSql/g) ?? []).toHaveLength(1);
    expect(operations).toContain('legalItemOrderedSql("*","legal_access_events","created_at,id",10001)');
    expect(operations).not.toContain(accessesSql);
    expect(service).toContain('legalItemOrderedSql("id,email,document_id,user_id,expires_at,revoked_at","legal_shares","created_at DESC",100)');
    expect(service).toContain('legalPointByKeySql("user_id,access","legal_members","item_id")');
    expect(service).not.toContain(sharesSql);
    expect(service).not.toContain(membersSql);
    expect(r9).toContain("legalPointByIdSql('*','legal_documents')");
    expect(r9).toContain("legalDestructionGuardSql('active')");
    expect(r9).not.toContain(documentByIdSql);
    expect(r9).not.toContain(activeSql);
    expect(r18).toContain("legalByIdAndCompanySql('legal_retention_policies')");
    expect(r18).toContain("legalItemOrderedSql('*','legal_events','created_at')");
    expect(r18).not.toContain(retentionPolicySql);
    expect(r18).not.toContain(eventsSql);
    expect(documents).toContain("legalDestructionGuardSql('executing')");
    expect(documents).not.toContain(destroyingSql);
  });

  it('leaves advisory locks, destruction-worker row locks and corpus assertions intact', async () => {
    const [r9, worker, corpus, citations] = await Promise.all([
      src('legal-read-tail-r9-sources.ts'), src('legal/destruction-worker.ts'),
      src('legal/law-corpus.ts'), src('legal/citations.ts'),
    ]);
    expect(r9).toContain('legalFilesAdvisoryLockSql');
    expect(r9).toContain('legalCompanyAdvisoryLockSql');
    expect(worker).toContain('FOR UPDATE OF x,i');
    expect(corpus).toContain("nativeLegalAuxiliaryRead<LawCorpusView>(a,'laws',ruleId)");
    expect(corpus).toContain('legalCorpusSourceCleanHead(');
    expect(citations).toContain('export function legalCitation');
  });

  it('retains every r9-r51 helper symbol', async () => {
    const names = [
      'r9', 'r15', 'r16', 'r18', 'r19', 'r20', 'r21', 'r22', 'r23', 'r24', 'r25',
      'r26', 'r27', 'r28', 'r29', 'r30', 'r31', 'r32', 'r33', 'r34', 'r35', 'r36',
      'r38', 'r39', 'r40', 'r41', 'r42', 'r43', 'r44', 'r45', 'r46', 'r47', 'r48',
      'r49', 'r51',
    ];
    const texts = await Promise.all([
      ...names.map(round => src(`legal-read-tail-${round}-sources.ts`)),
      src('legal-read-tail-r52-sources.ts'),
    ]);
    const symbols = [
      'legalSettingsEnabledSql', 'legalManagerAvailableSql', 'legalDestructionActiveSql',
      'legalDocumentsItemPairSql', 'legalDocumentByIdSql', 'legalMembersAggSql',
      'legalFilesAdvisoryLockSql', 'legalCompanyAdvisoryLockSql', 'legalLaterApprovalGuardSql',
      'legalItemSelectSql', 'legalRetentionPolicySelectSql', 'legalActorScopeProjectionSql',
      'legalEventsByItemSql', 'legalRateCountPrefix', 'legalDisposedDocumentPrefix',
      'legalPurchaseClockCteSql', 'legalApprovedCycleSql', 'legalHoldActiveSql',
      'legalHoldTargetGuardPrefix', 'legalMemberEditExistsSql', 'legalJsonbAggByItemSql',
      'legalVersionExistsSql', 'legalByIdAndItemSql', 'legalByIdAndCompanySql',
      'legalPurchaseClockExistsSql', 'legalOrderedHistorySql', 'legalRecentItemListSql',
      'legalPointByIdSql', 'legalStatePointSql', 'legalPointByIdAndKeySql',
      'legalItemOrderedSql', 'legalPointByKeySql', 'legalReleasedItemGuardSql',
      'legalMemberEditPointSql', 'legalFactDataSql', 'legalMetricCountPrefix',
      'legalDisposedGuardSql', 'legalHoldOwnTargetSql', 'legalExistsPointGuardHeadSql',
      'legalShareDocumentReadHead', 'legalFeatureSearchSql', 'legalEnvelopeSignerReadHead',
      'legalExternalUploadShareJoinHead', 'legalFilePreservationHoldTargetHead',
      'legalItemsByIdCompanyHead', 'legalActiveDestructionGuardHead',
      'legalExecutiveObligationSpineHead', 'legalExecutiveHoldSpineHead',
      'legalAttachmentAvailableSql', 'legalCorpusSourceCleanHead', 'legalMembersWhereHead',
      'legalSettingsWhereHead', 'legalShareDocumentImageHead', 'legalCleanDocumentGuardHead',
      'legalLawStateGuardSql', 'legalDestructionGuardSql',
    ];
    for (const symbol of symbols) expect(texts.join('\n')).toContain(`export const ${symbol}`);
  });

  it('retirement:ts-statements stays strictly below the recorded BEFORE=186 / AFTER=173', async () => {
    let total = 0;
    const walk = async (dir: string) => {
      for (const entry of await readdir(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) await walk(p);
        else if (entry.name.endsWith('.ts')) total += ((await readFile(p, 'utf8')).match(/SELECT /g) ?? []).length;
      }
    };
    await walk(srcRoot);
    expect(total).toBeLessThan(186);
    expect(total).toBeLessThan(173);
  });
});

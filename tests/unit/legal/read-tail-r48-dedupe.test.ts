import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import { finalRootOwnerProjectionSources as filesProjectionSources } from '@erpx/server-files/public/final-root-owner-projection-sources';
import {
  legalCorpusSourceCleanHead,
  legalMembersWhereHead,
  legalSettingsWhereHead,
} from '../../../packages/server-legal/src/adapters/legal-read-tail-r48-sources';
import {
  legalAttachmentAvailableSql,
} from '../../../packages/server-legal/src/adapters/legal-read-tail-r47-sources';
import {
  legalDisposedGuardSql,
} from '../../../packages/server-legal/src/adapters/legal-read-tail-r39-sources';
import { legalMembershipSql, legalMembershipExistsSql } from '../../../packages/server-legal/src/adapters/legal-members-read-sources';
import { legalMemberEditPointSql } from '../../../packages/server-legal/src/adapters/legal-read-tail-r38-sources';
import { legalSettingsEnabledSql } from '../../../packages/server-legal/src/adapters/legal-read-tail-r9-sources';

const srcRoot = resolve(__dirname, '../../../packages/server-legal/src');
const src = (name: string) => readFile(new URL(`../../../packages/server-legal/src/adapters/${name}`, import.meta.url), 'utf8');

const links = filesProjectionSources.attachment_links;
const files = filesProjectionSources.attachments;
const docItemHead = 'SELECT 1 FROM legal_documents d JOIN legal_items i ON i.id=d.item_id WHERE d.id=? AND ';
/** Exact pre-extraction law-corpus compound predicate. Only the trusted
 * link/file aliases and the fixed r39 disposal guard participate. */
const corpusCleanPredicate = `d.item_id=? AND i.company_id=? AND d.checksum=? AND ${legalDisposedGuardSql('x','d.id')} AND ${legalAttachmentAvailableSql()}`;
const corpusCleanLiteral = `EXISTS(${docItemHead}${corpusCleanPredicate})`;
/** Independent exact pre-extraction literal, built from raw owner projections. */
const attachmentSpine = `(d.object_key IS NULL OR EXISTS(SELECT 1 FROM (${links}) l JOIN (${files}) f ON f.id=l.attachment_id WHERE l.id=d.attachment_link_id AND l.state='bound' AND f.company_id=i.company_id AND f.source_state='available' AND f.revoked_at IS NULL AND f.checksum_sha256=d.checksum))`;
const expectedCorpusLiteral = `EXISTS(${docItemHead}d.item_id=? AND i.company_id=? AND d.checksum=? AND NOT EXISTS(SELECT 1 FROM legal_disposals x WHERE x.document_id=d.id) AND ${attachmentSpine})`;

describe('legal read-tail r48 compound corpus source-clean and members/settings head reuse', () => {
  it('renders the byte-identical corpus source-clean head for all three holders', () => {
    expect(legalCorpusSourceCleanHead('X')).toBe(`${docItemHead}X`);
    expect(legalCorpusSourceCleanHead(corpusCleanPredicate)).toBe(`${docItemHead}${corpusCleanPredicate}`);
    expect(corpusCleanLiteral).toContain("d.id=? AND d.item_id=? AND i.company_id=? AND d.checksum=?");
    expect(corpusCleanLiteral).toContain("NOT EXISTS(SELECT 1 FROM legal_disposals x WHERE x.document_id=d.id)");
    expect(corpusCleanLiteral).toBe(expectedCorpusLiteral);
    expect(corpusCleanLiteral).toContain("f.checksum_sha256=d.checksum");
    expect(docItemHead).toContain('JOIN legal_items i ON i.id=d.item_id WHERE d.id=? AND ');
  });

  it('renders byte-identical legal_members and legal_settings heads', () => {
    expect(legalMembersWhereHead('item_id=? AND user_id=?'))
      .toBe('SELECT 1 FROM legal_members WHERE item_id=? AND user_id=?');
    expect(legalMembersWhereHead("item_id=? AND user_id=? AND access='edit'"))
      .toBe("SELECT 1 FROM legal_members WHERE item_id=? AND user_id=? AND access='edit'");
    expect(legalSettingsWhereHead('company_id=? AND enabled'))
      .toBe('SELECT 1 FROM legal_settings WHERE company_id=? AND enabled');
    expect(legalSettingsWhereHead('company_id=? AND ai_enabled AND enabled'))
      .toBe('SELECT 1 FROM legal_settings WHERE company_id=? AND ai_enabled AND enabled');
    // The two AI-enabled spellings are NOT byte-identical: they stay distinct.
    expect(legalSettingsWhereHead('company_id=? AND enabled AND ai_enabled'))
      .not.toBe(legalSettingsWhereHead('company_id=? AND ai_enabled AND enabled'));
    // Refactored exported helpers render byte-identically to their pre-r48 literals.
    expect(legalMembershipSql).toBe('SELECT 1 FROM legal_members WHERE item_id=? AND user_id=?');
    expect(legalMembershipExistsSql).toBe('EXISTS(SELECT 1 FROM legal_members WHERE item_id=? AND user_id=?)');
    expect(legalMemberEditPointSql('?')).toBe("SELECT 1 FROM legal_members WHERE item_id=? AND user_id=? AND access='edit'");
    expect(legalMemberEditPointSql('em.id')).toBe("SELECT 1 FROM legal_members WHERE item_id=em.id AND user_id=? AND access='edit'");
    expect(legalSettingsEnabledSql).toBe('SELECT 1 FROM legal_settings WHERE company_id=? AND enabled');
  });

  it('adds zero new binds and keeps every rendered fragment read-only', () => {
    const all = [
      legalCorpusSourceCleanHead(corpusCleanPredicate),
      legalMembersWhereHead('item_id=? AND user_id=?'),
      legalMembersWhereHead("item_id=? AND user_id=? AND access='edit'"),
      legalSettingsWhereHead('company_id=? AND enabled'),
      legalSettingsWhereHead('company_id=? AND ai_enabled AND enabled'),
    ].join(' ');
    expect(all.match(/\?/g) ?? []).toHaveLength(10);
    expect(all).toContain('SELECT 1 FROM legal_documents');
    expect(all).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
  });

  it('keeps one definition and removes every inline head from the callers', async () => {
    const shared = await src('legal-read-tail-r48-sources.ts');
    expect(shared.match(/export const legalCorpusSourceCleanHead/g) ?? []).toHaveLength(1);
    expect(shared.match(/export const legalMembersWhereHead/g) ?? []).toHaveLength(1);
    expect(shared.match(/export const legalSettingsWhereHead/g) ?? []).toHaveLength(1);
    const [corpus, citations, knowledge, members, r38, r9, jobs] = await Promise.all([
      src('legal/law-corpus.ts'), src('legal/citations.ts'), src('legal/knowledge.ts'),
      src('legal-members-read-sources.ts'), src('legal-read-tail-r38-sources.ts'),
      src('legal-read-tail-r9-sources.ts'), src('legal/jobs.ts'),
    ]);
    for (const text of [corpus, citations, knowledge]) {
      expect(text).not.toContain('SELECT 1 FROM legal_documents d JOIN legal_items i ON i.id=d.item_id WHERE d.id=? AND ');
      expect(text).toContain('legalCorpusSourceCleanHead(');
    }
    for (const text of [members, r38]) {
      expect(text).not.toContain('"SELECT 1 FROM legal_members WHERE item_id=?');
      expect(text).not.toContain('`SELECT 1 FROM legal_members WHERE item_id=');
      expect(text).toContain('legalMembersWhereHead(');
    }
    expect(r9).not.toContain('"SELECT 1 FROM legal_settings WHERE company_id=? AND enabled"');
    expect(jobs).not.toContain('"SELECT 1 FROM legal_settings WHERE company_id=? AND ai_enabled AND enabled"');
    expect(r9).toContain("legalSettingsWhereHead('company_id=? AND enabled')");
    expect(jobs).toContain("legalSettingsWhereHead('company_id=? AND ai_enabled AND enabled')");
    expect(corpus).toContain("legalCorpusSourceCleanHead(`d.item_id=? AND i.company_id=? AND d.checksum=?");
  });

  it('leaves corpus/citation/advisory/FOR UPDATE reads intact', async () => {
    const [corpus, citations, knowledge, r9, destructionWorker] = await Promise.all([
      src('legal/law-corpus.ts'), src('legal/citations.ts'), src('legal/knowledge.ts'),
      src('legal-read-tail-r9-sources.ts'), src('legal/destruction-worker.ts'),
    ]);
    expect(corpus).toContain('legal_law_source_checks');
    expect(corpus).toContain("legalActiveDestructionGuardHead('x','i')");
    expect(corpus).toContain('legalAttachmentAvailableSql()');
    expect(corpus).toContain("legalDisposedGuardSql('x','d.id')");
    expect(citations).toContain('export function legalCitation');
    expect(knowledge).toContain('export const legalEvidenceDocumentAvailableSql');
    expect(r9).toContain('legalFilesAdvisoryLockSql');
    expect(r9).toContain('legalCompanyAdvisoryLockSql');
    expect(destructionWorker).toContain('FOR UPDATE OF x,i');
  });

  it('retains every r9-r47 helper symbol', async () => {
    const names = [
      'legal-read-tail-r9-sources.ts', 'legal-read-tail-r15-sources.ts', 'legal-read-tail-r16-sources.ts',
      'legal-read-tail-r18-sources.ts', 'legal-read-tail-r19-sources.ts', 'legal-read-tail-r20-sources.ts',
      'legal-read-tail-r21-sources.ts', 'legal-read-tail-r22-sources.ts', 'legal-read-tail-r23-sources.ts',
      'legal-read-tail-r24-sources.ts', 'legal-read-tail-r25-sources.ts', 'legal-read-tail-r26-sources.ts',
      'legal-read-tail-r27-sources.ts', 'legal-read-tail-r28-sources.ts', 'legal-read-tail-r29-sources.ts',
      'legal-read-tail-r30-sources.ts', 'legal-read-tail-r31-sources.ts', 'legal-read-tail-r32-sources.ts',
      'legal-read-tail-r33-sources.ts', 'legal-read-tail-r34-sources.ts', 'legal-read-tail-r35-sources.ts',
      'legal-read-tail-r36-sources.ts', 'legal-read-tail-r38-sources.ts', 'legal-read-tail-r39-sources.ts',
      'legal-read-tail-r40-sources.ts', 'legal-read-tail-r41-sources.ts', 'legal-read-tail-r42-sources.ts',
      'legal-read-tail-r43-sources.ts', 'legal-read-tail-r44-sources.ts', 'legal-read-tail-r45-sources.ts',
      'legal-read-tail-r46-sources.ts', 'legal-read-tail-r47-sources.ts',
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
      'legalDisposedGuardSql', 'legalHoldOwnTargetSql', 'legalExistsPointGuardHeadSql',
      'legalShareDocumentReadHead', 'legalFeatureSearchSql', 'legalEnvelopeSignerReadHead',
      'legalExternalUploadShareJoinHead', 'legalFilePreservationHoldTargetHead',
      'legalItemsByIdCompanyHead', 'legalActiveDestructionGuardHead',
      'legalExecutiveObligationSpineHead', 'legalExecutiveHoldSpineHead',
      'legalAttachmentAvailableSql',
    ];
    for (const symbol of symbols) expect(texts.join('\n')).toContain(`export const ${symbol}`);
  });

  it('retirement:ts-statements stays strictly below the recorded BEFORE=196 / AFTER=192', async () => {
    let total = 0;
    const walk = async (dir: string) => {
      for (const entry of await readdir(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) await walk(p);
        else if (entry.name.endsWith('.ts')) total += ((await readFile(p, 'utf8')).match(/SELECT /g) ?? []).length;
      }
    };
    await walk(srcRoot);
    expect(total).toBeLessThan(196);
    expect(total).toBeLessThan(192);
  });
});

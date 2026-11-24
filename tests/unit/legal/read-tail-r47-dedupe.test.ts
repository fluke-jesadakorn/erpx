import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import { finalRootOwnerProjectionSources as filesProjectionSources } from '@erpx/server-files/public/final-root-owner-projection-sources';
import {
  legalAttachmentAvailableSql,
} from '../../../packages/server-legal/src/adapters/legal-read-tail-r47-sources';
import {
  legalDisposedGuardSql,
} from '../../../packages/server-legal/src/adapters/legal-read-tail-r39-sources';

const srcRoot = resolve(__dirname, '../../../packages/server-legal/src');
const src = (name: string) => readFile(new URL(`../../../packages/server-legal/src/adapters/${name}`, import.meta.url), 'utf8');

const links = filesProjectionSources.attachment_links;
const files = filesProjectionSources.attachments;
/** Exact literals that were inline before the spine was extracted. Only the
 * trusted link/file aliases vary; rendering must be byte-identical down to the
 * state/company/source/revocation/checksum predicates and the parentheses. */
const lfSpine = `(d.object_key IS NULL OR EXISTS(SELECT 1 FROM (${links}) l JOIN (${files}) f ON f.id=l.attachment_id WHERE l.id=d.attachment_link_id AND l.state='bound' AND f.company_id=i.company_id AND f.source_state='available' AND f.revoked_at IS NULL AND f.checksum_sha256=d.checksum))`;
const cleanSpine = `(d.object_key IS NULL OR EXISTS(SELECT 1 FROM (${links}) clean_link JOIN (${files}) clean_file ON clean_file.id=clean_link.attachment_id WHERE clean_link.id=d.attachment_link_id AND clean_link.state='bound' AND clean_file.company_id=i.company_id AND clean_file.source_state='available' AND clean_file.revoked_at IS NULL AND clean_file.checksum_sha256=d.checksum))`;

describe('legal read-tail r47 attachment-availability spine and disposal-guard reuse family', () => {
  it('renders byte-identical attachment spines for all three holders', () => {
    expect(legalAttachmentAvailableSql()).toBe(lfSpine);
    expect(legalAttachmentAvailableSql('l', 'f')).toBe(lfSpine);
    expect(legalAttachmentAvailableSql('clean_link', 'clean_file')).toBe(cleanSpine);
    expect(lfSpine).toContain("l.state='bound'");
    expect(lfSpine).toContain("f.source_state='available'");
    expect(lfSpine).toContain('f.revoked_at IS NULL');
    expect(lfSpine).toContain('f.checksum_sha256=d.checksum');
    expect(cleanSpine).toContain("clean_link.state='bound'");
    expect(cleanSpine).toContain('clean_file.checksum_sha256=d.checksum');
  });

  it('renders byte-identical disposed guards for all three callers', () => {
    expect(legalDisposedGuardSql('disposed', 'd.id'))
      .toBe('NOT EXISTS(SELECT 1 FROM legal_disposals disposed WHERE disposed.document_id=d.id)');
    expect(legalDisposedGuardSql('x', 'c.document_id'))
      .toBe('NOT EXISTS(SELECT 1 FROM legal_disposals x WHERE x.document_id=c.document_id)');
    expect(legalDisposedGuardSql('x', 'd.id'))
      .toBe('NOT EXISTS(SELECT 1 FROM legal_disposals x WHERE x.document_id=d.id)');
  });

  it('adds zero new binds', () => {
    const all = [
      legalAttachmentAvailableSql(),
      legalAttachmentAvailableSql('clean_link', 'clean_file'),
      legalDisposedGuardSql('disposed', 'd.id'),
      legalDisposedGuardSql('x', 'c.document_id'),
      legalDisposedGuardSql('x', 'd.id'),
    ].join(' ');
    expect(all).not.toContain('?');
  });

  it('keeps one definition and removes every inline copy from the callers', async () => {
    const shared = await src('legal-read-tail-r47-sources.ts');
    expect(shared.match(/export const legalAttachmentAvailableSql/g) ?? []).toHaveLength(1);
    const [knowledge, corpus] = await Promise.all([
      src('legal/knowledge.ts'), src('legal/law-corpus.ts'),
    ]);
    expect(knowledge.match(/legalAttachmentAvailableSql\(/g) ?? []).toHaveLength(1);
    expect(knowledge.match(/legalDisposedGuardSql\(/g) ?? []).toHaveLength(1);
    expect(corpus.match(/legalAttachmentAvailableSql\(/g) ?? []).toHaveLength(2);
    expect(corpus.match(/legalDisposedGuardSql\(/g) ?? []).toHaveLength(2);
    for (const text of [knowledge, corpus]) {
      expect(text).not.toContain('object_key IS NULL OR EXISTS(SELECT 1 FROM (');
      expect(text).not.toContain('NOT EXISTS(SELECT 1 FROM legal_disposals');
      expect(text).not.toContain('final-root-owner-projection-sources');
    }
    expect(knowledge).toContain('legalAttachmentAvailableSql(\'clean_link\',\'clean_file\')');
    expect(corpus).toContain("legalDisposedGuardSql('x','c.document_id')");
    expect(corpus).toContain("legalDisposedGuardSql('x','d.id')");
  });

  it('keeps every fragment read-only (no writes or row locks)', () => {
    const all = [
      legalAttachmentAvailableSql(),
      legalAttachmentAvailableSql('clean_link', 'clean_file'),
      legalDisposedGuardSql('disposed', 'd.id'),
      legalDisposedGuardSql('x', 'c.document_id'),
    ].join(' ');
    expect(all).toContain('SELECT 1 FROM legal_disposals');
    expect(all).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
  });

  it('leaves corpus/citation/advisory/FOR UPDATE reads intact', async () => {
    const [corpus, citations, knowledge, r9, destructionWorker] = await Promise.all([
      src('legal/law-corpus.ts'), src('legal/citations.ts'), src('legal/knowledge.ts'),
      src('legal-read-tail-r9-sources.ts'), src('legal/destruction-worker.ts'),
    ]);
    expect(corpus).toContain('legal_law_source_checks');
    expect(corpus).toContain("legalActiveDestructionGuardHead('x','i')");
    expect(citations).toContain('export function legalCitation');
    expect(knowledge).toContain('export const legalEvidenceDocumentAvailableSql');
    expect(r9).toContain('legalFilesAdvisoryLockSql');
    expect(r9).toContain('legalCompanyAdvisoryLockSql');
    expect(destructionWorker).toContain('FOR UPDATE OF x,i');
  });

  it('leaves r9-r46 helpers untouched', async () => {
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
      'legal-read-tail-r46-sources.ts',
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
    ];
    for (const symbol of symbols) expect(texts.join('\n')).toContain(`export const ${symbol}`);
  });

  it('retirement:ts-statements is pinned at BEFORE=201 -> AFTER=196', async () => {
    let total = 0;
    const walk = async (dir: string) => {
      for (const entry of await readdir(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) await walk(p);
        else if (entry.name.endsWith('.ts')) total += ((await readFile(p, 'utf8')).match(/SELECT /g) ?? []).length;
      }
    };
    await walk(srcRoot);
    expect(total).toBeLessThan(201);
    expect(total).toBe(196);
  });
});

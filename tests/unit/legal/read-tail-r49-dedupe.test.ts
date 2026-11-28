import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import {
  legalCleanDocumentGuardHead,
  legalShareDocumentImageHead,
} from '../../../modules/legal/server/src/adapters/legal-read-tail-r49-sources';
import {
  legalShareDocumentReadHead,
} from '../../../modules/legal/server/src/adapters/legal-read-tail-r41-sources';
import {
  legalPointByIdSql,
} from '../../../modules/legal/server/src/adapters/legal-read-tail-r32-sources';
import {
  legalDisposedGuardSql,
} from '../../../modules/legal/server/src/adapters/legal-read-tail-r39-sources';
import {
  legalPortalSourceImageFields,
} from '../../../modules/legal/server/src/adapters/portal-purchases';

const srcRoot = resolve(__dirname, '../../../modules/legal/server/src');
const src = (name: string) => readFile(new URL(`../../../modules/legal/server/src/adapters/${name}`, import.meta.url), 'utf8');

/** Fixed share/document spine verbatim from the pre-r49 inline holders. */
const shareDocumentSpine = 'FROM legal_shares s JOIN legal_items i ON i.id=s.item_id JOIN legal_documents d ON d.id=s.document_id AND d.item_id=i.id';
/** Exact pre-r49 projections, including the original single-space line breaks. */
const originalProjection = `i.company_id,d.attachment_link_id,
 jsonb_build_object('share',to_jsonb(s),'item',to_jsonb(i),'document',to_jsonb(d),
 ${legalPortalSourceImageFields()})::text image
`;
const snapshotProjection = `COALESCE(jsonb_agg(jsonb_build_object(
 'share',to_jsonb(s),'item',to_jsonb(i),'document',to_jsonb(d),
 ${legalPortalSourceImageFields()}
 ) ORDER BY s.id),'[]'::jsonb)::text image
`;

describe('legal read-tail r49 share/document and clean-document guard head reuse', () => {
  it('delegates family 1 to the verified r41 head with byte-identical rendering', () => {
    // The r49 head is the r41 head with only its trailing space trimmed.
    expect(legalShareDocumentImageHead('x')).toBe(legalShareDocumentReadHead('x').trimEnd());
    // Independent oracles: the exact pre-r49 inline SQL, never built from r49.
    const imageTail = `
 WHERE s.id=? AND lower(s.email)=? AND (s.user_id IS NULL OR s.user_id=?) AND s.revoked_at IS NULL
 AND s.expires_at>floor(extract(epoch FROM clock_timestamp())*1000)::bigint AND \${await legalPortalPurchasePredicate()}`;
    const snapshotTail = `
 WHERE lower(s.email)=? AND (s.user_id IS NULL OR s.user_id=?) AND (?::text IS NULL OR s.id=?) AND s.revoked_at IS NULL
 AND s.expires_at>floor(extract(epoch FROM clock_timestamp())*1000)::bigint AND \${await legalPortalPurchasePredicate()}`;
    expect(legalShareDocumentImageHead(originalProjection) + imageTail)
      .toBe(`SELECT ${originalProjection} ${shareDocumentSpine}${imageTail}`);
    expect(legalShareDocumentImageHead(snapshotProjection) + snapshotTail)
      .toBe(`SELECT ${snapshotProjection} ${shareDocumentSpine}${snapshotTail}`);
    expect(originalProjection).toContain("jsonb_build_object('share',to_jsonb(s)");
    expect(snapshotProjection).toContain(") ORDER BY s.id),'[]'::jsonb)::text image");
  });

  it('renders byte-identical clean-document guards for both holders', () => {
    const queuedGuard = `${legalDisposedGuardSql('x','d.id')} AND NOT EXISTS(SELECT 1 FROM legal_destructions x WHERE x.item_id=d.item_id AND x.state='executing')`;
    expect(legalCleanDocumentGuardHead('d', queuedGuard))
      .toBe(`SELECT 1 FROM legal_documents d WHERE d.id=? AND d.item_id=? AND d.checksum=? AND ${queuedGuard}`);
    expect(legalCleanDocumentGuardHead('', 'object_key IS NOT DISTINCT FROM ?'))
      .toBe('SELECT 1 FROM legal_documents WHERE id=? AND item_id=? AND checksum=? AND object_key IS NOT DISTINCT FROM ?');
    // The base head is the verified r32 point read; the alias prefix is the only change.
    expect(legalPointByIdSql('1', 'legal_documents')).toBe('SELECT 1 FROM legal_documents WHERE id=?');
  });

  it('adds zero new binds and introduces no new SELECT text', async () => {
    const r49 = await src('legal-read-tail-r49-sources.ts');
    expect(r49).not.toContain('SELECT ');
    const all = [
      legalShareDocumentImageHead(originalProjection),
      legalShareDocumentImageHead(snapshotProjection),
      legalCleanDocumentGuardHead('d', `${legalDisposedGuardSql('x','d.id')} AND NOT EXISTS(SELECT 1 FROM legal_destructions x WHERE x.item_id=d.item_id AND x.state='executing')`),
      legalCleanDocumentGuardHead('', 'object_key IS NOT DISTINCT FROM ?'),
    ].join(' ');
    // Two binds per image projection + three queued + four destruction guards.
    expect((all.match(/\?/g) ?? []).length).toBe(11);
    expect(all).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
  });

  it('removes every inline head from the four callers', async () => {
    const shared = await src('legal-read-tail-r49-sources.ts');
    expect(shared.match(/export const legalShareDocumentImageHead/g) ?? []).toHaveLength(1);
    expect(shared.match(/export const legalCleanDocumentGuardHead/g) ?? []).toHaveLength(1);
    const [original, snapshot, queued, destruction] = await Promise.all([
      src('portal-original-read.ts'), src('portal-purchases.ts'),
      src('queued-purchases.ts'), src('legal/destruction-sources.ts'),
    ]);
    for (const text of [original, snapshot]) {
      expect(text).not.toContain(`SELECT i.company_id,d.attachment_link_id,`);
      expect(text).not.toContain('SELECT COALESCE(jsonb_agg(jsonb_build_object(');
      expect(text).not.toContain(shareDocumentSpine);
      expect(text).toContain('legalShareDocumentImageHead(projection)');
    }
    expect(queued).not.toContain('SELECT 1 FROM legal_documents d WHERE d.id=?');
    expect(queued).toContain("legalCleanDocumentGuardHead('d',");
    expect(destruction).not.toContain('SELECT 1 FROM legal_documents WHERE id=? AND item_id=? AND checksum=?');
    expect(destruction).toContain("legalCleanDocumentGuardHead('','object_key IS NOT DISTINCT FROM ?')");
  });

  it('leaves corpus/citation/advisory/FOR UPDATE reads intact', async () => {
    const [corpus, citations, knowledge, r9, destructionWorker] = await Promise.all([
      src('legal/law-corpus.ts'), src('legal/citations.ts'), src('legal/knowledge.ts'),
      src('legal-read-tail-r9-sources.ts'), src('legal/destruction-worker.ts'),
    ]);
    expect(corpus).toContain("nativeLegalAuxiliaryRead<LawCorpusView>(a,'laws',ruleId)");
    expect(corpus).toContain('legalCorpusSourceCleanHead(');
    expect(citations).toContain('export function legalCitation');
    expect(knowledge).toContain('export const legalEvidenceDocumentAvailableSql');
    expect(r9).toContain('legalFilesAdvisoryLockSql');
    expect(r9).toContain('legalCompanyAdvisoryLockSql');
    expect(destructionWorker).toContain('FOR UPDATE OF x,i');
  });

  it('retains every r9-r48 helper symbol', async () => {
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
      'legal-read-tail-r46-sources.ts', 'legal-read-tail-r47-sources.ts', 'legal-read-tail-r48-sources.ts',
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
      'legalCorpusSourceCleanHead', 'legalMembersWhereHead', 'legalSettingsWhereHead',
    ];
    for (const symbol of symbols) expect(texts.join('\n')).toContain(`export const ${symbol}`);
  });

  it('retirement:ts-statements stays strictly below the recorded BEFORE=192 / AFTER=173', async () => {
    let total = 0;
    const walk = async (dir: string) => {
      for (const entry of await readdir(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) await walk(p);
        else if (entry.name.endsWith('.ts')) total += ((await readFile(p, 'utf8')).match(/SELECT /g) ?? []).length;
      }
    };
    await walk(srcRoot);
    expect(total).toBeLessThan(192);
    expect(total).toBeLessThanOrEqual(173);
  });
});

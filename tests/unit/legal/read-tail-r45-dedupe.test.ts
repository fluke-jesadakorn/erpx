import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import { legalItemsByIdCompanyHead } from '../../../packages/server-legal/src/adapters/legal-read-tail-r45-sources';
import { legalMembersAggSql } from '../../../packages/server-legal/src/adapters/legal-read-tail-r9-sources';
import { legalJsonbAggByItemSql } from '../../../packages/server-legal/src/adapters/legal-read-tail-r25-sources';
import { legalFactDataSql } from '../../../packages/server-legal/src/adapters/legal-read-tail-r38-sources';
import { legalExternalUploadShareJoinHead } from '../../../packages/server-legal/src/adapters/legal-read-tail-r44-sources';

const srcRoot = resolve(__dirname, '../../../packages/server-legal/src');
const src = (name: string) => readFile(new URL(`../../../packages/server-legal/src/adapters/${name}`, import.meta.url), 'utf8');

/** The four member projections are the exact literals that were inline before
 * the head was extracted. Only the trusted projection and trusted key prefix
 * vary; rendering must be byte-identical, including both binds. */
const attachmentProjection =
  `i.*,jsonb_build_object('item',to_jsonb(i)-'version'-'updated_at','members',${legalMembersAggSql})::text image`;
const sourceProjection = 'to_jsonb(i) image';
const canonicalProjection =
  `jsonb_build_object('owner',?::jsonb,'settings',(SELECT to_jsonb(s) FROM legal_settings s WHERE s.company_id=i.company_id),'holds',${legalJsonbAggByItemSql("h","legal_holds")},'disposals',(SELECT jsonb_agg(to_jsonb(d) ORDER BY d.document_id) FROM legal_disposals d JOIN legal_documents doc ON doc.id=d.document_id WHERE doc.item_id=i.id),'destructions',${legalJsonbAggByItemSql("d","legal_destructions")},'uploads',(${legalExternalUploadShareJoinHead('jsonb_agg(to_jsonb(u) ORDER BY u.id)','u','share')}WHERE share.item_id=i.id))::text image`;
const featureTables = [
  'legal_case_events', 'legal_compliance_actions', 'legal_privacy_cases', 'legal_corporate_actions',
  'legal_ip_actions', 'legal_counsel_entries', 'legal_obligation_schedule', 'legal_custody_events',
  'legal_policy_acknowledgements', 'legal_intake_questions',
];
const witnessTable = 'legal_requests';
const witnessProjection =
  `jsonb_build_object('item',to_jsonb(i),'data',(${legalFactDataSql(witnessTable,'i.id')}),'members',${legalMembersAggSql},'documents',${legalJsonbAggByItemSql("d","legal_documents")},'shares',${legalJsonbAggByItemSql("s","legal_shares")},'executed',${legalJsonbAggByItemSql("e","legal_executed_copies")},'approvals',${legalJsonbAggByItemSql("ap","legal_approvals","cycle")},'route',(SELECT to_jsonb(ar) FROM legal_approval_routes ar WHERE ar.item_id=i.id),'authorities',${legalJsonbAggByItemSql("au","legal_authorities")},'retention',(SELECT to_jsonb(ra)||jsonb_build_object('policy',(SELECT to_jsonb(rp) FROM legal_retention_policies rp WHERE rp.id=ra.policy_id)) FROM legal_retention_assignments ra WHERE ra.item_id=i.id),'destructions',${legalJsonbAggByItemSql("dx","legal_destructions")},'features',(SELECT jsonb_agg(feature.image ORDER BY feature.image::text) FROM (${featureTables.map(table => `SELECT to_jsonb(f) image FROM ${table} f WHERE f.item_id=i.id`).join(' UNION ALL ')}) feature))::text image`;

describe('legal read-tail r45 parameterized item-by-id-and-company head family', () => {
  it('renders byte-identical SQL for both bare-key members', () => {
    expect(legalItemsByIdCompanyHead(attachmentProjection)).toBe(
      `SELECT ${attachmentProjection} FROM legal_items i WHERE id=? AND company_id=?`,
    );
    expect(legalItemsByIdCompanyHead(sourceProjection)).toBe(
      'SELECT to_jsonb(i) image FROM legal_items i WHERE id=? AND company_id=?',
    );
  });

  it('renders byte-identical SQL for both i.-prefixed witness members', () => {
    const canonical = legalItemsByIdCompanyHead(canonicalProjection, 'i.');
    const witness = legalItemsByIdCompanyHead(witnessProjection, 'i.');
    expect(canonical).toBe(`SELECT ${canonicalProjection} FROM legal_items i WHERE i.id=? AND i.company_id=?`);
    expect(witness).toBe(`SELECT ${witnessProjection} FROM legal_items i WHERE i.id=? AND i.company_id=?`);
    expect(canonical).toContain("'uploads',(SELECT jsonb_agg(to_jsonb(u) ORDER BY u.id)");
    expect(witness).toContain("'route',(SELECT to_jsonb(ar) FROM legal_approval_routes ar WHERE ar.item_id=i.id)");
  });

  it('keeps the head read-only (no writes or row locks)', () => {
    const all = [
      legalItemsByIdCompanyHead(sourceProjection),
      legalItemsByIdCompanyHead(canonicalProjection, 'i.'),
    ].join(' ');
    expect(all).toMatch(/SELECT .* FROM legal_items i WHERE/);
    expect(all).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
  });

  it('keeps one head definition and removes every inline head from the four callers', async () => {
    const shared = await src('legal-read-tail-r45-sources.ts');
    expect(shared.match(/export const legalItemsByIdCompanyHead/g) ?? []).toHaveLength(1);
    const callers: Record<string, number> = {
      'attachment-purchases.ts': 1,
      'canonical-attachment-read.ts': 1,
      'purchases.ts': 2,
    };
    for (const [file, count] of Object.entries(callers)) {
      const text = await src(file);
      expect(text.match(/legalItemsByIdCompanyHead\(/g) ?? []).toHaveLength(count);
      expect(text).not.toContain('SELECT to_jsonb(i) image FROM legal_items i WHERE id=? AND company_id=?');
      expect(text).not.toContain('::text image FROM legal_items i WHERE i.id=? AND i.company_id=?');
    }
  });

  it('leaves corpus/citation/advisory/FOR UPDATE reads intact', async () => {
    const [corpus, citations, r9, r23, r24, destructionWorker] = await Promise.all([
      src('legal/law-corpus.ts'), src('legal/citations.ts'),
      src('legal-read-tail-r9-sources.ts'), src('legal-read-tail-r23-sources.ts'),
      src('legal-read-tail-r24-sources.ts'), src('legal/destruction-worker.ts'),
    ]);
    expect(corpus).toContain("nativeLegalAuxiliaryRead<LawCorpusView>(a,'laws',ruleId)");
    expect(citations).toContain('export function legalCitation');
    expect(r9).toContain('legalFilesAdvisoryLockSql');
    expect(r9).toContain('legalCompanyAdvisoryLockSql');
    expect(r23).toContain('legalHoldActiveSql');
    expect(r24).toContain('legalHoldTargetGuardPrefix');
    expect(destructionWorker).toContain('FOR UPDATE OF x,i');
  });

  it('leaves r9-r44 helpers untouched', async () => {
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
      'legal-read-tail-r43-sources.ts', 'legal-read-tail-r44-sources.ts',
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
    ];
    for (const symbol of symbols) expect(texts.join('\n')).toContain(`export const ${symbol}`);
  });

  it('retirement:ts-statements stays strictly below the recorded BEFORE=206', async () => {
    let total = 0;
    const walk = async (dir: string) => {
      for (const entry of await readdir(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) await walk(p);
        else if (entry.name.endsWith('.ts')) total += ((await readFile(p, 'utf8')).match(/SELECT /g) ?? []).length;
      }
    };
    await walk(srcRoot);
    expect(total).toBeLessThan(206);
  });
});

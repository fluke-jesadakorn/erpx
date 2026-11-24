import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import {
  legalLawStateGuardSql,
} from '../../../packages/server-legal/src/adapters/legal-read-tail-r51-sources';
import {
  legalPointByIdSql,
} from '../../../packages/server-legal/src/adapters/legal-read-tail-r32-sources';

const srcRoot = resolve(__dirname, '../../../packages/server-legal/src');
const src = (name: string) => readFile(new URL(`../../../packages/server-legal/src/adapters/${name}`, import.meta.url), 'utf8');

/** Independent pre-r51 oracles: the exact inline SQL removed from law-corpus. */
const sourceGuardSql = `SELECT 1 FROM legal_law_sources WHERE id=? AND rule_id=? AND company_id=? AND state='active'`;
const revisionGuardSql = `SELECT 1 FROM legal_law_revisions WHERE id=? AND rule_id=? AND company_id=? AND state='approved'`;

describe('legal read-tail r51 law state guard reuse', () => {
  it('delegates to the verified r32 point head with byte-identical rendering', () => {
    expect(legalPointByIdSql('1', 'legal_law_sources')).toBe('SELECT 1 FROM legal_law_sources WHERE id=?');
    expect(legalPointByIdSql('1', 'legal_law_revisions')).toBe('SELECT 1 FROM legal_law_revisions WHERE id=?');
    expect(legalLawStateGuardSql('legal_law_sources', 'active')).toBe(sourceGuardSql);
    expect(legalLawStateGuardSql('legal_law_revisions', 'approved')).toBe(revisionGuardSql);
    expect(sourceGuardSql).toContain("AND state='active'");
    expect(revisionGuardSql).toContain("AND state='approved'");
  });

  it('adds three binds per guard and no write or lock text', () => {
    for (const sql of [
      legalLawStateGuardSql('legal_law_sources', 'active'),
      legalLawStateGuardSql('legal_law_revisions', 'approved'),
    ]) {
      expect((sql.match(/\?/g) ?? []).length).toBe(3);
      expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
  });

  it('introduces no SELECT token and one helper in the new source', async () => {
    const r51 = await src('legal-read-tail-r51-sources.ts');
    expect(r51).not.toContain('SELECT ');
    expect(r51.match(/export const legalLawStateGuardSql/g) ?? []).toHaveLength(1);
    expect(r51).toContain("from './legal-read-tail-r32-sources'");
  });

  it('removes both inline guards from law-corpus and preserves their bind order', async () => {
    const corpus = await src('legal/law-corpus.ts');
    expect(corpus).not.toContain(sourceGuardSql);
    expect(corpus).not.toContain(revisionGuardSql);
    expect(corpus).toContain("legalLawStateGuardSql('legal_law_sources','active')).bind(facts.sourceId,facts.id,facts.companyId).first()");
    expect(corpus).toContain("legalLawStateGuardSql('legal_law_revisions','approved')).bind(facts.revisionId,facts.id,facts.companyId).first()");
  });

  it('leaves corpus assertions, advisory locks and r9-r49 helpers intact', async () => {
    const [corpus, r9, worker, ...helpers] = await Promise.all([
      src('legal/law-corpus.ts'), src('legal-read-tail-r9-sources.ts'),
      src('legal/destruction-worker.ts'),
      ...[
        'r15', 'r16', 'r18', 'r19', 'r20', 'r21', 'r22', 'r23', 'r24', 'r25',
        'r26', 'r27', 'r28', 'r29', 'r30', 'r31', 'r32', 'r33', 'r34', 'r35',
        'r36', 'r38', 'r39', 'r40', 'r41', 'r42', 'r43', 'r44', 'r45', 'r46',
        'r47', 'r48', 'r49',
      ].map(round => src(`legal-read-tail-${round}-sources.ts`)),
    ]);
    expect(corpus).toContain('requireLawSourceCurrent:');
    expect(corpus).toContain('requireLawRevisionCurrent:');
    expect(corpus).toContain('legalCorpusSourceCleanHead(');
    expect(corpus).toContain('legalDisposedGuardSql(');
    expect(corpus).toContain('legal_law_source_checks');
    expect(r9).toContain('legalFilesAdvisoryLockSql');
    expect(r9).toContain('legalCompanyAdvisoryLockSql');
    expect(worker).toContain('FOR UPDATE OF x,i');
    const symbols = [
      'legalSettingsEnabledSql', 'legalManagerAvailableSql', 'legalDestructionActiveSql',
      'legalDocumentsItemPairSql', 'legalDocumentByIdSql', 'legalMembersAggSql',
      'legalLaterApprovalGuardSql', 'legalItemSelectSql', 'legalRetentionPolicySelectSql',
      'legalActorScopeProjectionSql', 'legalEventsByItemSql', 'legalRateCountPrefix',
      'legalDisposedDocumentPrefix', 'legalPurchaseClockCteSql', 'legalApprovedCycleSql',
      'legalHoldActiveSql', 'legalHoldTargetGuardPrefix', 'legalMemberEditExistsSql',
      'legalJsonbAggByItemSql', 'legalVersionExistsSql', 'legalByIdAndItemSql',
      'legalByIdAndCompanySql', 'legalPurchaseClockExistsSql', 'legalOrderedHistorySql',
      'legalRecentItemListSql', 'legalPointByIdSql', 'legalStatePointSql',
      'legalPointByIdAndKeySql', 'legalItemOrderedSql', 'legalPointByKeySql',
      'legalReleasedItemGuardSql', 'legalMemberEditPointSql', 'legalFactDataSql',
      'legalMetricCountPrefix', 'legalDisposedGuardSql', 'legalHoldOwnTargetSql',
      'legalExistsPointGuardHeadSql', 'legalShareDocumentReadHead', 'legalFeatureSearchSql',
      'legalEnvelopeSignerReadHead', 'legalExternalUploadShareJoinHead',
      'legalFilePreservationHoldTargetHead', 'legalItemsByIdCompanyHead',
      'legalActiveDestructionGuardHead', 'legalExecutiveObligationSpineHead',
      'legalExecutiveHoldSpineHead', 'legalAttachmentAvailableSql',
      'legalCorpusSourceCleanHead', 'legalMembersWhereHead', 'legalSettingsWhereHead',
      'legalShareDocumentImageHead', 'legalCleanDocumentGuardHead',
    ];
    for (const symbol of symbols) expect([r9, ...helpers].join('\n')).toContain(`export const ${symbol}`);
  });

  it('retirement:ts-statements is pinned at BEFORE=188 -> AFTER=186', async () => {
    let total = 0;
    const walk = async (dir: string) => {
      for (const entry of await readdir(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) await walk(p);
        else if (entry.name.endsWith('.ts')) total += ((await readFile(p, 'utf8')).match(/SELECT /g) ?? []).length;
      }
    };
    await walk(srcRoot);
    expect(total).toBeLessThan(188);
    expect(total).toBe(186);
  });
});

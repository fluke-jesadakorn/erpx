import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import { legalEnvelopeSignerReadHead } from '../../../packages/server-legal/src/adapters/legal-read-tail-r43-sources';

const srcRoot = resolve(__dirname, '../../../packages/server-legal/src');
const src = (name: string) => readFile(new URL(`../../../packages/server-legal/src/adapters/${name}`, import.meta.url), 'utf8');

const HEAD = 'FROM legal_envelopes e JOIN legal_signers s ON s.envelope_id=e.id WHERE ';

const members = [
  {
    file: 'legal/portal.ts',
    projection: 'e.id,e.state,e.consent,e.expires_at,s.state signer_state,s.viewed_at',
    tail: 'e.item_id=? AND e.document_id=? AND s.user_id=? ORDER BY e.created_at DESC LIMIT 10',
  },
  {
    file: 'legal/portal.ts',
    projection: 'e.*,s.ordinal,s.viewed_at',
    tail: "e.id=? AND e.item_id=? AND e.document_id=? AND s.user_id=? AND e.state='sent' AND s.state='pending' AND e.expires_at>?",
  },
  {
    file: 'legal/signing.ts',
    projection: 'e.*,s.ordinal,s.state signer_state,s.viewed_at',
    tail: 'e.id=? AND e.item_id=? AND s.user_id=?',
  },
  {
    file: 'legal/operations.ts',
    projection: 'e.*,s.user_id,s.ordinal,s.state signer_state,s.signed_at,s.evidence',
    tail: 'e.item_id=? ORDER BY e.created_at,s.ordinal',
  },
  {
    file: 'legal/integrations.ts',
    projection: '1',
    tail: "e.item_id=i.id AND e.state='sent' AND e.expires_at>? AND s.user_id=? AND s.state='pending'",
  },
];

const callers: Record<string, number> = {
  'legal/portal.ts': 2,
  'legal/signing.ts': 1,
  'legal/operations.ts': 1,
  'legal/integrations.ts': 1,
};

describe('legal read-tail r43 parameterized envelope/signer read head family', () => {
  it('renders byte-identical SQL for every member', () => {
    for (const m of members) expect(legalEnvelopeSignerReadHead(m.projection) + m.tail).toBe(`SELECT ${m.projection} ${HEAD}${m.tail}`);
  });

  it('keeps the head read-only (no writes or row locks)', () => {
    const all = members.map(m => legalEnvelopeSignerReadHead(m.projection)).join(' ');
    expect(all).toMatch(/SELECT \S+ FROM legal_envelopes e JOIN legal_signers s/);
    expect(all).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
  });

  it('keeps one definition and removes every inline head from the callers', async () => {
    const shared = await src('legal-read-tail-r43-sources.ts');
    expect(shared.match(/export const legalEnvelopeSignerReadHead/g) ?? []).toHaveLength(1);
    for (const [file, count] of Object.entries(callers)) {
      const text = await src(file);
      expect(text.match(/legalEnvelopeSignerReadHead\(/g) ?? []).toHaveLength(count);
      expect(text).not.toContain(HEAD);
    }
  });

  it('leaves the corpus/citation/lock-bearing reads intact', async () => {
    const [corpus, citations, r9, destructionWorker] = await Promise.all([
      src('legal/law-corpus.ts'), src('legal/citations.ts'),
      src('legal-read-tail-r9-sources.ts'), src('legal/destruction-worker.ts'),
    ]);
    expect(corpus).toContain('legal_law_source_checks');
    expect(citations).toContain('export function legalCitation');
    expect(r9).toContain('legalFilesAdvisoryLockSql');
    expect(r9).toContain('legalCompanyAdvisoryLockSql');
    expect(destructionWorker).toContain('FOR UPDATE OF x,i');
  });

  it('leaves r9/r15/r16/r18-r42 helpers untouched', async () => {
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
      'legalShareDocumentReadHead', 'legalFeatureSearchSql',
    ];
    for (const symbol of symbols) expect(texts.join('\n')).toContain(`export const ${symbol}`);
  });

  it('retirement:ts-statements stays strictly below the recorded BEFORE=216', async () => {
    let total = 0;
    const walk = async (dir: string) => {
      for (const entry of await readdir(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) await walk(p);
        else if (entry.name.endsWith('.ts')) total += ((await readFile(p, 'utf8')).match(/SELECT /g) ?? []).length;
      }
    };
    await walk(srcRoot);
    expect(total).toBeLessThan(216);
  });
});

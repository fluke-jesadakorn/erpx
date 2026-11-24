import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import { legalShareDocumentReadHead } from '../../../packages/server-legal/src/adapters/legal-read-tail-r41-sources';

const srcRoot = resolve(__dirname, '../../../packages/server-legal/src');
const src = (name: string) => readFile(new URL(`../../../packages/server-legal/src/adapters/${name}`, import.meta.url), 'utf8');

const JOIN = 'FROM legal_shares s JOIN legal_items i ON i.id=s.item_id JOIN legal_documents d ON d.id=s.document_id AND d.item_id=i.id ';

const members = [
  {
    file: 'portal-purchases.ts',
    call: "legalShareDocumentReadHead('s.id,s.item_id,s.document_id,s.version,i.company_id,i.kind')",
    projection: 's.id,s.item_id,s.document_id,s.version,i.company_id,i.kind',
    tail: 'WHERE s.id=? AND lower(s.email)=? AND (s.user_id IS NULL OR s.user_id=?) AND s.revoked_at IS NULL AND s.expires_at>${packageClockSql}',
    oracle: 'SELECT s.id,s.item_id,s.document_id,s.version,i.company_id,i.kind ' + JOIN + 'WHERE s.id=? AND lower(s.email)=? AND (s.user_id IS NULL OR s.user_id=?) AND s.revoked_at IS NULL AND s.expires_at>${packageClockSql}',
  },
  {
    file: 'portal-purchases.ts',
    call: 'legalShareDocumentReadHead("jsonb_build_object(\'share\',s.id,\'shareVersion\',s.version,\'item\',i.id,\'company\',i.company_id,\'kind\',i.kind,\'document\',d.id,\'documentVersion\',d.version,\'checksum\',d.checksum,\'bytes\',d.bytes,\'link\',d.attachment_link_id,\'source\',d.attachment_source_id,\'object\',d.object_key)::text image")',
    projection: "jsonb_build_object('share',s.id,'shareVersion',s.version,'item',i.id,'company',i.company_id,'kind',i.kind,'document',d.id,'documentVersion',d.version,'checksum',d.checksum,'bytes',d.bytes,'link',d.attachment_link_id,'source',d.attachment_source_id,'object',d.object_key)::text image",
    tail: 'WHERE s.id=? AND ${predicate}',
    oracle: "SELECT jsonb_build_object('share',s.id,'shareVersion',s.version,'item',i.id,'company',i.company_id,'kind',i.kind,'document',d.id,'documentVersion',d.version,'checksum',d.checksum,'bytes',d.bytes,'link',d.attachment_link_id,'source',d.attachment_source_id,'object',d.object_key)::text image " + JOIN + 'WHERE s.id=? AND ${predicate}',
  },
  {
    file: 'enrollment-continuation.ts',
    call: "legalShareDocumentReadHead('s.id')",
    projection: 's.id',
    tail: "JOIN legal_settings settings ON settings.company_id=i.company_id AND settings.enabled WHERE s.id=? AND lower(s.email)=? AND (s.user_id IS NULL OR s.user_id=?) AND s.revoked_at IS NULL AND s.expires_at>floor(extract(epoch FROM clock_timestamp())*1000)::bigint AND ${await legalPortalPurchasePredicate('i')} ${legalActiveDestructionGuard()}",
    oracle: 'SELECT s.id ' + JOIN + "JOIN legal_settings settings ON settings.company_id=i.company_id AND settings.enabled WHERE s.id=? AND lower(s.email)=? AND (s.user_id IS NULL OR s.user_id=?) AND s.revoked_at IS NULL AND s.expires_at>floor(extract(epoch FROM clock_timestamp())*1000)::bigint AND ${await legalPortalPurchasePredicate('i')} ${legalActiveDestructionGuard()}",
  },
  {
    file: 'legal/portal.ts',
    call: "legalShareDocumentReadHead('s.*,i.company_id,i.branch_id,i.department_id,i.version AS item_version,i.kind')",
    projection: 's.*,i.company_id,i.branch_id,i.department_id,i.version AS item_version,i.kind',
    tail: "WHERE ${await legalPortalPurchasePredicate('i',strictHistory)} AND s.id=? AND lower(s.email)=? AND (s.user_id IS NULL OR s.user_id=?) AND s.revoked_at IS NULL AND s.expires_at>?",
    oracle: 'SELECT s.*,i.company_id,i.branch_id,i.department_id,i.version AS item_version,i.kind ' + JOIN + "WHERE ${await legalPortalPurchasePredicate('i',strictHistory)} AND s.id=? AND lower(s.email)=? AND (s.user_id IS NULL OR s.user_id=?) AND s.revoked_at IS NULL AND s.expires_at>?",
  },
  {
    file: 'legal/enrollment.ts',
    call: "legalShareDocumentReadHead('s.email,s.id')",
    projection: 's.email,s.id',
    tail: "JOIN legal_settings c ON c.company_id=i.company_id AND c.enabled WHERE s.id=? AND s.user_id IS NULL AND s.revoked_at IS NULL AND s.expires_at>? AND ${await legalPortalPurchasePredicate('i')} ${legalActiveDestructionGuard()}",
    oracle: 'SELECT s.email,s.id ' + JOIN + "JOIN legal_settings c ON c.company_id=i.company_id AND c.enabled WHERE s.id=? AND s.user_id IS NULL AND s.revoked_at IS NULL AND s.expires_at>? AND ${await legalPortalPurchasePredicate('i')} ${legalActiveDestructionGuard()}",
  },
];

describe('legal read-tail r41 parameterized share-document read-head family', () => {
  it('renders byte-identical SQL for every member', () => {
    for (const m of members) expect(legalShareDocumentReadHead(m.projection) + m.tail).toBe(m.oracle);
  });

  it('keeps the head read-only (no writes or row locks)', () => {
    const all = members.map(m => legalShareDocumentReadHead(m.projection)).join(' ');
    expect(all).toMatch(/SELECT \S+ FROM legal_shares s JOIN legal_items i ON i\.id=s\.item_id/);
    expect(all).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
  });

  it('keeps one definition and removes every inline head from the five callers', async () => {
    const shared = await src('legal-read-tail-r41-sources.ts');
    expect(shared.match(/export const legalShareDocumentReadHead/g) ?? []).toHaveLength(1);
    for (const file of [...new Set(members.map(m => m.file))]) {
      const text = await src(file);
      expect(text).toContain('legalShareDocumentReadHead(');
      expect(text).not.toContain(`SELECT ${members.find(m => m.file === file)!.projection} ${JOIN}`);
    }
    for (const m of members) expect(await src(m.file)).toContain(m.call);
  });

  it('leaves the corpus/citation/lock-bearing reads intact', async () => {
    const [corpus, citations, r9, r24, destructionWorker] = await Promise.all([
      src('legal/law-corpus.ts'), src('legal/citations.ts'),
      src('legal-read-tail-r9-sources.ts'), src('legal-read-tail-r24-sources.ts'),
      src('legal/destruction-worker.ts'),
    ]);
    expect(corpus).toContain('legal_law_source_checks');
    expect(citations).toContain('export function legalCitation');
    expect(r9).toContain('legalFilesAdvisoryLockSql');
    expect(r9).toContain('legalCompanyAdvisoryLockSql');
    expect(r24).toContain('legalHoldTargetGuardPrefix');
    expect(destructionWorker).toContain('FOR UPDATE OF x,i');
  });

  it('leaves r9/r15/r16/r18-r40 helpers untouched', async () => {
    const names = [
      'legal-read-tail-r9-sources.ts', 'legal-read-tail-r15-sources.ts', 'legal-read-tail-r16-sources.ts',
      'legal-read-tail-r18-sources.ts', 'legal-read-tail-r19-sources.ts', 'legal-read-tail-r20-sources.ts',
      'legal-read-tail-r21-sources.ts', 'legal-read-tail-r22-sources.ts', 'legal-read-tail-r23-sources.ts',
      'legal-read-tail-r24-sources.ts', 'legal-read-tail-r25-sources.ts', 'legal-read-tail-r26-sources.ts',
      'legal-read-tail-r27-sources.ts', 'legal-read-tail-r28-sources.ts', 'legal-read-tail-r29-sources.ts',
      'legal-read-tail-r30-sources.ts', 'legal-read-tail-r31-sources.ts', 'legal-read-tail-r32-sources.ts',
      'legal-read-tail-r33-sources.ts', 'legal-read-tail-r34-sources.ts', 'legal-read-tail-r35-sources.ts',
      'legal-read-tail-r36-sources.ts', 'legal-read-tail-r38-sources.ts', 'legal-read-tail-r39-sources.ts',
      'legal-read-tail-r40-sources.ts',
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
    ];
    for (const symbol of symbols) expect(texts.join('\n')).toContain(`export const ${symbol}`);
  });

  it('retirement:ts-statements stays strictly below the recorded BEFORE=221', async () => {
    let total = 0;
    const walk = async (dir: string) => {
      for (const entry of await readdir(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) await walk(p);
        else if (entry.name.endsWith('.ts')) total += ((await readFile(p, 'utf8')).match(/SELECT /g) ?? []).length;
      }
    };
    await walk(srcRoot);
    expect(total).toBeLessThan(221);
  });
});

import { createHash } from 'node:crypto';
import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import { legalRecentItemListSql } from '../../../packages/server-legal/src/adapters/legal-read-tail-r31-sources';
import { legalItemOrderedSql } from '../../../packages/server-legal/src/adapters/legal-read-tail-r35-sources';

const sha256 = (s: string) => createHash('sha256').update(s).digest('hex');
const srcRoot = resolve(__dirname, '../../../packages/server-legal/src');
const src = (name: string) => readFile(new URL(`../../../packages/server-legal/src/adapters/${name}`, import.meta.url), 'utf8');

/** Independent pre-r61 byte + SHA oracles captured from the retired r31 inline
 * recent-item literals, one per surviving call site. Both projections bind only
 * the item id and carry a fixed created_at DESC order and row limit. */
const ORIGINALS: Array<[name: string, columns: string, table: string, limit: number, sql: string, sha: string, binds: number]> = [
  ['comments', 'id,body,visibility,created_by,created_at', 'legal_comments', 100,
    'SELECT id,body,visibility,created_by,created_at FROM legal_comments WHERE item_id=? ORDER BY created_at DESC LIMIT 100',
    '7f67df7bd79efeacd4c77dd4c290322e2c29138005cadb7565a0d4c451a848c0', 1],
  ['envelopes', 'id,state,document_id,expires_at,consent', 'legal_envelopes', 20,
    'SELECT id,state,document_id,expires_at,consent FROM legal_envelopes WHERE item_id=? ORDER BY created_at DESC LIMIT 20',
    'c460e73380c2dadb64a257dc699d39a32b4f321c44680dcf324d2257105097e3', 1],
];

describe('legal read-tail r61 recent-item list ordered-head reuse', () => {
  it('renders both call-site projections byte-identically to the pre-r61 oracles', () => {
    expect(ORIGINALS).toHaveLength(2);
    for (const [name, columns, table, limit, sql, digest] of ORIGINALS) {
      expect(legalRecentItemListSql(columns, table, limit), name).toBe(sql);
      expect(sha256(legalRecentItemListSql(columns, table, limit)), name).toBe(digest);
    }
  });

  it('folds both recent-item lists onto the single r35 ordered head with no inlined SELECT', () => {
    for (const [name, columns, table, limit, sql] of ORIGINALS) {
      // Independent base oracle: the r35 ordered-list head with the fixed order/limit.
      expect(legalItemOrderedSql(columns, table, 'created_at DESC', limit), name).toBe(sql);
      expect(legalRecentItemListSql(columns, table, limit), name)
        .toBe(legalItemOrderedSql(columns, table, 'created_at DESC', limit));
    }
  });

  it('preserves bind counts and rejects write or lock text', () => {
    for (const [name, columns, table, limit, sql, , binds] of ORIGINALS) {
      expect((sql.match(/\?/g) ?? []).length, name).toBe(binds);
      expect(legalRecentItemListSql(columns, table, limit), name).toBe(sql);
      expect(sql, name).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
    // The single bind is the item id, and it precedes the fixed order/limit tail.
    expect(ORIGINALS[0][4].indexOf('id=?')).toBeLessThan(ORIGINALS[0][4].indexOf('ORDER BY'));
  });

  it('leaves one zero-token head delegating to one ordered renderer with unchanged call sites', async () => {
    const [r31, r35, service] = await Promise.all([
      src('legal-read-tail-r31-sources.ts'), src('legal-read-tail-r35-sources.ts'), src('legal/service.ts'),
    ]);
    expect(r31.match(/SELECT /g) ?? []).toHaveLength(0);
    expect(r31.match(/export const legalRecentItemListSql/g) ?? []).toHaveLength(1);
    expect(r31).toContain("from './legal-read-tail-r35-sources'");
    expect(r31).toContain("legalItemOrderedSql(columns, table, 'created_at DESC', limit)");
    // R68: r35 now delegates its item-key head to r54, so it is zero-token too.
    expect((r35.match(/SELECT /g) ?? []).length).toBe(0);
    expect(r35.match(/export const legalItemOrderedSql/g) ?? []).toHaveLength(1);
    // The comments/envelopes call sites keep their exact projections, binds and first<T> shapes.
    expect(service).toContain('legalRecentItemListSql("id,body,visibility,created_by,created_at","legal_comments",100)');
    expect(service).toContain('legalRecentItemListSql("id,state,document_id,expires_at,consent","legal_envelopes",20)');
    for (const [, , , , sql] of ORIGINALS) {
      expect(service).not.toContain(sql);
    }
  });

  it('leaves the ordered head, corpus/citation, locks and the r9-r30 families untouched', async () => {
    const names = [
      'legal/operations.ts', 'legal/law-corpus.ts', 'legal/citations.ts', 'legal/access.ts',
      'legal/destruction-worker.ts', 'portal-purchases.ts',
      'legal-read-tail-r9-sources.ts', 'legal-read-tail-r15-sources.ts', 'legal-read-tail-r16-sources.ts',
      'legal-read-tail-r18-sources.ts', 'legal-read-tail-r19-sources.ts', 'legal-read-tail-r20-sources.ts',
      'legal-read-tail-r21-sources.ts', 'legal-read-tail-r22-sources.ts', 'legal-read-tail-r23-sources.ts',
      'legal-read-tail-r24-sources.ts', 'legal-read-tail-r25-sources.ts', 'legal-read-tail-r26-sources.ts',
      'legal-read-tail-r27-sources.ts', 'legal-read-tail-r28-sources.ts', 'legal-read-tail-r29-sources.ts',
      'legal-read-tail-r30-sources.ts',
    ];
    const [ops, corpus, citations, access, worker, purchases, ...others] = await Promise.all(names.map(src));
    // The r35 ordered head still backs documents/accesses/retention and the shares call site.
    expect(ops).toContain('legalItemOrderedSql("id,version,name,checksum,bytes,media_type,locked,created_at","legal_documents","version")');
    expect(ops).toContain('legalItemOrderedSql("id,state,manifest,requested_by,approved_by,approved_at,completed_at","legal_destructions","created_at")');
    expect(corpus).toContain("nativeLegalAuxiliaryRead<LawCorpusView>(a,'laws',ruleId)");
    expect(citations).toContain('export function legalCitation');
    expect(access).toContain('legalCompanyAdvisoryLockSql');
    expect(worker).toContain('FOR UPDATE OF x,i');
    expect(purchases).toContain('LOCK TABLE legal_settings,legal_destructions,legal_comments,legal_envelopes,legal_signers,legal_counsel_entries IN SHARE ROW EXCLUSIVE MODE');
    const symbols = [
      'legalSettingsEnabledSql', 'legalManagerAvailableSql', 'legalDestructionActiveSql',
      'legalDocumentsItemPairSql', 'legalDocumentByIdSql', 'legalMembersAggSql',
      'legalFilesAdvisoryLockSql', 'legalCompanyAdvisoryLockSql', 'legalLaterApprovalGuardSql',
      'legalItemSelectSql', 'legalRetentionPolicySelectSql', 'legalActorScopeProjectionSql',
      'legalEventsByItemSql', 'legalRateCountPrefix', 'legalDisposedDocumentPrefix',
      'legalPurchaseClockCteSql', 'legalApprovedCycleSql', 'legalHoldActiveSql',
      'legalHoldTargetGuardPrefix', 'legalMemberEditExistsSql', 'legalJsonbAggByItemSql',
      'legalVersionExistsSql', 'legalByIdAndItemSql', 'legalByIdAndCompanySql',
      'legalPurchaseClockExistsSql', 'legalOrderedHistorySql',
    ];
    for (const symbol of symbols) {
      expect(others.join('\n')).toContain(`export const ${symbol}`);
    }
  });

  it('retirement:ts-statements is BEFORE=138 -> AFTER=137', async () => {
    let total = 0;
    const walk = async (dir: string) => {
      for (const entry of await readdir(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) await walk(p);
        else if (entry.name.endsWith('.ts')) total += ((await readFile(p, 'utf8')).match(/SELECT /g) ?? []).length;
      }
    };
    await walk(srcRoot);
    expect(total).toBeLessThan(138);
    expect(total).toBeLessThanOrEqual(137);
  });
});

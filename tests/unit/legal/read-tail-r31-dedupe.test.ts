import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import { legalRecentItemListSql } from '../../../packages/server-legal/src/adapters/legal-read-tail-r31-sources';

const srcRoot = resolve(__dirname, '../../../packages/server-legal/src');
const src = (name: string) => readFile(new URL(`../../../packages/server-legal/src/adapters/${name}`, import.meta.url), 'utf8');
const recent = (columns: string, table: string, limit: number) =>
  `SELECT ${columns} FROM ${table} WHERE item_id=? ORDER BY created_at DESC LIMIT ${limit}`;

const members: [string, string, number][] = [
  ['id,body,visibility,created_by,created_at', 'legal_comments', 100],
  ['id,state,document_id,expires_at,consent', 'legal_envelopes', 20],
];

describe('legal read-tail r31 parameterized family dedupe', () => {
  it('renders byte-identical recent-item list SQL for both legal detail fact tables', () => {
    for (const [columns, table, limit] of members) {
      expect(legalRecentItemListSql(columns, table, limit)).toBe(recent(columns, table, limit));
    }
  });

  it('keeps the fragment read-only (no writes or row locks)', () => {
    const all = members.map(([c, t, l]) => legalRecentItemListSql(c, t, l)).join(' ');
    expect(all).toMatch(/SELECT \w+(,\w+){4} FROM legal_\w+ WHERE item_id=\? ORDER BY created_at DESC LIMIT \d+/);
    expect(all).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
  });

  it('keeps one definition and no inline family literals in the caller', async () => {
    const [shared, service] = await Promise.all([
      src('legal-read-tail-r31-sources.ts'),
      src('legal/service.ts'),
    ]);
    expect(shared.match(/export const legalRecentItemListSql/g) ?? []).toHaveLength(1);
    expect(service).toContain('legalRecentItemListSql("id,body,visibility,created_by,created_at","legal_comments",100)');
    expect(service).toContain('legalRecentItemListSql("id,state,document_id,expires_at,consent","legal_envelopes",20)');
    expect(service).not.toMatch(/SELECT id,body,visibility,created_by,created_at FROM legal_comments WHERE item_id=\? ORDER BY created_at DESC LIMIT 100"/);
    expect(service).not.toMatch(/SELECT id,state,document_id,expires_at,consent FROM legal_envelopes WHERE item_id=\? ORDER BY created_at DESC LIMIT 20"/);
  });

  it('leaves corpus/citation, locks and the r9/r15-r30 families untouched', async () => {
    const names = [
      'legal-read-tail-r9-sources.ts', 'legal-read-tail-r15-sources.ts', 'legal-read-tail-r16-sources.ts',
      'legal-read-tail-r18-sources.ts', 'legal-read-tail-r19-sources.ts', 'legal-read-tail-r20-sources.ts',
      'legal-read-tail-r21-sources.ts', 'legal-read-tail-r22-sources.ts', 'legal-read-tail-r23-sources.ts',
      'legal-read-tail-r24-sources.ts', 'legal-read-tail-r25-sources.ts', 'legal-read-tail-r26-sources.ts',
      'legal-read-tail-r27-sources.ts', 'legal-read-tail-r28-sources.ts', 'legal-read-tail-r29-sources.ts',
      'legal-read-tail-r30-sources.ts',
      'legal/law-corpus.ts', 'legal/citations.ts', 'legal/access.ts', 'legal/destruction-worker.ts',
    ];
    const texts = await Promise.all(names.map(src));
    const symbols = [
      'legalDocumentsItemPairSql', 'legalLaterApprovalGuardSql', 'legalItemSelectSql',
      'legalRetentionPolicySelectSql', 'legalRateCountPrefix', 'legalDisposedDocumentPrefix',
      'legalPurchaseClockCteSql', 'legalPurchaseClockExistsSql', 'legalApprovedCycleSql',
      'legalHoldActiveSql', 'legalHoldTargetGuardPrefix', 'legalJsonbAggByItemSql', 'legalVersionExistsSql',
      'legalByIdAndItemSql', 'legalByIdAndCompanySql', 'legalMemberEditExistsSql', 'legalEventsByItemSql',
      'legalOrderedHistorySql',
    ];
    for (const symbol of symbols) {
      expect(texts.slice(0, 16).join('\n')).toContain(`export const ${symbol}`);
    }
    expect(texts[16]).toContain('legal_law_source_checks');
    expect(texts[17]).toContain('export function legalCitation');
    expect(texts[18]).toContain('legalCompanyAdvisoryLockSql');
    expect(texts[19]).toContain('FOR UPDATE OF x,i');
  });

  it('retirement:ts-statements stays strictly below the recorded BEFORE=242', async () => {
    let total = 0;
    const walk = async (dir: string) => {
      for (const entry of await readdir(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) await walk(p);
        else if (entry.name.endsWith('.ts')) total += ((await readFile(p, 'utf8')).match(/SELECT/g) ?? []).length;
      }
    };
    await walk(srcRoot);
    expect(total).toBeLessThan(242);
    expect(total).toBeLessThanOrEqual(241);
  });
});

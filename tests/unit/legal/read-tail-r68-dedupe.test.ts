import { createHash } from 'node:crypto';
import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import { legalItemOrderedSql } from '../../../packages/server-legal/src/adapters/legal-read-tail-r35-sources';
import { legalItemKeyHeadSql } from '../../../packages/server-legal/src/adapters/legal-read-tail-r54-sources';
import { legalPointByKeySql } from '../../../packages/server-legal/src/adapters/legal-read-tail-r36-sources';
import { legalRecentItemListSql } from '../../../packages/server-legal/src/adapters/legal-read-tail-r31-sources';

const sha256 = (s: string) => createHash('sha256').update(s).digest('hex');
const srcRoot = resolve(__dirname, '../../../packages/server-legal/src');
const src = (name: string) => readFile(new URL(`../../../packages/server-legal/src/adapters/${name}`, import.meta.url), 'utf8');

/** Independent pre-r68 byte + SHA oracles for the seven r35 item-ordered members.
 * Every member renders one `item_id=?` bind and keeps its exact ORDER BY/LIMIT tail. */
const members: [string, string, string, number | undefined, string, string][] = [
  ['id,version,name,checksum,bytes,media_type,locked,created_at', 'legal_documents', 'version', undefined,
    'SELECT id,version,name,checksum,bytes,media_type,locked,created_at FROM legal_documents WHERE item_id=? ORDER BY version',
    '0ded41f8d9c4ed2cd39ffc4bdf8d5bfa13437cff1e3ba5aed8c90c77c0e857fa'],
  ['id,state,manifest,requested_by,approved_by,approved_at,completed_at', 'legal_destructions', 'created_at', undefined,
    'SELECT id,state,manifest,requested_by,approved_by,approved_at,completed_at FROM legal_destructions WHERE item_id=? ORDER BY created_at',
    '85d8c091b7dfec329790030c6e83fd4ccd09dd0d4f02768f3b7f71d0bcd1b5c5'],
  ['*', 'legal_documents', 'version DESC', 100,
    'SELECT * FROM legal_documents WHERE item_id=? ORDER BY version DESC LIMIT 100',
    'c88e980e7eb2bb92bc532e246c5c3d272cb5f2b86fa7c022bf865bd3f7eb2336'],
  ['*', 'legal_access_events', 'created_at,id', 10001,
    'SELECT * FROM legal_access_events WHERE item_id=? ORDER BY created_at,id LIMIT 10001',
    '38a8c782b1c5dcd2f7527ea6022a1bd156d575e79df4ac3ec46ec3b16180b66f'],
  ['id,email,document_id,user_id,expires_at,revoked_at', 'legal_shares', 'created_at DESC', 100,
    'SELECT id,email,document_id,user_id,expires_at,revoked_at FROM legal_shares WHERE item_id=? ORDER BY created_at DESC LIMIT 100',
    '8e01151c15b64d96aa69e4341b4085b4f5c0b51e4455e2e16be9b1cc89d42c06'],
  ['*', 'legal_events', 'created_at', undefined,
    'SELECT * FROM legal_events WHERE item_id=? ORDER BY created_at',
    '4d7d0cb8d59125c244ee4eed960db3fef5f58dfdc37e1434b38437d0d1384880'],
  ['x', 'legal_items', 'created_at DESC', 7,
    'SELECT x FROM legal_items WHERE item_id=? ORDER BY created_at DESC LIMIT 7',
    'a461aa854b98ee56ef5f7ceb6d0bd1003d627e2b2bb5021e1bc3d81f68d2de80'],
];

describe('legal read-tail r68 item-ordered head reuse', () => {
  it('renders every folded head byte-identically to its pre-r68 oracle', () => {
    for (const [columns, table, order, limit, oracle, oracleSha] of members) {
      const head = legalItemOrderedSql(columns, table, order, limit);
      expect(head).toBe(oracle);
      expect(sha256(head)).toBe(oracleSha);
      expect(head).toBe(
        legalItemKeyHeadSql(columns, table) + ` ORDER BY ${order}${limit === undefined ? '' : ` LIMIT ${limit}`}`,
      );
      expect(head).toBe(
        legalPointByKeySql(columns, table, 'item_id') + ` ORDER BY ${order}${limit === undefined ? '' : ` LIMIT ${limit}`}`,
      );
      expect((head.match(/\?/g) ?? []).length).toBe(1);
    }
  });

  it('keeps the r31 recent-item projection byte-identical', () => {
    const recent = legalRecentItemListSql('*', 'legal_items', 25);
    expect(recent).toBe('SELECT * FROM legal_items WHERE item_id=? ORDER BY created_at DESC LIMIT 25');
    expect(sha256(recent)).toBe('cbd36ca8b8cccbc107daaccc9b74d19bd48b1e5917ffe9d856604bc538fea6a0');
  });

  it('leaves r35 delegating to the single verified r54/r36 item-key head', async () => {
    const [r35, r54, r36] = await Promise.all([
      src('legal-read-tail-r35-sources.ts'), src('legal-read-tail-r54-sources.ts'),
      src('legal-read-tail-r36-sources.ts'),
    ]);
    expect(r35).toContain("from './legal-read-tail-r54-sources'");
    expect(r35).toContain('legalItemKeyHeadSql(columns, table)');
    expect(r35).not.toContain('SELECT ');
    expect((r35.match(/export const legalItemOrderedSql/g) ?? []).length).toBe(1);
    const line = r35.split('\n').find(l => l.includes('legalItemKeyHeadSql(columns, table)'))!;
    expect(line).not.toContain('SELECT ');
    expect((r54.match(/export const legalItemKeyHeadSql/g) ?? []).length).toBe(1);
    expect((r36.match(/export const legalPointByKeySql/g) ?? []).length).toBe(1);
    expect(r54).toContain("legalPointByKeySql(columns, table, 'item_id')");
  });

  it('keeps every caller and its locks/native text unchanged', async () => {
    const [operations, service, r18, r31, worker, r9] = await Promise.all([
      src('legal/operations.ts'), src('legal/service.ts'), src('legal-read-tail-r18-sources.ts'),
      src('legal-read-tail-r31-sources.ts'), src('legal/destruction-worker.ts'),
      src('legal-read-tail-r9-sources.ts'),
    ]);
    expect(operations).toContain('legalItemOrderedSql("id,version,name,checksum,bytes,media_type,locked,created_at","legal_documents","version")');
    expect(operations).toContain('legalItemOrderedSql("id,state,manifest,requested_by,approved_by,approved_at,completed_at","legal_destructions","created_at")');
    expect(operations).toContain('legalItemOrderedSql("*","legal_access_events","created_at,id",10001)');
    expect(service).toContain('legalItemOrderedSql("*","legal_documents","version DESC",100)');
    expect(service).toContain('legalItemOrderedSql("id,email,document_id,user_id,expires_at,revoked_at","legal_shares","created_at DESC",100)');
    expect(r18).toContain("legalItemOrderedSql('*','legal_events','created_at')");
    expect(r31).toContain('legalItemOrderedSql(columns, table, \'created_at DESC\', limit)');
    for (const text of [operations, service]) {
      expect(text).not.toContain('SELECT * FROM legal_documents WHERE item_id=? ORDER BY version DESC LIMIT 100');
    }
    expect(worker).toContain('FOR UPDATE OF x,i');
    expect(r9).toContain('legalFilesAdvisoryLockSql');
    expect(r9).toContain('legalCompanyAdvisoryLockSql');
    expect(r9).toContain('pg_advisory_xact_lock');
  });

  it('retirement:ts-statements is BEFORE=131 -> AFTER=130', async () => {
    let total = 0;
    const walk = async (dir: string) => {
      for (const entry of await readdir(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) await walk(p);
        else if (entry.name.endsWith('.ts')) total += ((await readFile(p, 'utf8')).match(/SELECT /g) ?? []).length;
      }
    };
    await walk(srcRoot);
    expect(total).toBeLessThan(131);
    expect(total).toBeLessThanOrEqual(130);
    expect(total).toBe(91);
  });
});

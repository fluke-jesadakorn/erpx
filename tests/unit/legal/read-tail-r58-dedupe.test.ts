import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import { legalItemKeyHeadSql } from '../../../packages/server-legal/src/adapters/legal-read-tail-r54-sources';
import { legalPointByKeySql } from '../../../packages/server-legal/src/adapters/legal-read-tail-r36-sources';
import { legalDisposedGuardSql } from '../../../packages/server-legal/src/adapters/legal-read-tail-r39-sources';

const srcRoot = resolve(__dirname, '../../../packages/server-legal/src');
const src = (name: string) => readFile(new URL(`../../../packages/server-legal/src/adapters/${name}`, import.meta.url), 'utf8');

/** Independent pre-r58 byte oracle captured from the removed inline holder. */
const documentsSql = 'SELECT id,checksum,object_key,attachment_source_id,attachment_link_id FROM legal_documents WHERE item_id=? AND NOT EXISTS(SELECT 1 FROM legal_disposals disposed WHERE disposed.document_id=legal_documents.id)';
const columns = 'id,checksum,object_key,attachment_source_id,attachment_link_id';
const guard = legalDisposedGuardSql('disposed', 'legal_documents.id');

describe('legal read-tail r58 destruction manifest documents head reuse', () => {
  it('renders byte-identical SQL through the r54 item-key head plus the existing disposed guard', () => {
    expect(legalItemKeyHeadSql(columns, 'legal_documents') + ` AND ${guard}`).toBe(documentsSql);
    // Independent base oracle: the r36 arbitrary-key head with the fixed item_id key and the same guard.
    expect(legalPointByKeySql(columns, 'legal_documents', 'item_id') + ` AND ${guard}`).toBe(documentsSql);
    expect(legalItemKeyHeadSql(columns, 'legal_documents')).toBe(legalPointByKeySql(columns, 'legal_documents', 'item_id'));
  });

  it('keeps the original single bind, disposed guard and read-only text', () => {
    const rendered = legalItemKeyHeadSql(columns, 'legal_documents') + ` AND ${guard}`;
    expect((rendered.match(/\?/g) ?? []).length).toBe(1);
    expect(rendered).toContain(guard);
    expect(rendered).toBe(documentsSql);
    expect(rendered).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
  });

  it('adds zero tokens: both bases already carry their counted SELECT', async () => {
    const [r54, r36] = await Promise.all([
      src('legal-read-tail-r54-sources.ts'), src('legal-read-tail-r36-sources.ts'),
    ]);
    expect(r54).not.toContain('SELECT ');
    expect(r54.match(/export const legalItemKeyHeadSql/g) ?? []).toHaveLength(1);
    expect(r54).toContain("from './legal-read-tail-r36-sources'");
    expect(r36).toContain('export const legalPointByKeySql');
    expect((r36.match(/SELECT /g) ?? []).length).toBe(1);
  });

  it('removes the inline holder and leaves protected and excluded reads intact', async () => {
    const [destruction, citations, worker, r39, r44, r49] = await Promise.all([
      src('legal/destruction-sources.ts'), src('legal/citations.ts'), src('legal/destruction-worker.ts'),
      src('legal-read-tail-r39-sources.ts'), src('legal-read-tail-r44-sources.ts'), src('legal-read-tail-r49-sources.ts'),
    ]);
    expect(destruction).not.toContain(documentsSql);
    expect(destruction).not.toContain(`SELECT ${columns} FROM legal_documents WHERE item_id=?`);
    expect(destruction).toContain("legalItemKeyHeadSql('id,checksum,object_key,attachment_source_id,attachment_link_id','legal_documents')");
    expect(destruction).toContain("legalDisposedGuardSql('disposed','legal_documents.id')");
    // Sibling heads in the same adapter and the excluded citation/destruction writer reads are untouched.
    expect(destruction).toContain('legalExternalUploadShareJoinHead(');
    expect(destruction).toContain("legalCleanDocumentGuardHead('','object_key IS NOT DISTINCT FROM ?')");
    expect(citations).toContain('export function legalCitation');
    expect(worker).toContain('FOR UPDATE OF x,i');
    expect(r39).toContain('export const legalDisposedGuardSql');
    expect(r44).toContain('export const legalExternalUploadShareJoinHead');
    expect(r49).toContain('export const legalCleanDocumentGuardHead');
  });

  it('retirement:ts-statements is BEFORE=142 -> AFTER=141', async () => {
    let total = 0;
    const walk = async (dir: string) => {
      for (const entry of await readdir(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) await walk(p);
        else if (entry.name.endsWith('.ts')) total += ((await readFile(p, 'utf8')).match(/SELECT /g) ?? []).length;
      }
    };
    await walk(srcRoot);
    expect(total).toBeLessThan(142);
    expect(total).toBeLessThanOrEqual(141);
  });
});

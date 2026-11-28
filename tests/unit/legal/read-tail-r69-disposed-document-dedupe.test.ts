import { createHash } from 'node:crypto';
import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import { legalDisposedDocumentPrefix } from '../../../modules/legal/server/src/adapters/legal-read-tail-r20-sources';
import { legalPointByKeySql } from '../../../modules/legal/server/src/adapters/legal-read-tail-r36-sources';

const sha256 = (s: string) => createHash('sha256').update(s).digest('hex');
const srcRoot = resolve(__dirname, '../../../modules/legal/server/src');
const src = (name: string) => readFile(new URL(`../../../modules/legal/server/src/adapters/${name}`, import.meta.url), 'utf8');

/** Independent pre-r69 byte + SHA oracle for the retired r20 inline head. */
const retired = 'SELECT 1 FROM legal_disposals WHERE document_id';
const oracle = 'ab0cf381595667b10eb86e9c58454429c9d086d40dcd1d8cba120da8cd524cbe';

describe('legal read-tail r69 disposed-document head reuse', () => {
  it('renders byte-identically to the pre-r69 oracle and the r36 point head', () => {
    expect(retired.match(/SELECT /g)).toHaveLength(1);
    expect(retired.match(/\?/g)).toBeNull();
    expect(sha256(retired)).toBe(oracle);
    expect(legalDisposedDocumentPrefix).toBe(retired);
    expect(sha256(legalDisposedDocumentPrefix)).toBe(oracle);
    const head = legalPointByKeySql('1', 'legal_disposals', 'document_id');
    expect(head).toBe(`${retired}=?`);
    expect(head.replace('=?', '')).toBe(legalDisposedDocumentPrefix);
    expect(head.match(/\?/g)).toHaveLength(1);
  });

  it('leaves r20 SELECT-free and delegating, with r36 the one point-read renderer', async () => {
    const [r20, r36] = await Promise.all([
      src('legal-read-tail-r20-sources.ts'),
      src('legal-read-tail-r36-sources.ts'),
    ]);
    expect(r20).not.toContain('SELECT');
    expect(r20).toContain("legalPointByKeySql('1', 'legal_disposals', 'document_id')");
    expect(r20.match(/export const legalDisposedDocumentPrefix/g) ?? []).toHaveLength(1);
    expect(r36.match(/export const legalPointByKeySql/g) ?? []).toHaveLength(1);
    for (const text of [r20, r36]) {
      expect(text).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
      expect(text).not.toMatch(/db\.prepare|sqlx|include_str!|fallback/i);
    }
  });

  it('keeps all three call sites and their bind order byte-identical', async () => {
    const [documents, copies] = await Promise.all([src('legal/documents.ts'), src('legal/executed-copies.ts')]);
    const point = `${legalDisposedDocumentPrefix}=?`;
    const inList = `${legalDisposedDocumentPrefix} IN (?,?)`;
    expect(point).toBe('SELECT 1 FROM legal_disposals WHERE document_id=?');
    expect(inList).toBe('SELECT 1 FROM legal_disposals WHERE document_id IN (?,?)');
    expect(point.match(/\?/g)).toHaveLength(1);
    expect(inList.match(/\?/g)).toHaveLength(2);
    expect(documents).toContain('${legalDisposedDocumentPrefix}=?');
    expect(documents).toContain('NOT EXISTS(${legalDisposedDocumentPrefix}=?)');
    expect(copies).toContain('NOT EXISTS(${legalDisposedDocumentPrefix} IN (?,?))');
    for (const [name, text] of [['documents', documents], ['executed-copies', copies]]) {
      expect(text).not.toContain(retired);
      expect(text).toMatch(/legalDisposedDocumentPrefix/);
    }
  });

  it('retirement:ts-statements lowers the legal SELECT KPI from 36 to 35', async () => {
    let total = 0;
    const walk = async (dir: string) => {
      for (const entry of await readdir(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) await walk(p);
        else if (entry.name.endsWith('.ts')) total += ((await readFile(p, 'utf8')).match(/SELECT /g) ?? []).length;
      }
    };
    await walk(srcRoot);
    expect(total).toBe(35);
    expect(total).toBeLessThan(36);
  });
});

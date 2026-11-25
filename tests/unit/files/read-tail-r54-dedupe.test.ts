import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { PUBLICATION_ID_HEAD_SQL } from '../../../packages/server-files/src/adapters/read-tail-r54-source-sql';

const rootDir = resolve(__dirname, '../../..');
const srcRoot = resolve(rootDir, 'packages/server-files/src');
// Fixed pre-r54 baseline: both inline publication-id heads are still literal in
// this commit, so the rendered-SQL oracles stay valid after the slice is
// integrated and HEAD advances.
const BASE = 'd107e637ea1c3b0173d308fcfa1d38a0701aa54a';
const HELPER = 'adapters/read-tail-r54-source-sql.ts';
const MIG = 'adapters/attachments/migration.ts';

const src = (rel: string) => readFileSync(resolve(srcRoot, rel), 'utf8');
const git = (...a: string[]) => execFileSync('git', a, { cwd: rootDir, encoding: 'utf8' });
const before = (rel: string) => git('show', `${BASE}:packages/server-files/src/${rel}`);
const count = (text: string, needle: string) => text.split(needle).length - 1;
const sha = (text: string) => createHash('sha256').update(text).digest('hex');
const tsFiles = (dir: string): string[] => readdirSync(dir, { withFileTypes: true }).flatMap(entry => {
  const path = resolve(dir, entry.name);
  return entry.isDirectory() ? tsFiles(path) : entry.name.endsWith('.ts') ? [path] : [];
});

/** The single retired inline head and the two divergent tails. */
const HEAD = 'SELECT publication.id';
type Site = {
  alias: 'dangling' | 'derivative';
  tail: string;
  sql: string;
  sqlSha: string;
  bytes: number;
  binds: number;
  orderBy: number;
};
const SITES: Site[] = [
  {
    alias: 'dangling',
    tail: ' FROM attachment_publications publication\n    LEFT JOIN attachment_links link ON link.id=publication.source_link_id\n    LEFT JOIN attachment_sets set_row ON set_row.id=link.set_id\n    WHERE set_row.id IS NULL OR publication.company_id IS DISTINCT FROM set_row.company_id ORDER BY publication.id',
    sql: 'SELECT publication.id FROM attachment_publications publication\n    LEFT JOIN attachment_links link ON link.id=publication.source_link_id\n    LEFT JOIN attachment_sets set_row ON set_row.id=link.set_id\n    WHERE set_row.id IS NULL OR publication.company_id IS DISTINCT FROM set_row.company_id ORDER BY publication.id',
    sqlSha: '6004941d7449bc3c5191f935b449c7ddcde52b40be78d44a63b502091631e174',
    bytes: 315,
    binds: 0,
    orderBy: 1,
  },
  {
    alias: 'derivative',
    tail: ',publication.company_id,publication.derivative_object_key,publication.derivative_bytes,publication.derivative_checksum_sha256,link.attachment_id\n    FROM attachment_publications publication LEFT JOIN attachment_links link ON link.id=publication.source_link_id',
    sql: 'SELECT publication.id,publication.company_id,publication.derivative_object_key,publication.derivative_bytes,publication.derivative_checksum_sha256,link.attachment_id\n    FROM attachment_publications publication LEFT JOIN attachment_links link ON link.id=publication.source_link_id',
    sqlSha: 'f3f6092adb3b3de84df681aba4f1f40d510af9eaf6ecafffa46bec1a020bf6fc',
    bytes: 280,
    binds: 0,
    orderBy: 0,
  },
];

const lockRe = /INSERT\s+INTO|DELETE\s+FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|FOR\s+KEY\s+SHARE|LOCK\s+TABLE|pg_advisory/i;
const renderSql = (site: Site) => PUBLICATION_ID_HEAD_SQL + site.tail;
/** Render the owner file back to BASE: substitute the composition form, drop the import. */
const render = (text: string) => text
  .split('\n')
  .filter(line => !line.includes('read-tail-r54-source-sql'))
  .join('\n')
  .replaceAll('${PUBLICATION_ID_HEAD_SQL}', HEAD);

describe('Files read-tail r54 per-site byte oracles', () => {
  test('the shared head is a 21-byte read-only publication-id projection', () => {
    expect(PUBLICATION_ID_HEAD_SQL).toBe(HEAD);
    expect(sha(HEAD)).toBe('250afc98b614bcda1ef5533a072b0bc5881b58c30d25660c68fcd687988542ef');
    expect(Buffer.byteLength(HEAD)).toBe(21);
    expect(count(HEAD, 'SELECT ')).toBe(1);
    expect(HEAD.startsWith('SELECT ')).toBe(true);
    expect(HEAD.endsWith('publication.id')).toBe(true);
    expect(HEAD).not.toMatch(lockRe);
  });

  for (const site of SITES) {
    test(`${site.alias} witness: head and tail render byte-exact`, () => {
      expect(renderSql(site)).toBe(site.sql);
      expect(sha(site.sql)).toBe(site.sqlSha);
      expect(Buffer.byteLength(site.sql)).toBe(site.bytes);
      expect(count(site.sql, 'SELECT ')).toBe(1);
      expect((site.sql.match(/\?/g) ?? []).length).toBe(site.binds);
      expect(count(site.sql, 'ORDER BY')).toBe(site.orderBy);
      expect(site.sql).not.toMatch(lockRe);
      expect(site.sql.startsWith(HEAD)).toBe(true);
      expect(site.tail.startsWith(HEAD)).toBe(false);
      expect(site.tail.length).toBe(site.bytes - HEAD.length);
    });
  }

  test('the two tails were one byte-identical head with divergent tails', () => {
    expect(SITES[0].tail).not.toBe(SITES[1].tail);
    expect(SITES[0].tail).toContain(' FROM attachment_publications publication');
    expect(SITES[1].tail).toContain(',publication.company_id,');
    expect(SITES[0].bytes).not.toBe(SITES[1].bytes);
    expect(SITES[0].orderBy).not.toBe(SITES[1].orderBy);
  });
});

describe('Files read-tail r54 publication-id head fold', () => {
  test('one owner-local renderer replaces the two inline heads', () => {
    const helper = src(HELPER);
    expect(count(helper, 'export const PUBLICATION_ID_HEAD_SQL =')).toBe(1);
    expect(count(helper, 'SELECT ')).toBe(1);
    expect(helper).not.toMatch(/prepare\(|\.bind\(|nativeWrite\(/);
    expect(helper.split('\n').find(l => l.startsWith('export const'))!).not.toMatch(lockRe);

    // No residual exact literal anywhere in the owner source except the helper.
    let residual = 0;
    for (const path of tsFiles(srcRoot)) residual += count(readFileSync(path, 'utf8'), HEAD);
    expect(residual).toBe(1);
  });

  test('each site keeps its own tail and neither introduces a bind or lock', () => {
    const mig = src(MIG);
    expect(count(mig, '${PUBLICATION_ID_HEAD_SQL}')).toBe(2);
    expect(mig).toContain('${PUBLICATION_ID_HEAD_SQL} FROM attachment_publications publication');
    expect(mig).toContain('${PUBLICATION_ID_HEAD_SQL},publication.company_id,publication.derivative_object_key');
    expect(mig).not.toContain('`SELECT publication.id FROM attachment_publications publication');
    expect(mig).not.toContain('`SELECT publication.id,publication.company_id');
  });

  test('rendered SQL is byte-identical to BASE; binds, locks and native writes unchanged', () => {
    const base = before(MIG);
    const after = src(MIG);
    expect(render(after)).toBe(base);
    for (const needle of [
      '.bind(', 'nativeWrite(', 'FOR UPDATE', 'FOR SHARE', 'FOR KEY SHARE',
      'LOCK TABLE', 'pg_advisory_xact_lock', 'ORDER BY',
    ]) {
      expect(count(after, needle)).toBe(count(base, needle));
    }

    // Excluded advisory/row-lock statements and native registry writes stay untouched.
    expect(base).toContain('nativeWrite(');
    expect(count(after, 'nativeWrite(')).toBe(24);
    const mig = src(MIG);
    expect(mig).toContain('ORPHAN_PRESERVATION_EVENT_HEAD_SQL');
    expect(mig).toContain('readReconcileAttachmentStorageEffect5()');
    expect(mig).toContain('readReconcileAttachmentStoragePublications()');
  });
});

describe('Files read-tail r54 KPI pins', () => {
  test('retirement:ts-statements is 221, strictly below the r53 BEFORE=222 (222->221)', () => {
    let literal = 0, total = 0;
    for (const path of tsFiles(srcRoot)) {
      const text = readFileSync(path, 'utf8');
      literal += (text.match(/SELECT /g) ?? []).length;
      total += (text.match(/SELECT\s/g) ?? []).length;
    }
    // Two inline heads -> one alias-free owner-local renderer (-1).
    expect(literal).toBe(221);
    expect(total).toBe(222);
    expect(literal).toBeLessThan(222);
    expect(total).toBeLessThan(223);
  });
});

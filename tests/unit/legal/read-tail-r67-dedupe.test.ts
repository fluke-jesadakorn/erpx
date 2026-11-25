import { createHash } from 'node:crypto';
import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import { legalReleasedItemGuardSql } from '../../../packages/server-legal/src/adapters/legal-read-tail-r38-sources';
import { legalPointByKeySql } from '../../../packages/server-legal/src/adapters/legal-read-tail-r36-sources';

const sha256 = (s: string) => createHash('sha256').update(s).digest('hex');
const srcRoot = resolve(__dirname, '../../../packages/server-legal/src');
const src = (name: string) => readFile(new URL(`../../../packages/server-legal/src/adapters/${name}`, import.meta.url), 'utf8');

/** Independent pre-r67 byte + SHA oracles for the two r38 released-item guard heads.
 * Each head has exactly two binds (`i.id=?` and `i.<key>=?`); every tail bind stays
 * at the call site. */
const members: ['company_id' | 'version', string, string][] = [
  ['company_id', 'SELECT 1 FROM legal_items i WHERE i.id=? AND i.company_id=?',
    '4f7733b4957ecaf6db090b76d3f96c3da0dd036063bd9e06c7a91d226b5bbb8d'],
  ['version', 'SELECT 1 FROM legal_items i WHERE i.id=? AND i.version=?',
    '5dab6fad673fd422cf7965bf4dc73fb40c904eb821c60bdbf49f370276234fa3'],
];

describe('legal read-tail r67 released-item point-guard head reuse', () => {
  it('renders every folded head byte-identically to its pre-r67 oracle', () => {
    for (const [key, oracle, oracleSha] of members) {
      const head = legalReleasedItemGuardSql(key);
      expect(head).toBe(oracle);
      expect(sha256(head)).toBe(oracleSha);
      expect(head).toBe(
        legalPointByKeySql('1', 'legal_items i', 'i.id' as Parameters<typeof legalPointByKeySql>[2]) +
        ` AND i.${key}=?`,
      );
      expect((head.match(/\?/g) ?? []).length).toBe(2);
    }
  });

  it('keeps the full guard strings and their binds byte-identical', () => {
    const guards: [string, string, string][] = [
      [legalReleasedItemGuardSql('company_id'),
        " AND (i.company_id=? OR i.kind IN ('rule','template'))",
        "SELECT 1 FROM legal_items i WHERE i.id=? AND i.company_id=? AND (i.company_id=? OR i.kind IN ('rule','template'))"],
      [legalReleasedItemGuardSql('version'),
        ' AND i.updated_at>?',
        'SELECT 1 FROM legal_items i WHERE i.id=? AND i.version=? AND i.updated_at>?'],
    ];
    for (const [head, tail, oracle] of guards) {
      expect(head + tail).toBe(oracle);
      expect((head + tail).match(/\?/g)?.length).toBe((oracle.match(/\?/g) ?? []).length);
    }
  });

  it('leaves r38 delegating to the single r36 point-read renderer', async () => {
    const [r38, r36] = await Promise.all([
      src('legal-read-tail-r38-sources.ts'), src('legal-read-tail-r36-sources.ts'),
    ]);
    expect(r38).toContain("from './legal-read-tail-r36-sources'");
    expect(r38).toContain("legalPointByKeySql('1', 'legal_items i', 'i.id'");
    expect(r38).not.toContain('SELECT 1 FROM legal_items i');
    const line = r38.split('\n').find(l => l.includes('legalPointByKeySql('))!;
    expect(line).not.toContain('SELECT ');
    expect((r38.match(/SELECT /g) ?? []).length).toBe(2);
    expect((r36.match(/export const legalPointByKeySql/g) ?? []).length).toBe(1);
  });

  it('keeps both callers and their locks/native text unchanged', async () => {
    const [purchases, access, r9, r24, worker] = await Promise.all([
      src('purchases.ts'), src('legal/access.ts'),
      src('legal-read-tail-r9-sources.ts'), src('legal-read-tail-r24-sources.ts'),
      src('legal/destruction-worker.ts'),
    ]);
    expect(purchases).toContain("legalReleasedItemGuardSql('company_id')");
    expect(access).toContain("legalReleasedItemGuardSql('version')");
    expect(purchases).not.toContain('SELECT 1 FROM legal_items i WHERE i.id=?');
    expect(access).not.toContain('SELECT 1 FROM legal_items i WHERE i.id=?');
    expect(r9).toContain('legalFilesAdvisoryLockSql');
    expect(r9).toContain('legalCompanyAdvisoryLockSql');
    expect(r9).toContain('pg_advisory_xact_lock');
    expect(r24).toContain('legalHoldTargetGuardPrefix');
    expect(worker).toContain('FOR UPDATE OF x,i');
  });

  it('retirement:ts-statements is BEFORE=132 -> AFTER=131', async () => {
    let total = 0;
    const walk = async (dir: string) => {
      for (const entry of await readdir(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) await walk(p);
        else if (entry.name.endsWith('.ts')) total += ((await readFile(p, 'utf8')).match(/SELECT /g) ?? []).length;
      }
    };
    await walk(srcRoot);
    expect(total).toBeLessThan(132);
    expect(total).toBeLessThanOrEqual(131);
    expect(total).toBe(109);
  });
});

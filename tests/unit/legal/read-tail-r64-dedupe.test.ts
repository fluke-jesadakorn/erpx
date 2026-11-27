import { createHash } from 'node:crypto';
import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import { legalVersionExistsSql } from '../../../packages/server-legal/src/adapters/legal-read-tail-r26-sources';
import { legalPointByIdSql } from '../../../packages/server-legal/src/adapters/legal-read-tail-r32-sources';

const sha256 = (s: string) => createHash('sha256').update(s).digest('hex');
const srcRoot = resolve(__dirname, '../../../packages/server-legal/src');
const src = (name: string) => readFile(new URL(`../../../packages/server-legal/src/adapters/${name}`, import.meta.url), 'utf8');

/** Independent pre-r64 byte + SHA oracles captured from the exact rendered SQL of
 * the three executed version-assertion call sites. Each keeps the fixed id-key head
 * and the local ` AND version=?` tail with the caller's original id-then-version bind order. */
const ORIGINALS: Array<[name: string, sql: string, sha: string, binds: number]> = [
  ['governance/authority-current', 'EXISTS(SELECT 1 FROM legal_authorities WHERE id=? AND version=?)', '2d5aaa198392239e0c922c11a2fa71521fa8da251e4a0db07b16dbc94deb9c67', 2],
  ['trust/trust-version', 'EXISTS(SELECT 1 FROM legal_trust_material WHERE id=? AND version=?)', '6e9eb0b36c69cfb2014f500f697fe176f34cb31e3cf285401778aaa6924f008f', 2],
  ['retention/retention-policy-version', 'EXISTS(SELECT 1 FROM legal_retention_policies WHERE id=? AND version=?)', '4aac3d70ca9c6af3b32470af0dd77516bc93dd0157722c5c8cc8b71914fe1d49', 2],
];

const ACTUAL: Record<string, string> = {
  'governance/authority-current': legalVersionExistsSql('legal_authorities'),
  'trust/trust-version': legalVersionExistsSql('legal_trust_material'),
  'retention/retention-policy-version': legalVersionExistsSql('legal_retention_policies'),
};

const TABLES: Record<string, string> = {
  'governance/authority-current': 'legal_authorities',
  'trust/trust-version': 'legal_trust_material',
  'retention/retention-policy-version': 'legal_retention_policies',
};

describe('legal read-tail r64 version-exists head reuse', () => {
  it('renders every call site byte-identically to the pre-r64 SHA oracles', () => {
    expect(ORIGINALS).toHaveLength(3);
    for (const [name, sql, digest] of ORIGINALS) {
      expect(ACTUAL[name], name).toBe(sql);
      expect(sha256(ACTUAL[name]), name).toBe(digest);
    }
  });

  it('folds the id-key head onto r32 and keeps the version tail local', () => {
    for (const [name, sql, , binds] of ORIGINALS) {
      const head = legalPointByIdSql('1', TABLES[name]);
      expect(head, name).toBe(`SELECT 1 FROM ${TABLES[name]} WHERE id=?`);
      expect((head.match(/\?/g) ?? []).length, name).toBe(1);
      expect(ACTUAL[name], name).toBe(`EXISTS(${head} AND version=?)`);
      expect(ACTUAL[name].endsWith(' AND version=?)'), name).toBe(true);
      expect((sql.match(/\?/g) ?? []).length, name).toBe(binds);
      expect(sql.indexOf('id=?'), name).toBeLessThan(sql.indexOf('version=?'));
      expect(sql, name).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
  });

  it('leaves r26 SELECT-free and delegating, with r32 the id-point renderer', async () => {
    const [r26, r32] = await Promise.all([
      src('legal-read-tail-r26-sources.ts'), src('legal-read-tail-r32-sources.ts'),
    ]);
    expect(r26.match(/SELECT /g) ?? []).toHaveLength(0);
    expect(r26.match(/export const legalVersionExistsSql/g) ?? []).toHaveLength(1);
    expect(r26).toContain("from './legal-read-tail-r32-sources'");
    expect(r26).toContain("legalPointByIdSql('1', table)");
    expect(r26).toContain(' AND version=?)');
    expect(r32.match(/SELECT /g) ?? []).toHaveLength(0);
    expect(r32.match(/export const legalPointByIdSql/g) ?? []).toHaveLength(1);
    expect(r32).toContain("from './legal-read-tail-r36-sources'");
  });

  it('keeps the three callers and their bind order unchanged, locks/native untouched', async () => {
    const [governance, trust, retention, access, worker] = await Promise.all([
      src('legal/governance.ts'), src('legal/trust.ts'), src('legal/retention.ts'),
      src('legal/access.ts'), src('legal/destruction-worker.ts'),
    ]);
    expect(governance).toContain('legalVersionExistsSql("legal_authorities")');
    expect(trust).toContain('legalVersionExistsSql("legal_trust_material")');
    expect(retention).toContain('legalVersionExistsSql("legal_retention_policies")');
    for (const file of [governance, trust, retention]) {
      expect(file).not.toMatch(/EXISTS\(SELECT 1 FROM legal_\w+ WHERE id=\? AND version=\?\)/);
    }
    expect(trust).toContain('await native.decision({ key, state, id: row.id, version: row.version })');
    expect(governance).toContain('companyLock:()=>stage(lock(db,a.companyId))');
    expect(retention).toContain('companyLock:()=>stage(lock(db,a.companyId))');
    expect(access).toContain('legalCompanyAdvisoryLockSql');
    expect(worker).toContain('FOR UPDATE OF x,i');
  });

  it('retirement:ts-statements is BEFORE=135 -> AFTER=134', async () => {
    let total = 0;
    const walk = async (dir: string) => {
      for (const entry of await readdir(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) await walk(p);
        else if (entry.name.endsWith('.ts')) total += ((await readFile(p, 'utf8')).match(/SELECT /g) ?? []).length;
      }
    };
    await walk(srcRoot);
    expect(total).toBeLessThan(135);
    expect(total).toBeLessThanOrEqual(134);
    expect(total).toBe(35);
  });
});

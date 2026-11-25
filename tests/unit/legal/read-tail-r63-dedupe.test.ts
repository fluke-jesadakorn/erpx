import { createHash } from 'node:crypto';
import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import { legalLaterApprovalGuardSql } from '../../../packages/server-legal/src/adapters/legal-read-tail-r15-sources';
import { legalPointByKeySql } from '../../../packages/server-legal/src/adapters/legal-read-tail-r36-sources';
import { legalExistsPointGuardHeadSql } from '../../../packages/server-legal/src/adapters/legal-read-tail-r40-sources';

const sha256 = (s: string) => createHash('sha256').update(s).digest('hex');
const srcRoot = resolve(__dirname, '../../../packages/server-legal/src');
const src = (name: string) => readFile(new URL(`../../../packages/server-legal/src/adapters/${name}`, import.meta.url), 'utf8');

/** Independent pre-r63 byte + SHA oracles. The r15 later-approval guard is shared
 * verbatim by two executed-copy assertion call sites; both keep their original
 * item-id then cycle bind order. The composed oracle is the exact documents.ts
 * approval assertion head+tail+guard, rendered from the unchanged r40 renderer. */
const GUARD = 'NOT EXISTS(SELECT 1 FROM legal_approvals WHERE item_id=? AND cycle>?)';
const HEAD = 'SELECT 1 FROM legal_approvals WHERE item_id=?';
const COMPOSED =
  "EXISTS(SELECT 1 FROM legal_approvals WHERE item_id=? AND cycle=? AND document_id=? AND document_hash=? AND decision='approved') AND " +
  GUARD;
const ORIGINALS: Array<[name: string, sql: string, sha: string, binds: number]> = [
  ['r15/guard', GUARD, '6e9b176a93e7c4ddd3d0031e5cbcdd73b1ddd7d7b9a6afe2b1d95b2772f09266', 2],
  ['r36/head', HEAD, '540e74e1ba6081045847b23f6deaece6e47b159014b7f2e8c9ed79e97a37a025', 1],
  ['documents/approval-assertion', COMPOSED, '8ba72c26057f9be070b0778eb030123deafc1502fbe936abff8c4d77761229f5', 6],
];

const ACTUAL: Record<string, string> = {
  'r15/guard': legalLaterApprovalGuardSql,
  'r36/head': legalPointByKeySql('1', 'legal_approvals', 'item_id'),
  'documents/approval-assertion':
    legalExistsPointGuardHeadSql('legal_approvals', 'item_id') +
    "cycle=? AND document_id=? AND document_hash=? AND decision='approved') AND " +
    legalLaterApprovalGuardSql,
};

describe('legal read-tail r63 later-approval guard head reuse', () => {
  it('renders every family member byte-identically to the pre-r63 SHA oracles', () => {
    expect(ORIGINALS).toHaveLength(3);
    for (const [name, sql, digest] of ORIGINALS) {
      expect(ACTUAL[name], name).toBe(sql);
      expect(sha256(ACTUAL[name]), name).toBe(digest);
    }
  });

  it('folds the r15 guard head onto the single r36 arbitrary-key renderer', () => {
    for (const [name, sql, , binds] of ORIGINALS) {
      expect((sql.match(/\?/g) ?? []).length, name).toBe(binds);
      expect(sql, name).toContain('legal_approvals WHERE item_id=?');
      expect(sql, name).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
    // The head is exactly the r36 renderer with the fixed 'item_id' key; the
    // trusted `cycle>?` tail is appended verbatim and remains the last bind.
    expect(legalPointByKeySql('1', 'legal_approvals', 'item_id')).toBe(HEAD);
    expect(`${legalPointByKeySql('1', 'legal_approvals', 'item_id')} AND cycle>?`)
      .toBe(GUARD.slice('NOT EXISTS('.length, -1));
    expect(legalLaterApprovalGuardSql).toBe(`NOT EXISTS(${HEAD} AND cycle>?)`);
    expect(GUARD.indexOf('item_id=?')).toBeLessThan(GUARD.indexOf('cycle>?'));
    expect((GUARD.match(/\?/g) ?? []).length).toBe(2);
  });

  it('leaves r15 SELECT-free and delegating, with r36 the one point-read renderer', async () => {
    const [r15, r36] = await Promise.all([
      src('legal-read-tail-r15-sources.ts'), src('legal-read-tail-r36-sources.ts'),
    ]);
    expect(r15.match(/SELECT /g) ?? []).toHaveLength(0);
    expect(r15.match(/export const legalLaterApprovalGuardSql/g) ?? []).toHaveLength(1);
    expect(r15).toContain("from './legal-read-tail-r36-sources'");
    expect(r15).toContain("legalPointByKeySql('1', 'legal_approvals', 'item_id')");
    expect((r36.match(/SELECT /g) ?? []).length).toBe(1);
    expect(r36.match(/export const legalPointByKeySql/g) ?? []).toHaveLength(1);
    expect(r36).toContain("'document_id' | 'id' | 'item_id' | 'destruction_id' | 'envelope_id' | 'source_id'");
  });

  it('keeps both call sites and their protected lock text unchanged', async () => {
    const [documents, copies, worker, purchases] = await Promise.all([
      src('legal/documents.ts'), src('legal/executed-copies.ts'),
      src('legal/destruction-worker.ts'), src('portal-purchases.ts'),
    ]);
    for (const file of [documents, copies]) {
      expect(file).toContain('legalLaterApprovalGuardSql');
      expect(file).not.toContain(GUARD);
    }
    expect(documents).toContain(
      "legalExistsPointGuardHeadSql('legal_approvals','item_id')",
    );
    expect(copies).toContain('"legal-executed-current"');
    expect(worker).toContain('FOR UPDATE OF x,i');
    expect(purchases).toContain(
      'LOCK TABLE legal_settings,legal_destructions,legal_comments,legal_envelopes,legal_signers,legal_counsel_entries IN SHARE ROW EXCLUSIVE MODE',
    );
  });

  it('retirement:ts-statements is BEFORE=136 -> AFTER=135', async () => {
    let total = 0;
    const walk = async (dir: string) => {
      for (const entry of await readdir(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) await walk(p);
        else if (entry.name.endsWith('.ts')) total += ((await readFile(p, 'utf8')).match(/SELECT /g) ?? []).length;
      }
    };
    await walk(srcRoot);
    expect(total).toBeLessThan(136);
    expect(total).toBeLessThanOrEqual(135);
    expect(total).toBe(105);
  });
});

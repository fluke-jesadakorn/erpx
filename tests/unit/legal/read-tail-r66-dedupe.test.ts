import { createHash } from 'node:crypto';
import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import { legalExistsPointGuardHeadSql } from '../../../packages/server-legal/src/adapters/legal-read-tail-r40-sources';
import { legalPointByKeySql } from '../../../packages/server-legal/src/adapters/legal-read-tail-r36-sources';

const sha256 = (s: string) => createHash('sha256').update(s).digest('hex');
const srcRoot = resolve(__dirname, '../../../packages/server-legal/src');
const src = (name: string) => readFile(new URL(`../../../packages/server-legal/src/adapters/${name}`, import.meta.url), 'utf8');

/** Independent pre-r66 byte + SHA oracles for the four r40 head members. Each
 * head has exactly one bind (`key=?`); every tail bind stays at the call site. */
const members: [string, string, string][] = [
  ['legal_approvals', 'item_id', 'ae802f2c304d5edc7c06a9161def8e0462a97645afc5c394f3f1270680891b1a'],
  ['legal_documents', 'id', '173f07fee54b668c1a878a4d771caac06e1289ff70067b4f28ddc1e9e8ab2e4b'],
  ['legal_intake_forms', 'id', '92f14a3303f7a796a5de469236ebfb277d00671615db8c00c6ee1103014ae810'],
  ['legal_envelopes', 'id', 'ad295de014362828cafdbb64f42053923c2cb77bd5b9b0e6718b401604d6eb34'],
];

describe('legal read-tail r66 EXISTS point-guard head reuse', () => {
  it('renders every folded head byte-identically to its pre-r66 oracle', () => {
    for (const [table, key, oracleSha] of members) {
      const head = legalExistsPointGuardHeadSql(table, key as 'id');
      const expected = `EXISTS(SELECT 1 FROM ${table} WHERE ${key}=? AND `;
      expect(head).toBe(expected);
      expect(sha256(head)).toBe(oracleSha);
      expect(head).toBe(`EXISTS(${legalPointByKeySql('1', table, key as 'id')} AND `);
      expect((head.match(/\?/g) ?? []).length).toBe(1);
    }
  });

  it('keeps the full guard strings and their binds byte-identical', () => {
    const guards: [string, string, string][] = [
      [legalExistsPointGuardHeadSql('legal_approvals', 'item_id'),
        "cycle=? AND document_id=? AND document_hash=? AND decision='approved') AND ",
        "EXISTS(SELECT 1 FROM legal_approvals WHERE item_id=? AND cycle=? AND document_id=? AND document_hash=? AND decision='approved') AND "],
      [legalExistsPointGuardHeadSql('legal_documents', 'id'),
        'item_id=?) AND NOT EXISTS(${legalDisposedDocumentPrefix}=?)',
        'EXISTS(SELECT 1 FROM legal_documents WHERE id=? AND item_id=?) AND NOT EXISTS(${legalDisposedDocumentPrefix}=?)'],
      [legalExistsPointGuardHeadSql('legal_intake_forms', 'id'),
        "version=? AND state='draft')",
        "EXISTS(SELECT 1 FROM legal_intake_forms WHERE id=? AND version=? AND state='draft')"],
      [legalExistsPointGuardHeadSql('legal_envelopes', 'id'),
        "state='sent' AND expires_at>?)",
        "EXISTS(SELECT 1 FROM legal_envelopes WHERE id=? AND state='sent' AND expires_at>?)"],
    ];
    for (const [head, tail, oracle] of guards) {
      expect(head + tail).toBe(oracle);
      expect((head + tail).match(/\?/g)?.length).toBe((oracle.match(/\?/g) ?? []).length);
    }
  });

  it('leaves r40 delegating to the single r36 point-read renderer', async () => {
    const [r40, r36] = await Promise.all([
      src('legal-read-tail-r40-sources.ts'), src('legal-read-tail-r36-sources.ts'),
    ]);
    expect(r40).toContain("from './legal-read-tail-r36-sources'");
    expect(r40).toContain("legalPointByKeySql('1', table, key as Parameters<typeof legalPointByKeySql>[2])");
    const line = r40.split('\n').find(l => l.includes('EXISTS(${legalPointByKeySql'))!;
    expect(line).not.toContain('SELECT ');
    expect((r40.match(/SELECT /g) ?? []).length).toBe(0);
    expect((r36.match(/export const legalPointByKeySql/g) ?? []).length).toBe(1);
  });

  it('keeps all four callers and their locks/native text unchanged', async () => {
    const [documents, governance, signing, r9, r24, worker] = await Promise.all([
      src('legal/documents.ts'), src('legal/governance.ts'), src('legal/signing.ts'),
      src('legal-read-tail-r9-sources.ts'), src('legal-read-tail-r24-sources.ts'),
      src('legal/destruction-worker.ts'),
    ]);
    expect(documents).toContain("legalExistsPointGuardHeadSql('legal_approvals','item_id')");
    expect(documents).toContain("legalExistsPointGuardHeadSql('legal_documents','id')");
    expect(governance).toContain("legalExistsPointGuardHeadSql('legal_intake_forms','id')");
    expect(signing).toContain("legalExistsPointGuardHeadSql('legal_envelopes','id')");
    expect(documents).not.toContain('EXISTS(SELECT 1 FROM legal_approvals WHERE item_id=? AND cycle=?');
    expect(governance).not.toContain('EXISTS(SELECT 1 FROM legal_intake_forms WHERE id=? AND version=?');
    expect(signing).not.toContain("EXISTS(SELECT 1 FROM legal_envelopes WHERE id=? AND state='sent'");
    expect(r9).toContain('legalFilesAdvisoryLockSql');
    expect(r9).toContain('legalCompanyAdvisoryLockSql');
    expect(r9).toContain('pg_advisory_xact_lock');
    expect(r24).toContain('legalHoldTargetGuardPrefix');
    expect(worker).toContain('FOR UPDATE OF x,i');
  });

  it('retirement:ts-statements is BEFORE=133 -> AFTER=132', async () => {
    let total = 0;
    const walk = async (dir: string) => {
      for (const entry of await readdir(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) await walk(p);
        else if (entry.name.endsWith('.ts')) total += ((await readFile(p, 'utf8')).match(/SELECT /g) ?? []).length;
      }
    };
    await walk(srcRoot);
    expect(total).toBeLessThan(133);
    expect(total).toBeLessThanOrEqual(132);
    expect(total).toBe(60);
  });
});

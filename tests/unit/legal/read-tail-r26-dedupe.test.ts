import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import { legalVersionExistsSql } from '../../../packages/server-legal/src/adapters/legal-read-tail-r26-sources';

const srcRoot = resolve(__dirname, '../../../packages/server-legal/src');
const src = (name: string) => readFile(new URL(`../../../packages/server-legal/src/adapters/${name}`, import.meta.url), 'utf8');
const versionExists = (table: string) => `EXISTS(SELECT 1 FROM ${table} WHERE id=? AND version=?)`;

const tables = ['legal_authorities', 'legal_trust_material', 'legal_retention_policies'];

describe('legal read-tail r26 parameterized family dedupe', () => {
  it('renders byte-identical version-assertion SQL for each legal table', () => {
    for (const table of tables) {
      expect(legalVersionExistsSql(table)).toBe(versionExists(table));
    }
  });

  it('keeps the fragment read-only (no writes or row locks)', () => {
    const all = tables.map(legalVersionExistsSql).join(' ');
    expect(all).toMatch(/EXISTS\(SELECT 1 FROM legal_/);
    expect(all).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
  });

  it('keeps one definition and no inline family literals in the three callers', async () => {
    const [shared, governance, trust, retention] = await Promise.all([
      src('legal-read-tail-r26-sources.ts'),
      src('legal/governance.ts'),
      src('legal/trust.ts'),
      src('legal/retention.ts'),
    ]);
    expect(shared.match(/export const legalVersionExistsSql/g) ?? []).toHaveLength(1);
    expect(governance).toContain('legalVersionExistsSql("legal_authorities")');
    expect(trust).toContain('legalVersionExistsSql("legal_trust_material")');
    expect(retention).toContain('legalVersionExistsSql("legal_retention_policies")');
    for (const file of [governance, trust, retention]) {
      expect(file).not.toMatch(/EXISTS\(SELECT 1 FROM legal_\w+ WHERE id=\? AND version=\?\)/);
    }
  });

  it('leaves corpus/citation, locks and the r9/r16-r25 families untouched', async () => {
    const [r9, r16, r25, corpus, citations, access] = await Promise.all([
      src('legal-read-tail-r9-sources.ts'),
      src('legal-read-tail-r16-sources.ts'),
      src('legal-read-tail-r25-sources.ts'),
      src('legal/law-corpus.ts'),
      src('legal/citations.ts'),
      src('legal/access.ts'),
    ]);
    expect(r9).toContain('export const legalDocumentsItemPairSql');
    expect(r16).toContain('export const legalItemSelectSql');
    expect(r25).toContain('export const legalJsonbAggByItemSql');
    expect(corpus).toContain("nativeLegalAuxiliaryRead<LawCorpusView>(a,'laws',ruleId)");
    expect(citations).toContain('export function legalCitation');
    expect(access).toContain('legalCompanyAdvisoryLockSql');
    expect(r9).toContain('legalCompanyAdvisoryLockSql');
  });

  it('retirement:ts-statements stays strictly below the recorded 251 r25 baseline', async () => {
    let total = 0;
    const walk = async (dir: string) => {
      for (const entry of await readdir(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) await walk(p);
        else if (entry.name.endsWith('.ts')) total += ((await readFile(p, 'utf8')).match(/SELECT/g) ?? []).length;
      }
    };
    await walk(srcRoot);
    expect(total).toBeLessThan(251);
    expect(total).toBeLessThanOrEqual(249);
  });
});

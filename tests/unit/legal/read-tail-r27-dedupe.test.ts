import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import { legalByIdAndItemSql } from '../../../modules/legal/server/src/adapters/legal-read-tail-r27-sources';

const srcRoot = resolve(__dirname, '../../../modules/legal/server/src');
const src = (name: string) => readFile(new URL(`../../../modules/legal/server/src/adapters/${name}`, import.meta.url), 'utf8');
const byIdAndItem = (table: string) => `SELECT * FROM ${table} WHERE id=? AND item_id=?`;

const tables = ['legal_authorities', 'legal_documents', 'legal_destructions'];

describe('legal read-tail r27 parameterized family dedupe', () => {
  it('renders byte-identical item point-read SQL for each legal fact table', () => {
    for (const table of tables) {
      expect(legalByIdAndItemSql(table)).toBe(byIdAndItem(table));
    }
  });

  it('keeps the fragment read-only (no writes or row locks)', () => {
    const all = tables.map(legalByIdAndItemSql).join(' ');
    expect(all).toMatch(/SELECT \* FROM legal_/);
    expect(all).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
  });

  it('keeps one definition and no inline family literals in the four callers', async () => {
    const [shared, governance, approval, retention, features] = await Promise.all([
      src('legal-read-tail-r27-sources.ts'),
      src('legal/governance.ts'),
      src('legal/approval.ts'),
      src('legal/retention.ts'),
      src('legal/features.ts'),
    ]);
    expect(shared.match(/export const legalByIdAndItemSql/g) ?? []).toHaveLength(1);
    expect(governance).toContain('legalByIdAndItemSql("legal_authorities")');
    expect(approval).toContain('legalByIdAndItemSql("legal_documents")');
    expect(retention).toContain('legalByIdAndItemSql("legal_destructions")');
    expect(features).toContain('legalByIdAndItemSql(featureSpecs[facts.capability].table)');
    for (const file of [governance, approval, retention, features]) {
      expect(file).not.toMatch(/SELECT \* FROM (legal_\w+|\$\{featureSpecs\[facts\.capability\]\.table\}) WHERE id=\? AND item_id=\?/);
    }
  });

  it('leaves corpus/citation, locks and the r9/r16-r26 families untouched', async () => {
    const [r9, r16, r25, r26, corpus, citations, access] = await Promise.all([
      src('legal-read-tail-r9-sources.ts'),
      src('legal-read-tail-r16-sources.ts'),
      src('legal-read-tail-r25-sources.ts'),
      src('legal-read-tail-r26-sources.ts'),
      src('legal/law-corpus.ts'),
      src('legal/citations.ts'),
      src('legal/access.ts'),
    ]);
    expect(r9).toContain('export const legalDocumentsItemPairSql');
    expect(r16).toContain('export const legalItemSelectSql');
    expect(r25).toContain('export const legalJsonbAggByItemSql');
    expect(r26).toContain('export const legalVersionExistsSql');
    expect(corpus).toContain("nativeLegalAuxiliaryRead<LawCorpusView>(a,'laws',ruleId)");
    expect(citations).toContain('export function legalCitation');
    expect(access).toContain('legalCompanyAdvisoryLockSql');
    expect(r9).toContain('legalCompanyAdvisoryLockSql');
  });

  it('retirement:ts-statements stays strictly below the recorded 249 r26 baseline', async () => {
    let total = 0;
    const walk = async (dir: string) => {
      for (const entry of await readdir(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) await walk(p);
        else if (entry.name.endsWith('.ts')) total += ((await readFile(p, 'utf8')).match(/SELECT/g) ?? []).length;
      }
    };
    await walk(srcRoot);
    expect(total).toBeLessThan(249);
    expect(total).toBeLessThanOrEqual(246);
  });
});

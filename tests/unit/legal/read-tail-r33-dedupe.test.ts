import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import { legalStatePointSql } from '../../../packages/server-legal/src/adapters/legal-read-tail-r33-sources';

const srcRoot = resolve(__dirname, '../../../packages/server-legal/src');
const src = (name: string) => readFile(new URL(`../../../packages/server-legal/src/adapters/${name}`, import.meta.url), 'utf8');
const point = (columns: string, table: string, key2: string, state: string) =>
  `SELECT ${columns} FROM ${table} WHERE id=? AND ${key2}=? AND state='${state}'`;

const members: [string, string, 'company_id' | 'document_id', 'active' | 'approved'][] = [
  ['fields,sla_days', 'legal_intake_forms', 'company_id', 'approved'],
  ['source_url', 'legal_law_sources', 'company_id', 'active'],
  ['1', 'legal_text_reviews', 'document_id', 'approved'],
];

describe('legal read-tail r33 state-constrained point-read family', () => {
  it('renders byte-identical SQL for every member', () => {
    for (const [columns, table, key2, state] of members) {
      expect(legalStatePointSql(columns, table, key2, state)).toBe(point(columns, table, key2, state));
    }
  });

  it('keeps the fragment read-only (no writes or row locks)', () => {
    const all = members.map(([c, t, k, s]) => legalStatePointSql(c, t, k, s)).join(' ');
    expect(all).toMatch(/SELECT [\w,*]+ FROM legal_\w+ WHERE id=\? AND \w+=\? AND state='(active|approved)'/);
    expect(all).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
  });

  it('keeps one definition and removes every inline family literal from the callers', async () => {
    const [shared, governance, sourceWorker, knowledge] = await Promise.all([
      src('legal-read-tail-r33-sources.ts'),
      src('legal/governance.ts'),
      src('legal/law-source-worker.ts'),
      src('legal/knowledge.ts'),
    ]);
    expect(shared.match(/export const legalStatePointSql/g) ?? []).toHaveLength(1);
    expect(governance).toContain('legalStatePointSql("fields,sla_days","legal_intake_forms","company_id","approved")');
    expect(sourceWorker).toContain('legalStatePointSql("source_url","legal_law_sources","company_id","active")');
    expect(knowledge).toContain("from '../native-lane-26/client'");
    expect(knowledge).not.toContain('legalStatePointSql("1","legal_text_reviews"');
    for (const [c, t, k, s] of members) {
      const inline = `SELECT ${c} FROM ${t} WHERE id=? AND ${k}=? AND state='${s}'`;
      expect(governance).not.toContain(inline);
      expect(sourceWorker).not.toContain(inline);
      expect(knowledge).not.toContain(inline);
    }
  });

  it('leaves corpus/citation/locks and the r9/r16-r32 families untouched', async () => {
    const names = [
      'legal-read-tail-r9-sources.ts', 'legal-read-tail-r16-sources.ts',
      'legal-read-tail-r18-sources.ts', 'legal-read-tail-r19-sources.ts', 'legal-read-tail-r20-sources.ts',
      'legal-read-tail-r21-sources.ts', 'legal-read-tail-r22-sources.ts', 'legal-read-tail-r23-sources.ts',
      'legal-read-tail-r24-sources.ts', 'legal-read-tail-r25-sources.ts', 'legal-read-tail-r26-sources.ts',
      'legal-read-tail-r27-sources.ts', 'legal-read-tail-r28-sources.ts', 'legal-read-tail-r29-sources.ts',
      'legal-read-tail-r30-sources.ts', 'legal-read-tail-r31-sources.ts', 'legal-read-tail-r32-sources.ts',
      'legal/law-corpus.ts', 'legal/citations.ts', 'legal/access.ts', 'legal/destruction-worker.ts',
    ];
    const texts = await Promise.all(names.map(src));
    const symbols = [
      'legalDocumentByIdSql', 'legalItemSelectSql',
      'legalRetentionPolicySelectSql', 'legalRateCountPrefix', 'legalDisposedDocumentPrefix',
      'legalPurchaseClockCteSql', 'legalPurchaseClockExistsSql', 'legalApprovedCycleSql',
      'legalHoldActiveSql', 'legalHoldTargetGuardPrefix', 'legalJsonbAggByItemSql', 'legalVersionExistsSql',
      'legalByIdAndItemSql', 'legalByIdAndCompanySql', 'legalMemberEditExistsSql', 'legalEventsByItemSql',
      'legalOrderedHistorySql', 'legalRecentItemListSql', 'legalPointByIdSql',
    ];
    for (const symbol of symbols) {
      expect(texts.slice(0, 17).join('\n')).toContain(`export const ${symbol}`);
    }
    expect(texts[17]).toContain("nativeLegalAuxiliaryRead<LawCorpusView>(a,'laws',ruleId)");
    expect(texts[18]).toContain('export function legalCitation');
    expect(texts[19]).toContain('legalCompanyAdvisoryLockSql');
    expect(texts[20]).toContain('FOR UPDATE OF x,i');
  });

  it('retirement:ts-statements stays strictly below the recorded BEFORE=238', async () => {
    let total = 0;
    const walk = async (dir: string) => {
      for (const entry of await readdir(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) await walk(p);
        else if (entry.name.endsWith('.ts')) total += ((await readFile(p, 'utf8')).match(/SELECT /g) ?? []).length;
      }
    };
    await walk(srcRoot);
    expect(total).toBeLessThan(238);
    expect(total).toBeLessThanOrEqual(237);
  });
});

import { describe, it, expect } from 'vitest';
import { readFile } from 'node:fs/promises';

const adapter = (name: string) =>
  new URL(`../../../modules/marketing/server/src/adapters/${name}`, import.meta.url);

const expected =
  'SELECT id FROM marketing_assets UNION ALL SELECT id FROM marketing_lead_candidates UNION ALL SELECT id FROM marketing_campaigns UNION ALL SELECT id FROM marketing_campaign_costs';

describe('marketing source-id projection dedupe', () => {
  it('emits the byte-identical source-id statement', async () => {
    const { marketingSourceIdsSql } = await import(
      '../../../modules/marketing/server/src/adapters/marketing-source-id-sql.ts'
    );
    expect(marketingSourceIdsSql()).toBe(expected);
  });

  it('keeps one query token in the shared union frame and none in the source-id wrapper', async () => {
    const helper = await readFile(adapter('marketing-source-id-sql.ts'), 'utf8');
    const union = await readFile(adapter('marketing-purchase-union-sql.ts'), 'utf8');
    const record = await readFile(adapter('record-purchases.ts'), 'utf8');
    expect(helper.match(/SELECT /g) ?? []).toHaveLength(0);
    expect(union.match(/SELECT /g) ?? []).toHaveLength(0);
    expect(union).toContain('marketingSelectHeadSql(projection, ` FROM ${from}`)');
    expect(helper).toContain("marketingPurchaseUnionSql(sourceIdTables.map(table => ['id', table] as const))");
    expect(record).not.toContain(expected);
    expect(record).toContain('marketingSourceIdsSql()');
  });

  it('keeps downstream interpolation and binds unchanged', async () => {
    const record = await readFile(adapter('record-purchases.ts'), 'utf8');
    expect(record).toContain("marketingSourceRowGuardSql('('+nativeIds+') native_marketing','native_marketing.id='+id)");
    expect(record).toContain("marketingPointExistsSql(`(${nativeIds}) actual`,'id=?')");
    expect(record).not.toContain('`SELECT 1 FROM (${nativeIds}) actual WHERE id=?`');
  });
});

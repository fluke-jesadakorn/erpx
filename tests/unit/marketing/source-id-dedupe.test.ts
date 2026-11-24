import { describe, it, expect } from 'vitest';
import { readFile } from 'node:fs/promises';

const adapter = (name: string) =>
  new URL(`../../../packages/server-marketing/src/adapters/${name}`, import.meta.url);

const expected =
  'SELECT id FROM marketing_assets UNION ALL SELECT id FROM marketing_lead_candidates UNION ALL SELECT id FROM marketing_campaigns UNION ALL SELECT id FROM marketing_campaign_costs';

describe('marketing source-id projection dedupe', () => {
  it('emits the byte-identical source-id statement', async () => {
    const { marketingSourceIdsSql } = await import(
      '../../../packages/server-marketing/src/adapters/marketing-source-id-sql.ts'
    );
    expect(marketingSourceIdsSql()).toBe(expected);
  });

  it('keeps one query token in the helper and no inline literal in the consumer', async () => {
    const helper = await readFile(adapter('marketing-source-id-sql.ts'), 'utf8');
    const record = await readFile(adapter('record-purchases.ts'), 'utf8');
    expect(helper.match(/SELECT /g)).toHaveLength(1);
    expect(record).not.toContain(expected);
    expect(record).toContain('marketingSourceIdsSql()');
  });

  it('keeps downstream interpolation and binds unchanged', async () => {
    const record = await readFile(adapter('record-purchases.ts'), 'utf8');
    expect(record).toContain('EXISTS(SELECT 1 FROM (${nativeIds}) native_marketing');
    expect(record).toContain('SELECT 1 FROM (${nativeIds}) actual WHERE id=?');
  });
});

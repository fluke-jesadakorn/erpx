import { describe, it, expect } from 'vitest';
import { readFile } from 'node:fs/promises';

const adapter = () =>
  new URL('../../../packages/server-marketing/src/adapters/package-obligations.ts', import.meta.url);

const original = {
  engagements: 'SELECT * FROM marketing_engagement_events',
  costs: 'SELECT * FROM marketing_campaign_costs',
  touches: 'SELECT * FROM marketing_touches',
  campaigns: 'SELECT * FROM marketing_campaigns',
} as const;

describe('marketing settlement source dedupe', () => {
  it('keeps one canonical SELECT literal and no inline copies in the source', async () => {
    const source = await readFile(adapter(), 'utf8');
    for (const sql of Object.values(original)) expect(source).not.toContain(`'${sql}'`);
    expect(source).toContain('settlementSourceRows');
    expect(source.match(/return `SELECT \* FROM \$\{table\}`/g)).toHaveLength(1);
  });

  it('emits the original byte-identical settlement source statements', async () => {
    const { marketingPackageSettlementSources } = await import(
      '../../../packages/server-marketing/src/adapters/package-obligations.ts'
    );
    for (const [variant, sql] of Object.entries(original)) {
      expect(marketingPackageSettlementSources[variant as keyof typeof original]).toBe(sql);
    }
  });
});

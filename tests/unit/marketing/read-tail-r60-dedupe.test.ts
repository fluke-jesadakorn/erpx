import { describe, it, expect } from 'vitest';
import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { marketingPurchaseUnionSql } from '../../../packages/server-marketing/src/adapters/marketing-purchase-union-sql';

const src = (name: string) =>
  readFile(new URL(`../../../packages/server-marketing/src/adapters/${name}`, import.meta.url), 'utf8');

async function selectTokens(): Promise<number> {
  let total = 0;
  const walk = async (dir: string) => {
    for (const entry of await readdir(dir, { withFileTypes: true })) {
      const path = resolve(dir, entry.name);
      if (entry.isDirectory()) await walk(path);
      else if (entry.name.endsWith('.ts'))
        total += ((await readFile(path, 'utf8')).match(/SELECT /g) ?? []).length;
    }
  };
  await walk(resolve('packages/server-marketing/src'));
  return total;
}

describe('marketing read-tail r60 purchase-union head cutover', () => {
  it('emits byte-identical arms with the UNION ALL join in the caller', () => {
    expect(
      marketingPurchaseUnionSql([
        ['id', 'marketing_assets'],
        ['cost.id,cost.company_id', 'marketing_campaign_costs cost'],
      ]),
    ).toBe(
      'SELECT id FROM marketing_assets UNION ALL SELECT cost.id,cost.company_id FROM marketing_campaign_costs cost',
    );
  });

  it('deletes the local SELECT token and delegates arms to the shared head', async () => {
    const head = await src('marketing-purchase-union-sql.ts');
    expect(head.match(/SELECT /g)).toBeNull();
    expect(head).toContain("import { marketingSelectHeadSql } from './select-head-sql';");
    expect(head).toContain("marketingSelectHeadSql(projection, ` FROM ${from}`)).join(' UNION ALL ')");
    expect(await src('marketing-source-id-sql.ts')).toContain('marketingPurchaseUnionSql(sourceIdTables');
    expect(await src('record-purchases.ts')).toContain('const rows=marketingPurchaseUnionSql([');
  });

  it('retirement:ts-statements stays strictly below BEFORE=10 at 8 after r61', async () => {
    expect(await selectTokens()).toBe(6);
  });
});

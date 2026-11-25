import { describe, it, expect } from 'vitest';
import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { marketingProjectionSourceSql } from '../../../packages/server-marketing/src/adapters/owner-projection-source';

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

describe('marketing read-tail r59 owner-projection head cutover', () => {
  it('emits the byte-identical quoted projection from the shared selection head', () => {
    expect(marketingProjectionSourceSql(['id', 'company_id'], 'marketing_campaigns')).toBe(
      'SELECT "id","company_id" FROM marketing_campaigns',
    );
    expect(marketingProjectionSourceSql(['"a"', 'b c'], 'marketing_campaign_costs c')).toBe(
      'SELECT ""a"","b c" FROM marketing_campaign_costs c',
    );
  });

  it('deletes the local SELECT token and delegates to the registered head', async () => {
    const head = await src('owner-projection-source.ts');
    expect(head.match(/SELECT /g)).toBeNull();
    expect(head).toContain("import { marketingSelectHeadSql } from './select-head-sql';");
    expect(head).toContain("marketingSelectHeadSql(columns.map((column) => `\"${column}\"`).join(','), ` FROM ${table}`)");
    for (const name of [
      'hospitality-owner-projection-sources.ts',
      'last-audit-owner-projection-sources.ts',
      'commercial-owner-projection-sources.ts',
    ])
      expect(await src(name)).toContain('marketingProjectionSourceSql(');
  });

  it('retirement:ts-statements stays strictly below BEFORE=11 at 9 after r60', async () => {
    expect(await selectTokens()).toBe(1);
  });
});

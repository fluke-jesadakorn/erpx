import { describe, it, expect } from 'vitest';
import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { marketingOwnerPointReadSql } from '../../../packages/server-marketing/src/adapters/owner-point-read-sql';

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

describe('marketing read-tail r65 point-read head cutover', () => {
  it('emits byte-identical two-bind point reads for both qualifier modes', () => {
    expect(marketingOwnerPointReadSql('kind', '(rows) actual')).toBe(
      'SELECT kind FROM (rows) actual WHERE id=? AND company_id=?',
    );
    expect(marketingOwnerPointReadSql('to_jsonb(source)', 'marketing_assets source', true)).toBe(
      'SELECT to_jsonb(source) FROM marketing_assets source WHERE source.id=? AND source.company_id=?',
    );
  });

  it('deletes the local selection token and delegates to the shared head', async () => {
    const head = await src('owner-point-read-sql.ts');
    expect(head.match(/SELECT /g)).toBeNull();
    expect(head).toContain("import { marketingSelectHeadSql } from './select-head-sql';");
    expect(head).toContain("marketingSelectHeadSql(projection, ` FROM ${source} WHERE ${q}id=? AND ${q}company_id=?`)");
    for (const name of [
      'attachment-owner-fact-read-sql.ts',
      'attachment-source-witness.ts',
      'record-purchases.ts',
    ])
      expect(await src(name)).toContain('marketingOwnerPointReadSql(');
  });

  it('retirement:ts-statements stays strictly below BEFORE=5 at 3 after r66', async () => {
    expect(await selectTokens()).toBe(1);
  });
});

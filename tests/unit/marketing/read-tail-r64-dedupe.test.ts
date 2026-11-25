import { describe, it, expect } from 'vitest';
import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { marketingReadTailSql } from '../../../packages/server-marketing/src/adapters/read-tail-sql';

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

describe('marketing read-tail r64 read-tail head cutover', () => {
  it('emits byte-identical list, point and plain-source statements', () => {
    expect(marketingReadTailSql('marketing_assets')).toBe('SELECT * FROM marketing_assets');
    expect(marketingReadTailSql('marketing_assets', ' WHERE company_id=? ORDER BY updated_at DESC')).toBe(
      'SELECT * FROM marketing_assets WHERE company_id=? ORDER BY updated_at DESC',
    );
  });

  it('deletes the local selection token and delegates to the shared head', async () => {
    const head = await src('read-tail-sql.ts');
    expect(head.match(/SELECT /g)).toBeNull();
    expect(head).toContain("import { marketingSelectHeadSql } from './select-head-sql';");
    expect(head).toContain("marketingSelectHeadSql('*', ` FROM ${table}${tail}`)");
    for (const name of [
      'marketing-local-point-read-sql.ts',
      'marketing-company-list-sql.ts',
      'marketing-plain-source-sql.ts',
    ])
      expect(await src(name)).toContain('marketingReadTailSql(');
  });

  it('retirement:ts-statements stays strictly below BEFORE=6 at 4 after r65', async () => {
    expect(await selectTokens()).toBe(1);
  });
});

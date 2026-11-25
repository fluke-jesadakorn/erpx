import { describe, it, expect } from 'vitest';
import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { marketingPointExistsSql } from '../../../packages/server-marketing/src/adapters/marketing-point-exists-sql';

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

describe('marketing read-tail r66 point-existence head cutover', () => {
  it('emits byte-identical probes for both admitted FROM gaps', () => {
    expect(marketingPointExistsSql('t', 'id=?')).toBe('SELECT 1 FROM t WHERE id=?');
    expect(marketingPointExistsSql('(rows) actual', 'id=?', '')).toBe('SELECT 1 FROM(rows) actual WHERE id=?');
  });

  it('deletes the local selection token and delegates to the shared head', async () => {
    const head = await src('marketing-point-exists-sql.ts');
    expect(head.match(/SELECT /g)).toBeNull();
    expect(head).toContain("import { marketingSelectHeadSql } from './select-head-sql';");
    expect(head).toContain("marketingSelectHeadSql('1', ` FROM${gap}${source} WHERE ${where}`)");
    for (const name of [
      'crm-purchase-sources.ts',
      'attachment-owner-version-guard.ts',
      'attachment-read.ts',
      'record-purchases.ts',
      'package-obligations.ts',
    ])
      expect(await src(name)).toContain('marketingPointExistsSql(');
  });

  it('retirement:ts-statements is BEFORE=4 -> AFTER=3', async () => {
    expect(await selectTokens()).toBe(1);
  });
});

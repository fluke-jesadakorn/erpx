import { describe, it, expect } from 'vitest';
import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { formAssetReadHead } from '../../../packages/server-marketing/src/adapters/form-asset-read-head';

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

describe('marketing read-tail r62 form-asset head cutover', () => {
  it('emits byte-identical content and form-row statements', () => {
    expect(formAssetReadHead({ kind: 'content', projection: '1' })).toBe(
      "SELECT 1 FROM marketing_assets WHERE id=? AND company_id=? AND kind='content' AND status='active' AND version=?",
    );
    expect(formAssetReadHead({ kind: 'form-row' })).toBe(
      "SELECT id,company_id,version,status,configuration_json FROM marketing_assets WHERE id=? AND company_id=? AND kind='form'",
    );
  });

  it('deletes the local SELECT token and delegates to the shared head', async () => {
    const head = await src('form-asset-read-head.ts');
    expect(head.match(/SELECT /g)).toBeNull();
    expect(head).toContain("import { marketingSelectHeadSql } from './select-head-sql';");
    expect(head).toContain("marketingSelectHeadSql(projection, ` FROM marketing_assets WHERE ${predicate}`)");
    expect(await src('public-form-content-source.ts')).toContain("formAssetReadHead({ kind: 'content', projection })");
    expect(await src('purchases.ts')).toContain("formAssetReadHead({kind:'form-row'})");
  });

  it('retirement:ts-statements stays strictly below BEFORE=8 at 6 after r63', async () => {
    expect(await selectTokens()).toBe(6);
  });
});

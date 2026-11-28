import { describe, it, expect } from 'vitest';
import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';

const src = (name: string) =>
  readFile(new URL(`../../../modules/marketing/server/src/adapters/${name}`, import.meta.url), 'utf8');

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
  await walk(resolve('modules/marketing/server/src'));
  return total;
}

describe('marketing read-tail r63 can_act selection cutover', () => {
  it('deletes the inline can_act selection and delegates to the shared head', async () => {
    const record = await src('record-purchases.ts');
    expect(record.match(/SELECT /g)).toBeNull();
    expect(record).toContain("import {marketingSelectHeadSql} from './select-head-sql';");
    expect(record).toContain('database.prepare(marketingSelectHeadSql(`marketing_work.id,');
    expect(record).toContain(' can_act FROM (${marketingPurchaseCandidateRows}) marketing_work WHERE');
    expect(record).toContain('AND ${visible.where}`)).bind(actor.assignmentId');
  });

  it('keeps the caller-owned guard, bind order and shared heads intact', async () => {
    const record = await src('record-purchases.ts');
    expect(record).toContain("marketingSourceRowGuardSql('marketing_lead_candidates native'");
    expect(record).toContain('marketingOwnerPointReadSql(\'kind\',`(${rows}) actual`)');
    expect(record).toContain('marketingPointExistsSql(');
    for (const [name, count] of [
      ['select-head-sql.ts', 1],
      ['read-tail-sql.ts', 0],
      ['owner-point-read-sql.ts', 0],
      ['marketing-point-exists-sql.ts', 0],
    ] as const)
      expect((await src(name)).match(/SELECT /g) ?? []).toHaveLength(count);
  });

  it('retirement:ts-statements stays strictly below BEFORE=7 at 5 after r64', async () => {
    expect(await selectTokens()).toBe(1);
  });
});

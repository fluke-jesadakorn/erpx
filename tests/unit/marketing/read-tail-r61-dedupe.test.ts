import { describe, it, expect } from 'vitest';
import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { marketingAnalyticsTouchSql } from '../../../packages/server-marketing/src/adapters/analytics-touch-source-sql';

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

describe('marketing read-tail r61 analytics touch head cutover', () => {
  it('emits byte-identical enforced and legacy touch projections', () => {
    expect(marketingAnalyticsTouchSql(true, 'TOUCH_ROWS')).toBe(
      'SELECT touch.*,lead.branch_id,lead.department_id FROM (TOUCH_ROWS) touch JOIN crm_leads lead ON lead.id=touch.lead_id AND lead.company_id=touch.company_id',
    );
    expect(marketingAnalyticsTouchSql(false, 'TOUCH_ROWS')).toBe(
      'SELECT touch.*,lead.branch_id,lead.department_id FROM marketing_touches touch JOIN crm_leads lead ON lead.id=touch.lead_id',
    );
  });

  it('deletes the local SELECT token and delegates to the shared head', async () => {
    const helper = await src('analytics-touch-source-sql.ts');
    expect(helper.match(/SELECT /g)).toBeNull();
    expect(helper).toContain("import { marketingSelectHeadSql } from './select-head-sql';");
    expect(helper).toContain("'touch.*,lead.branch_id,lead.department_id'");
    const consumer = await src('analytics-sources.ts');
    expect(consumer).toContain('touch:marketingAnalyticsTouchSql(true,touchRows)');
    expect(consumer).toContain('touch:marketingAnalyticsTouchSql(false,touchRows)');
  });

  it('retirement:ts-statements stays strictly below BEFORE=9 at 7 after r62', async () => {
    expect(await selectTokens()).toBe(3);
  });
});

import { describe, it, expect } from 'vitest';
import { readFile } from 'node:fs/promises';

const adapter = (name: string) =>
  new URL(`../../../packages/server-marketing/src/adapters/${name}`, import.meta.url);

const rows = 'TOUCH_ROWS';
const enforced =
  'SELECT touch.*,lead.branch_id,lead.department_id FROM (TOUCH_ROWS) touch JOIN crm_leads lead ON lead.id=touch.lead_id AND lead.company_id=touch.company_id';
const legacy =
  'SELECT touch.*,lead.branch_id,lead.department_id FROM marketing_touches touch JOIN crm_leads lead ON lead.id=touch.lead_id';

describe('marketing analytics touch projection dedupe', () => {
  it('emits byte-identical enforced and legacy touch projections', async () => {
    const { marketingAnalyticsTouchSql } = await import(
      '../../../packages/server-marketing/src/adapters/analytics-touch-source-sql.ts'
    );
    expect(marketingAnalyticsTouchSql(true, rows)).toBe(enforced);
    expect(marketingAnalyticsTouchSql(false, rows)).toBe(legacy);
  });

  it('keeps one query token in the helper and no inline literal in the consumer', async () => {
    const helper = await readFile(adapter('analytics-touch-source-sql.ts'), 'utf8');
    const source = await readFile(adapter('analytics-sources.ts'), 'utf8');
    expect(helper.match(/SELECT /g)).toHaveLength(1);
    expect(source).not.toContain('lead.department_id FROM (${touchRows})');
    expect(source).not.toContain(legacy);
    expect(source).toContain('touch:marketingAnalyticsTouchSql(true,touchRows)');
    expect(source).toContain('touch:marketingAnalyticsTouchSql(false,touchRows)');
  });

  it('preserves the outer source selection and binds', async () => {
    const source = await readFile(adapter('analytics-sources.ts'), 'utf8');
    expect(source).toContain('(${definitions[kind]}) marketing_report');
    expect(source).toContain('sql:legacy[kind],args:[] as unknown[]');
  });

  it('retirement:ts-statements stays strictly below the BEFORE=80 baseline', async () => {
    const files: string[] = [];
    const walk = async (dir: URL) => {
      for (const e of await (await import('node:fs/promises')).readdir(dir, { withFileTypes: true })) {
        const p = new URL(e.name + (e.isDirectory() ? '/' : ''), dir);
        if (e.isDirectory()) await walk(p);
        else if (e.name.endsWith('.ts')) files.push((await readFile(p, 'utf8')));
      }
    };
    await walk(new URL('../../../packages/server-marketing/src/', import.meta.url));
    const total = files.reduce((a, s) => a + (s.match(/SELECT /g) ?? []).length, 0);
    expect(total).toBeLessThan(80);
  });
});

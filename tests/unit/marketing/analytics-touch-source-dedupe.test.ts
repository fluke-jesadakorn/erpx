import { describe, it, expect } from 'vitest';
import { readFile } from 'node:fs/promises';

const adapter = (name: string) =>
  new URL(`../../../modules/marketing/server/src/adapters/${name}`, import.meta.url);

const rows = 'TOUCH_ROWS';
const enforced =
  'SELECT touch.*,lead.branch_id,lead.department_id FROM (TOUCH_ROWS) touch JOIN crm_leads lead ON lead.id=touch.lead_id AND lead.company_id=touch.company_id';
const legacy =
  'SELECT touch.*,lead.branch_id,lead.department_id FROM marketing_touches touch JOIN crm_leads lead ON lead.id=touch.lead_id';

describe('marketing analytics touch projection dedupe', () => {
  it('emits byte-identical enforced and legacy touch projections', async () => {
    const { marketingAnalyticsTouchSql } = await import(
      '../../../modules/marketing/server/src/adapters/analytics-touch-source-sql.ts'
    );
    expect(marketingAnalyticsTouchSql(true, rows)).toBe(enforced);
    expect(marketingAnalyticsTouchSql(false, rows)).toBe(legacy);
  });

  it('keeps one query token in the helper and no inline literal in the consumer', async () => {
    const helper = await readFile(adapter('analytics-touch-source-sql.ts'), 'utf8');
    const source = await readFile(adapter('analytics-sources.ts'), 'utf8');
    expect(helper.match(/SELECT /g) ?? []).toHaveLength(0);
    expect(helper).toContain("import { marketingSelectHeadSql } from './select-head-sql';");
    expect(helper).toContain("marketingSelectHeadSql(");
    expect(source).not.toContain('lead.department_id FROM (${touchRows})');
    expect(source).not.toContain(legacy);
    expect(source).toContain('touch:marketingAnalyticsTouchSql(true,touchRows)');
    expect(source).toContain('touch:marketingAnalyticsTouchSql(false,touchRows)');
    // r56 routes the touch-row source head through the shared renderer, so the
    // consumer no longer owns a SELECT token and emits byte-identical SQL.
    expect(source.match(/SELECT /g)).toBeNull();
    expect(source).toContain("marketingSelectHeadSql('touch.*',` FROM marketing_touches touch");
  });

  it('preserves the outer source selection and binds', async () => {
    const source = await readFile(adapter('analytics-sources.ts'), 'utf8');
    expect(source).toContain(
      "marketingClockedReportFrameSql('marketing_report_clock','', 'marketing_report.*', definitions[kind], 'marketing_report')",
    );
    expect(source).toContain('sql:legacy[kind],args:[] as unknown[]');
  });

  it('retirement:ts-statements stays strictly below the BEFORE=79 baseline', async () => {
    const files: string[] = [];
    const walk = async (dir: URL) => {
      for (const e of await (await import('node:fs/promises')).readdir(dir, { withFileTypes: true })) {
        const p = new URL(e.name + (e.isDirectory() ? '/' : ''), dir);
        if (e.isDirectory()) await walk(p);
        else if (e.name.endsWith('.ts')) files.push((await readFile(p, 'utf8')));
      }
    };
    await walk(new URL('../../../modules/marketing/server/src/', import.meta.url));
    const total = files.reduce((a, s) => a + (s.match(/SELECT /g) ?? []).length, 0);
    expect(total).toBeLessThan(79);
  });
});

describe('marketing materialized clock body dedupe', () => {
  const clock = '(SELECT floor(extract(epoch FROM clock_timestamp())*1000)::bigint now_ms)';

  it('emits byte-identical clock bodies with each caller’s exact spacing', async () => {
    const { marketingClockBody } = await import(
      '../../../modules/marketing/server/src/adapters/materialized-clock-body-sql.ts'
    );
    expect(marketingClockBody('')).toBe(clock);
    expect(marketingClockBody(' ')).toBe(` ${clock}`);
  });

  it('keeps one query token in the helper and no inline clock literal in the three consumers', async () => {
    const helper = await readFile(adapter('materialized-clock-body-sql.ts'), 'utf8');
    expect(helper.match(/SELECT /g) ?? []).toHaveLength(0);
    for (const name of ['analytics-sources.ts', 'metric-purchases.ts', 'record-purchases.ts']) {
      const source = await readFile(adapter(name), 'utf8');
      expect(source).not.toContain('SELECT ${packageClockSql} now_ms');
      expect(source).not.toContain('packageClockSql,');
      expect(source).toContain('marketingClockedReportFrameSql(');
    }
    // r41 moves the frame (including the clock body) into one helper; every
    // caller still passes its exact gap, preserved byte-for-byte by the head.
    expect(await readFile(adapter('analytics-sources.ts'), 'utf8')).toContain(
      "marketingClockedReportFrameSql('marketing_report_clock','', ",
    );
    expect(await readFile(adapter('metric-purchases.ts'), 'utf8')).toContain(
      "marketingClockedReportFrameSql('marketing_metric_clock',' ', ",
    );
    expect(await readFile(adapter('record-purchases.ts'), 'utf8')).toContain(
      "marketingClockedReportFrameSql('marketing_clock','', ",
    );
  });

  it('leaves native read mounts and locks untouched', async () => {
    const record = await readFile(adapter('record-purchases.ts'), 'utf8');
    expect(record).toContain("marketingSourceRowGuardSql('('+nativeIds+') native_marketing','native_marketing.id='+id)");
    // r40 renders the record-read point read from the shared head; the byte
    // oracle is frozen in read-tail-r40-dedupe.test.ts.
    expect(record).not.toContain("SELECT kind FROM (${rows}) actual");
    expect(record).toContain("marketingOwnerPointReadSql('kind',`(${rows}) actual`)");
  });
});

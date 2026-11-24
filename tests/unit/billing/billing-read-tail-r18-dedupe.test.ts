import { readFile, readdir } from 'node:fs/promises';
import { describe, expect, it } from 'vitest';
import { BILLING_PLAN_NAME_LATERAL_SQL } from '../../../packages/server-billing/src/adapters/billing-read-tail-r18-sql';

const LATERAL = 'LEFT JOIN LATERAL (SELECT name FROM billing_plans WHERE id=e.plan_id ORDER BY revision DESC LIMIT 1) p ON TRUE';

async function tsFiles(directory: URL): Promise<string[]> {
  const found: string[] = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (entry.isDirectory()) found.push(...await tsFiles(new URL(`${entry.name}/`, directory)));
    else if (entry.isFile() && entry.name.endsWith('.ts')) found.push(new URL(entry.name, directory).pathname);
  }
  return found;
}

describe('billing read-tail r18 cross-file LATERAL dedupe', () => {
  it('declares the plan-name LATERAL SQL exactly once in the shared module', () => {
    expect(BILLING_PLAN_NAME_LATERAL_SQL).toBe(LATERAL);
    expect(BILLING_PLAN_NAME_LATERAL_SQL.split('SELECT').length - 1).toBe(1);
  });

  it('routes both executive readers through the shared fragment with byte-identical SQL', async () => {
    for (const file of [
      'executive-main-evidence-sources.ts',
      'executive-metric-projections.ts',
    ]) {
      const text = await readFile(
        new URL(`../../../packages/server-billing/src/adapters/${file}`, import.meta.url),
        'utf8',
      );
      expect(text.split("from './billing-read-tail-r18-sql'").length - 1).toBe(1);
      expect(text.split('${BILLING_PLAN_NAME_LATERAL_SQL}').length - 1).toBe(1);
      expect(text).not.toContain(LATERAL);
    }
  });

  it('keeps the fragment read-only and leaves advisory/FOR UPDATE reads untouched', async () => {
    expect(LATERAL).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    const billing = await readFile(
      new URL('../../../packages/server-billing/src/adapters/billing.ts', import.meta.url),
      'utf8',
    );
    expect(billing).toContain('"SELECT pg_advisory_xact_lock(hashtextextended($1,0))"');
    expect(billing).toContain('"SELECT * FROM billing_enrollments WHERE holding_id=$1 AND ended_at IS NULL FOR UPDATE"');
  });

  it('lowers the billing owner SELECT KPI strictly below the r17 after=75 baseline', async () => {
    const files = await tsFiles(new URL('../../../packages/server-billing/src/', import.meta.url));
    let count = 0;
    for (const file of files) count += (await readFile(file, 'utf8')).split('SELECT').length - 1;
    expect(count).toBeLessThanOrEqual(69);
    expect(count).toBeLessThan(75);
  });
});

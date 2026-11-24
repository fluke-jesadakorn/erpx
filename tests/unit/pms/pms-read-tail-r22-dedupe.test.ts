import { readFile, readdir } from 'node:fs/promises';
import { describe, expect, it } from 'vitest';

const dir = new URL('../../../packages/server-pms/src/', import.meta.url);
const src = (name: string) => readFile(new URL(name, dir), 'utf8');
const SHARED = 'adapters/pms-read-tail-r22-sql.ts';

const STRIPE_REFUND = 'SELECT id,property_id,guest_account_id,amount_minor,refund_amount_minor,refund_state,last_error,refund_reason,payer_email FROM pms_stripe_payments';
const ROLLOUT_ENABLED = 'SELECT module_id FROM company_feature_rollouts f WHERE company_id=?';
const ROLLOUT_ACTIVE = 'SELECT 1 FROM company_feature_rollouts f WHERE company_id=? AND module_id=? AND';
const RECIPIENT_FROM = 'FROM (SELECT ?::text user_id) recipient CROSS JOIN (${pmsSourcesSql}) pr';

const STRIPE_CALL_SITES = ['adapters/pms-stripe.ts', 'adapters/pms.ts', 'adapters/pms/record-details.ts'];
const ENABLED_CALL_SITES = ['adapters/feature-availability.ts', 'adapters/hospitality-rollout.ts'];

async function tsFiles(directory: URL): Promise<string[]> {
  const found: string[] = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (entry.isDirectory()) found.push(...await tsFiles(new URL(`${entry.name}/`, directory)));
    else if (entry.isFile() && entry.name.endsWith('.ts')) found.push(new URL(entry.name, directory).pathname);
  }
  return found;
}

describe('pms read-tail r22 byte-identical fragment dedupe', () => {
  it('declares each shared SQL text exactly once', async () => {
    const shared = await src(SHARED);
    for (const sql of [STRIPE_REFUND, ROLLOUT_ENABLED, ROLLOUT_ACTIVE, RECIPIENT_FROM]) {
      expect(shared.split(sql).length - 1).toBe(1);
    }
  });

  it('routes every stripe refund reader through the shared columns with no inline literal', async () => {
    for (const name of STRIPE_CALL_SITES) {
      const text = await src(name);
      expect(text.split('PMS_STRIPE_REFUND_COLUMNS_SQL').length - 1).toBe(2); // import + use
      expect(text).not.toContain(STRIPE_REFUND);
    }
  });

  it('routes every rollout reader through the shared fragments with no inline literal', async () => {
    for (const name of ENABLED_CALL_SITES) {
      const text = await src(name);
      expect(text.split('PMS_ROLLOUT_ENABLED_SQL').length - 1).toBe(2); // import + use
      expect(text).not.toContain(ROLLOUT_ENABLED);
      expect(text).not.toContain(ROLLOUT_ACTIVE);
    }
    expect((await src(ENABLED_CALL_SITES[0])).split('PMS_ROLLOUT_ACTIVE_SQL').length - 1).toBe(3); // import + 2 uses
    expect((await src(ENABLED_CALL_SITES[1])).split('PMS_ROLLOUT_ACTIVE_SQL').length - 1).toBe(2); // import + use
  });

  it('routes both source-scoped readers through the shared recipient FROM fragment', async () => {
    const text = await src('adapters/pms/source.ts');
    expect(text.split('PMS_RECIPIENT_FROM_SQL').length - 1).toBe(3); // import + 2 uses
    expect(text).not.toContain(RECIPIENT_FROM);
  });

  it('keeps the fragments read-only, leaves locks inline and does not re-dedupe r17-r21', async () => {
    for (const sql of [STRIPE_REFUND, ROLLOUT_ENABLED, ROLLOUT_ACTIVE, RECIPIENT_FROM]) {
      expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
    expect(await src('adapters/pms-stripe.ts')).toContain('FOR UPDATE');
    expect(await src('adapters/feature-availability.ts')).toContain('FOR SHARE');
    const r21 = await src('adapters/pms-read-tail-r21-sql.ts');
    expect(r21.split('CROSS JOIN (SELECT ?::text user_id) recipient').length - 1).toBe(1);
    expect(await src('adapters/collaboration-home-sources.ts')).toContain('pmsSourceRecipientProbeSql()');
    expect(await src('adapters/pms-read-tail-r35-sql.ts')).toContain('PMS_RECIPIENT_CROSS_JOIN_SQL');
  });

  it('lowers the pms owner SELECT KPI strictly below the r21 after=474 baseline', async () => {
    let count = 0;
    for (const file of await tsFiles(dir)) count += (await readFile(file, 'utf8')).split('SELECT ').length - 1;
    expect(count).toBe(317);
    expect(count).toBeLessThan(474);
  });
});

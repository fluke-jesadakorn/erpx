import { readFile } from 'node:fs/promises';
import { describe, expect, it } from 'vitest';
import {
  BILLING_INVOICE_BY_ID_SQL,
  BILLING_ACCOUNT_BY_HOLDING_SQL,
  BILLING_CUSTOMER_BY_HOLDING_SQL,
} from '../../../packages/server-billing/src/adapters/billing-read-tail-r15-sql';

const GROUPS: [string, string, string, number][] = [
  ['BILLING_INVOICE_BY_ID_SQL', BILLING_INVOICE_BY_ID_SQL, 'SELECT * FROM billing_invoices WHERE id=$1', 3],
  ['BILLING_ACCOUNT_BY_HOLDING_SQL', BILLING_ACCOUNT_BY_HOLDING_SQL, 'SELECT * FROM billing_accounts WHERE holding_id=$1', 3],
  ['BILLING_CUSTOMER_BY_HOLDING_SQL', BILLING_CUSTOMER_BY_HOLDING_SQL, 'SELECT customer_id FROM billing_accounts WHERE holding_id=$1', 3],
];

describe('billing read-tail r15 byte-identical SQL dedupe', () => {
  it('keeps one byte-identical literal per family in the shared constants file', async () => {
    const shared = await readFile(
      new URL('../../../packages/server-billing/src/adapters/billing-read-tail-r15-sql.ts', import.meta.url),
      'utf8',
    );
    expect(shared.split('SELECT ').length - 1).toBe(3);
    for (const [name, actual, literal] of GROUPS) {
      expect(actual).toBe(literal);
      expect(shared.split(literal).length - 1).toBe(1);
      expect(shared).toContain(name);
    }
  });

  it('removes every inline duplicate from the owner-local jobs adapter', async () => {
    const text = await readFile(
      new URL('../../../packages/server-billing/src/adapters/billing-jobs.ts', import.meta.url),
      'utf8',
    );
    expect(text.split("'./billing-read-tail-r15-sql'").length - 1).toBe(1);
    for (const [name, , literal, uses] of GROUPS) {
      expect(text.split(literal).length - 1).toBe(0);
      expect(text.split(name).length - 1).toBe(uses);
    }
  });
});

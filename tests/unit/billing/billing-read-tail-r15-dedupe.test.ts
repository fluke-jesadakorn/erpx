import { readFile } from 'node:fs/promises';
import { describe, expect, it } from 'vitest';
import { BILLING_INVOICE_BY_ID_SQL } from '../../../modules/billing/server/src/adapters/billing-read-tail-r15-sql';

const literal = 'SELECT * FROM billing_invoices WHERE id=$1';

describe('billing read-tail r15 byte-identical SQL dedupe', () => {
  it('keeps the invoice-by-id literal byte-identical in the r15 constants file', async () => {
    const shared = await readFile(
      new URL('../../../modules/billing/server/src/adapters/billing-read-tail-r15-sql.ts', import.meta.url),
      'utf8',
    );
    expect(BILLING_INVOICE_BY_ID_SQL).toBe(literal);
    expect(shared.split('SELECT ').length - 1).toBe(0); // r35 routes the constant through the r29 head
    expect(shared.split(literal).length - 1).toBe(0);
    expect(shared).toContain('BILLING_INVOICE_BY_ID_SQL');
    expect(shared).not.toContain('BILLING_ACCOUNT_BY_HOLDING_SQL');
    expect(shared).not.toContain('BILLING_CUSTOMER_BY_HOLDING_SQL');
  });

  it('routes the account and customer point reads through the r29 text', async () => {
    const text = await readFile(
      new URL('../../../modules/billing/server/src/adapters/billing-jobs.ts', import.meta.url),
      'utf8',
    );
    expect(text.split("'./billing-read-tail-r15-sql'").length - 1).toBe(1);
    expect(text.split("'./billing-read-tail-r29-sql'").length - 1).toBe(1);
    expect(text.split('BILLING_INVOICE_BY_ID_SQL').length - 1).toBe(3);
    expect(text.split("billingPointReadSql('*','billing_accounts','holding_id=$1')").length - 1).toBe(2);
    expect(text.split("billingPointReadSql('customer_id','billing_accounts','holding_id=$1')").length - 1).toBe(2);
    expect(text).not.toContain('SELECT * FROM billing_accounts WHERE holding_id=$1');
    expect(text).not.toContain('SELECT customer_id FROM billing_accounts WHERE holding_id=$1');
  });
});

import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test, vi } from 'vitest';

vi.mock('server-only', () => ({}));

import { commercialOwnerProjectionSources as commerce } from '@erpx/commerce-server/public/commercial-owner-projection-sources';
import { commercialOwnerProjectionSources as identity } from '@erpx/identity-access-server/public/commercial-owner-projection-sources';
import { commercialOwnerProjectionSources as payroll } from '@erpx/people-payroll-server/public/commercial-owner-projection-sources';
import { salesRecipientEligibility } from '../../../modules/sales/server/src/adapters/recipient';
import { SALES_RD_FOLLOWUP_RECIPIENTS } from '../../../modules/sales/server/src/adapters/native-sales-read';

const srcRoot = resolve(__dirname, '../../../modules/sales/server/src');
const read = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const source = read('native_sales_read_sql.rs');
const followups = read('followups.ts');
const transport = read('native-sales-read.ts');
const obligations = read('package-obligations.ts');

const scope = 'o.company_id=? AND (?::text IS NULL OR o.branch_id=?) AND (?::text IS NULL OR o.department_id=?)';
/** The exact pre-cut rendered TypeScript statement, recomputed from owner-local
 * projections. No bind, alias, permission or column may drift under the cut. */
const rendered = `SELECT DISTINCT c.owner_id id,p.legal_name name FROM (SELECT DISTINCT user_id owner_id FROM (${identity.assignments}) assignments WHERE company_id=?) c JOIN (${payroll.hr_people}) p ON p.user_id=c.owner_id AND p.company_id=? JOIN (${commerce.commerce_orders}) o ON o.id=? WHERE ${scope} AND ${salesRecipientEligibility()} ORDER BY p.legal_name`;

describe('native Sales followup recipients read', () => {
  test('Rust registry owns the byte-identical retired statement', () => {
    expect(SALES_RD_FOLLOWUP_RECIPIENTS).toBe('sales_rd_followup_recipients');
    expect(rendered).toHaveLength(2774);
    expect((rendered.match(/\?/g) ?? []).length).toBe(8);
    expect(createHash('sha256').update(rendered).digest('hex')).toBe(
      'afbc62b98feb6c454dd818d17f53d4882191b25ce507642430140d9f7e8147c6',
    );
    const escaped = rendered.replace(/\\/g, '\\\\').replace(/"/g, '\\"');
    expect(source).toContain(`pub const RD_FOLLOWUP_RECIPIENTS: &str = "sales_rd_followup_recipients";`);
    expect(source).toContain(`"${escaped}"`);
    expect(source).toContain('bind_count: 8,');
    expect(source.match(/READ_FOLLOWUP_RECIPIENTS,/g) ?? []).toHaveLength(1); // registry entry
    for (const word of ['INSERT', 'UPDATE ', 'DELETE', 'LOCK TABLE']) expect(rendered.toUpperCase()).not.toContain(word);
  });

  test('TS adapter transports bounded identities with no SQL mirror', () => {
    expect(transport).toContain("export const SALES_RD_FOLLOWUP_RECIPIENTS = 'sales_rd_followup_recipients';");
    expect(followups).toContain('nativeSalesReadAll<{id:string;name:string}>(SALES_RD_FOLLOWUP_RECIPIENTS,[a.companyId,a.companyId,orderId,...args(a)],db)');
    expect(followups).not.toContain('SELECT DISTINCT c.owner_id');
    expect(followups).not.toContain('identity_accessProjectionSources');
    expect(followups).not.toContain('people_payrollProjectionSources');
    expect(followups).toContain("pointRead('*','customer_followup_events','followup_id',' ORDER BY version DESC')");
  });

  test('r7-r17 generators and source locks stay untouched', () => {
    for (const literal of [
      'const orderDeliveryExistsSql=',
      'const commandAuditPrefixSql=',
      'const allRows=',
      'const factKind=',
      'const scopedSource=',
      'const ecommerceOrderScope=',
    ]) expect(obligations).toContain(literal);
    expect(read('followup-purchases.ts')).toContain('const followupExists=');
    expect(read('followup-purchases.ts').match(/LOCK TABLE/g) ?? []).toHaveLength(1);
    expect(obligations).toContain('LOCK TABLE sales_price_lists,sales_price_list_items,customer_followups,customer_followup_events,customer_contact_notes,customer_contact_note_history,commerce_customer_notifications,commerce_customer_notification_settings,sales_notification_execution_authorizations,sales_notification_execution_attempts,sales_notification_reconciliations,command_receipts,audit_events IN SHARE MODE');
  });
});

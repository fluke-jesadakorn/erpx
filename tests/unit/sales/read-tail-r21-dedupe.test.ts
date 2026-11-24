import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test, vi } from 'vitest';
import ts from 'typescript';

vi.mock('server-only', () => ({ }));

const srcRoot = resolve(__dirname, '../../../packages/server-sales/src');
const helper = readFileSync(resolve(srcRoot, 'adapters/followup-customer-order-read.ts'), 'utf8');
const customers = readFileSync(resolve(srcRoot, 'adapters/customers.ts'), 'utf8');

// Pre-dedupe rendered literals copied from HEAD. The generator must compose both
// join reads byte-for-byte; no scope, column, predicate or order may drift.
const orders = 'ORDERS_PROJECTION';
const scope = 'o.company_id=? AND (?::text IS NULL OR o.branch_id=?) AND (?::text IS NULL OR o.department_id=?)';
const purchaseWhere = 'PURCHASE_WHERE';
const pre = {
  heads: `SELECT DISTINCT ON (o.party_id) o.party_id,c.id,c.due_date,c.next_action,c.order_id FROM customer_followups c JOIN (${orders}) o ON o.id=c.order_id AND o.company_id=c.company_id WHERE ${scope} AND ${purchaseWhere} AND o.party_id IN (?) AND c.status='open' ORDER BY o.party_id,c.due_date,c.id`,
  followups: `SELECT c.id,c.next_action,c.due_date,c.status,c.order_id,o.number order_number FROM customer_followups c JOIN (${orders}) o ON o.id=c.order_id AND o.company_id=c.company_id WHERE ${scope} AND ${purchaseWhere} AND o.party_id=? AND c.status='open' ORDER BY c.due_date,c.id LIMIT 30`,
} as const;

type Gen = (orders: string, columns: string, scope: string, purchaseWhere: string, predicate: string, order: string) => string;
function loadGenerator(): Gen {
  const file = ts.createSourceFile('followup-customer-order-read.ts', helper, ts.ScriptTarget.Latest, true, ts.ScriptKind.TS);
  const decls = file.statements
    .filter(ts.isVariableStatement)
    .flatMap(s => [...s.declarationList.declarations])
    .filter(d => d.name.getText(file) === 'followupCustomerOrderRead');
  expect(decls).toHaveLength(1);
  const code = ts.transpileModule(`const ${decls[0].getText(file)};`, {
    compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS },
  }).outputText;
  return new Function(`${code}\nreturn followupCustomerOrderRead;`)() as Gen;
}

describe('Sales read-tail r21 parameterized followup/customer order join family', () => {
  const gen = loadGenerator();

  test.each([
    ['heads', gen(orders, 'DISTINCT ON (o.party_id) o.party_id,c.id,c.due_date,c.next_action,c.order_id', scope, purchaseWhere, "o.party_id IN (?) AND c.status='open'", 'ORDER BY o.party_id,c.due_date,c.id'), pre.heads],
    ['followups', gen(orders, 'c.id,c.next_action,c.due_date,c.status,c.order_id,o.number order_number', scope, purchaseWhere, "o.party_id=? AND c.status='open'", 'ORDER BY c.due_date,c.id LIMIT 30'), pre.followups],
  ] as ['heads' | 'followups', string, string][])(
    '%s stays byte-identical to the pre-dedupe literal',
    (_name, actual, expected) => {
      expect(actual).toBe(expected);
      expect(createHash('sha256').update(actual).digest('hex')).toBe(
        createHash('sha256').update(expected).digest('hex'),
      );
    },
  );

  test('the family is one generator literal with two callers and no inlining', () => {
    expect(helper.match(/const followupCustomerOrderRead\s*=/g) ?? []).toHaveLength(1);
    expect((helper.match(/SELECT/g) ?? []).length).toBe(1);
    expect(customers).toContain("followupCustomerOrderRead(commerceProjectionSources.commerce_orders,'DISTINCT ON (o.party_id)");
    expect(customers).toContain("followupCustomerOrderRead(commerceProjectionSources.commerce_orders,'c.id,c.next_action,c.due_date,c.status,c.order_id,o.number order_number'");
    expect(customers).not.toContain("`SELECT DISTINCT ON (o.party_id)");
    expect(customers).not.toContain("`SELECT c.id,c.next_action,c.due_date,c.status,c.order_id,o.number order_number");
    expect(helper).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });

  test('r7-r20 generators, assignmentReadSql, locks and commercial-owner projections stay untouched', () => {
    const obligations = readFileSync(resolve(srcRoot, 'adapters/package-obligations.ts'), 'utf8');
    const finalRoot = readFileSync(resolve(srcRoot, 'adapters/final-root-owner-projection-sources.ts'), 'utf8');
    const followupSource = readFileSync(resolve(srcRoot, 'adapters/followup-source-sql.ts'), 'utf8');
    const purchases = readFileSync(resolve(srcRoot, 'adapters/followup-purchases.ts'), 'utf8');
    const metric = readFileSync(resolve(srcRoot, 'adapters/metric-purchases.ts'), 'utf8');
    const byKey = readFileSync(resolve(srcRoot, 'adapters/by-key.ts'), 'utf8');
    const pointRead = readFileSync(resolve(srcRoot, 'adapters/point-read.ts'), 'utf8');
    const commercial = readFileSync(resolve(srcRoot, 'adapters/commercial-owner-projection-sources.ts'), 'utf8');
    for (const literal of [
      'const orderDeliveryExistsSql=', // r7
      'const commandAuditPrefixSql=', // r9
      'const allRows=', // r13
      'const factKind=', // r14
      'const scopedSource=', // r16
      'const ecommerceNoteOrderScope=', // r7
      'const ecommerceFollowupOrderScope=', // r7
      'const ecommerceNoticeOrderScope=', // r7
    ]) expect(obligations).toContain(literal);
    expect(finalRoot).toMatch(/const tableRead\s*=/); // r15
    expect(followupSource).toContain('export const actorScopeTuple='); // r12/r15
    expect(purchases).toContain('const followupExists='); // r17
    expect(metric).toContain('const crmAssignmentExists='); // r11
    expect(byKey).toContain('export const byKey ='); // r19
    expect(pointRead).toContain('export const pointRead ='); // r20
    // assignmentReadSql consumers remain; no lock or commercial projection edited here
    expect(purchases).toContain('assignmentReadSql(');
    expect(metric).toContain('assignmentReadSql(');
    expect(purchases).toContain('LOCK TABLE customer_followups,customer_followup_events IN SHARE ROW EXCLUSIVE MODE');
    expect(obligations).toContain('LOCK TABLE sales_price_lists,sales_price_list_items,customer_followups,customer_followup_events,customer_contact_notes,customer_contact_note_history,commerce_customer_notifications,commerce_customer_notification_settings,sales_notification_execution_authorizations,sales_notification_execution_attempts,sales_notification_reconciliations,command_receipts,audit_events IN SHARE MODE');
    expect((commercial.match(/SELECT/g) ?? []).length).toBe(3);
  });
});

describe('Sales read-tail r21 retirement check', () => {
  test('retirement:ts-statements stays strictly below the r21 BEFORE=66 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT/g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(66);
  });
});

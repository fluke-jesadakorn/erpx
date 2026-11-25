import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test, vi } from 'vitest';
import ts from 'typescript';

vi.mock('server-only', () => ({}));

const srcRoot = resolve(__dirname, '../../../packages/server-sales/src');
const obligations = readFileSync(resolve(srcRoot, 'adapters/package-obligations.ts'), 'utf8');

/** Load the owner-local generators by AST name and run them in isolation, the
 * same way the r22 receipt/fact family is verified. No caller input reaches SQL. */
function loadGenerators(names: string[]): Record<string, unknown> {
  const file = ts.createSourceFile('package-obligations.ts', obligations, ts.ScriptTarget.Latest, true, ts.ScriptKind.TS);
  const decls = file.statements
    .filter(ts.isVariableStatement)
    .flatMap(s => [...s.declarationList.declarations])
    .filter(d => names.includes(d.name.getText(file)));
  expect(decls).toHaveLength(names.length);
  const code = ts.transpileModule(decls.map(d => `const ${d.getText(file)};`).join('\n'), {
    compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS },
  }).outputText;
  return new Function(`${code}\nreturn {${names.join(',')}};`)() as Record<string, unknown>;
}

type Gen2 = (operation: string, predicate: string) => string;
type Gen1 = (predicate: string) => string;
type Loaded = {
  reconciledAttempt: string;
  commandReceiptGuard: Gen2;
  attemptGuard: Gen1;
};

describe('Sales read-tail r26 parameterized presence-guard families', () => {
  const loaded = loadGenerators(['reconciledAttempt', 'commandReceiptGuard', 'attemptGuard']) as Loaded;
  const rec = loaded.reconciledAttempt;

  // Pre-dedupe rendered literals copied from the r25 baseline. Each generator
  // must compose its guard byte-for-byte; no operation, alias or predicate may
  // drift. reconciledAttempt is an owner-local fixed token, not caller input.
  const pre = {
    orderFulfillReceipt: "SELECT 1 FROM command_receipts receipt WHERE receipt.operation='commerce.order.fulfill' AND receipt.record_id=f.order_id AND receipt.authority_json::jsonb->>'companyId'=f.company_id AND receipt.result_json::jsonb->>'fulfillmentId'=f.id",
    customerNoticeReceipt: "SELECT 1 FROM command_receipts receipt WHERE receipt.operation='commerce.customer-notification.queue' AND receipt.record_id=notice.fulfillment_id AND receipt.actor_id=notice.created_by AND receipt.authority_json::jsonb->>'companyId'=notice.company_id AND receipt.result_json::jsonb->>'id'=notice.id",
    noticeAttempt: `SELECT 1 FROM sales_notification_execution_attempts attempt WHERE attempt.notification_id=notice.id AND (attempt.company_id<>notice.company_id OR (attempt.state='sending' OR attempt.state='unknown' AND NOT ${rec}))`,
    resolutionAttempt: `SELECT 1 FROM sales_notification_execution_attempts attempt WHERE attempt.id=resolution.attempt_id AND attempt.notification_id=notice.id AND attempt.company_id=notice.company_id AND attempt.state='unknown' AND ${rec}`,
  } as const;

  const orderFulfillPredicate = "receipt.record_id=f.order_id AND receipt.authority_json::jsonb->>'companyId'=f.company_id AND receipt.result_json::jsonb->>'fulfillmentId'=f.id";
  const queuePredicate = "receipt.record_id=notice.fulfillment_id AND receipt.actor_id=notice.created_by AND receipt.authority_json::jsonb->>'companyId'=notice.company_id AND receipt.result_json::jsonb->>'id'=notice.id";
  const noticePredicate = `attempt.notification_id=notice.id AND (attempt.company_id<>notice.company_id OR (attempt.state='sending' OR attempt.state='unknown' AND NOT ${rec}))`;
  const resolutionPredicate = `attempt.id=resolution.attempt_id AND attempt.notification_id=notice.id AND attempt.company_id=notice.company_id AND attempt.state='unknown' AND ${rec}`;

  test.each([
    ['orderFulfillReceipt', loaded.commandReceiptGuard('commerce.order.fulfill', orderFulfillPredicate), pre.orderFulfillReceipt],
    ['customerNoticeReceipt', loaded.commandReceiptGuard('commerce.customer-notification.queue', queuePredicate), pre.customerNoticeReceipt],
    ['noticeAttempt', loaded.attemptGuard(noticePredicate), pre.noticeAttempt],
    ['resolutionAttempt', loaded.attemptGuard(resolutionPredicate), pre.resolutionAttempt],
  ] as ['orderFulfillReceipt' | 'customerNoticeReceipt' | 'noticeAttempt' | 'resolutionAttempt', string, string][])(
    '%s stays byte-identical to its pre-dedupe literal',
    (_name, actual, expected) => {
      expect(actual).toBe(expected);
      expect(createHash('sha256').update(actual).digest('hex')).toBe(
        createHash('sha256').update(expected).digest('hex'),
      );
    },
  );

  test('each family is one generator literal with two callers and no inlining', () => {
    expect(obligations.match(/const commandReceiptGuard\s*=/g) ?? []).toHaveLength(1);
    expect(obligations.match(/const attemptGuard\s*=/g) ?? []).toHaveLength(1);
    const family = obligations.slice(obligations.indexOf('const commandReceiptGuard='), obligations.indexOf('const deliveryFactExists='));
    expect((family.match(/SELECT/g) ?? []).length).toBe(2); // one per generator
    expect(obligations).not.toContain("commandReceiptGuard('commerce.order.fulfill'");
    expect(obligations).toContain('SALES_RD_PACKAGE_SETTLEMENT_ECOMMERCE');
    expect(obligations).not.toContain("commandReceiptGuard('commerce.customer-notification.queue'");
    expect((obligations.match(/\$\{attemptGuard\(/g) ?? []).length).toBe(0);
    expect(obligations).not.toContain(pre.orderFulfillReceipt);
    expect(obligations).not.toContain(pre.customerNoticeReceipt);
    expect(obligations).not.toContain("SELECT 1 FROM command_receipts receipt WHERE receipt.operation='commerce.order.fulfill'");
    expect(obligations).not.toContain('SELECT 1 FROM sales_notification_execution_attempts attempt WHERE attempt.notification_id');
    expect(family).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });

  test('r1-r25 generators, assignmentReadSql, locks, registry and native reads stay untouched', () => {
    for (const literal of [
      'const reconciledAttempt=',
      'const orderDeliveryExistsSql=', // r7
      'const commandAuditPrefixSql=', // r9
      'const ecommerceOrderScope=', // r7, collapsed in r27
      'const allRows=', // r13
      'const factKind=', // r14
      'const scopedSource=', // r16
      'const deliveryFactExists=', // r22
    ]) expect(obligations).toContain(literal);
    const finalRoot = readFileSync(resolve(srcRoot, 'adapters/final-root-owner-projection-sources.ts'), 'utf8');
    const source = readFileSync(resolve(srcRoot, 'adapters/followup-source-sql.ts'), 'utf8');
    const purchases = readFileSync(resolve(srcRoot, 'adapters/followup-purchases.ts'), 'utf8');
    const metric = readFileSync(resolve(srcRoot, 'adapters/metric-purchases.ts'), 'utf8');
    const byKey = readFileSync(resolve(srcRoot, 'adapters/by-key.ts'), 'utf8');
    const pointRead = readFileSync(resolve(srcRoot, 'adapters/point-read.ts'), 'utf8');
    const orderRead = readFileSync(resolve(srcRoot, 'adapters/followup-customer-order-read.ts'), 'utf8');
    const notes = readFileSync(resolve(srcRoot, 'adapters/notes.ts'), 'utf8');
    const native = readFileSync(resolve(srcRoot, 'adapters/native-sales-read.ts'), 'utf8');
    const rustRegistry = readFileSync(resolve(srcRoot, 'adapters/native_sales_read_sql.rs'), 'utf8');
    const commercial = readFileSync(resolve(srcRoot, 'adapters/commercial-owner-projection-sources.ts'), 'utf8');
    expect(finalRoot).toMatch(/const tableRead\s*=/); // r15
    expect(source).toContain('export const actorScopeTuple='); // r12/r15
    expect(purchases).toContain('const followupExists='); // r17
    expect(metric).toContain('SALES_RD_CRM_ASSIGNMENT_PLAIN'); // r11
    expect(byKey).toContain('export const byKey ='); // r19
    expect(pointRead).toContain('export const pointRead ='); // r20
    expect(orderRead).toContain('export const followupCustomerOrderRead ='); // r21
    // lane 30 retired both customer-note SELECT literals from TypeScript.
    expect(notes).not.toContain('SELECT');
    expect(notes).toContain("from './native-lane-30/client'");
    expect(purchases).toContain('assignmentReadSql(');
    expect(metric).toContain('assignmentReadSql(');
    expect(purchases).toContain('LOCK TABLE customer_followups,customer_followup_events IN SHARE ROW EXCLUSIVE MODE');
    expect(obligations).toContain('LOCK TABLE sales_price_lists,sales_price_list_items,customer_followups,customer_followup_events,customer_contact_notes,customer_contact_note_history,commerce_customer_notifications,commerce_customer_notification_settings,sales_notification_execution_authorizations,sales_notification_execution_attempts,sales_notification_reconciliations,command_receipts,audit_events IN SHARE MODE');
    expect(native).toContain('export const SALES_RD_PRICE_LIST_ACTIVE');
    expect(native).toContain('export const SALES_RD_PRICE_LIST_ITEMS_ACTIVE');
    expect(native).toContain('export const SALES_RD_NOTIFICATION_ATTEMPT');
    expect(native).toContain('export const SALES_RD_NOTIFICATION_RECONCILIATION_INSTALLED');
    expect((rustRegistry.match(/pub const RD_\w+: &str =/g) ?? []).length).toBe(27);
    expect((commercial.match(/SELECT/g) ?? []).length).toBe(0);
  });
});

describe('Sales read-tail r26 retirement check', () => {
  test('retirement:ts-statements stays strictly below the r27 BEFORE=60 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT/g) ?? []).length;
      }
    };
    walk(srcRoot);
    // r28 collapsed three authority-scope reads into one generator (-2);
    // r29 collapsed three scope-tuple literals into one generator (-2).
    // r30 shared tableRead (-3) and priceListSelect (-1).
    // r33 folds the tail generator into the primary read generator (-1).
    // r35: delivery-presence head shared by order/return/notice guards (-1).
    expect(total).toBe(19);
    expect(total).toBeLessThan(60);
  });
});

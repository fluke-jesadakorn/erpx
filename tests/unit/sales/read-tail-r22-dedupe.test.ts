import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test, vi } from 'vitest';
import ts from 'typescript';

vi.mock('server-only', () => ({}));

const srcRoot = resolve(__dirname, '../../../packages/server-sales/src');
const obligations = readFileSync(resolve(srcRoot, 'adapters/package-obligations.ts'), 'utf8');

// Pre-dedupe rendered literals copied from HEAD. The generator must compose both
// delivery-fulfillment guards byte-for-byte; no alias or tail predicate may drift.
const pre = {
  return: 'SELECT 1 FROM deliveries f WHERE f.id=r.fulfillment_id AND f.order_id=r.order_id AND f.invoice_id=r.invoice_id',
  customerNotice: 'SELECT 1 FROM deliveries f WHERE f.id=notice.fulfillment_id AND f.order_id=notice.order_id AND f.company_id=notice.company_id AND f.party_id=notice.party_id',
} as const;

type Gen = (alias: string, tail: string) => string;
function loadGenerator(): Gen {
  const file = ts.createSourceFile('package-obligations.ts', obligations, ts.ScriptTarget.Latest, true, ts.ScriptKind.TS);
  const names = ['deliveryPresence', 'deliveryFactExists'];
  const decls = file.statements
    .filter(ts.isVariableStatement)
    .flatMap(s => [...s.declarationList.declarations])
    .filter(d => names.includes(d.name.getText(file)));
  expect(decls).toHaveLength(2);
  const code = ts.transpileModule(decls.map(d => `const ${d.getText(file)};`).join('\n'), {
    compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS },
  }).outputText;
  return new Function(`${code}\nreturn deliveryFactExists;`)() as Gen;
}

describe('Sales read-tail r22 parameterized delivery-fulfillment existence family', () => {
  const gen = loadGenerator();

  test.each([
    ['return', gen('r', ' AND f.invoice_id=r.invoice_id'), pre.return],
    ['customerNotice', gen('notice', ' AND f.company_id=notice.company_id AND f.party_id=notice.party_id'), pre.customerNotice],
  ] as ['return' | 'customerNotice', string, string][])(
    '%s stays byte-identical to the pre-dedupe literal',
    (_name, actual, expected) => {
      expect(actual).toBe(expected);
      expect(createHash('sha256').update(actual).digest('hex')).toBe(
        createHash('sha256').update(expected).digest('hex'),
      );
    },
  );

  test('the family is one shared head, one delegator and two callers, with no inlining', () => {
    expect(obligations.match(/const deliveryPresence\s*=/g) ?? []).toHaveLength(1);
    expect(obligations.match(/const deliveryFactExists\s*=/g) ?? []).toHaveLength(1);
    const head = obligations.slice(obligations.indexOf('const deliveryPresence='), obligations.indexOf('const orderDeliveryExistsSql='));
    expect((head.match(/SELECT/g) ?? []).length).toBe(1);
    const helper = obligations.slice(obligations.indexOf('const deliveryFactExists='), obligations.indexOf('type Input='));
    expect((helper.match(/SELECT/g) ?? []).length).toBe(0);
    expect(helper).toContain('deliveryPresence(');
    expect(obligations).toContain("deliveryFactExists('r',' AND f.invoice_id=r.invoice_id')");
    expect(obligations).toContain("deliveryFactExists('notice',' AND f.company_id=notice.company_id AND f.party_id=notice.party_id')");
    expect(obligations).not.toContain(pre.return);
    expect(obligations).not.toContain(pre.customerNotice);
    expect(helper).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });

  test('r1-r21 generators, assignmentReadSql, locks and native reads stay untouched', () => {
    const finalRoot = readFileSync(resolve(srcRoot, 'adapters/final-root-owner-projection-sources.ts'), 'utf8');
    const followupSource = readFileSync(resolve(srcRoot, 'adapters/followup-source-sql.ts'), 'utf8');
    const purchases = readFileSync(resolve(srcRoot, 'adapters/followup-purchases.ts'), 'utf8');
    const metric = readFileSync(resolve(srcRoot, 'adapters/metric-purchases.ts'), 'utf8');
    const byKey = readFileSync(resolve(srcRoot, 'adapters/by-key.ts'), 'utf8');
    const pointRead = readFileSync(resolve(srcRoot, 'adapters/point-read.ts'), 'utf8');
    const orderRead = readFileSync(resolve(srcRoot, 'adapters/followup-customer-order-read.ts'), 'utf8');
    const notes = readFileSync(resolve(srcRoot, 'adapters/notes.ts'), 'utf8');
    const native = readFileSync(resolve(srcRoot, 'adapters/native-sales-read.ts'), 'utf8');
    const commercial = readFileSync(resolve(srcRoot, 'adapters/commercial-owner-projection-sources.ts'), 'utf8');
    for (const literal of [
      'const reconciledAttempt=',
      'const orderDeliveryExistsSql=', // r7
      'const commandAuditPrefixSql=', // r9
      'const ecommerceOrderScope=', // r7, collapsed in r27
      'const allRows=', // r13
      'const factKind=', // r14
      'const scopedSource=', // r16
    ]) expect(obligations).toContain(literal);
    expect(finalRoot).toMatch(/const tableRead\s*=/); // r15
    expect(followupSource).toContain('export const actorScopeTuple='); // r12/r15
    expect(purchases).toContain('const followupExists='); // r17
    expect(metric).toContain('const crmAssignmentExists='); // r11
    expect(byKey).toContain('export const byKey ='); // r19
    expect(pointRead).toContain('export const pointRead ='); // r20
    expect(orderRead).toContain('export const followupCustomerOrderRead ='); // r21
    // lane 30 retired both customer-note SELECT literals from TypeScript.
    expect(notes).not.toContain('SELECT');
    expect(notes).toContain("from './native-lane-30/client'");
    // assignmentReadSql consumers remain; no lock or native read edited here
    expect(purchases).toContain('assignmentReadSql(');
    expect(metric).toContain('assignmentReadSql(');
    expect(purchases).toContain('LOCK TABLE customer_followups,customer_followup_events IN SHARE ROW EXCLUSIVE MODE');
    expect(obligations).toContain('LOCK TABLE sales_price_lists,sales_price_list_items,customer_followups,customer_followup_events,customer_contact_notes,customer_contact_note_history,commerce_customer_notifications,commerce_customer_notification_settings,sales_notification_execution_authorizations,sales_notification_execution_attempts,sales_notification_reconciliations,command_receipts,audit_events IN SHARE MODE');
    expect(native).toContain('export const SALES_RD_PRICE_LIST_ACTIVE');
    expect(native).toContain('export const SALES_RD_PRICE_LIST_ITEMS_ACTIVE');
    expect((commercial.match(/SELECT/g) ?? []).length).toBe(0);
  });
});

describe('Sales read-tail r22 retirement check', () => {
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
    expect(total).toBe(42);
    expect(total).toBeLessThan(60);
  });
});

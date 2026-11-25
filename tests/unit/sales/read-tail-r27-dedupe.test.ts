import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test, vi } from 'vitest';
import ts from 'typescript';

vi.mock('server-only', () => ({}));

import { readSalesPackageSettlementFacts } from '../../../packages/server-sales/src/adapters/package-obligations';

const srcRoot = resolve(__dirname, '../../../packages/server-sales/src');
const obligations = readFileSync(resolve(srcRoot, 'adapters/package-obligations.ts'), 'utf8');

// Pre-r27 rendered literals copied from the r26 baseline. The alias is the only
// varying token; no relation, column or predicate may drift under the generator.
const pre = {
  note: 'EXISTS(SELECT 1 FROM orders o WHERE o.id=note.order_id)',
  followup: 'EXISTS(SELECT 1 FROM orders o WHERE o.id=followup.order_id)',
  notice: 'EXISTS(SELECT 1 FROM orders o WHERE o.id=notice.order_id)',
} as const;

function loadGenerator(): (alias: string) => string {
  const file = ts.createSourceFile('package-obligations.ts', obligations, ts.ScriptTarget.Latest, true, ts.ScriptKind.TS);
  const decls = file.statements
    .filter(ts.isVariableStatement)
    .flatMap(s => [...s.declarationList.declarations])
    .filter(d => d.name.getText(file) === 'ecommerceOrderScope');
  expect(decls).toHaveLength(1);
  const code = ts.transpileModule(`const ${decls[0].getText(file)};`, {
    compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS },
  }).outputText;
  return new Function(`${code}\nreturn ecommerceOrderScope;`)() as (alias: string) => string;
}

// SHA-256 of the full statement rendered at the pre-r14 HEAD, still asserted by
// the r14 byte-identity oracle. Both selections must remain byte-identical.
const preRendered = {
  sales: '6e58228f3864bfe8bc0af13a59ec45bb2682081a8c83fef082f610a8a4c9ba5f',
  ecommerce: '3706e17efee2b1ca4f41237de188b66864314297adc4c11a5d7c5ab3e4a29684',
} as const;

async function rendered(selection: 'sales' | 'ecommerce'): Promise<string> {
  const captured: string[] = [];
  const db = {
    prepare(sql: string) {
      captured.push(sql);
      return { bind: () => ({ all: async () => ({ results: [] }) }) };
    },
  };
  await readSalesPackageSettlementFacts(db as never, 'c1', selection);
  if (captured.length !== 1) throw new Error(`expected one prepared statement, got ${captured.length}`);
  return captured[0]!;
}

describe('Sales read-tail r27 parameterized ecommerce order-scope family', () => {
  const gen = loadGenerator();

  test.each([
    ['note', gen('note'), pre.note],
    ['followup', gen('followup'), pre.followup],
    ['notice', gen('notice'), pre.notice],
  ] as ['note' | 'followup' | 'notice', string, string][])(
    '%s scope stays byte-identical to the pre-dedupe literal',
    (_name, actual, expected) => {
      expect(actual).toBe(expected);
      expect(createHash('sha256').update(actual).digest('hex')).toBe(
        createHash('sha256').update(expected).digest('hex'),
      );
    },
  );

  test.each(Object.entries(preRendered) as ['sales' | 'ecommerce', string][])(
    '%s full statement stays byte-identical to the pre-r27 render',
    async (selection, sha) => {
      expect(createHash('sha256').update(await rendered(selection)).digest('hex')).toBe(sha);
    },
  );

  test('one generator literal with eight callers and no inlined old constants', () => {
    expect(obligations.match(/const ecommerceOrderScope\s*=/g) ?? []).toHaveLength(1);
    expect((obligations.match(/ecommerceOrderScope\('note'\)/g) ?? []).length).toBe(2);
    expect((obligations.match(/ecommerceOrderScope\('followup'\)/g) ?? []).length).toBe(2);
    expect((obligations.match(/ecommerceOrderScope\('notice'\)/g) ?? []).length).toBe(4);
    expect(obligations.match(/const ecommerce(?:Note|Followup|Notice)OrderScope\s*=/g) ?? []).toHaveLength(0);
    expect(obligations).not.toContain(pre.note);
    expect(obligations).not.toContain(pre.followup);
    expect(obligations).not.toContain(pre.notice);
    const family = obligations.slice(obligations.indexOf('const ecommerceOrderScope='), obligations.indexOf('const deliveryPresence='));
    expect((family.match(/SELECT/g) ?? []).length).toBe(1);
    expect(family).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });

  test('r1-r26 generators, assignmentReadSql, locks and native reads stay untouched', () => {
    for (const literal of [
      'const reconciledAttempt=',
      'const ecommerceOrderScope=', // r7, collapsed in r27
      'const orderDeliveryExistsSql=', // r7
      'const commandAuditPrefixSql=', // r9
      'const allRows=', // r13
      'const factKind=', // r14
      'const scopedSource=', // r16
      'const deliveryFactExists=', // r22
      'const commandReceiptGuard=', // r26
      'const attemptGuard=', // r26
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
    expect(metric).toContain('const crmAssignmentExists='); // r11
    expect(byKey).toContain('export const byKey ='); // r19
    expect(pointRead).toContain('export const pointRead ='); // r20
    expect(orderRead).toContain('export const followupCustomerOrderRead ='); // r21
    // lane 30 retired the two customer-note SELECT literals from TypeScript;
    // the native Sales axum owner now executes and projects them.
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
    expect((rustRegistry.match(/pub const RD_\w+: &str =/g) ?? []).length).toBe(15);
    expect((commercial.match(/SELECT/g) ?? []).length).toBe(0);
  });
});

describe('Sales read-tail r27 retirement check', () => {
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
    // r27 collapsed three order-scope literals (-2); r28 collapsed three
    // authority-scope reads into one generator (-2); r29 collapsed three
    // scope-tuple literals into one generator (-2); r30 shared tableRead (-3)
    // and priceListSelect (-1).
    // r33 folds the tail generator into the primary read generator (-1).
    // r35: delivery-presence head shared by order/return/notice guards (-1).
    // lane 30 retires the two customer-note SELECT literals (-2) to the native
    // Sales axum owner.
    expect(total).toBe(32);
    expect(total).toBeLessThan(60);
  });
});

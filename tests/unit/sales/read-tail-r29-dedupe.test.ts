import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test, vi } from 'vitest';
import ts from 'typescript';

vi.mock('server-only', () => ({}));

import { readSalesPackageSettlementFacts } from '../../../packages/server-sales/src/adapters/package-obligations';

const srcRoot = resolve(__dirname, '../../../packages/server-sales/src');
const read = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const helper = read('read-tail-r29-sql.ts');
const source = read('followup-source-sql.ts');
const pricing = read('pricing-purchases.ts');
const purchases = read('followup-purchases.ts');
const metric = read('metric-purchases.ts');
const obligations = read('package-obligations.ts');

// SHA-256 of each pre-r29 inline scope-tuple literal. Only the closed `mode`
// varies; no column, bind or surrounding SQL may drift under the generator.
const pre = {
  actor: '1459a347ab36cfdd44cc7cd35ecfcdfb80c5c8b2193b51b14e4ba973885327ec',
  company: '940ca898b24f5a0374df4ef7549a0db2f6cb3ba9b1f1d7fe807461cc645d1f36',
  scope: 'ab105d6a6b61cd796b415ffda0fc43d9afac1afca484ab1443ca6084589cc4ef',
} as const;
const literals = {
  actor: '(SELECT ?::text company_id,?::text branch_id,?::text department_id)',
  company: '(SELECT ?::text company_id,NULL::text branch_id,NULL::text department_id)',
  scope: '(SELECT ?::text company_id)',
} as const;

// Full-statement oracles inherited from the pre-r27 baseline. Both selections
// must remain byte-identical after the scope tuple moves behind scopeTuple().
const preRendered = {
  sales: '6e58228f3864bfe8bc0af13a59ec45bb2682081a8c83fef082f610a8a4c9ba5f',
  ecommerce: '3706e17efee2b1ca4f41237de188b66864314297adc4c11a5d7c5ab3e4a29684',
} as const;

const sha = (value: string) => createHash('sha256').update(value).digest('hex');

function loadGenerator(): (mode: 'actor' | 'company' | 'scope') => string {
  const file = ts.createSourceFile('read-tail-r29-sql.ts', helper, ts.ScriptTarget.Latest, true, ts.ScriptKind.TS);
  const decls = file.statements
    .filter(ts.isVariableStatement)
    .flatMap(s => [...s.declarationList.declarations])
    .filter(d => d.name.getText(file) === 'scopeTuple');
  expect(decls).toHaveLength(1);
  const code = ts.transpileModule(`const ${decls[0]!.getText(file)};`, {
    compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS },
  }).outputText;
  return new Function(`${code}\nreturn scopeTuple;`)() as (mode: 'actor' | 'company' | 'scope') => string;
}

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

describe('Sales read-tail r29 scope-tuple family', () => {
  const gen = loadGenerator();

  test.each(Object.entries(literals) as ['actor' | 'company' | 'scope', string][])(
    '%s scope tuple stays byte-identical to the pre-r29 literal',
    (mode, expected) => {
      const actual = gen(mode);
      expect(actual).toBe(expected);
      expect(sha(actual)).toBe(pre[mode]);
    },
  );

  test.each(Object.entries(preRendered) as ['sales' | 'ecommerce', string][])(
    '%s full settlement statement stays byte-identical to the pre-r27 render',
    async (selection, expected) => {
      expect(sha(await rendered(selection))).toBe(expected);
    },
  );

  test('one closed generator with three modes and no inlined old literals', () => {
    expect(helper.match(/export const scopeTuple\s*=/g) ?? []).toHaveLength(1);
    expect(helper).toContain("export type ScopeTupleMode = 'actor' | 'company' | 'scope'");
    expect((helper.match(/SELECT/g) ?? []).length).toBe(1);
    expect(helper).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);

    expect(source).toContain("export const actorScopeTuple=scopeTuple('actor');");
    expect(pricing).toContain("authorityScopeRead(assignmentReadSql('pricing_scope',permission),scopeTuple('company'),'pricing_scope')");
    expect(obligations).toContain("WITH scope AS${scopeTuple('scope')},orders AS");
    for (const literal of Object.values(literals)) {
      expect(source).not.toContain(literal);
      expect(pricing).not.toContain(literal);
      expect(obligations).not.toContain(literal);
    }
    expect(source.match(/export const actorScopeTuple=/g) ?? []).toHaveLength(1);
  });

  test('binds, locks and r1-r28 generators stay untouched', () => {
    expect(pricing).toContain('.bind(actor.assignmentId,actor.userId,actor.role,actor.companyId)');
    expect(obligations).toContain('.bind(companyId)');
    expect(purchases).toContain('.bind(actor.assignmentId,actor.userId,actor.role,actor.companyId,record?.branch_id??actor.branchId,record?.department_id??actor.departmentId)');
    expect(metric).toContain('.bind(actor.assignmentId,actor.userId,actor.role,actor.assignmentId,actor.companyId,actor.branchId,actor.departmentId)');

    expect(source).toContain('export const authorityScopeRead=');
    expect(source).toContain('export const clockNowMs=');
    expect(source).toContain('export const followupEvidencePrefix=');
    expect(source).toContain('export const followupMetricPrefix=');
    expect(pricing).toContain('authorityScopeRead(');
    expect(pricing).toContain('assignmentReadSql(');
    expect(purchases).toContain('assignmentReadSql(');
    expect(metric).toContain('assignmentReadSql(');

    expect(purchases).toContain('LOCK TABLE customer_followups,customer_followup_events IN SHARE ROW EXCLUSIVE MODE');
    expect(pricing).toContain('LOCK TABLE sales_price_lists,sales_price_list_items IN SHARE ROW EXCLUSIVE MODE');
    expect(obligations).toContain('LOCK TABLE sales_price_lists,sales_price_list_items,customer_followups,customer_followup_events,customer_contact_notes,customer_contact_note_history,commerce_customer_notifications,commerce_customer_notification_settings,sales_notification_execution_authorizations,sales_notification_execution_attempts,sales_notification_reconciliations,command_receipts,audit_events IN SHARE MODE');

    for (const literal of [
      'const reconciledAttempt=',
      'const ecommerceOrderScope=',
      'const orderDeliveryExistsSql=',
      'const commandAuditPrefixSql=',
      'const allRows=',
      'const factKind=',
      'const scopedSource=',
      'const deliveryFactExists=',
      'const commandReceiptGuard=',
      'const attemptGuard=',
    ]) expect(obligations).toContain(literal);

    const finalRoot = read('final-root-owner-projection-sources.ts');
    const byKey = read('by-key.ts');
    const pointRead = read('point-read.ts');
    const orderRead = read('followup-customer-order-read.ts');
    const notes = read('notes.ts');
    const native = read('native-sales-read.ts');
    const rustRegistry = readFileSync(resolve(srcRoot, 'adapters/native_sales_read_sql.rs'), 'utf8');
    const commercial = read('commercial-owner-projection-sources.ts');
    expect(finalRoot).toMatch(/const tableRead\s*=/);
    expect(purchases).toContain('const followupExists=');
    expect(metric).toContain('const crmAssignmentExists=');
    expect(byKey).toContain('export const byKey =');
    expect(pointRead).toContain('export const pointRead =');
    expect(orderRead).toContain('export const followupCustomerOrderRead =');
    // lane 30 retired both customer-note SELECT literals from TypeScript.
    expect(notes).not.toContain('SELECT');
    expect(notes).toContain("from './native-lane-30/client'");
    expect(native).toContain('export const SALES_RD_PRICE_LIST_ACTIVE');
    expect(native).toContain('export const SALES_RD_PRICE_LIST_ITEMS_ACTIVE');
    expect(native).toContain('export const SALES_RD_NOTIFICATION_ATTEMPT');
    expect(native).toContain('export const SALES_RD_NOTIFICATION_RECONCILIATION_INSTALLED');
    expect((rustRegistry.match(/pub const RD_\w+: &str =/g) ?? []).length).toBe(15);
    expect((commercial.match(/SELECT/g) ?? []).length).toBe(0);
  });
});

describe('Sales read-tail r29 retirement check', () => {
  test('retirement:ts-statements stays strictly below the r28 BEFORE=56 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT/g) ?? []).length;
      }
    };
    walk(srcRoot);
    // Three scope-tuple literals collapse into one generator (-2); r30 shares
    // tableRead (-3) and priceListSelect (-1).
    // r33 folds the tail generator into the primary read generator (-1).
    // r35: delivery-presence head shared by order/return/notice guards (-1).
    expect(total).toBe(32);
    expect(total).toBeLessThan(54);
  });
});

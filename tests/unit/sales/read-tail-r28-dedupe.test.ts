import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test, vi } from 'vitest';
import ts from 'typescript';

vi.mock('server-only', () => ({}));

import { assignmentReadSql } from '@erpx/server-identity_access/public/record-source-authority';

const srcRoot = resolve(__dirname, '../../../packages/server-sales/src');
const read = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const source = read('followup-source-sql.ts');
const pricing = read('pricing-purchases.ts');
const purchases = read('followup-purchases.ts');
const metric = read('metric-purchases.ts');

// SHA-256 of each rendered statement at the pre-r28 (BEFORE=58) source. Only
// the authority token, scope tuple, relation and optional guard vary; no
// column, permission check or bind may drift under the shared generator.
const pre = {
  pricing: '172d6ec948d417cbe094a4f2680dff96c5807b1ae3749da9036868635e4da27d',
  followup: '27dd2ceb1b57757c4d4a227733a0442b27f86092e8c46cbd661da3bcea0a74ab',
  metric: '8fdc97d69ebb06f24bba2824dfdb3009c57ee9ec0fc03d16a9f3921d4501e87c',
} as const;

function transpileDecl(file: ts.SourceFile, name: string): string {
  const decls = file.statements
    .filter(ts.isVariableStatement)
    .flatMap(s => [...s.declarationList.declarations])
    .filter(d => d.name.getText(file) === name);
  expect(decls).toHaveLength(1);
  return ts.transpileModule(`const ${decls[0]!.getText(file)};`, {
    compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS },
  }).outputText;
}

const sourceAst = ts.createSourceFile('followup-source-sql.ts', source, ts.ScriptTarget.Latest, true, ts.ScriptKind.TS);
const genCode = transpileDecl(sourceAst, 'authorityScopeRead');
const authorityScopeRead = new Function(`${genCode}\nreturn authorityScopeRead;`)() as (
  authority: string, tuple: string, relation: string, extra?: string,
) => string;
const registry = read('native_sales_read_sql.rs');
function crmAssignmentExists(): string {
  const marker = 'const SQL_CRM_ASSIGNMENT_PLAIN: &str = "';
  const at = registry.indexOf(marker);
  expect(at).toBeGreaterThanOrEqual(0);
  const end = registry.indexOf('";', at + marker.length);
  expect(end).toBeGreaterThan(at);
  return registry.slice(at + marker.length, end).replace(/\\"/g, '"');
}

const actorScopeTuple = '(SELECT ?::text company_id,?::text branch_id,?::text department_id)';
const companyScopeTuple = '(SELECT ?::text company_id,NULL::text branch_id,NULL::text department_id)';
const permission = 'commerce.read';
const sha = (value: string) => createHash('sha256').update(value).digest('hex');

describe('Sales read-tail r28 assignment-authority scope-read family', () => {
  test.each(Object.entries(pre) as [keyof typeof pre, string][])(
    '%s authority read stays byte-identical to the pre-r28 render',
    (kind, expected) => {
      const rendered =
        kind === 'pricing'
          ? authorityScopeRead(assignmentReadSql('pricing_scope', permission), companyScopeTuple, 'pricing_scope')
          : kind === 'followup'
            ? authorityScopeRead(assignmentReadSql('followup_authority', permission), actorScopeTuple, 'followup_authority')
            : authorityScopeRead(assignmentReadSql('sales_metric_scope', permission), actorScopeTuple, 'sales_metric_scope', ` AND ${crmAssignmentExists()}`);
      expect(sha(rendered)).toBe(expected);
    },
  );

  test('one generator literal with three callers and no inlined old reads', () => {
    expect(source.match(/const authorityScopeRead\s*=/g) ?? []).toHaveLength(1);
    expect(pricing).toContain("authorityScopeRead(assignmentReadSql('pricing_scope',permission),");
    expect(purchases).toContain("authorityScopeRead(assignmentReadSql('followup_authority',permission),");
    expect(metric).toContain('authorityScopeRead(assignmentReadSql("sales_metric_scope","commerce.read"),');
    expect(pricing).not.toMatch(/`SELECT \$\{assignmentReadSql\('pricing_scope'/);
    expect(purchases).not.toMatch(/`SELECT \$\{assignmentReadSql\('followup_authority'/);
    expect(metric).not.toMatch(/`SELECT \$\{assignmentReadSql\("sales_metric_scope"/);
    const family = source.slice(source.indexOf('const authorityScopeRead='), source.length);
    expect((family.match(/SELECT/g) ?? []).length).toBe(1);
    expect(family).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });

  test('r1-r27 generators, assignmentReadSql, locks, registry and native reads stay untouched', () => {
    const obligations = read('package-obligations.ts');
    const finalRoot = read('final-root-owner-projection-sources.ts');
    const notes = read('notes.ts');
    const byKey = read('by-key.ts');
    const pointRead = read('point-read.ts');
    const orderRead = read('followup-customer-order-read.ts');
    const native = read('native-sales-read.ts');
    const rustRegistry = readFileSync(resolve(srcRoot, 'adapters/native_sales_read_sql.rs'), 'utf8');
    const commercial = read('commercial-owner-projection-sources.ts');

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

    expect(finalRoot).toMatch(/const tableRead\s*=/); // r15
    expect(source).toContain('export const actorScopeTuple='); // r12/r15
    expect(source).not.toContain('export const clockNowMs=');
    expect(source).not.toContain('export const followupEvidencePrefix=');
    expect(source).not.toContain('export const followupMetricPrefix=');
    expect(purchases).toContain('const followupExists='); // r17
    expect(metric).toContain('SALES_RD_CRM_ASSIGNMENT_PLAIN'); // r11
    expect(byKey).toContain('export const byKey ='); // r19
    expect(pointRead).toContain('export const pointRead ='); // r20
    expect(orderRead).toContain('export const followupCustomerOrderRead ='); // r21
    // lane 30 retired both customer-note SELECT literals from TypeScript.
    expect(notes).not.toContain('SELECT');
    expect(notes).toContain("from './native-lane-30/client'");
    expect(pricing).toContain('assignmentReadSql(');
    expect(purchases).toContain('assignmentReadSql(');
    expect(metric).toContain('assignmentReadSql(');

    expect(purchases).toContain('LOCK TABLE customer_followups,customer_followup_events IN SHARE ROW EXCLUSIVE MODE');
    expect(pricing).toContain('LOCK TABLE sales_price_lists,sales_price_list_items IN SHARE ROW EXCLUSIVE MODE');
    expect(obligations).toContain('LOCK TABLE sales_price_lists,sales_price_list_items,customer_followups,customer_followup_events,customer_contact_notes,customer_contact_note_history,commerce_customer_notifications,commerce_customer_notification_settings,sales_notification_execution_authorizations,sales_notification_execution_attempts,sales_notification_reconciliations,command_receipts,audit_events IN SHARE MODE');

    expect(native).toContain('export const SALES_RD_PRICE_LIST_ACTIVE');
    expect(native).toContain('export const SALES_RD_PRICE_LIST_ITEMS_ACTIVE');
    expect(native).toContain('export const SALES_RD_NOTIFICATION_ATTEMPT');
    expect(native).toContain('export const SALES_RD_NOTIFICATION_RECONCILIATION_INSTALLED');
    expect((rustRegistry.match(/pub const RD_\w+: &str =/g) ?? []).length).toBe(30);
    expect((commercial.match(/SELECT/g) ?? []).length).toBe(0);
  });
});

describe('Sales read-tail r28 retirement check', () => {
  test('retirement:ts-statements stays strictly below the r28 BEFORE=58 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT/g) ?? []).length;
      }
    };
    walk(srcRoot);
    // r28 collapsed three authority-scope literals into one generator (-2);
    // r29 collapsed three scope-tuple literals into one generator (-2).
    // r30 shared tableRead (-3) and priceListSelect (-1).
    // r33 folds the tail generator into the primary read generator (-1).
    // r35: delivery-presence head shared by order/return/notice guards (-1).
    expect(total).toBe(16);
    expect(total).toBeLessThan(58);
  });
});

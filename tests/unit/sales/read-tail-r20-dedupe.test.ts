import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test, vi } from 'vitest';
import ts from 'typescript';

vi.mock('server-only', () => ({}));

const srcRoot = resolve(__dirname, '../../../packages/server-sales/src');
const helper = readFileSync(resolve(srcRoot, 'adapters/point-read.ts'), 'utf8');
const workflowSource = readFileSync(resolve(srcRoot, 'adapters/workflow-notification-source.ts'), 'utf8');
const followups = readFileSync(resolve(srcRoot, 'adapters/followups.ts'), 'utf8');

// Pre-dedupe rendered literals copied from HEAD. The generator must render both
// inlined single-key reads byte-for-byte; no relation, column or order may drift.
const pre = {
  notification: 'SELECT id FROM commerce_customer_notifications WHERE fulfillment_id=?',
  followupHistory: 'SELECT * FROM customer_followup_events WHERE followup_id=? ORDER BY version DESC',
} as const;

function loadGenerator(): (columns: string, table: string, column: string, order?: string) => string {
  const file = ts.createSourceFile('point-read.ts', helper, ts.ScriptTarget.Latest, true, ts.ScriptKind.TS);
  const decls = file.statements
    .filter(ts.isVariableStatement)
    .flatMap(s => [...s.declarationList.declarations])
    .filter(d => d.name.getText(file) === 'pointRead');
  expect(decls).toHaveLength(1);
  const code = ts.transpileModule(`const ${decls[0].getText(file)};`, {
    compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS },
  }).outputText;
  // pointRead now delegates to the owner-local tableRead generator injected here
  // byte-identically, keeping this loader isolated from cross-owner module imports.
  const tableReadStub = 'const tableRead=(c,t)=>`SELECT ${c} FROM ${t}`;';
  return new Function(`${tableReadStub}\n${code}\nreturn pointRead;`)() as (columns: string, table: string, column: string, order?: string) => string;
}

describe('Sales read-tail r20 parameterized point-read family', () => {
  const gen = loadGenerator();

  test.each([
    ['notification', gen('id', 'commerce_customer_notifications', 'fulfillment_id'), pre.notification],
    ['followupHistory', gen('*', 'customer_followup_events', 'followup_id', ' ORDER BY version DESC'), pre.followupHistory],
  ] as ['notification' | 'followupHistory', string, string][])(
    '%s stays byte-identical to the pre-dedupe literal',
    (_name, actual, expected) => {
      expect(actual).toBe(expected);
      expect(createHash('sha256').update(actual).digest('hex')).toBe(
        createHash('sha256').update(expected).digest('hex'),
      );
    },
  );

  test('the family is one generator literal with two callers and no inlining', () => {
    expect(helper.match(/const pointRead\s*=/g) ?? []).toHaveLength(1);
    expect((helper.match(/SELECT/g) ?? []).length).toBe(0);
    expect(workflowSource).toContain("pointRead('id','commerce_customer_notifications','fulfillment_id')");
    expect(followups).toContain("pointRead('*','customer_followup_events','followup_id',' ORDER BY version DESC')");
    expect(workflowSource).not.toContain(`'${pre.notification}'`);
    expect(followups).not.toContain(`'${pre.followupHistory}'`);
    expect(helper).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });

  test('r7-r19 generators, assignmentReadSql, locks and commercial-owner projections stay untouched', () => {
    const obligations = readFileSync(resolve(srcRoot, 'adapters/package-obligations.ts'), 'utf8');
    const finalRoot = readFileSync(resolve(srcRoot, 'adapters/final-root-owner-projection-sources.ts'), 'utf8');
    const followupSource = readFileSync(resolve(srcRoot, 'adapters/followup-source-sql.ts'), 'utf8');
    const purchases = readFileSync(resolve(srcRoot, 'adapters/followup-purchases.ts'), 'utf8');
    const metric = readFileSync(resolve(srcRoot, 'adapters/metric-purchases.ts'), 'utf8');
    const byKey = readFileSync(resolve(srcRoot, 'adapters/by-key.ts'), 'utf8');
    const commercial = readFileSync(resolve(srcRoot, 'adapters/commercial-owner-projection-sources.ts'), 'utf8');
    for (const literal of [
      'const orderDeliveryExistsSql=', // r7
      'const commandAuditPrefixSql=', // r9
      'const allRows=', // r13
      'const factKind=', // r14
      'const scopedSource=', // r16
      'const ecommerceOrderScope=', // r7, collapsed in r27
    ]) expect(obligations).toContain(literal);
    expect(finalRoot).toMatch(/const tableRead\s*=/); // r15
    expect(followupSource).toContain('export const actorScopeTuple='); // r12/r15
    expect(purchases).toContain('const followupExists='); // r17
    expect(metric).toContain('const crmAssignmentExists='); // r11
    expect(byKey).toContain('export const byKey ='); // r19
    // assignmentReadSql consumers remain; no lock or commercial projection edited here
    expect(purchases).toContain('assignmentReadSql(');
    expect(metric).toContain('assignmentReadSql(');
    expect(purchases).toContain('LOCK TABLE customer_followups,customer_followup_events IN SHARE ROW EXCLUSIVE MODE');
    expect(obligations).toContain('LOCK TABLE sales_price_lists,sales_price_list_items,customer_followups,customer_followup_events,customer_contact_notes,customer_contact_note_history,commerce_customer_notifications,commerce_customer_notification_settings,sales_notification_execution_authorizations,sales_notification_execution_attempts,sales_notification_reconciliations,command_receipts,audit_events IN SHARE MODE');
    expect((commercial.match(/SELECT/g) ?? []).length).toBe(0);
  });
});

describe('Sales read-tail r20 retirement check', () => {
  test('retirement:ts-statements stays strictly below the r20 BEFORE=67 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT/g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(67);
  });
});

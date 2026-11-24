import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test, vi } from 'vitest';
import ts from 'typescript';

vi.mock('server-only', () => ({}));

import { pointRead } from '../../../packages/server-sales/src/adapters/point-read';

const srcRoot = resolve(__dirname, '../../../packages/server-sales/src');
const helper = readFileSync(resolve(srcRoot, 'adapters/by-key.ts'), 'utf8');
const notifications = readFileSync(resolve(srcRoot, 'adapters/notifications.ts'), 'utf8');
const pricing = readFileSync(resolve(srcRoot, 'adapters/pricing.ts'), 'utf8');

// Pre-dedupe rendered fragments copied from HEAD. The generator must render the
// two inlined single-key reads byte-for-byte; no relation or column may drift.
const pre = {
  settings: 'SELECT * FROM commerce_customer_notification_settings WHERE company_id=?',
  items: 'SELECT * FROM sales_price_list_items WHERE price_list_id=?',
} as const;

function loadGenerator(): (table: string, column: string) => string {
  const file = ts.createSourceFile('by-key.ts', helper, ts.ScriptTarget.Latest, true, ts.ScriptKind.TS);
  const decls = file.statements
    .filter(ts.isVariableStatement)
    .flatMap(s => [...s.declarationList.declarations])
    .filter(d => d.name.getText(file) === 'byKey');
  expect(decls).toHaveLength(1);
  const code = ts.transpileModule(`const ${decls[0].getText(file)};`, {
    compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS },
  }).outputText;
  return new Function('pointRead', `${code}\nreturn byKey;`)(pointRead) as (table: string, column: string) => string;
}

describe('Sales read-tail r19 byKey single-key read family', () => {
  const gen = loadGenerator();

  test.each([
    ['settings', gen('commerce_customer_notification_settings', 'company_id'), pre.settings],
    ['items', gen('sales_price_list_items', 'price_list_id'), pre.items],
  ] as ['settings' | 'items', string, string][])(
    '%s stays byte-identical to the pre-dedupe literal',
    (_name, actual, expected) => {
      expect(actual).toBe(expected);
      expect(createHash('sha256').update(actual).digest('hex')).toBe(
        createHash('sha256').update(expected).digest('hex'),
      );
    },
  );

  test('the family is one generator literal with two callers and no inlining', () => {
    expect(helper.match(/const byKey\s*=/g) ?? []).toHaveLength(1);
    expect(notifications).toContain("byKey('commerce_customer_notification_settings','company_id')");
    expect(pricing).toContain("byKey('sales_price_list_items','price_list_id')");
    expect(notifications).not.toContain(`'${pre.settings}'`);
    expect(pricing).not.toContain(`'${pre.items}'`);
    expect(helper).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });

  test('r7-r17 generators and source locks stay independent and untouched', () => {
    const obligations = readFileSync(resolve(srcRoot, 'adapters/package-obligations.ts'), 'utf8');
    const finalRoot = readFileSync(resolve(srcRoot, 'adapters/final-root-owner-projection-sources.ts'), 'utf8');
    const followup = readFileSync(resolve(srcRoot, 'adapters/followup-purchases.ts'), 'utf8');
    const source = readFileSync(resolve(srcRoot, 'adapters/followup-source-sql.ts'), 'utf8');
    for (const literal of [
      'const orderDeliveryExistsSql=', // r7
      'const commandAuditPrefixSql=', // r9
      'const allRows=', // r13
      'const factKind=', // r14
      'const scopedSource=', // r16
      'const ecommerceOrderScope=', // r7, collapsed in r27
    ]) expect(obligations).toContain(literal);
    expect(finalRoot).toMatch(/const tableRead\s*=/); // r15
    expect(source).toContain('export const actorScopeTuple='); // r12/r15
    expect(followup).toContain('const followupExists='); // r17
    expect(followup.match(/LOCK TABLE/g) ?? []).toHaveLength(1);
    expect(followup).toContain('LOCK TABLE customer_followups,customer_followup_events IN SHARE ROW EXCLUSIVE MODE');
    expect(obligations).toContain('LOCK TABLE sales_price_lists,sales_price_list_items,customer_followups,customer_followup_events,customer_contact_notes,customer_contact_note_history,commerce_customer_notifications,commerce_customer_notification_settings,sales_notification_execution_authorizations,sales_notification_execution_attempts,sales_notification_reconciliations,command_receipts,audit_events IN SHARE MODE');
  });
});

describe('Sales read-tail r19 retirement check', () => {
  test('retirement:ts-statements stays strictly below the r19 BEFORE=68 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT/g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(68);
  });
});

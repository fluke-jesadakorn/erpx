import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test, vi } from 'vitest';
import ts from 'typescript';

vi.mock('server-only', () => ({}));

const srcRoot = resolve(__dirname, '../../../packages/server-sales/src');
const src = readFileSync(resolve(srcRoot, 'adapters/followup-purchases.ts'), 'utf8');

// Pre-dedupe rendered fragments copied from HEAD. The generator must render the
// three inlined guards byte-for-byte; no predicate, alias, bind or relation may drift.
const pre = {
  native: 'SELECT 1 FROM customer_followups native WHERE native.id=workspace_records.id',
  projection: 'SELECT 1 FROM customer_followups followup_projection WHERE followup_projection.id=e.source_id AND followup_projection.company_id=e.company_id AND PROJECTION_WHERE',
  plain: 'SELECT 1 FROM customer_followups WHERE id=?',
} as const;

function loadGenerator(): (predicate: string, alias?: string) => string {
  const file = ts.createSourceFile('followup-purchases.ts', src, ts.ScriptTarget.Latest, true, ts.ScriptKind.TS);
  const decls = file.statements
    .filter(ts.isVariableStatement)
    .flatMap(s => [...s.declarationList.declarations])
    .filter(d => d.name.getText(file) === 'followupExists');
  expect(decls).toHaveLength(1);
  const code = ts.transpileModule(`const ${decls[0].getText(file)};`, {
    compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS },
  }).outputText;
  return new Function(`${code}\nreturn followupExists;`)() as (predicate: string, alias?: string) => string;
}

describe('Sales read-tail r17 followupExists generator family', () => {
  const gen = loadGenerator();

  test.each([
    ['native', gen('native.id=workspace_records.id', 'native'), pre.native],
    ['projection', gen('followup_projection.id=e.source_id AND followup_projection.company_id=e.company_id AND PROJECTION_WHERE', 'followup_projection'), pre.projection],
    ['plain', gen('id=?'), pre.plain],
  ] as ['native' | 'projection' | 'plain', string, string][])(
    '%s guard stays byte-identical to the pre-dedupe render',
    (_name, actual, expected) => {
      expect(actual).toBe(expected);
      expect(createHash('sha256').update(actual).digest('hex')).toBe(
        createHash('sha256').update(expected).digest('hex'),
      );
    },
  );

  test('the family is one generator literal with three callers and no inlined guard', () => {
    expect(src.match(/const followupExists=/g) ?? []).toHaveLength(1);
    expect(src.match(/followupExists\(/g) ?? []).toHaveLength(3);
    expect(src.match(/SELECT 1 FROM customer_followups/g) ?? []).toHaveLength(1);
    const family = src.slice(src.indexOf('const followupExists='), src.indexOf('const capability='));
    expect(family).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });

  test('r7-r16 generators and source locks stay independent and untouched', () => {
    const obligations = readFileSync(resolve(srcRoot, 'adapters/package-obligations.ts'), 'utf8');
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
    expect(src.match(/LOCK TABLE/g) ?? []).toHaveLength(1);
    expect(src).toContain('LOCK TABLE customer_followups,customer_followup_events IN SHARE ROW EXCLUSIVE MODE');
    expect(obligations).toContain('LOCK TABLE sales_price_lists,sales_price_list_items,customer_followups,customer_followup_events,customer_contact_notes,customer_contact_note_history,commerce_customer_notifications,commerce_customer_notification_settings,sales_notification_execution_authorizations,sales_notification_execution_attempts,sales_notification_reconciliations,command_receipts,audit_events IN SHARE MODE');
  });
});

describe('Sales read-tail r17 retirement check', () => {
  test('retirement:ts-statements stays strictly below the r17 BEFORE=70 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT/g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(70);
  });
});

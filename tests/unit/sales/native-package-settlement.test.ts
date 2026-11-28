import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test, vi } from 'vitest';

vi.mock('server-only', () => ({}));

import {
  SALES_RD_PACKAGE_SETTLEMENT_SALES,
  SALES_RD_PACKAGE_SETTLEMENT_ECOMMERCE,
} from '../../../modules/sales/server/src/adapters/native-sales-read';

const srcRoot = resolve(__dirname, '../../../modules/sales/server/src');
const read = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const registry = read('native_sales_read_sql.rs');
const obligations = read('package-obligations.ts');
const transport = read('native-sales-read.ts');

function rustSql(constant: string): string {
  const marker = `const ${constant}: &str = "`;
  const at = registry.indexOf(marker);
  expect(at).toBeGreaterThanOrEqual(0);
  const end = registry.indexOf('";', at + marker.length);
  expect(end).toBeGreaterThan(at);
  return registry.slice(at + marker.length, end).replace(/\\n/g, '\n').replace(/\\"/g, '"');
}

const families = [
  ['sales', 'SQL_PACKAGE_SETTLEMENT_SALES', SALES_RD_PACKAGE_SETTLEMENT_SALES, 18424, '6e58228f3864bfe8bc0af13a59ec45bb2682081a8c83fef082f610a8a4c9ba5f'],
  ['ecommerce', 'SQL_PACKAGE_SETTLEMENT_ECOMMERCE', SALES_RD_PACKAGE_SETTLEMENT_ECOMMERCE, 26250, '3706e17efee2b1ca4f41237de188b66864314297adc4c11a5d7c5ab3e4a29684'],
] as const;

describe('native Sales package settlement reads', () => {
  test.each(families)('%s statement is byte-identical and allow-listed', (_name, constant, kind, length, hash) => {
    const sql = rustSql(constant);
    expect(sql).toHaveLength(length);
    expect(createHash('sha256').update(sql).digest('hex')).toBe(hash);
    expect((sql.match(/\?/g) ?? []).length).toBe(1);
    expect(sql).toContain('WITH scope AS');
    expect(registry).toContain(`"${kind}"`);
    expect(registry).toContain('bind_count: 1,');
    for (const word of ['INSERT INTO', 'DELETE FROM', 'UPDATE ', 'LOCK TABLE']) expect(sql.toUpperCase()).not.toContain(word);
  });

  test('TS owns only transport and keeps the source-lock guard', () => {
    expect(transport).toContain(`export const SALES_RD_PACKAGE_SETTLEMENT_SALES = '${SALES_RD_PACKAGE_SETTLEMENT_SALES}';`);
    expect(transport).toContain(`export const SALES_RD_PACKAGE_SETTLEMENT_ECOMMERCE = '${SALES_RD_PACKAGE_SETTLEMENT_ECOMMERCE}';`);
    expect(obligations).toContain('const kind=selection===\'ecommerce\'?SALES_RD_PACKAGE_SETTLEMENT_ECOMMERCE:SALES_RD_PACKAGE_SETTLEMENT_SALES;');
    expect(obligations).toContain('nativeSalesReadAll<{kind:string;id:string;fact:unknown;pending:boolean}>(kind,[_companyId],database)');
    expect(obligations).not.toContain('WITH scope AS${scopeTuple(\'scope\')}');
    expect(obligations).toContain('LOCK TABLE sales_price_lists,sales_price_list_items,customer_followups,customer_followup_events,customer_contact_notes,customer_contact_note_history,commerce_customer_notifications,commerce_customer_notification_settings,sales_notification_execution_authorizations,sales_notification_execution_attempts,sales_notification_reconciliations,command_receipts,audit_events IN SHARE MODE');
  });
});

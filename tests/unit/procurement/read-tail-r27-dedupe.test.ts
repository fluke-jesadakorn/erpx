import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { settingsRead } from '../../../modules/procurement/server/src/adapters/read-tail-r27';

const src = (name: string) => readFileSync(
  new URL(`../../../modules/procurement/server/src/adapters/${name}`, import.meta.url), 'utf8',
);

describe('Procurement read-tail r27 parameterized family dedupe', () => {
  test('settingsRead renders the original currency and full settings statements', () => {
    expect(settingsRead('currency', 'SETTINGS')).toBe(
      'SELECT currency FROM (SETTINGS) commerce_settings WHERE company_id=?',
    );
    expect(settingsRead('*', 'SETTINGS')).toBe(
      'SELECT * FROM (SETTINGS) commerce_settings WHERE company_id=?',
    );
  });

  test('both callers use the helper and keep no inline settings fragment', () => {
    expect(src('rfqs.ts')).toContain('const currencyRead=settingsRead("currency",commercePlatformSources.commerce_settings);');
    expect(src('supplier-bills.ts')).toContain('settingsRead("*",commercePlatformSources.commerce_settings)');
    expect(src('rfqs.ts')).toContain('import {settingsRead} from \'./read-tail-r27\';');
    expect(src('supplier-bills.ts')).toContain('import {settingsRead} from \'./read-tail-r27\';');
    expect(src('rfqs.ts')).not.toContain('SELECT currency FROM (${commercePlatformSources.commerce_settings}) commerce_settings WHERE company_id=?`;');
    expect(src('supplier-bills.ts')).not.toContain('SELECT * FROM (${commercePlatformSources.commerce_settings}) commerce_settings WHERE company_id=?`');
  });

  test('no write, lock, command_receipts or r17-r26 helper was touched', () => {
    const tail = src('read-tail-sql.ts');
    expect(tail).toContain('SELECT 1 FROM command_receipts WHERE command_id=?');
    expect(tail).toContain('export const pointRead =');
    expect(tail).toContain('export const orderedRead =');
    expect(src('read-tail-r19.ts')).toContain('export const tableRead =');
    expect(src('read-tail-r20.ts')).toContain('export const expenseAccountBy =');
    expect(src('read-tail-r21.ts')).toContain('export const jsonbAggBy =');
    expect(src('read-tail-r22.ts')).toContain('export const jsonbAggJoinBy =');
    expect(src('read-tail-r23.ts')).toContain('export const billReceiptBinding =');
    expect(src('read-tail-r24.ts')).toContain('export const eligiblePurchaseCount =');
    expect(src('read-tail-r25.ts')).toContain('export const foreignBillReceiptWitness =');
    expect(src('read-tail-r26.ts')).toContain('export const fulfillmentLinesBy =');
    expect(src('purchases.ts')).toContain('FOR UPDATE');
    expect(src('bill-purchases.ts')).toContain('FOR SHARE OF f,o');
    expect(src('attachment-purchases.ts')).toContain('FOR SHARE');
  });

  test('worktree SELECT count is strictly lower than the recorded BEFORE=136', () => {
    const root = new URL('../../../modules/procurement/server/src/', import.meta.url);
    const files: URL[] = [];
    const walk = (dir: URL) => {
      for (const entry of readdirSync(dir, { withFileTypes: true }) as Dirent[]) {
        const p = new URL(entry.name + (entry.isDirectory() ? '/' : ''), dir);
        if (entry.isDirectory()) walk(p);
        else if (entry.name.endsWith('.ts')) files.push(p);
      }
    };
    walk(root);
    const total = files.reduce((n, p) => n + (readFileSync(p, 'utf8').match(/SELECT /g) ?? []).length, 0);
    expect(total).toBeLessThan(136);
  });
});

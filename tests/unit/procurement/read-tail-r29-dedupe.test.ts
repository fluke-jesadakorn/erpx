import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { policyVersionGuard } from '../../../modules/procurement/server/src/adapters/read-tail-r29';

const src = (name: string) => readFileSync(
  new URL(`../../../modules/procurement/server/src/adapters/${name}`, import.meta.url), 'utf8',
);

describe('Procurement read-tail r29 parameterized family dedupe', () => {
  test('policyVersionGuard renders the original absent and versioned statements', () => {
    expect(policyVersionGuard(true)).toBe(
      'NOT EXISTS(SELECT 1 FROM procurement_invoice_policies WHERE company_id=?)',
    );
    expect(policyVersionGuard(false)).toBe(
      'EXISTS(SELECT 1 FROM procurement_invoice_policies WHERE company_id=? AND version=?)',
    );
  });

  test('supplier-bills policy-version guard uses the helper with no inline statements', () => {
    const s = src('supplier-bills.ts');
    expect(s).toContain("import {policyVersionGuard} from './read-tail-r29';");
    expect(s).toContain("assertion(write,key,'policy-version',policyVersionGuard(input.expectedVersion===0),input.expectedVersion===0?[actor.companyId]:[actor.companyId,input.expectedVersion])");
    expect(s).not.toContain("'NOT EXISTS(SELECT 1 FROM procurement_invoice_policies WHERE company_id=?)'");
    expect(s).not.toContain("'EXISTS(SELECT 1 FROM procurement_invoice_policies WHERE company_id=? AND version=?)'");
  });

  test('no write, lock, command_receipts or r17-r28 helper was touched', () => {
    expect(src('read-tail-sql.ts')).toContain('SELECT 1 FROM command_receipts WHERE command_id=?');
    expect(src('read-tail-r19.ts')).toContain('export const tableRead =');
    expect(src('read-tail-r20.ts')).toContain('export const expenseAccountBy =');
    expect(src('read-tail-r21.ts')).toContain('export const jsonbAggBy =');
    expect(src('read-tail-r22.ts')).toContain('export const jsonbAggJoinBy =');
    expect(src('read-tail-r23.ts')).toContain('export const billReceiptBinding =');
    expect(src('read-tail-r24.ts')).toContain('export const eligiblePurchaseCount =');
    expect(src('read-tail-r25.ts')).toContain('export const foreignBillReceiptWitness =');
    expect(src('read-tail-r26.ts')).toContain('export const fulfillmentLinesBy =');
    expect(src('read-tail-r27.ts')).toContain('export const settingsRead =');
    expect(src('read-tail-r28.ts')).toContain('export const attachmentVersionWitness =');
    expect(src('purchases.ts')).toContain('FOR UPDATE');
    expect(src('bill-purchases.ts')).toContain('FOR SHARE OF f,o');
    expect(src('attachment-purchases.ts')).toContain('FOR SHARE');
  });

  test('worktree SELECT count is strictly lower than the recorded BEFORE=132', () => {
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
    expect(total).toBeLessThan(132);
  });
});

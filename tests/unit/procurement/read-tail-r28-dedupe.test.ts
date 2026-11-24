import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { attachmentVersionWitness } from '../../../packages/server-procurement/src/adapters/read-tail-r28';

const src = (name: string) => readFileSync(
  new URL(`../../../packages/server-procurement/src/adapters/${name}`, import.meta.url), 'utf8',
);

describe('Procurement read-tail r28 parameterized family dedupe', () => {
  test('attachmentVersionWitness renders the original RFQ and supplier-bill statements', () => {
    expect(attachmentVersionWitness('STEPS', 'r', 'procurement_rfq')).toBe(
      '  AND EXISTS(WITH RECURSIVE versions(version) AS(SELECT event.version UNION SELECT binding.next_version FROM versions\n' +
      '   JOIN (STEPS) binding ON binding.previous_version=versions.version AND binding.company_id=r.company_id\n' +
      "   AND binding.target_type='procurement_rfq' AND binding.target_id=r.id WHERE binding.next_version<=r.version)\n" +
      '   SELECT 1 FROM versions WHERE version=r.version)',
    );
    expect(attachmentVersionWitness('STEPS', 'bill', 'supplier_bill_match')).toBe(
      '  AND EXISTS(WITH RECURSIVE versions(version) AS(SELECT event.version UNION SELECT binding.next_version FROM versions\n' +
      '   JOIN (STEPS) binding ON binding.previous_version=versions.version AND binding.company_id=bill.company_id\n' +
      "   AND binding.target_type='supplier_bill_match' AND binding.target_id=bill.id WHERE binding.next_version<=bill.version)\n" +
      '   SELECT 1 FROM versions WHERE version=bill.version)',
    );
  });

  test('both obligation branches use the helper and keep no inline CTE fragment', () => {
    const s = src('package-obligations.ts');
    expect(s).toContain('${attachmentVersionWitness(recordAttachmentVersionSteps,"r","procurement_rfq")}');
    expect(s).toContain('${attachmentVersionWitness(recordAttachmentVersionSteps,"bill","supplier_bill_match")}');
    expect(s).toContain("import {attachmentVersionWitness} from './read-tail-r28';");
    expect(s).not.toContain("AND binding.target_type='procurement_rfq' AND binding.target_id=r.id");
    expect(s).not.toContain("AND binding.target_type='supplier_bill_match' AND binding.target_id=bill.id");
  });

  test('no write, lock, command_receipts or r17-r27 helper was touched', () => {
    expect(src('read-tail-sql.ts')).toContain('SELECT 1 FROM command_receipts WHERE command_id=?');
    expect(src('read-tail-sql.ts')).toContain('export const pointRead =');
    expect(src('read-tail-r19.ts')).toContain('export const tableRead =');
    expect(src('read-tail-r20.ts')).toContain('export const expenseAccountBy =');
    expect(src('read-tail-r21.ts')).toContain('export const jsonbAggBy =');
    expect(src('read-tail-r22.ts')).toContain('export const jsonbAggJoinBy =');
    expect(src('read-tail-r23.ts')).toContain('export const billReceiptBinding =');
    expect(src('read-tail-r24.ts')).toContain('export const eligiblePurchaseCount =');
    expect(src('read-tail-r25.ts')).toContain('export const foreignBillReceiptWitness =');
    expect(src('read-tail-r26.ts')).toContain('export const fulfillmentLinesBy =');
    expect(src('read-tail-r27.ts')).toContain('export const settingsRead =');
    expect(src('purchases.ts')).toContain('FOR UPDATE');
    expect(src('bill-purchases.ts')).toContain('FOR SHARE OF f,o');
    expect(src('attachment-purchases.ts')).toContain('FOR SHARE');
  });

  test('worktree SELECT count is strictly lower than the recorded BEFORE=135', () => {
    const root = new URL('../../../packages/server-procurement/src/', import.meta.url);
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
    expect(total).toBeLessThan(135);
  });
});

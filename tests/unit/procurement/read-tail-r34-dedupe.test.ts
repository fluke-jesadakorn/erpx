import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import {
  calendarScopedListRead, approvedRequestExists, supplierBillPointGuard, rfqScopedRead,
} from '../../../packages/server-procurement/src/adapters/read-tail-r34';

const src = (name: string) => readFileSync(
  new URL(`../../../packages/server-procurement/src/adapters/${name}`, import.meta.url), 'utf8',
);

describe('Procurement read-tail r34 parameterized family dedupe', () => {
  test('calendarScopedListRead renders the three scoped list reads byte-identically', () => {
    expect(calendarScopedListRead('r.*', 'procurement_rfqs r', 'BR', 'LIST', 'RFQ_PRED', 'SW', 'r.needed_date,r.id'))
      .toBe('SELECT r.* FROM procurement_rfqs r WHERE BR AND LIST AND RFQ_PRED AND SW ORDER BY r.needed_date,r.id LIMIT ?');
    expect(calendarScopedListRead('r.*', 'procurement_bill_matches r', 'BR', 'LIST', 'BILL_PRED', 'SW', 'r.due_date,r.id'))
      .toBe('SELECT r.* FROM procurement_bill_matches r WHERE BR AND LIST AND BILL_PRED AND SW ORDER BY r.due_date,r.id LIMIT ?');
    expect(calendarScopedListRead('COLS', 'FROM', 'BR', 'LIST', 'PROMISE_PRED', 'SW', 'p.id'))
      .toBe('SELECT COLS FROM FROM WHERE BR AND LIST AND PROMISE_PRED AND SW ORDER BY p.id LIMIT ?');
  });

  test('approvedRequestExists renders the three approved-source guards byte-identically', () => {
    expect(approvedRequestExists('(SRC) e JOIN L', 'e.', 'e.id=? AND e.company_id=?', '?', '?'))
      .toBe("EXISTS(SELECT 1 FROM (SRC) e JOIN L WHERE e.id=? AND e.company_id=? AND e.evidence_version=? AND e.items_json=? AND e.approval_json::jsonb->>'decision'='approved')");
    expect(approvedRequestExists('(SRC) source', 'source.', "source.id=r.request_id AND source.state='in_progress'", 'r.ev', 'r.ij'))
      .toBe("EXISTS(SELECT 1 FROM (SRC) source WHERE source.id=r.request_id AND source.state='in_progress' AND source.evidence_version=r.ev AND source.items_json=r.ij AND source.approval_json::jsonb->>'decision'='approved')");
    expect(approvedRequestExists('(SRC) employee_requests', '', "id=? AND kind='purchase'", '?', '?'))
      .toBe("EXISTS(SELECT 1 FROM (SRC) employee_requests WHERE id=? AND kind='purchase' AND evidence_version=? AND items_json=? AND approval_json::jsonb->>'decision'='approved')");
  });

  test('supplierBillPointGuard renders both point guards byte-identically', () => {
    expect(supplierBillPointGuard(" AND company_id=? AND fulfillment_id=? AND status='approved' AND version=? AND invoice_id IS NULL"))
      .toBe("EXISTS(SELECT 1 FROM procurement_bill_matches WHERE id=? AND company_id=? AND fulfillment_id=? AND status='approved' AND version=? AND invoice_id IS NULL)");
    expect(supplierBillPointGuard(" AND version=? AND status='invoiced' AND invoice_id=?"))
      .toBe("EXISTS(SELECT 1 FROM procurement_bill_matches WHERE id=? AND version=? AND status='invoiced' AND invoice_id=?)");
  });

  test('rfqScopedRead renders both RFQ master reads byte-identically', () => {
    expect(rfqScopedRead('r.id=? AND SCOPE')).toBe('SELECT r.* FROM procurement_rfqs r WHERE r.id=? AND SCOPE');
    expect(rfqScopedRead('SCOPE ORDER BY r.updated_at DESC LIMIT 100'))
      .toBe('SELECT r.* FROM procurement_rfqs r WHERE SCOPE ORDER BY r.updated_at DESC LIMIT 100');
  });

  test('all ten call sites use the helpers and keep no inline statements', () => {
    expect(src('calendar-procurement-rfqs-facts.ts')).toContain('calendarScopedListRead(');
    expect(src('calendar-supplier-bills-facts.ts')).toContain('calendarScopedListRead(');
    expect(src('calendar-supplier-promises-facts.ts')).toContain('calendarScopedListRead(');
    expect(src('rfqs.ts')).toContain('rfqScopedRead(');
    expect(src('rfqs.ts')).toContain('approvedRequestExists(');
    expect(src('supplier-bills.ts')).toContain('supplierBillPointGuard(');
    expect(src('calendar-procurement-rfqs-facts.ts')).not.toContain('SELECT r.* FROM procurement_rfqs r WHERE ${branchDepartmentScope');
    expect(src('supplier-bills.ts')).not.toContain("EXISTS(SELECT 1 FROM procurement_bill_matches WHERE id=? AND company_id=?");
    const helper = src('read-tail-r34.ts');
    for (const name of ['calendarScopedListRead', 'approvedRequestExists', 'supplierBillPointGuard', 'rfqScopedRead'])
      expect(helper).toContain(`export const ${name} =`);
  });

  test('no write, lock, command_receipts or r17-r33 helper was touched', () => {
    expect(src('read-tail-sql.ts')).toContain('SELECT 1 FROM command_receipts WHERE command_id=?');
    expect(src('read-tail-sql.ts')).toContain('export const pointRead =');
    expect(src('read-tail-sql.ts')).toContain('export const orderedRead =');
    for (const [n, e] of [
      ['r19', 'tableRead'], ['r20', 'receiptCount'], ['r21', 'jsonbAggBy'], ['r22', 'jsonbAggJoinBy'],
      ['r23', 'billReceiptBinding'], ['r24', 'eligiblePurchaseCount'], ['r25', 'foreignBillReceiptWitness'],
      ['r26', 'fulfillmentLinesBy'], ['r27', 'settingsRead'], ['r28', 'attachmentVersionWitness'],
      ['r29', 'policyVersionGuard'], ['r30', 'orderScopedAggregate'], ['r31', 'sourceImage'],
      ['r32', 'progressOrderAggregate'], ['r33', 'quotedProjection'],
    ] as const) expect(src(`read-tail-${n}.ts`)).toContain(`export const ${e} =`);
    expect(src('purchases.ts')).toContain('FOR UPDATE');
    expect(src('bill-purchases.ts')).toContain('FOR SHARE OF f,o');
    expect(src('attachment-purchases.ts')).toContain('FOR SHARE');
  });

  test('worktree SELECT count is strictly lower than the recorded BEFORE=120', () => {
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
    expect(total).toBeLessThan(120);
  });
});

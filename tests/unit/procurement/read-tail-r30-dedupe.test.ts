import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { assignmentReadSql } from '@erpx/server-identity_access/public/record-source-authority';
import { workforceActorScopeColumns } from '../../../packages/server-procurement/src/adapters/read-tail-sql';
import { requestOrderLink } from '../../../packages/server-procurement/src/adapters/read-tail-r19';
import {
  assignmentScopeProbe,
  versionStatusScopeGuard,
  activeMasterLookup,
  orderScopedAggregate,
} from '../../../packages/server-procurement/src/adapters/read-tail-r30';

const src = (name: string) => readFileSync(
  new URL(`../../../packages/server-procurement/src/adapters/${name}`, import.meta.url), 'utf8',
);

describe('Procurement read-tail r30 parameterized family dedupe', () => {
  test('assignmentScopeProbe renders both original IAM assignment-scope probes byte-identically', () => {
    expect(assignmentScopeProbe('procurement_scope', 'permission')).toBe(
      `SELECT ${assignmentReadSql('procurement_scope', 'permission')} allowed FROM ${workforceActorScopeColumns} procurement_scope`,
    );
    expect(assignmentScopeProbe('bill_scope', 'permission')).toBe(
      `SELECT ${assignmentReadSql('bill_scope', 'permission')} allowed FROM ${workforceActorScopeColumns} bill_scope`,
    );
  });

  test('versionStatusScopeGuard renders both original version/status scoped guards byte-identically', () => {
    expect(versionStatusScopeGuard('procurement_rfqs', 'r', 'SCOPE')).toBe(
      'EXISTS(SELECT 1 FROM procurement_rfqs r WHERE r.id=? AND r.version=? AND r.status=? AND SCOPE)',
    );
    expect(versionStatusScopeGuard('procurement_bill_matches', 'm', 'SCOPE')).toBe(
      'EXISTS(SELECT 1 FROM procurement_bill_matches m WHERE m.id=? AND m.version=? AND m.status=? AND SCOPE)',
    );
  });

  test('activeMasterLookup renders the three original RFQ workspace master reads byte-identically', () => {
    expect(activeMasterLookup('id,sku,name,unit', 'P', 'commerce_products', 'sku')).toBe(
      "SELECT id,sku,name,unit FROM (P) commerce_products WHERE company_id=? AND status='active' ORDER BY sku",
    );
    expect(activeMasterLookup('id,name,branch_id', 'W', 'commerce_warehouses', 'name', '', ' AND (?::text IS NULL OR branch_id=?)')).toBe(
      "SELECT id,name,branch_id FROM (W) commerce_warehouses WHERE company_id=? AND status='active' AND (?::text IS NULL OR branch_id=?) ORDER BY name",
    );
    expect(activeMasterLookup('id,name', 'S', 'finance_parties', 'name', " AND kind='supplier'")).toBe(
      "SELECT id,name FROM (S) finance_parties WHERE company_id=? AND kind='supplier' AND status='active' ORDER BY name",
    );
  });

  test('orderScopedAggregate renders the three original request-progress order aggregates byte-identically', () => {
    expect(orderScopedAggregate('DISTINCT o.id,o.number,o.state,o.created_at', '(O) o', requestOrderLink('l', 'o'), 'o', " AND o.kind='purchase'", ' ORDER BY o.created_at DESC,o.id DESC')).toBe(
      'SELECT DISTINCT o.id,o.number,o.state,o.created_at FROM (O) o WHERE o.company_id=? AND o.kind=\'purchase\' AND (o.id=? OR EXISTS(SELECT 1 FROM procurement_request_orders l WHERE l.request_id=? AND l.order_id=o.id)) ORDER BY o.created_at DESC,o.id DESC',
    );
    expect(orderScopedAggregate('COALESCE(SUM(l.quantity_milli),0) ordered_milli,COALESCE(SUM(l.fulfilled_milli),0) received_milli', '(L) l JOIN (O) o ON o.id=l.order_id', requestOrderLink('link', 'o'), 'o')).toBe(
      'SELECT COALESCE(SUM(l.quantity_milli),0) ordered_milli,COALESCE(SUM(l.fulfilled_milli),0) received_milli FROM (L) l JOIN (O) o ON o.id=l.order_id WHERE o.company_id=? AND (o.id=? OR EXISTS(SELECT 1 FROM procurement_request_orders link WHERE link.request_id=? AND link.order_id=o.id))',
    );
    expect(orderScopedAggregate('COUNT(*) receipt_count', '(F) f JOIN (O) o ON o.id=f.order_id', requestOrderLink('link', 'o'), 'o')).toBe(
      'SELECT COUNT(*) receipt_count FROM (F) f JOIN (O) o ON o.id=f.order_id WHERE o.company_id=? AND (o.id=? OR EXISTS(SELECT 1 FROM procurement_request_orders link WHERE link.request_id=? AND link.order_id=o.id))',
    );
  });

  test('callers use the helpers and keep no inline family statements', () => {
    expect(src('purchases.ts')).toContain('assignmentScopeProbe("procurement_scope",permission)');
    expect(src('bill-purchases.ts')).toContain('assignmentScopeProbe("bill_scope",permission)');
    expect(src('purchases.ts')).not.toContain('SELECT ${assignmentReadSql(');
    expect(src('bill-purchases.ts')).not.toContain('SELECT ${assignmentReadSql(');
    expect(src('rfqs.ts')).toContain('versionStatusScopeGuard("procurement_rfqs","r",scope)');
    expect(src('supplier-bills.ts')).toContain('versionStatusScopeGuard("procurement_bill_matches","m",scope)');
    expect(src('rfqs.ts')).not.toContain('EXISTS(SELECT 1 FROM procurement_rfqs r WHERE r.id=? AND r.version=?');
    expect(src('supplier-bills.ts')).not.toContain('EXISTS(SELECT 1 FROM procurement_bill_matches m WHERE m.id=? AND m.version=?');
    expect(src('rfqs.ts')).not.toContain("SELECT id,sku,name,unit FROM (${commercePlatformSources.commerce_products})");
    expect(src('rfqs.ts')).not.toContain("SELECT id,name FROM (${financePlatformSources.finance_parties})");
    expect(src('purchase-request-progress.ts')).not.toContain('SELECT DISTINCT o.id,o.number,o.state,o.created_at FROM (${commercePlatformSources.commerce_orders})');
    expect(src('purchase-request-progress.ts')).not.toContain('SELECT COUNT(*) receipt_count FROM (${commercePlatformSources.commerce_fulfillments})');
  });

  test('no write, lock, command_receipts or r17-r29 helper was touched', () => {
    expect(src('read-tail-sql.ts')).toContain('SELECT 1 FROM command_receipts WHERE command_id=?');
    expect(src('read-tail-sql.ts')).toContain('export const pointRead =');
    expect(src('read-tail-sql.ts')).toContain('export const orderedRead =');
    expect(src('read-tail-r19.ts')).toContain('export const tableRead =');
    expect(src('read-tail-r20.ts')).toContain('export const receiptCount =');
    expect(src('read-tail-r21.ts')).toContain('export const jsonbAggBy =');
    expect(src('read-tail-r22.ts')).toContain('export const jsonbAggJoinBy =');
    expect(src('read-tail-r23.ts')).toContain('export const billReceiptBinding =');
    expect(src('read-tail-r24.ts')).toContain('export const eligiblePurchaseCount =');
    expect(src('read-tail-r25.ts')).toContain('export const foreignBillReceiptWitness =');
    expect(src('read-tail-r26.ts')).toContain('export const fulfillmentLinesBy =');
    expect(src('read-tail-r27.ts')).toContain('export const settingsRead =');
    expect(src('read-tail-r28.ts')).toContain('export const attachmentVersionWitness =');
    expect(src('read-tail-r29.ts')).toContain('export const policyVersionGuard =');
    expect(src('purchases.ts')).toContain('FOR UPDATE');
    expect(src('bill-purchases.ts')).toContain('FOR SHARE OF f,o');
    expect(src('attachment-purchases.ts')).toContain('FOR SHARE');
  });

  test('worktree SELECT count is strictly lower than the recorded BEFORE=131', () => {
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
    expect(total).toBeLessThan(131);
  });
});

/**
 * FIXTURE-ONLY ORACLE — retained pre-cutover customer-return Quality hold.
 *
 * Production now obtains these hold statements from the native Quality owner
 * (`server-quality/adapters/customer_return_hold_statement.rs`) through the
 * native HTTP port. This copy exists only so the commerce-fulfillment fixture
 * can seed equivalent rows without a native service.
 *
 * Tracked for removal by `retire.quality.residual-writers` (removalTaskId
 * `migrate.commerce.return-approval-native`): delete once the fixture seeds the
 * hold through the native owner directly.
 */
import type { SqlDatabase, SqlStatement } from '@erpx/platform-runtime-server/public/runtime/types';

export function fixtureCustomerReturnHold(
  db: SqlDatabase,
  input: {
    id: string; key: string; branchId: string; warehouseId: string; productId: string;
    lotId?: string | null; quantityMilli: number; reason: string; dueDate: string; sourceId: string;
  },
  companyId: string,
  userId: string,
  now: number,
): SqlStatement[] {
  const hold = db.prepare(
    "INSERT INTO inventory_quality_holds(id,company_id,branch_id,warehouse_id,product_id,reason,due_date,created_by,created_at,updated_at,lot_id,held_quantity_milli,source_type,source_id,spec_id,spec_version,quantity_scope) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,'partial')"
  ).bind(
    input.id, companyId, input.branchId, input.warehouseId, input.productId,
    input.reason, input.dueDate, userId, now, now, input.lotId ?? null,
    input.quantityMilli, 'customer_return', input.sourceId, null, null,
  );
  return [hold];
}

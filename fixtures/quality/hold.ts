/** Declared isolated Quality hold source for tests and local simulations.
 *
 * Manual hold creation is owned by the native `quality.hold.create` command.
 * This fixture writes the same source row that native preparation commits —
 * current stock branch, whole held quantity, active specification snapshot and
 * the eligible-recipient handoff read by the retained Work/Inbox surfaces —
 * inside one serializable transaction under the frozen stock-then-Quality lock
 * order. It never mints a command receipt.
 */
import {createHash} from 'node:crypto';
import {isTransactionalSqlDatabase,type SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import {prepareQualityNotification} from '@erpx/server-collaboration/public/quality-notifications';

export type QualityHoldSeed = {
  warehouseId:string;
  productId:string;
  lotId?:string;
  expectedStockVersion:number;
  expectedLotVersion?:number;
  dueDate:string;
  reason:string;
  createdAt?:number;
};

export type QualityHoldSeedReceipt = {id:string;status:'held';version:1};

export type QualityReleaseSeed = {
  companyId: string;
  holdId: string;
  releasedBy: string;
  inspectorId: string;
  reason: string;
  releasedAt?: number;
};

export type QualityReleaseSeedReceipt = {id: string; status: 'released'; version: number};

/** Declared isolated Quality release source for cross-owner oracles.
 *
 * Independent release is owned by the native `quality.hold.release` command.
 * This fixture writes the same hold source row that command commits — a passing
 * inspection snapshot, the independent inspector, the releasing manager, the
 * released status/version and the release receipt witness — without running the
 * command or its other audit/outbox effects. Owner suites that only need the
 * quarantine cleared use this instead of re-testing Quality.
 */
export async function seedNativeQualityRelease(
  db: SqlDatabase,
  input: QualityReleaseSeed,
): Promise<QualityReleaseSeedReceipt> {
  if (!isTransactionalSqlDatabase(db))
    throw new Error('NATIVE_QUALITY_RELEASE_SEED_REQUIRES_TRANSACTION');
  const now = input.releasedAt ?? Date.now();
  return db.transaction(async tx => {
    const hold = await tx.prepare(
      "SELECT id,version,spec_id,spec_version FROM inventory_quality_holds \
       WHERE id=? AND company_id=? AND status='held' FOR UPDATE",
    ).bind(input.holdId, input.companyId).first<{id: string; version: number; spec_id: string | null; spec_version: number | null}>();
    if (!hold) throw new Error('NATIVE_QUALITY_RELEASE_HOLD_NOT_FOUND');
    const version = Number(hold.version);
    await tx.prepare(
      'INSERT INTO inventory_quality_inspections(id,hold_id,reviewed_version,result,evidence,inspected_by,created_at,spec_id,spec_version) VALUES(?,?,?,?,?,?,?,?,?)',
    ).bind(`inspection-${input.holdId}`, input.holdId, version, 'pass', input.reason, input.inspectorId, now, hold.spec_id, hold.spec_version).run();
    await tx.prepare(
      "UPDATE inventory_quality_holds SET status='released',last_result='pass',last_inspector=?,released_by=?,release_reason=?,released_at=?,version=version+1,updated_at=? WHERE id=?",
    ).bind(input.inspectorId, input.releasedBy, input.reason, now, now, input.holdId).run();
    // Package-obligation reads require the independent release receipt that the
    // native command commits. Seed the same receipt identity, actor, version and
    // company witness instead of leaving the hold looking unreleased.
    await tx.prepare(
      'INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) VALUES(?,?,?,?,?,?,?,?) ON CONFLICT(command_id) DO NOTHING',
    ).bind(
      `quality-release-${input.holdId}`,
      'quality.hold.release',
      input.releasedBy,
      createHash('sha256').update(`quality.hold.release\0${input.holdId}\0${version}\0${input.releasedBy}`).digest('hex'),
      input.holdId,
      JSON.stringify({id: input.holdId, status: 'released', version: version + 1}),
      now,
      JSON.stringify({companyId: input.companyId, branchId: null, departmentId: null}),
    ).run();
    return {id: input.holdId, status: 'released' as const, version: version + 1};
  });
}

export async function seedNativeQualityHold(
  db:SqlDatabase,
  actor:WorkContext,
  input:QualityHoldSeed,
  key:string,
):Promise<QualityHoldSeedReceipt>{
  if (!isTransactionalSqlDatabase(db))
    throw new Error('NATIVE_QUALITY_HOLD_SEED_REQUIRES_TRANSACTION');
  const id=`quality-${key}`,createdAt=input.createdAt??Date.now();
  return db.transaction(async tx=>{
    const stock=await tx.prepare(
      "SELECT s.quantity_milli,s.version,w.branch_id FROM commerce_stock s \
       JOIN commerce_warehouses w ON w.id=s.warehouse_id \
       JOIN commerce_products p ON p.id=s.product_id \
       WHERE s.warehouse_id=? AND s.product_id=? AND w.company_id=? \
         AND p.company_id=w.company_id AND w.status='active' AND p.status='active' \
       FOR UPDATE OF s",
    ).bind(input.warehouseId,input.productId,actor.companyId).first<{quantity_milli:number;version:number;branch_id:string}>();
    if(!stock)throw new Error('NATIVE_QUALITY_HOLD_STOCK_NOT_FOUND');
    if(Number(stock.version)!==input.expectedStockVersion)throw new Error('NATIVE_QUALITY_HOLD_STOCK_CHANGED');
    const lot=input.lotId?await tx.prepare(
      'SELECT l.id,b.quantity_milli,b.version FROM inventory_lots l \
       JOIN inventory_lot_balances b ON b.lot_id=l.id \
       WHERE l.id=? AND l.company_id=? AND l.product_id=? AND b.warehouse_id=? AND b.quantity_milli>0',
    ).bind(input.lotId,actor.companyId,input.productId,input.warehouseId).first<{id:string;quantity_milli:number;version:number}>():null;
    if(input.lotId&&(!lot||Number(lot.version)!==input.expectedLotVersion))
      throw new Error('NATIVE_QUALITY_HOLD_LOT_CHANGED');
    const spec=await tx.prepare(
      "SELECT id,version FROM inventory_quality_specs \
       WHERE company_id=? AND product_id=? AND status='active' \
       ORDER BY version DESC,id LIMIT 2",
    ).bind(actor.companyId,input.productId).all<{id:string;version:number}>();
    if(spec.results.length>1)throw new Error('NATIVE_QUALITY_HOLD_ACTIVE_SPEC_INVALID');
    const active=spec.results[0]??null,heldQuantity=Number(lot?.quantity_milli??stock.quantity_milli);
    await tx.prepare(
      'INSERT INTO inventory_quality_holds(id,company_id,branch_id,warehouse_id,product_id,reason,due_date,created_by,created_at,updated_at,lot_id,held_quantity_milli,spec_id,spec_version) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?)',
    ).bind(id,actor.companyId,stock.branch_id,input.warehouseId,input.productId,input.reason,input.dueDate,actor.userId,createdAt,createdAt,input.lotId??null,heldQuantity,active?.id??null,active?.version??null).run();
    // Downstream Work/Inbox assertions read the declared handoff row. The
    // native hold-create composition stages only the source row today, so the
    // fixture supplies the handoff explicitly rather than implying it.
    await prepareQualityNotification(tx,key,id).run();
    return {id,status:'held' as const,version:1 as const};
  });
}

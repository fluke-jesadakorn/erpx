import "server-only";
import { createHash } from "node:crypto";
import type { PostgresDatabase } from "@erpx/server-runtime/public/runtime/postgres";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { assertPosPackageObligationSources } from "@erpx/server-pos/adapters/package-obligations";
import { assertPmsPackageObligationSources, pmsPackageSettlementSources as hotel } from "@erpx/server-pms/public/package-obligations";

type Input = { companyId: string; packageId: string; packageVersion: number; moduleId: string; now: number };
/** Stored service mode survives a later register edit. Recipe lineage and the
 * current register also retain the same addon scope used by ordinary commands. */
const restaurantSales = `SELECT 'pos-restaurant'::text package_id,s.* FROM pos_sales s JOIN pos_registers r ON r.id=s.register_id AND r.company_id=s.company_id
  WHERE s.service_mode IN ('dine_in','takeaway') OR r.mode='restaurant' OR EXISTS(SELECT 1 FROM pos_lines l JOIN pos_fulfillments f ON f.id=l.fulfillment_id AND f.company_id=s.company_id WHERE l.sale_id=s.id AND f.kind='recipe')`;
const hotelSales = `SELECT 'pos-hotel-charge'::text package_id,s.* FROM pos_sales s WHERE EXISTS(
    SELECT 1 FROM (${hotel.roomCharges}) source JOIN (${hotel.properties}) p ON p.id=source.property_id AND p.company_id=source.company_id
    WHERE source.company_id=s.company_id AND p.branch_id=s.branch_id AND source.source_key='pos:'||s.id||':'||s.id)
    OR EXISTS(SELECT 1 FROM pos_room_charge_reversal_requests request WHERE request.company_id=s.company_id AND request.sale_id=s.id)`;
function sourceSql(packageId:string) {
  if(!['pos-restaurant','pos-hotel-charge'].includes(packageId))throw new Error('POS_ADDON_PACKAGE_OBLIGATION_SCOPE_REQUIRED');
  const sales=packageId==='pos-restaurant'?restaurantSales:hotelSales;
// Keep one company_id from the native fulfillment; consumers need no owner rows.
const stockFulfillments = `SELECT DISTINCT s.package_id,f.company_id,f.id,f.version,f.state,f.kind,f.cost_minor,f.quantity_milli,f.currency,f.accounts_json FROM (${sales}) s JOIN pos_lines l ON l.sale_id=s.id JOIN pos_fulfillments f ON f.id=l.fulfillment_id AND f.company_id=s.company_id`;
const shifts = `SELECT DISTINCT 'pos-restaurant'::text package_id,s.* FROM pos_shifts s JOIN pos_registers r ON r.id=s.register_id AND r.company_id=s.company_id
  WHERE r.mode='restaurant' OR EXISTS(SELECT 1 FROM (${sales}) sale WHERE sale.package_id='pos-restaurant' AND sale.company_id=s.company_id AND sale.shift_id=s.id)`;
const payments = `SELECT s.package_id,p.* FROM (${sales}) s JOIN pos_payments p ON p.sale_id=s.id AND p.company_id=s.company_id`;
const refunds = `SELECT s.package_id,r.* FROM (${sales}) s JOIN pos_refunds r ON r.sale_id=s.id AND r.company_id=s.company_id`;
const refundPayments = `SELECT r.package_id,r.company_id,r.sale_id,p.* FROM (${refunds}) r JOIN pos_refund_payments p ON p.refund_id=r.id`;
const events = `SELECT DISTINCT p.package_id,e.company_id,e.id,e.applied_at FROM pos_stripe_events e JOIN (${payments}) p
    ON p.company_id=e.company_id AND p.stripe_account=e.account_id AND p.livemode=e.livemode
    WHERE e.payload_json::jsonb#>>'{metadata,pos_payment_id}'=p.id
  UNION SELECT DISTINCT rp.package_id,e.company_id,e.id,e.applied_at FROM pos_stripe_events e JOIN (${refundPayments}) rp ON rp.company_id=e.company_id
    JOIN pos_payments p ON p.id=rp.payment_id AND p.company_id=rp.company_id AND p.stripe_account=e.account_id AND p.livemode=e.livemode
    WHERE e.payload_json::jsonb#>>'{metadata,pos_refund_payment_id}'=rp.id`;
const postingKeys = `SELECT s.package_id,s.company_id,'sale:'||s.id source_key,s.id source_id,s.currency,s.accounts_json FROM (${sales}) s
  UNION SELECT s.package_id,s.company_id,'allocate:'||s.id,s.id,s.currency,s.accounts_json FROM (${sales}) s
  UNION SELECT s.package_id,s.company_id,'room-reversal:'||s.id,s.id,s.currency,s.accounts_json FROM (${sales}) s
  UNION SELECT p.package_id,p.company_id,'receipt:'||p.id,s.id,s.currency,s.accounts_json FROM (${payments}) p JOIN (${sales}) s ON s.id=p.sale_id AND s.package_id=p.package_id AND s.company_id=p.company_id
  UNION SELECT f.package_id,f.company_id,prefix||f.id,f.id,f.currency,f.accounts_json FROM (${stockFulfillments}) f CROSS JOIN (VALUES ('consume:'),('deliver:'),('waste:')) source(prefix)
  UNION SELECT r.package_id,r.company_id,'refund:'||r.id,r.id,s.currency,s.accounts_json FROM (${refunds}) r JOIN (${sales}) s ON s.id=r.sale_id AND s.package_id=r.package_id AND s.company_id=r.company_id
  UNION SELECT r.package_id,r.company_id,'return:'||r.id||':'||l.id,r.id,s.currency,s.accounts_json FROM (${refunds}) r JOIN pos_lines l ON l.sale_id=r.sale_id JOIN (${sales}) s ON s.id=r.sale_id AND s.package_id=r.package_id AND s.company_id=r.company_id
  UNION SELECT rp.package_id,rp.company_id,'refund-payment:'||rp.id,rp.refund_id,s.currency,s.accounts_json FROM (${refundPayments}) rp JOIN (${sales}) s ON s.id=rp.sale_id AND s.package_id=rp.package_id AND s.company_id=rp.company_id
  -- Old cash/shift rows do not capture currency/account policy. Bind their
  -- exact native source and memo; never reinterpret history using today's settings.
  UNION SELECT s.package_id,s.company_id,'shift:'||s.id,s.id,NULL::text,NULL::text FROM (${shifts}) s
  UNION SELECT s.package_id,s.company_id,'cash:'||m.id,s.id,NULL::text,NULL::text FROM (${shifts}) s JOIN pos_cash_movements m ON m.shift_id=s.id`;
return {sales,stockFulfillments,shifts,payments,refunds,refundPayments,events,postingKeys};
}
/** Closed source SQL for the two included owners. No base product collections
 * or global Stripe connection/payout balances are attributed to an addon. */
export function posAddonPackageSettlementSources(packageId:string) {
 const {sales,stockFulfillments,payments,refunds,refundPayments,postingKeys}=sourceSql(packageId);
 return {
  sales: `SELECT package_id,company_id,id,version,state,invoice_id,currency,total_minor,accounts_json FROM (${sales}) source`,
  payments: `SELECT package_id,company_id,id,sale_id,state,version,finance_payment_id,amount_minor,method FROM (${payments}) source`,
  fulfillments: stockFulfillments,
  reservations: `SELECT f.package_id,f.company_id,r.fulfillment_id,r.warehouse_id,r.product_id,r.quantity_milli,f.version FROM pos_reservations r JOIN (${stockFulfillments}) f ON f.id=r.fulfillment_id`,
  stockMoves: `SELECT f.package_id,m.* FROM pos_stock_moves m JOIN (${stockFulfillments}) f ON f.id=m.fulfillment_id AND f.company_id=m.company_id`,
  requiredPostings: `SELECT s.package_id,s.company_id,prefix||s.id source_key FROM (${sales}) s CROSS JOIN (VALUES ('sale:'),('allocate:')) source(prefix) WHERE s.invoice_id IS NOT NULL AND s.total_minor>0
    UNION SELECT p.package_id,p.company_id,'receipt:'||p.id FROM (${payments}) p WHERE p.state='succeeded' AND p.amount_minor>0
    UNION SELECT f.package_id,f.company_id,'consume:'||f.id FROM (${stockFulfillments}) f WHERE f.cost_minor>0
    UNION SELECT f.package_id,f.company_id,'deliver:'||f.id FROM (${stockFulfillments}) f WHERE f.kind='recipe' AND f.state='delivered' AND f.cost_minor>0
    UNION SELECT f.package_id,f.company_id,'waste:'||f.id FROM (${stockFulfillments}) f WHERE f.kind='recipe' AND f.state='cancelled' AND f.cost_minor>0
    UNION SELECT r.package_id,r.company_id,'refund:'||r.id FROM (${refunds}) r WHERE r.state IN ('processing','completed') AND r.amount_minor>0
    UNION SELECT rp.package_id,rp.company_id,'refund-payment:'||rp.id FROM (${refundPayments}) rp WHERE rp.state='succeeded'
    UNION SELECT s.package_id,s.company_id,'room-reversal:'||s.id FROM (${sales}) s JOIN pos_room_charge_reversals r ON r.sale_id=s.id AND r.company_id=s.company_id WHERE r.restored_cost_minor+r.wasted_cost_minor>0`,
  postings: `SELECT source.package_id,source.source_id expected_source_id,source.currency expected_currency,source.accounts_json expected_accounts_json,p.* FROM pos_postings p JOIN (${postingKeys}) source ON source.company_id=p.company_id AND source.source_key=p.source_key`,
} as const;
}
export function assertPosAddonPackageObligationSources(owner: PostgresDatabase, database: SqlDatabase, companyId: string, packageId: string) {
  assertPosPackageObligationSources(owner, database, companyId);
  if (packageId === 'pos-hotel-charge') assertPmsPackageObligationSources(owner, database, companyId);
}
export async function posAddonPackageObligations(owner: PostgresDatabase, database: SqlDatabase, input: Input) {
  assertPosAddonPackageObligationSources(owner, database, input.companyId, input.packageId);
  if (input.moduleId !== 'pos' || !['pos-restaurant','pos-hotel-charge'].includes(input.packageId) || input.packageVersion !== 1 || !Number.isSafeInteger(input.now) || input.now < 0)
    throw new Error('POS_ADDON_PACKAGE_OBLIGATION_SCOPE_REQUIRED');
  const {sales,stockFulfillments,shifts,payments,refunds,refundPayments,events}=sourceSql(input.packageId);
  const facts = (await database.prepare(`WITH scope AS (SELECT ?::text company_id,?::text package_id),
    sales AS MATERIALIZED (SELECT source.* FROM (${sales}) source,scope WHERE source.company_id=scope.company_id AND source.package_id=scope.package_id),
    fulfillments AS MATERIALIZED (SELECT source.* FROM (${stockFulfillments}) source,scope WHERE source.company_id=scope.company_id AND source.package_id=scope.package_id),
    payments AS MATERIALIZED (SELECT source.* FROM (${payments}) source,scope WHERE source.company_id=scope.company_id AND source.package_id=scope.package_id),
    refunds AS MATERIALIZED (SELECT source.* FROM (${refunds}) source,scope WHERE source.company_id=scope.company_id AND source.package_id=scope.package_id),
    payouts AS MATERIALIZED (SELECT source.* FROM (${refundPayments}) source,scope WHERE source.company_id=scope.company_id AND source.package_id=scope.package_id),
    callbacks AS MATERIALIZED (SELECT source.* FROM (${events}) source,scope WHERE source.company_id=scope.company_id AND source.package_id=scope.package_id)
    SELECT 'sale' kind,s.id,s.version,s.state FROM sales s WHERE s.state NOT IN ('completed','cancelled','merged')
    UNION ALL SELECT 'shift',s.id,s.version,s.state FROM (${shifts}) s,scope WHERE s.company_id=scope.company_id AND s.package_id=scope.package_id AND s.state<>'reviewed'
    UNION ALL SELECT 'cash',m.id,0,'unapproved' FROM pos_cash_movements m JOIN (${shifts}) s ON s.id=m.shift_id,scope WHERE s.company_id=scope.company_id AND s.package_id=scope.package_id AND m.approved_by IS NULL
    UNION ALL SELECT 'kitchen',f.id,f.version,f.state FROM fulfillments f WHERE f.kind='recipe' AND f.state NOT IN ('delivered','cancelled')
    UNION ALL SELECT 'payment',p.id,p.version,p.state FROM payments p WHERE p.state IN ('requested','pending','exception')
    UNION ALL SELECT 'refund',r.id,r.version,r.state FROM refunds r WHERE r.state NOT IN ('completed','rejected')
    UNION ALL SELECT 'refund-payment',p.id,0,p.state FROM payouts p WHERE p.state<>'succeeded'
    UNION ALL SELECT 'room-reversal',r.id,r.version,r.state FROM pos_room_charge_reversal_requests r JOIN sales s ON s.id=r.sale_id AND s.company_id=r.company_id WHERE r.state IN ('requested','pos_approved')
    UNION ALL SELECT 'callback',e.id,0,'unapplied' FROM callbacks e WHERE e.applied_at IS NULL
    UNION ALL SELECT 'job',j.id,j.attempts,j.state FROM pos_jobs j,scope WHERE j.company_id=scope.company_id AND j.state<>'completed' AND (
      j.kind='complete_sale' AND EXISTS(SELECT 1 FROM sales s WHERE s.id=j.record_id)
      OR j.kind IN ('checkout','check_payment') AND EXISTS(SELECT 1 FROM payments p WHERE p.id=j.record_id)
      OR j.kind='refund' AND EXISTS(SELECT 1 FROM payouts p WHERE p.id=j.record_id)
      OR j.kind='event' AND EXISTS(SELECT 1 FROM callbacks e WHERE e.id=j.record_id))
    ORDER BY kind,id`).bind(input.companyId,input.packageId).all()).results;
  return { companyId:input.companyId,packageId:input.packageId,moduleId:'pos',observedAt:input.now,
    revision:createHash('sha256').update(JSON.stringify({companyId:input.companyId,packageVersion:input.packageVersion,packageId:input.packageId,facts})).digest('hex'),outstanding:facts.length };
}
